import test from 'node:test'
import assert from 'node:assert/strict'
import {readFileSync} from 'node:fs'

const base='www/vue-v'+JSON.parse(readFileSync('local-vue/package.json','utf8')).version+'/js/'
const dashboard=readFileSync(base+'dashboard-app.js','utf8')
const schedule=readFileSync(base+'change-schedule-app.js','utf8')
const server=readFileSync('src/local_web.rs','utf8')

test('machine/schedule pages use SSE, local version watchdog and reconnect refresh',()=>{
 for(const page of [dashboard,schedule]){
  assert.ok(page.includes("new EventSource('/api/live')"))
  assert.ok(page.includes("fetch('/api/json-versions',{cache:'no-store'})"))
  assert.ok(page.includes('jsonVersionTimer=setInterval(()=>void checkLocalJsonVersions(),8000)'))
  assert.ok(page.includes('liveJsonStream.onopen=()=>{'))
  assert.ok(page.includes('void load();'))
  assert.ok(page.includes('liveJsonStream?.close()'))
  assert.ok(page.includes('clearInterval(jsonVersionTimer)'))
  assert.ok(page.includes('refreshLocalSchedule'))
 }
})
test('schedule SSE events bypass old cache throttle and refresh production times',()=>{
 assert.ok(schedule.includes('void refreshLocalSchedule(true).then(()=>void load())'))
 assert.ok(dashboard.includes('void refreshLocalSchedule(true).then(() => {'))
 assert.ok(schedule.includes('setInterval(() => load(), 15000)'))
})
test('pending dashboard updates replay when initial JSON fetch is still running',()=>{
 assert.ok(dashboard.includes('let loadBusy = false, loadQueued = false;'))
 assert.ok(dashboard.includes('if (loadBusy) {loadQueued=true;return;}'))
 assert.ok(dashboard.includes('queueMicrotask(()=>void load())'))
})
test('agent owns a metadata-only localhost JSON version API',()=>{
 assert.ok(server.includes('route=="/api/json-versions"'))
 assert.ok(server.includes('fn json_version(path:&Path)->Option<String>'))
 assert.ok(server.includes('"machines":json_version(&state.data_path)'))
 assert.ok(server.includes('"schedule":json_version(&state.data_path.with_file_name("schedule-local.json"))'))
 assert.ok(server.includes('TcpListener::bind("127.0.0.1:8787")'))
})
test('version watchdog reacts to machine, schedule and settings modifications',async()=>{
 const start=dashboard.indexOf('    async function checkLocalJsonVersions() {')
 const stop=dashboard.indexOf('    function startLocalJsonLive() {',start)
 assert.ok(start>0&&stop>start,'watchdog source must be present')
 const code=dashboard.slice(start,stop)
 let next={ok:true,machines:'1:100',schedule:'2:200',aliases:'3:30',colors:'4:40'}
 const events=[]
 const fnHead=[
  'let versionRequestBusy=false,knownJsonVersions=null,liveJsonConnected=true,refreshTimer=null;',
  "function ensureLocalFallback(){throw Error('Should not need fallback')}",
 ].join('\n')
 const fnTail='\nreturn {check:()=>checkLocalJsonVersions()};'
 const api=new Function('fetch','onLocalJsonEvent','setInterval',fnHead+'\n'+code+fnTail)(
  async()=>({ok:true,json:async()=>({...next})}),
  e=>events.push(JSON.parse(e.data).kind),
  ()=>1
 )
 await api.check()
 assert.deepEqual(events,[])
 next={...next,machines:'5:101'}
 await api.check()
 assert.deepEqual(events,['machines'])
 next={...next,schedule:'6:201'}
 await api.check()
 assert.deepEqual(events,['machines','schedule'])
 next={...next,aliases:'7:32'}
 await api.check()
 assert.deepEqual(events,['machines','schedule','settings'])
})

test('header countdown uses the real agent cycle without DB requests',()=>{
 const ui=readFileSync(base+'local-horizontal-tabs.js','utf8')
 assert.ok(ui.includes('全機械の品番一覧</button><button type="button" id="mics-local-refresh"'))
 assert.ok(ui.includes("fetch('/api/status',{cache:'no-store'})"))
 assert.ok(ui.includes('Date.parse(refreshStatus?.next_poll_at'))
 assert.ok(ui.includes('setInterval(renderRefreshClock,1000)'))
 assert.ok(ui.includes("refreshButton.addEventListener('click',()=>location.reload())"))
 assert.ok(server.includes('"poll_interval_seconds":state.poll_interval_seconds'))
 assert.ok(server.includes('"next_poll_at":state.next_poll_at'))
 const agent=readFileSync('src/main.rs','utf8')
 assert.match(agent,/const ACTIVE_SCHEDULE_SECONDS: u64 = 20;/)
 assert.match(agent,/const STANDBY_MIRROR_SECONDS: u64 = 100;/)
 assert.match(agent,/const FAST_PRODUCTION_SECONDS: u64 = 5;/)
 assert.ok(agent.includes('AdaptivePolling::new(FAST_PRODUCTION_SECONDS,c.interval)'))
 assert.ok(agent.includes('local.set_poll_interval(polling.production_seconds())'))
 assert.ok(agent.includes('local.set_poll_interval(STANDBY_MIRROR_SECONDS)'))
 assert.ok(agent.includes('local.schedule_next_poll(Some(next_data_wait))'))
 assert.ok(agent.includes('local.schedule_next_poll(if valid_url&&!mirror_secret.is_empty(){Some(mirror_wait)}else{None})'))
 assert.ok(agent.includes('remaining_poll_seconds(last_mirror,STANDBY_MIRROR_SECONDS)'))
 assert.ok(agent.includes('let production_due=polling::due(last_data_poll,polling.production_seconds())'))
 assert.ok(agent.includes('let schedule_due=polling::due(schedule_polled_at,polling.schedule_seconds(ACTIVE_SCHEDULE_SECONDS))'))
 assert.ok(agent.includes('if !production_due&&!schedule_due&&!material_due'))
 assert.ok(agent.includes('if production_due {'))
 assert.ok(agent.includes('if schedule_due{'))
 assert.match(agent,/let data_wait=remaining_poll_seconds\(last_data_poll,polling\.production_seconds\(\)\)\s*\.min\(remaining_poll_seconds\(schedule_polled_at,polling\.schedule_seconds\(ACTIVE_SCHEDULE_SECONDS\)\)\)/)
 assert.match(agent,/next_data_wait=remaining_poll_seconds\(last_data_poll,polling\.production_seconds\(\)\)\s*\.min\(remaining_poll_seconds\(schedule_polled_at,polling\.schedule_seconds\(ACTIVE_SCHEDULE_SECONDS\)\)\)/)
 assert.ok(agent.includes('next_wait=next_data_wait.min(lease_wait_seconds())'))
 assert.ok(agent.includes('thread::sleep(Duration::from_secs(next_wait))'))
 assert.ok(!agent.includes('local.schedule_next_poll(Some(c.interval))'))
})

test('material weight uses remaining pieces independently of cavity count',()=>{
 const ui=readFileSync(base+'local-horizontal-tabs.js','utf8')
 const a=ui.indexOf('function materialEstimate('),b=ui.indexOf('function titleText(',a)
 assert.ok(a>0&&b>a)
 const estimate=new Function(ui.slice(a,b)+'return materialEstimate;')()
 assert.equal(estimate(10001,4,117).shots,2501)
 assert.equal(estimate(10001,4,117).produced,10001)
 assert.equal(estimate(10001,4,117).totalKg,1170.117)
 assert.equal(estimate(10000,4,117).totalKg,1170)
 assert.equal(estimate(0,4,117).totalKg,0)
 assert.equal(estimate(10000,undefined,110).totalKg,1100)
 assert.equal(estimate(100,0,117).totalKg,11.7)
 assert.equal(estimate(100,2.5,117).totalKg,11.7)
 assert.equal(estimate(1,8,25).totalKg,0.025)
 for(const args of [[-1,4,117],['',4,117],[100,4,null]])assert.equal(estimate(...args),null)
 assert.ok(ui.includes('qty_plan:m.qty_plan??r.qty_plan'))
 assert.ok(ui.includes('pick_qty:m.pick_qty??r.pick_qty'))
 assert.ok(ui.includes('qty_remaining:r.qty_remaining??m.qty_remaining'))
 assert.ok(ui.includes('mics-material-remaining-qty'))
 assert.ok(ui.includes('WEIGHT・残数を確認してください'))
})

test('clicking any all-products cell opens details; keyboard activation works',()=>{
 const ui=readFileSync(base+'local-horizontal-tabs.js','utf8')
 assert.ok(ui.includes("target.closest('tr[data-mics-detail]')"))
 assert.ok(ui.includes("productRows.addEventListener('click'"))
 assert.ok(ui.includes("productRows.addEventListener('keydown'"))
 assert.ok(ui.includes("e.key!=='Enter'&&e.key!==' '"))
 assert.ok(ui.includes('tabindex="0" role="button" aria-label="品番の詳細:'))
 assert.ok(!ui.includes('<button type="button" data-mics-detail='))
 const css=readFileSync('www/vue-v'+JSON.parse(readFileSync('local-vue/package.json','utf8')).version+'/css/local-horizontal-tabs.css','utf8')
 assert.ok(css.includes('#mics-products tr[data-mics-detail]:focus-visible'))
})

test('remaining quantity used for resin, not total plan',()=>{
 const ui=readFileSync(base+'local-horizontal-tabs.js','utf8')
 assert.ok(ui.includes("activeByOrder.get(machineKey(r)+'|'+String(r.order_no??''))"))
 assert.ok(ui.includes('qty_remaining:r.qty_remaining??m.qty_remaining'))
 assert.ok(ui.includes('remaining_from_live:r.qty_remaining==null&&m.qty_remaining!=null'))
 assert.ok(!ui.includes('materialEstimate(planInput?.value'))
 const a=ui.indexOf('function materialEstimate('),b=ui.indexOf('function titleText(',a)
 const calculate=new Function(ui.slice(a,b)+'return materialEstimate')()
 assert.equal(calculate(1001,4,117).totalKg,117.117)
 assert.equal(calculate(0,4,117).totalKg,0)
 assert.equal(calculate('',4,117),null)
})
