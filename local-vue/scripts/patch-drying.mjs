// Add a fourth alias category, 乾燥, by mold code/name to the original UI.
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
 const badge=(expr)=>'<span v-if="extractDryingMold('+expr+',state.aliasCfg)" class="inline-block rounded-full border border-orange-400 bg-orange-100 text-orange-900 font-bold text-[11px] px-2 py-0.5 whitespace-nowrap">♨ 乾燥</span>';
 change('common.js',s=>{
  s=once(s,
   '    stamp: Array.isArray(src.stamp) ? normalizeRuleList(src.stamp) : normalizeRuleList(DEFAULT_CHANGE_ALIASES.stamp),',
   '    stamp: Array.isArray(src.stamp) ? normalizeRuleList(src.stamp) : normalizeRuleList(DEFAULT_CHANGE_ALIASES.stamp),\n    drying: Array.isArray(src.drying) ? normalizeRuleList(src.drying) : [],',
   'normalize');
  const fn=[
  "// A drying requirement follows the mold even without a mold/color change.",
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
   '          <p v-if="state.cat === \'drying\'" class="text-xs text-orange-900 bg-orange-50 border border-orange-200 rounded-lg p-2 mb-3">♨ 乾燥: 判定名に金型名または金型番号を入力。認識キーワードに同じ金型の別名や番号を追加します。品名では判定せず、対象金型には常に乾燥タグを表示します。</p>\n          <!-- Rules Table -->','alias hint');
  s=once(s,'@click="quickFillRule(r.product_name)"',
   '@click="quickFillRule(state.cat===\'drying\' ? (r.mold_no || r.mold_name) : r.product_name)"','quickfill');
  s=once(s,"                  <td class=\"px-2 py-1.5 min-w-[140px] text-xs text-slate-600\">{{ r.mold_name || '—' }}</td>",
   "                  <td class=\"px-2 py-1.5 min-w-[140px] text-xs text-slate-600\">{{ r.mold_name || '—' }} <span v-if=\"r.drying\" class=\"px-1.5 rounded bg-orange-100 border border-orange-400 text-orange-900 font-bold\">♨ 乾燥</span></td>",'preview badge');
  return s;
 });
 change('dashboard-app.js',s=>{
  s=once(s,'extractColor, extractStamp, extractMoldAlias, productColorStyle,',
   'extractColor, extractStamp, extractMoldAlias, extractDryingMold, productColorStyle,','dashboard import');
  s=once(s,'      productColorStyle,\n      setView,',
   '      productColorStyle,\n      extractDryingMold,\n      setView,','dashboard setup');
  s=once(s,'                  <div class="prod-title text-[13px]',
   '                  '+badge('m')+'\n                  <div class="prod-title text-[13px]','dashboard card');
  s=once(s,'                    <div class="text-xs text-slate-600">{{ m.mold_name }}</div>',
   '                    <div class="text-xs text-slate-600">{{ m.mold_name }} '+badge('m')+'</div>','dashboard table');
  s=once(s,'              productName: r.product_name,',
   '              productName: r.product_name,\n              drying: Boolean(extractDryingMold(r,state.aliasCfg)),','dashboard gantt');
  s=once(s,'<change-tags :types="bar.types" :gantt="true" />{{ bar.productName }}',
   '<change-tags :types="bar.types" :gantt="true" /><span v-if="bar.drying" class="rounded px-1 bg-orange-100 text-orange-900 font-bold">♨ 乾燥</span>{{ bar.productName }}','dashboard gantt badge');
  s=once(s,"                      {{ metricsByMachine[m.machine_no].change.row.mold_name || '—' }}",
   "                      {{ metricsByMachine[m.machine_no].change.row.mold_name || '—' }} "+badge('metricsByMachine[m.machine_no].change.row'),'next mold');
  return s;
 });
 change('change-schedule-app.js',s=>{
  s=once(s,'extractColor, productColorStyle, annotateScheduleChanges, getAliasConfig,',
   'extractColor, productColorStyle, extractDryingMold, annotateScheduleChanges, getAliasConfig,','schedule import');
  s=once(s,'      productColorStyle,\n      dayPills,',
   '      productColorStyle,\n      extractDryingMold,\n      dayPills,','schedule setup');
  s=once(s,"          mold_name: r.mold_name || '',",
   "          mold_name: r.mold_name || '',\n          drying: Boolean(extractDryingMold(r,state.aliasCfg)),",'schedule events');
  const eventTag='<change-tags :types="ev.types" :small="true" />';
  if(!s.includes(eventTag))throw Error('Schedule change tags missing');
  s=s.replaceAll(eventTag,eventTag+'<span v-if="ev.drying" class="rounded px-1 bg-orange-100 border border-orange-400 text-orange-900 text-[11px] font-bold">♨ 乾燥</span>');
  const header='<div class="font-bold text-[12.5px] truncate mb-0.5" :title="row.m.mold_name">{{ row.m.mold_name || \'—\' }}</div>';
  if(!s.includes(header))throw Error('Schedule current mold missing');
  return s.replaceAll(header,header+'\n                        '+badge('row.m'));
 });
}
