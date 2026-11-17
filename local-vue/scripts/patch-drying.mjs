// 乾燥 alias identifies molds; display badge ONLY at actual 型替え/色替え events.
// Run after standard online/local transforms; preserve all original layouts.
import {readFileSync,writeFileSync} from 'node:fs';
import {resolve} from 'node:path';
export function patchDryingTemplate(root){
 const change=(file,fn)=>{
  const path=resolve(root,'js',file),old=readFileSync(path,'utf8'),now=fn(old);
  if(old===now)throw Error('No drying changes in '+file);
  writeFileSync(path,now);
 };
 const once=(s,a,b,label)=>{
  if(s.split(a).length!==2)throw Error('Drying anchor '+label+' missing/duplicate');
  return s.replace(a,b);
 };
 const badge=(expr,types)=>'<span v-if="isDryingChange('+expr+','+types+',state.aliasCfg)" class="inline-block rounded-full border border-orange-400 bg-orange-100 text-orange-900 font-bold text-[11px] px-2 py-0.5 whitespace-nowrap">♨ 乾燥</span>';
 change('common.js',s=>{
  s=once(s,
   '    stamp: Array.isArray(src.stamp) ? normalizeRuleList(src.stamp) : normalizeRuleList(DEFAULT_CHANGE_ALIASES.stamp),',
   '    stamp: Array.isArray(src.stamp) ? normalizeRuleList(src.stamp) : normalizeRuleList(DEFAULT_CHANGE_ALIASES.stamp),\n    drying: Array.isArray(src.drying) ? normalizeRuleList(src.drying) : [],',
   'normalize');
  const fn=[
  "// Only actual mold/color transitions of a tagged mold display 乾燥.",
  "const dryingKey=v=>String(v??'').normalize('NFKC').replace(/\\s+/g,'').toLowerCase();",
  "export function extractDryingMold(row,cfg=null){",
  " const rules=(cfg||getAliasConfig()).drying||[];",
  " const code=dryingKey(row?.mold_no),name=dryingKey(row?.mold_name);",
  " if(!code&&!name)return '';",
  " const root=v=>v.replace(/-[a-z0-9]+$/i,'');",
  " for(const group of rules){",
  "  if(typeof group?.canonical!=='string')continue;",
  "  for(const raw of [group.canonical,...(group.aliases||[])]){",
  "   const alias=dryingKey(raw);if(!alias)continue;",
  "   if(code&&(code===alias||root(code)===root(alias)))return group.canonical;",
  "   if(name&&(name===alias||(alias.length>=3&&name.includes(alias))))return group.canonical;",
  "  }",
  " }",
  " return '';",
  "}",
  "export const isDryingMold=(row,cfg)=>Boolean(extractDryingMold(row,cfg));",
  "export const isDryingChange=(row,types,cfg)=>Boolean(Array.isArray(types) &&",
  "  (types.includes('型替え')||types.includes('色替え')) && isDryingMold(row,cfg));",
  "",
  ].join('\n');
  return once(s,'export function extractMoldAlias(',fn+'export function extractMoldAlias(','matcher');
 });
 change('change-aliases-app.js',s=>{
  s=once(s,'saveAliasConfig, resetAliasConfig, extractColor, extractStamp, extractMoldAlias,',
   'saveAliasConfig, resetAliasConfig, extractColor, extractStamp, extractMoldAlias, extractDryingMold,','alias imports');
  s=once(s,"      { id: 'stamp', label: '🔖 刻印替えエイリアス (刻印)', count: (state.cfg.stamp || []).length },",
   "      { id: 'stamp', label: '🔖 刻印替えエイリアス (刻印)', count: (state.cfg.stamp || []).length },\n      { id: 'drying', label: '♨ 乾燥 (金型別)', count: (state.cfg.drying || []).length },",'alias tab');
  s=once(s,': extractMoldAlias(p, state.cfg);',
   ": state.cat === 'drying' ? extractDryingMold(p,state.cfg) : extractMoldAlias(p,state.cfg);",'match counts');
  s=once(s,'        const stamp = extractStamp({ product_name: p.product_name }, state.cfg);',
   '        const stamp = extractStamp({ product_name: p.product_name }, state.cfg);\n        const drying = extractDryingMold(p,state.cfg);','preview match');
  s=once(s,'        rows.push({ ...p, color, moldAlias, stamp });',
   '        rows.push({ ...p, color, moldAlias, stamp, drying });','preview rows');
  s=once(s,'      productPreview,\n      switchCategory,',
   '      productPreview,\n      extractDryingMold,\n      switchCategory,','alias setup');
  s=once(s,'          <!-- Rules Table -->',
   '          <p v-if="state.cat === \'drying\'" class="text-xs text-orange-900 bg-orange-50 border border-orange-200 rounded-lg p-2 mb-3">♨ 乾燥: 判定名に金型名または金型番号を入力。認識キーワードに同じ金型の別名や番号を追加します。品名では判定せず、型替え・色替えが発生した行のみ「乾燥」タグを表示します。</p>\n          <!-- Rules Table -->','alias hint');
  s=once(s,'@click="quickFillRule(r.product_name)"',
   '@click="quickFillRule(state.cat===\'drying\' ? (r.mold_no || r.mold_name) : r.product_name)"','quickfill');
  return s;
 });
 change('dashboard-app.js',s=>{
  s=once(s,'extractColor, extractStamp, extractMoldAlias, productColorStyle,',
   'extractColor, extractStamp, extractMoldAlias, extractDryingMold, isDryingChange, productColorStyle,','dashboard import');
  s=once(s,'      productColorStyle,\n      setView,',
   '      productColorStyle,\n      extractDryingMold,\n      isDryingChange,\n      setView,','dashboard setup');
  s=once(s,'              productName: r.product_name,',
   '              productName: r.product_name,\n              drying: Boolean(isDryingChange(r,a >= ws ? types : [],state.aliasCfg)),','dashboard gantt');
  s=once(s,'<change-tags :types="bar.types" :gantt="true" />{{ bar.productName }}',
   '<change-tags :types="bar.types" :gantt="true" /><span v-if="bar.drying" class="rounded px-1 bg-orange-100 text-orange-900 font-bold">♨ 乾燥</span>{{ bar.productName }}','dashboard gantt badge');
  s=once(s,"                      {{ metricsByMachine[m.machine_no].change.row.mold_name || '—' }}",
   "                      {{ metricsByMachine[m.machine_no].change.row.mold_name || '—' }} "+badge('metricsByMachine[m.machine_no].change.row','metricsByMachine[m.machine_no].change.types'),'next mold');
  return s;
 });
 change('change-schedule-app.js',s=>{
  s=once(s,'extractColor, productColorStyle, annotateScheduleChanges, getAliasConfig,',
   'extractColor, productColorStyle, extractDryingMold, isDryingChange, annotateScheduleChanges, getAliasConfig,','schedule import');
  s=once(s,'      productColorStyle,\n      dayPills,',
   '      productColorStyle,\n      extractDryingMold,\n      isDryingChange,\n      dayPills,','schedule setup');
  s=once(s,"          mold_name: r.mold_name || '',",
   "          mold_name: r.mold_name || '',\n          drying: Boolean(isDryingChange(r,item.types,state.aliasCfg)),",'schedule events');

  // Keep original event-only schedule. A drying alias never creates an event.
  s=once(s,'          cur: r.order_no === m.order_no,',
    '          cur: r.order_no === m.order_no,\n          drying: Boolean(isDryingChange(r,types,state.aliasCfg)),',
    'modal gated');
  s=once(s,'                      {{ row.r.mold_name }}',
    '                      {{ row.r.mold_name }} <span v-if="row.drying" class="inline-block px-1.5 rounded border border-orange-400 bg-orange-100 text-orange-900 font-bold">♨ 乾燥</span>',
    'modal event badge');

  const eventTag='<change-tags :types="ev.types" :small="true" />';
  if(!s.includes(eventTag))throw Error('Schedule change tags missing');
  s=s.replaceAll(eventTag,eventTag+'<span v-if="ev.drying" class="rounded px-1 bg-orange-100 border border-orange-400 text-orange-900 text-[11px] font-bold">♨ 乾燥</span>');
  return s;
 });
}
