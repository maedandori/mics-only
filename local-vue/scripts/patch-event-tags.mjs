// Display 乾燥 and stamp change tags consistently on node and 区分 rows.
// Run after existing mold/drying/stamp patches; do not change event detection.
import {readFileSync,writeFileSync} from 'node:fs';
import {resolve} from 'node:path';
export function patchEventTags(root){
 const once=(s,a,b,name)=>{const n=s.split(a).length-1;if(n!==1)throw Error('Tag patch '+name+' '+n);return s.replace(a,b)};
 const edit=(name,fn)=>{const file=resolve(root,'js',name),old=readFileSync(file,'utf8'),next=fn(old);if(old===next)throw Error('No tag change '+name);writeFileSync(file,next)};
 edit('common.js',s=>once(s,'export function extractMoldAlias(',
  ['// Visual tags: keep change detection independent of alias decorations.',
   'export function changeEventTags(row,types,cfg=null){',
   ' const result=Array.isArray(types)?[...new Set(types)]:[];',
   " if(isDryingChange(row,result,cfg)&&!result.includes('乾燥'))result.push('乾燥');",
   ' return result;',
   '}',''].join('\n')+'export function extractMoldAlias(','shared label helper'));
 edit('components.js',s=>once(s,
  "t === '型替え' ? 'mold' : (t === '刻印替え' ? 'stamp' : (t === '刻印確認' ? 'stamp-check' : 'color')),",
  "t === '型替え' ? 'mold' : (t === '乾燥' ? 'drying' : (t === '刻印替え' ? 'stamp' : (t === '刻印確認' ? 'stamp-check' : 'color'))),",
  'drying style map'));
 edit('dashboard-app.js',s=>{
  s=once(s,'extractColor, extractStamp, extractMoldAlias, extractDryingMold, isDryingChange, productColorStyle,',
     'extractColor, extractStamp, extractMoldAlias, extractDryingMold, isDryingChange, changeEventTags, productColorStyle,','dashboard import');
  s=once(s,'      isDryingChange,\n      setView,','      isDryingChange,\n      changeEventTags,\n      setView,','dashboard expose');
  const from=':types="metricsByMachine[m.machine_no].change.types"';
  if(s.split(from).length<3)throw Error('Missing both grid and table node tag groups');
  s=s.replaceAll(from,':types="changeEventTags(metricsByMachine[m.machine_no].change.row,metricsByMachine[m.machine_no].change.types,state.aliasCfg)"');
  s=once(s,':types="modalDetail.c.types"',':types="changeEventTags(modalDetail.c.row,modalDetail.c.types,state.aliasCfg)"','machine detail');
  s=once(s,'              types: a >= ws ? types : [],','              types: a >= ws ? types : [],\n              displayTypes: changeEventTags(r,a >= ws ? types : [],state.aliasCfg),','Gantt data');
  s=once(s,'<change-tags :types="bar.types" :gantt="true" /><span v-if="bar.drying" class="rounded px-1 bg-orange-100 text-orange-900 font-bold">♨ 乾燥</span>{{ bar.productName }}',
    '<change-tags :types="bar.displayTypes" :gantt="true" />{{ bar.productName }}','Gantt badges');
  s=once(s,'              types,\n              cur: r.order_no === m.order_no,',
    '              types,\n              displayTypes: changeEventTags(r,types,state.aliasCfg),\n              cur: r.order_no === m.order_no,','machine detail rows');
  s=once(s,'<change-tags v-if="row.types.length" :types="row.types" :small="true" />',
    '<change-tags v-if="row.displayTypes.length" :types="row.displayTypes" :small="true" />','detail table badge');
  const redundant=/<span v-if="isDryingChange\(metricsByMachine\[m\.machine_no\]\.change\.row,metricsByMachine\[m\.machine_no\]\.change\.types,state\.aliasCfg\)"[^>]*>♨ 乾燥<\/span>/g;
  let count=0;s=s.replace(redundant,()=>{count++;return ''});if(count!==1)throw Error('Previous node drying badge '+count);
  return s;
 });
 edit('change-schedule-app.js',s=>{
  s=once(s,'extractColor, productColorStyle, extractDryingMold, isDryingChange, annotateScheduleChanges, getAliasConfig,',
    'extractColor, productColorStyle, extractDryingMold, isDryingChange, changeEventTags, annotateScheduleChanges, getAliasConfig,','schedule import');
  s=once(s,'      isDryingChange,\n      dayPills,','      isDryingChange,\n      changeEventTags,\n      dayPills,','schedule expose');
  s=once(s,'          types: item.types,',
    '          types: item.types,\n          displayTypes: changeEventTags(r,item.types,state.aliasCfg),','event tags');
  const oldTag='<change-tags :types="ev.types" :small="true" />';
  const n=s.split(oldTag).length-1;if(n<4)throw Error('Expected 区分 badges '+n);
  s=s.replaceAll(oldTag,'<change-tags :types="ev.displayTypes" :small="true" />');
  const extra='<span v-if="ev.drying" class="rounded px-1 bg-orange-100 border border-orange-400 text-orange-900 text-[11px] font-bold">♨ 乾燥</span>';
  if(s.split(extra).length-1!==n)throw Error('Old redundant 乾燥 schedule badge count mismatch');
  s=s.replaceAll(extra,'');
  s=once(s,'          cur: r.order_no === m.order_no,',
    '          displayTypes: changeEventTags(r,types,state.aliasCfg),\n          cur: r.order_no === m.order_no,','schedule modal row');
  s=once(s,'<change-tags v-if="row.types.length" :types="row.types" :small="true" />',
    '<change-tags v-if="row.displayTypes.length" :types="row.displayTypes" :small="true" />','modal badge');
  let count=0;
  s=s.replace(/<span v-if="row\.drying"[^>]*>♨ 乾燥<\/span>/g,()=>{count++;return ''});
  if(count!==1)throw Error('Duplicate old modal drying badge '+count);
  return s;
 });
 const cssPath=resolve(root,'css/tailwind-custom.css');
 let css=readFileSync(cssPath,'utf8');
 if(css.includes('.chg.drying'))throw Error('CSS drying tag already exists');
 css+='\n/* Drying tag for actual change events only. */\n.chg.drying, .g-chg.drying { background:#fff7ed;color:#9a3412;border:1px solid #fb923c;font-weight:700;white-space:nowrap; }\n';
 writeFileSync(cssPath,css);
}
