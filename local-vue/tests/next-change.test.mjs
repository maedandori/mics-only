import test from 'node:test'
import {runInNewContext} from 'node:vm'
import assert from 'node:assert/strict'
import {readFileSync} from 'node:fs'
import {pathToFileURL} from 'node:url'

const root='www/vue-v'+JSON.parse(readFileSync('local-vue/package.json','utf8')).version+'/js/'
const common=await import(pathToFileURL(process.cwd()+'/'+root+'common.js').href)
const dashboard=readFileSync(root+'dashboard-app.js','utf8')
const start=dashboard.indexOf('function findNextChange(')
const end=dashboard.indexOf('\nfunction calcMetrics(',start)
assert.ok(start>=0 && end>start,'Next-change classifier must exist')
const getNext=new Function('parseDate','extractColor','extractStamp','extractMoldAlias','detectChangeTypes',
 dashboard.slice(start,end)+'\nreturn findNextChange;')(
 common.parseDate,common.extractColor,common.extractStamp,common.extractMoldAlias,common.detectChangeTypes
)
const cfg=common.normalizeAliasConfig({color:[],mold:[],stamp:[],drying:[],animations:[]})
const t=(delta)=>new Date(Date.now()+delta*60000).toISOString()
const row=(seq,no,product,mold,color,start,end)=>({
 sequence:seq,order_no:no,product_name:product,mold_no:mold,mold_name:mold,
 color,start_at:t(start),end_at:t(end)
})
const sched={rows:[
 row(1,'101','A','M1','WHITE',-60,60),
 row(2,'102','B','M1','WHITE',60,120),
 row(3,'103','C','M1','BLACK',120,180),
 row(4,'104','D','M2','BLACK',180,240)
]}
test('品番替え appears for next product when mold and color are unchanged',()=>{
 const next=getNext({order_no:'101',mold_no:'M1',product_name:'A'},sched,cfg)
 assert.deepEqual(next.types,['品番替え'])
 assert.equal(next.row.order_no,'102')
})
test('color and mold changes are detected on the following order',()=>{
 assert.deepEqual(getNext({order_no:'102',mold_no:'M1',product_name:'B'},sched,cfg).types,['色替え'])
 assert.deepEqual(getNext({order_no:'103',mold_no:'M1',product_name:'C'},sched,cfg).types,['型替え'])
})
test('current machine selection is resilient to repeated historical order numbers',()=>{
 const repeated={rows:[
 row(1,'500','OLDER','M1','WHITE',-240,-220),
 row(2,'501','BETWEEN','M1','WHITE',-220,-190),
 row(3,500,'NOW','M1','WHITE',-30,30),
 row(4,'503','NEXT','M2','WHITE',30,90)
 ]}
 const next=getNext({order_no:'500',mold_no:'M1',product_name:'NOW'},repeated,cfg)
 assert.equal(next.row.order_no,'503')
 assert.deepEqual(next.types,['型替え'])
})
test('node shows change type, predicted/scheduled time, next product, and reuses JSON',()=>{
 assert.ok(dashboard.includes('変更予測'))
 assert.ok(dashboard.includes('次の品番'))
 assert.ok(dashboard.includes("types.push('品番替え')"))
 assert.ok(dashboard.includes('refreshLocalSchedule(true)'))
 assert.ok(dashboard.includes('changeAt = micsScheduledChangeAt(change.row)'))
 assert.ok(!dashboard.includes('changeAt=new Date(Date.now()+changeSec*1000)'))
 assert.ok(!dashboard.includes("fetch('/api/mics/machines'"))
})

test('machine cards count down every minute in JST using predicted or scheduled time',()=>{
 const js=readFileSync(root+'next-change-countdown.js','utf8')
 assert.match(dashboard,/data-mics-target=.*change\.row\.start_at/)
 assert.doesNotMatch(js,/\bfetch\s*\(/)
 let now=Date.parse('2026-11-18T11:44:00+09:00')
 class Clock extends Date { static now(){return now} }
 const scheduled={textContent:'',getAttribute:()=> '2026/11/19 00:24'}
 const predicted={textContent:'',getAttribute:()=> '2026-11-19T00:24:00+09:00'}
 const intervals=[]
 const document={
  readyState:'complete',hidden:false,
  querySelectorAll:()=>[scheduled,predicted],
  getElementById:()=>null,addEventListener:()=>{}
 }
 runInNewContext(js,{Date:Clock,document,setInterval:fn=>intervals.push(fn)})
 assert.equal(intervals.length,1)
 assert.equal(scheduled.textContent,'(あと12時間40分)')
 assert.equal(predicted.textContent,'(あと12時間40分)')
 now+=60_000;intervals[0]()
 assert.equal(scheduled.textContent,'(あと12時間39分)')
 now=Date.parse('2026-11-19T00:25:00+09:00');intervals[0]()
 assert.equal(scheduled.textContent,'(予定時刻経過)')
})

test('next-change time comes from MICS MSTART in JST, not remaining quantity or browser time',()=>{
 const start=dashboard.indexOf('function micsScheduledChangeAt(')
 const end=dashboard.indexOf('\nfunction calcMetrics(',start)
 assert.ok(start>=0&&end>start)
 const scheduledAt=new Function(dashboard.slice(start,end)+'\nreturn micsScheduledChangeAt;')()
 assert.equal(scheduledAt({start_at:'2026-11-25 09:50:00'}).toISOString(),'2026-11-25T00:50:00.000Z')
 assert.equal(scheduledAt({start_at:'2026-11-26 00:10:00'}).toISOString(),'2026-11-25T15:10:00.000Z')
 assert.equal(scheduledAt({start_at:'2026-11-25T09:50:00+09:00',qty_remaining:999999,cycle_seconds:200}).toISOString(),'2026-11-25T00:50:00.000Z')
 assert.equal(scheduledAt({start_at:'invalid'}),null)
 assert.equal(scheduledAt({}),null)
})
