import test from 'node:test'
import assert from 'node:assert/strict'
import {readFileSync} from 'node:fs'

const base='www/vue-v0.2.40/js/'
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
