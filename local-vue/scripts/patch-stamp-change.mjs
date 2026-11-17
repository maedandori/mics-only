// Shared stamp transition rules and a separate 刻印確認 tag for mold switches.
// Uses the original template's JavaScript components, layout and CSS.
import {readFileSync,writeFileSync} from 'node:fs';
import {resolve} from 'node:path';

export function patchStampChanges(root){
 const once=(text,from,to,label)=>{
  const count=text.split(from).length-1;
  if(count!==1)throw Error('Stamp patch anchor '+label+': '+count);
  return text.replace(from,to);
 };
 const edit=(path,fn)=>{
  const full=resolve(root,path),before=readFileSync(full,'utf8'),after=fn(before);
  if(before===after)throw Error('No stamp patch changes in '+path);
  writeFileSync(full,after);
 };
 edit('js/common.js',s=>{
  const old=[
   "  if (nextRow.mold_no !== prevMold || (prevMoldAlias && nextMoldAlias && nextMoldAlias !== prevMoldAlias)) {",
   "    types.push('型替え');",
   "  }",
   "  const nextColor = extractColor(nextRow, cfg);",
   "  if (nextColor && prevColor && nextColor !== prevColor) types.push('色替え');",
   "  const nextStamp = extractStamp(nextRow, cfg);",
   "  if ((nextStamp || prevStamp) && nextStamp !== prevStamp) types.push('刻印替え');"
  ].join('\n');
  const updated=[
   "  const moldChanged = nextRow.mold_no !== prevMold ||",
   "    (prevMoldAlias && nextMoldAlias && nextMoldAlias !== prevMoldAlias);",
   "  if (moldChanged) types.push('型替え');",
   "  const nextColor = extractColor(nextRow, cfg);",
   "  if (nextColor && prevColor && nextColor !== prevColor) types.push('色替え');",
   "  const nextStamp = extractStamp(nextRow, cfg);",
   "  // 刻印替え: same mold but different stamp, including stamp install/removal.",
   "  // 刻印確認: mold changed and the NEXT product has a recognized 刻印 alias.",
   "  // With no stamps, a normal mold change must not generate a stamp tag.",
   "  if (moldChanged) {",
   "    if (nextStamp) types.push('刻印確認');",
   "  } else if ((nextStamp || prevStamp) && nextStamp !== prevStamp) {",
   "    types.push('刻印替え');",
   "  }"
  ].join('\n');
  return once(s,old,updated,'detectChangeTypes');
 });
 edit('js/components.js',s=>once(s,
  "t === '型替え' ? 'mold' : (t === '刻印替え' ? 'stamp' : 'color'),",
  "t === '型替え' ? 'mold' : (t === '刻印替え' ? 'stamp' : (t === '刻印確認' ? 'stamp-check' : 'color')),",
  'ChangeTags color class'));
 edit('css/tailwind-custom.css',s=>{
  const style=[
   ".chg.stamp-check, .g-chg.stamp-check {",
   "  background: #0f766e;",
   "  color: #ffffff;",
   "  border: 1px solid #5eead4;",
   "}",
   ""
  ].join('\n');
  if(s.includes('.chg.stamp-check'))throw Error('stamp-check style already added');
  return s+'\n'+style;
 });
 edit('js/change-schedule-app.js',s=>{
  s=once(s,
   "if (state.typeFilter === '色替え') return !ev.types.includes('型替え') && !ev.types.includes('刻印替え') && ev.types.includes('色替え');",
   "if (state.typeFilter === '色替え') return !ev.types.includes('型替え') && !ev.types.includes('刻印替え') && !ev.types.includes('刻印確認') && ev.types.includes('色替え');",
   'color-only filter');
  const changed=s.replaceAll(
   "ev.types.includes('型替え') || ev.types.includes('刻印替え')",
   "ev.types.includes('型替え') || ev.types.includes('刻印替え') || ev.types.includes('刻印確認')");
  if(changed===s)throw Error('mold grouping not found');
  s=changed.replaceAll(
   "!ev.types.includes('型替え') && !ev.types.includes('刻印替え')",
   "!ev.types.includes('型替え') && !ev.types.includes('刻印替え') && !ev.types.includes('刻印確認')");
  // Avoid a duplicate !刻印確認 in the color-only filter.
  s=s.replaceAll("!ev.types.includes('刻印確認') && !ev.types.includes('刻印確認')","!ev.types.includes('刻印確認')");
  const buttonEnd="              >刻印替え</button>";
  const newButton=[
   buttonEnd,
   '              <button',
   '                type="button"',
   '                class="px-3 py-1.5 text-sm border-l border-slate-300 cursor-pointer transition-colors"',
   `                :class="state.typeFilter === '刻印確認' ? 'bg-teal-700 text-white font-semibold' : 'hover:bg-slate-50'"`,
   `                @click="state.typeFilter = '刻印確認'"`,
   '              >刻印確認</button>'
  ].join('\n');
  s=once(s,buttonEnd,newButton,'new stamp confirmation filter');
  s=s.replaceAll("型替え・刻印替え","型替え・刻印替え・刻印確認");
  const header='<span class="chg stamp text-[11px] px-1.5 py-0.5">刻印替え</span>';
  s=once(s,header,header+'\n                    <span class="chg stamp-check text-[11px] px-1.5 py-0.5">刻印確認</span>','table header tag');
  return s;
 });
}
