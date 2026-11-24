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
 const scheduledTime=[
  'function micsScheduledChangeAt(row){',
  " const raw=String(row?.start_at??'').trim();",
  ' if(!raw)return null;',
  " const iso=raw.replace(' ','T');",
  " const explicit=/(?:Z|[+-]\\d{2}:?\\d{2})$/i.test(iso);",
  " const ms=Date.parse(explicit?iso:iso+'+09:00');",
  ' return Number.isFinite(ms)?new Date(ms):null;',
  '}',
  ''
 ].join('\n');
 s=s.slice(0,a)+classifier+scheduledTime+s.slice(b);
 s=once(s,'void refreshLocalSchedule().then(() => {','void refreshLocalSchedule(true).then(() => {','SSE force');
 const oldEta=[
  '  if (change && change.row && etaSec != null) {',
  '    changeSec = etaSec + change.between;',
  '    changeAt = sampled ? new Date(sampled.getTime() + changeSec * 1000) : null;',
  '  }'].join('\n');
 // MICS owns the schedule. MSTART is Japan-local; do not overwrite it with a separate cycle forecast.
 const newEta=[
  '  if (change?.row) {',
  '    changeAt = micsScheduledChangeAt(change.row);',
  '    changeSec = changeAt ? Math.max(0,(changeAt.getTime()-Date.now())/1000) : null;',
  '  }'
 ].join('\n');
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
 // Use a two-line compact next-change display, matching the online machine node.
 // Display the original MICS schedule timestamp for all machine statuses.
 const compact=[
  '              <div class="sec-border border-t pt-1.5 mt-1.5 mics-next-compact">',
  '                <template v-if="metricsByMachine[m.machine_no].change?.row">',
  '                  <div class="mics-next-line">',
  '                    <span class="mics-next-title">次の変更</span>',
  '                    <span class="mics-next-head-right">',
  '                      <span v-for="(type,idx) in (metricsByMachine[m.machine_no].change.types || [])" :key="idx" class="mics-next-tag" :class="type === \'型替え\' ? \'mold\' : \'color\'">{{ type }}</span>',
  '                      <b class="mics-next-time" title="MICS予定時刻">· {{ fmtDT(metricsByMachine[m.machine_no].changeAt || metricsByMachine[m.machine_no].change.row.start_at,true,false) }}</b>',
   '                      <span class="mics-next-remain" :data-mics-target="metricsByMachine[m.machine_no].changeAt ? metricsByMachine[m.machine_no].changeAt.toISOString() : metricsByMachine[m.machine_no].change.row.start_at"></span>',
  '                    </span>',
  '                  </div>',
  '                  <div class="mics-next-line mics-next-detail">',
  '                    <span class="mics-next-title">次の金型</span>',
  '                    <span class="mics-next-head-right">',
  '                      <b>{{ metricsByMachine[m.machine_no].change.row.mold_name || metricsByMachine[m.machine_no].change.row.product_name || "—" }}</b>',
  '                      <small>({{ metricsByMachine[m.machine_no].change.row.mold_no || "—" }})</small>',
  '                      <span v-if="metricsByMachine[m.machine_no].change.nextColor" class="mics-next-product">{{ metricsByMachine[m.machine_no].change.nextColor }}</span>',
  '                    </span>',
  '                  </div>',
  '                </template>',
  '                <div v-else class="mics-next-line"><span class="mics-next-title">次の変更</span><b>変更予定なし</b></div>',
  '              </div>',
  ''
 ].join('\n');
 block=compact;
 s=s.slice(0,start)+block+s.slice(stop);
 save('js/dashboard-app.js',s);
 let components=read('js/components.js');
 components=once(components,"t === '刻印確認' ? 'stamp-check' : 'color'","t === '刻印確認' ? 'stamp-check' : (t === '品番替え' ? 'product' : 'color')",'product badge');
 save('js/components.js',components);
 save('css/tailwind-custom.css',read('css/tailwind-custom.css')+'\n.chg.product,.g-chg.product{background:#fef3c7;color:#92400e;border:1px solid #fbbf24;}\n');

 const compactCss=[
  '.mics-next-compact{font-size:12px;line-height:1.4;}',
  '.mics-next-line{display:flex;align-items:baseline;justify-content:space-between;gap:6px;min-width:0;}',
  '.mics-next-detail{margin-top:4px;}',
  '.mics-next-title{flex:0 0 auto;font-weight:500;color:#334155;}',
  '.mics-next-head-right{display:flex;align-items:baseline;justify-content:flex-end;flex-wrap:wrap;gap:3px;min-width:0;text-align:right;}',
  '.mics-next-head-right b{font-weight:800;overflow-wrap:anywhere;}',
  '.mics-next-time{white-space:nowrap;color:#0f172a;}',
  '.mics-next-remain{white-space:nowrap;color:#0f172a;font-size:10px;}',
  '.mics-next-tag{display:inline-block;padding:1px 6px;border-radius:4px;background:#fbbf24;color:#111827;font-size:11px;font-weight:800;white-space:nowrap;}',
  '.mics-next-tag.mold{background:#dc2626;color:white;}',
  '.mics-next-product{display:inline-block;border:1px solid #f59e0b;color:#92400e;border-radius:4px;padding:1px 5px;background:#fff7ed;font-size:11px;}',
  '.mics-next-detail small{font-size:10px;color:#475569;}',
  '@media(max-width:480px){.mics-next-line{flex-wrap:wrap;}.mics-next-head-right{justify-content:flex-start;text-align:left;}}',
  ''
 ].join('\n');
 save('css/tailwind-custom.css',read('css/tailwind-custom.css')+'\n'+compactCss);
  // Always show a live countdown for the forecast OR the scheduled time.
  // Only the text node is updated: no dashboard rerender or remote requests.
  save('js/next-change-countdown.js',"/* Local machine card countdown, independent of database and dashboard rendering. */\n(() => {\n  'use strict';\n  const selector = '.mics-next-remain[data-mics-target]';\n  function parseTarget(raw) {\n    if (!raw) return NaN;\n    let value = String(raw).trim();\n    // MICS schedule datetimes are Japan-local when they have no explicit offset.\n    if (/^\\d{4}[-/]\\d{1,2}[-/]\\d{1,2}[ T]\\d{1,2}:\\d{2}/.test(value)) {\n      value = value.replace(/\\//g, '-').replace(' ', 'T');\n      if (!/(?:Z|[+-]\\d{2}:?\\d{2})$/i.test(value)) value += '+09:00';\n    }\n    return Date.parse(value);\n  }\n  function updateCountdowns() {\n    const now = Date.now();\n    for (const element of document.querySelectorAll(selector)) {\n      const target = parseTarget(element.getAttribute('data-mics-target'));\n      const minutes = Number.isFinite(target) ? Math.ceil((target - now) / 60000) : NaN;\n      const label = !Number.isFinite(minutes) ? ''\n        : minutes <= 0 ? '(予定時刻経過)'\n        : '(あと' + Math.floor(minutes / 60) + '時間' + (minutes % 60) + '分)';\n      if (element.textContent !== label) element.textContent = label;\n    }\n  }\n  function init() {\n    updateCountdowns();\n    const root = document.getElementById('app');\n    if (root && typeof MutationObserver === 'function') {\n      // Refresh new cards after native JS dashboard re-renders. Ignore our text changes.\n      const observer = new MutationObserver(records => {\n        const cardsAdded = records.some(record => Array.from(record.addedNodes).some(node =>\n          node.nodeType === 1 && (node.matches?.(selector) || node.querySelector?.(selector))));\n        if (cardsAdded) updateCountdowns();\n      });\n      observer.observe(root, {childList: true, subtree: true});\n    }\n    // Text-only update: no full dashboard re-render and no HTTP/database requests.\n    setInterval(updateCountdowns, 10000);\n    document.addEventListener('visibilitychange', () => {\n      if (!document.hidden) updateCountdowns();\n    });\n  }\n  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', init, {once: true});\n  else init();\n})();");
  const html=read('index.html');
  save('index.html',once(html,'</body>','<script src="js/next-change-countdown.js" defer></script>\n</body>','countdown loader'));

}
