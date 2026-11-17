import {readFileSync,writeFileSync} from 'node:fs';
import {resolve} from 'node:path';
export function patchNextChange(root){
 const read=f=>readFileSync(resolve(root,f),'utf8');
 const save=(f,c)=>writeFileSync(resolve(root,f),c);
 const once=(s,a,b,k)=>{if(s.split(a).length!==2)throw Error('Next change anchor '+k);return s.replace(a,b)};
 let common=read('js/common.js');
 common=once(common,'export async function refreshLocalSchedule() {','export async function refreshLocalSchedule(force=false) {','schedule function');
 common=once(common,'if (Date.now() - scheduleRequestedAt < 5000 && lastScheduleText) return;','if (!force && Date.now() - scheduleRequestedAt < 5000 && lastScheduleText) return;','schedule throttle');
 save('js/common.js',common);
 let s=read('js/dashboard-app.js');
 const a=s.indexOf('function findNextChange(m, sched, aliasCfg) {');
 const b=s.indexOf('\nfunction calcMetrics(m, schedules, aliasCfg) {',a);
 if(a<0||b<=a)throw Error('Missing next-change function');
 const classifier=[
  'function findNextChange(m,sched,cfg){',
  ' if(!sched?.rows?.length||!m.order_no)return null;',
  " const norm=x=>String(x??'').trim().normalize('NFKC').toLowerCase(),time=x=>parseDate(x)?.getTime();",
  ' const rows=[...sched.rows].sort((a,b)=>{const x=time(a.start_at),y=time(b.start_at);return Number.isFinite(x)&&Number.isFinite(y)&&x!==y?x-y:Number(a.sequence||0)-Number(b.sequence||0)});',
  ' const mold=norm(m.mold_no),prod=norm(m.product_name),order=norm(m.order_no);',
  ' let found=rows.map((r,i)=>({r,i})).filter(({r})=>norm(r.order_no)===order&&(!mold||norm(r.mold_no)===mold));',
  ' if(!found.length&&mold&&prod)found=rows.map((r,i)=>({r,i})).filter(({r})=>norm(r.mold_no)===mold&&norm(r.product_name)===prod);',
  ' if(!found.length)return null;',
  ' const before=found.filter(({r})=>Number.isFinite(time(r.start_at))&&time(r.start_at)<=Date.now());',
  ' const current=before.length?before[before.length-1]:found[0],cur=current.r;',
  ' const color=extractColor({color:cur.color,product_name:cur.product_name||m.product_name},cfg),stamp=extractStamp(cur,cfg),alias=extractMoldAlias(cur,cfg);',
  ' let between=0;',
  ' for(let i=current.i+1;i<rows.length;i++){',
  '  const r=rows[i],t=time(r.start_at),end=time(r.end_at);',
  '  if(!r.mold_no){if(Number.isFinite(t)&&Number.isFinite(end)&&end>t)between+=(end-t)/1000;continue}',
  '  const types=detectChangeTypes(cur.mold_no||m.mold_no,color,r,stamp,alias,cfg);',
  "  if(!types.length&&norm(r.product_name)!==norm(cur.product_name||m.product_name))types.push('品番替え');",
  '  if(types.length)return {types,type:types.join("・"),row:r,nextColor:extractColor(r,cfg),between};',
  '  if(Number.isFinite(t)&&Number.isFinite(end)&&end>t)between+=(end-t)/1000;',
  ' }',
  ' return null;',
  '}',
  ''
 ].join('\n');
 s=s.slice(0,a)+classifier+s.slice(b);
 s=once(s,'void refreshLocalSchedule().then(() => {','void refreshLocalSchedule(true).then(() => {','SSE force');
 const oldEta=[
  '  if (change && change.row && etaSec != null) {',
  '    changeSec = etaSec + change.between;',
  '    changeAt = sampled ? new Date(sampled.getTime() + changeSec * 1000) : null;',
  '  }'].join('\n');
 const newEta=[
  "  if(change?.row&&['200','210'].includes(String(m.status_code))&&m.qty_remaining!=null",
  '     &&m.pick_qty!=null&&m.cycle_seconds!=null&&cyc>0&&pick>0&&Number.isFinite(remaining)',
  '     &&remaining>=0&&sampled&&sampled.getTime()<=Date.now()+60000',
  '     &&Date.now()-sampled.getTime()<=120000){',
  '    changeSec=Math.ceil(remaining/pick)*cyc+Math.max(0,Number(change.between)||0);',
  '    changeAt=new Date(Date.now()+changeSec*1000);',
  '  }'].join('\n');
 s=once(s,oldEta,newEta,'ETA');
 const start=s.indexOf('              <div class="sec-border border-t pt-1.5 mt-1.5">');
 const stop=s.indexOf('            </article>',start);
 if(start<0||stop<=start)throw Error('Node next-change section missing');
 let block=s.slice(start,stop);
 const extras=[
  '                  <div class="flex justify-between items-baseline gap-2 mt-1 text-[12.5px]">',
  '                    <span class="text-slate-700 font-semibold">変更予測</span>',
  '                    <b v-if="metricsByMachine[m.machine_no].changeAt" class="text-red-700">{{ fmtDT(metricsByMachine[m.machine_no].changeAt,true,false) }}</b>',
  '                    <b v-else class="text-slate-700">予定 {{ fmtDT(metricsByMachine[m.machine_no].change.row.start_at,true,false) }}</b>',
  '                  </div>',
  '                  <div class="flex items-baseline gap-1.5 mt-1 text-[12px] min-w-0">',
  '                    <span class="shrink-0 text-slate-700">次の品番</span>',
  '                    <b class="truncate min-w-0">{{ metricsByMachine[m.machine_no].change.row.order_no || "—" }} · {{ metricsByMachine[m.machine_no].change.row.product_name || "—" }}</b>',
  '                  </div>',
  ''].join('\n');
 block=once(block,'                </template>\n                <div v-else class=',extras+'                </template>\n                <div v-else class=','node display');
 s=s.slice(0,start)+block+s.slice(stop);
 save('js/dashboard-app.js',s);
 let components=read('js/components.js');
 components=once(components,"t === '刻印確認' ? 'stamp-check' : 'color'","t === '刻印確認' ? 'stamp-check' : (t === '品番替え' ? 'product' : 'color')",'product badge');
 save('js/components.js',components);
 save('css/tailwind-custom.css',read('css/tailwind-custom.css')+'\n.chg.product,.g-chg.product{background:#fef3c7;color:#92400e;border:1px solid #fbbf24;}\n');
}
