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
  s=once(s,'    function collectMachineEvents(m) {',
`    const DRYING_RESIN_RULES = [
      ['ABS', /(^|[^A-Z0-9])ABS(?=$|[^A-Z0-9])/],
      ['PS-TH21', /(^|[^A-Z0-9])PS-TH21(?=$|[^A-Z0-9])/],
      ['PC', /(^|[^A-Z0-9])PC(?=$|[^A-Z0-9])/],
      ['ナイロン', /ナイロン|(^|[^A-Z0-9])NYLON(?=$|[^A-Z0-9])/],
      ['AS', /(^|[^A-Z0-9])AS(?=$|[^A-Z0-9])/],
    ];
    function detectDryingResins(r) {
      const raw = [
        ...(Array.isArray(r?.material_names) ? r.material_names : []),
        ...(Array.isArray(r?.material_details) ? r.material_details.map((x) => x?.name) : []),
      ].filter((x) => typeof x === 'string' && x.trim().length);
      const out = [];
      for (const name of raw) {
        const norm = name.normalize('NFKC').toUpperCase().replace(/[‐‑‒–—−]/g, '-');
        for (const [label, re] of DRYING_RESIN_RULES) {
          if (re.test(norm) && !out.includes(label)) out.push(label);
        }
      }
      return out;
    }

    function collectMachineEvents(m) {`,'drying resin helper');
  s=once(s,'          drying: Boolean(isDryingChange(r,item.types,state.aliasCfg)),',
`          drying: Boolean(isDryingChange(r,item.types,state.aliasCfg)),
          dryingResins: detectDryingResins(r),
          materialNames: (Array.isArray(r.material_names) && r.material_names.length ? r.material_names : (Array.isArray(r.material_details) ? r.material_details.map((x) => x?.name).filter(Boolean) : [])),
          dryingMoldLabel: extractDryingMold(r, state.aliasCfg) || '',`,'event drying details');
  s=once(s,
"      const shiftNos = state.shiftSubTab === 'all' ? [1, 2] : [state.shiftSubTab === '2' ? 2 : 1];",
"      const shiftNos = state.shiftSubTab === 'drying' ? [] : (state.shiftSubTab === 'all' ? [1, 2] : [state.shiftSubTab === '2' ? 2 : 1]);",
'drying shiftNos');
  s=once(s,
`      return {
        title: \`⏱ 当日・時間順 (2直) — \${fmtDayTitle(selectedDate.value)}\`,
        subtitle: \`稼働時間帯: \${fmtShortDay(selectedDate.value)} 07:30 〜 \${fmtShortDay(nextDate.value)} 07:30 (全14台 / 計 \${dayEvents.length}件)\`,
        s1Count,
        s2Count,
        totalCount: dayEvents.length,
        groups,
      };`,
`      const dryingEvents = dayEvents.filter((ev) => ev.drying || (ev.dryingResins && ev.dryingResins.length > 0)).sort(byTime);
      const dryingMachineNos = [...new Set(dryingEvents.map((ev) => ev.machine_no))].sort(compareMachineNo);
      const dryingS1Count = dryingEvents.filter((ev) => ev.shiftInfo.shift === 1).length;
      const dryingS2Count = dryingEvents.filter((ev) => ev.shiftInfo.shift === 2).length;

      return {
        title: \`⏱ 当日・時間順 (2直) — \${fmtDayTitle(selectedDate.value)}\`,
        subtitle: \`稼働時間帯: \${fmtShortDay(selectedDate.value)} 07:30 〜 \${fmtShortDay(nextDate.value)} 07:30 (全14台 / 計 \${dayEvents.length}件)\`,
        s1Count,
        s2Count,
        totalCount: dayEvents.length,
        dryingCount: dryingEvents.length,
        dryingS1Count,
        dryingS2Count,
        dryingMachineNos,
        dryingEvents,
        groups,
      };`,'today drying model');
  s=once(s,
`                <button
                  type="button"
                  class="px-3 py-1.5 text-sm border-l border-slate-300 cursor-pointer transition-colors"
                  :class="state.shiftSubTab === 'all' ? 'bg-blue-600 text-white font-semibold' : 'hover:bg-slate-50'"
                  @click="setShiftSub('all')"
                >🔄 1直・2直 両方 <small class="opacity-85 ml-0.5 font-medium">({{ todayChronologicalModel.totalCount }}件)</small></button>
              </div>
            </div>`,
`                <button
                  type="button"
                  class="px-3 py-1.5 text-sm border-l border-slate-300 cursor-pointer transition-colors"
                  :class="state.shiftSubTab === 'all' ? 'bg-blue-600 text-white font-semibold' : 'hover:bg-slate-50'"
                  @click="setShiftSub('all')"
                >🔄 1直・2直 両方 <small class="opacity-85 ml-0.5 font-medium">({{ todayChronologicalModel.totalCount }}件)</small></button>
                <button
                  id="shift-sub-drying"
                  type="button"
                  class="px-3.5 py-1.5 text-sm border-l border-slate-300 cursor-pointer transition-all inline-flex items-center gap-1.5"
                  :class="state.shiftSubTab === 'drying' ? 'bg-orange-600 text-white font-extrabold' : (todayChronologicalModel.dryingCount > 0 ? 'bg-amber-50 hover:bg-amber-100 text-orange-950 font-extrabold' : 'hover:bg-slate-50 text-slate-600')"
                  @click="setShiftSub('drying')"
                >
                  <span>♨ 原材料乾燥</span>
                  <span
                    v-if="todayChronologicalModel.dryingCount > 0"
                    class="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-xs font-black tracking-wide shadow-sm animate-pulse"
                    :class="state.shiftSubTab === 'drying' ? 'bg-white text-orange-700 ring-2 ring-amber-200' : 'bg-red-600 text-white ring-2 ring-amber-300'"
                  >⚠️ {{ todayChronologicalModel.dryingCount }}件 ({{ todayChronologicalModel.dryingMachineNos.length }}台)</span>
                  <small v-else class="opacity-80 ml-0.5 font-medium">(0件)</small>
                </button>
              </div>
              <button
                v-if="todayChronologicalModel.dryingCount > 0 && state.shiftSubTab !== 'drying'"
                type="button"
                class="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg border-2 border-orange-400 bg-orange-50 hover:bg-orange-100 text-orange-950 text-xs font-extrabold cursor-pointer shadow-sm"
                @click="setShiftSub('drying')"
              >
                <span class="px-1.5 py-0.5 rounded bg-red-600 text-white font-black">要乾燥 {{ todayChronologicalModel.dryingCount }}件</span>
                <span>対象号機: {{ todayChronologicalModel.dryingMachineNos.map((n) => n + '号機').join('・') }}</span>
                <span class="underline text-orange-800 ml-1">一覧を見る →</span>
              </button>
            </div>

            <div v-if="state.shiftSubTab === 'drying'" id="drying-schedule-panel" class="mb-4">
              <div
                v-if="todayChronologicalModel.dryingCount > 0"
                class="mb-3 p-3.5 rounded-xl border-2 border-orange-400 bg-gradient-to-r from-orange-50 via-amber-50 to-orange-50 shadow-sm"
              >
                <div class="flex flex-wrap items-center justify-between gap-2 mb-2">
                  <div class="flex items-center gap-2 flex-wrap">
                    <span class="px-2.5 py-1 rounded-full bg-red-600 text-white text-xs font-black tracking-wide shadow-sm animate-pulse">
                      ⚠️ 乾燥必須 {{ todayChronologicalModel.dryingCount }}件 ({{ todayChronologicalModel.dryingMachineNos.length }}台)
                    </span>
                    <b class="text-sm text-orange-950">本日の原材料乾燥・予備乾燥 対象一覧 (1直 {{ todayChronologicalModel.dryingS1Count }}件 / 2直 {{ todayChronologicalModel.dryingS2Count }}件)</b>
                  </div>
                  <span class="text-xs font-bold text-orange-900">※ 事前乾燥の入れ忘れに注意してください</span>
                </div>
                <div class="flex flex-wrap gap-1.5">
                  <button
                    v-for="ev in todayChronologicalModel.dryingEvents"
                    :key="'pill-' + ev.key"
                    type="button"
                    class="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-lg border border-orange-400 bg-white hover:bg-orange-100 text-slate-900 text-xs font-bold cursor-pointer shadow-xs"
                    @click="openMachineModal(ev.machine_no)"
                  >
                    <span class="px-1.5 py-0.5 rounded bg-orange-600 text-white font-black">{{ ev.machine_no }}号機</span>
                    <span class="font-mono text-slate-700">{{ ev.shiftInfo.shift }}直 {{ ev.timeLabel }}</span>
                    <span v-if="ev.dryingResins && ev.dryingResins.length" class="px-1.5 py-0.5 rounded bg-amber-100 text-amber-950 border border-amber-300">{{ ev.dryingResins.join(' / ') }}</span>
                    <span v-else class="text-orange-900">{{ ev.mold_name || ev.mold_no }}</span>
                  </button>
                </div>
              </div>

              <div class="bg-white border border-orange-300 rounded-xl overflow-hidden shadow-sm">
                <div class="flex items-center gap-2 px-3 py-2.5 bg-orange-100/80 border-b border-orange-300 text-sm">
                  <span class="chg drying text-[11px] px-1.5 py-0.5">♨ 乾燥</span>
                  <b class="text-orange-950">原材料乾燥 対象一覧 (1直・2直 時間順)</b>
                  <span
                    class="ml-auto text-xs font-black px-2.5 py-0.5 rounded-full"
                    :class="todayChronologicalModel.dryingCount > 0 ? 'bg-red-600 text-white shadow-xs' : 'bg-white border border-slate-300 text-slate-600'"
                  >{{ todayChronologicalModel.dryingCount }}件</span>
                </div>
                <div v-if="todayChronologicalModel.dryingEvents.length" class="overflow-auto">
                  <table class="w-full border-collapse text-[13px]">
                    <thead>
                      <tr class="bg-orange-50/70 text-xs text-slate-700 font-semibold">
                        <th class="px-2 py-1.5 border-b border-orange-200 text-left">直 / 時刻</th>
                        <th class="px-2 py-1.5 border-b border-orange-200 text-left">号機</th>
                        <th class="px-2 py-1.5 border-b border-orange-200 text-left">区分</th>
                        <th class="px-2 py-1.5 border-b border-orange-200 text-left">乾燥材質・対象</th>
                        <th class="px-2 py-1.5 border-b border-orange-200 text-left">金型 (変更前 → 変更後)</th>
                        <th class="px-2 py-1.5 border-b border-orange-200 text-left">色・刻印 / 品名</th>
                        <th class="px-2 py-1.5 border-b border-orange-200 text-left">受注番号</th>
                      </tr>
                    </thead>
                    <tbody>
                      <tr
                        v-for="ev in todayChronologicalModel.dryingEvents"
                        :key="'dry-' + ev.key"
                        class="border-b border-slate-200 cursor-pointer hover:bg-orange-50/60"
                        @click="openMachineModal(ev.machine_no)"
                      >
                        <td class="px-2 py-1.5 font-mono whitespace-nowrap">
                          <span class="text-[11px] font-bold px-1.5 py-0.5 rounded mr-1" :class="ev.shiftInfo.shift === 1 ? 'bg-amber-100 text-amber-900' : 'bg-indigo-100 text-indigo-900'">{{ ev.shiftInfo.shift }}直</span>
                          <span class="font-extrabold text-xs px-1.5 rounded bg-slate-200 text-slate-900">{{ ev.timeLabel }}</span>
                        </td>
                        <td class="px-2 py-1.5 whitespace-nowrap">
                          <span class="inline-flex items-center gap-1.5 px-2 py-0.5 rounded-lg border border-orange-400 bg-orange-50 text-orange-950 text-[12.5px] whitespace-nowrap">
                            <b>{{ ev.machine_no }}号機</b>
                          </span>
                        </td>
                        <td class="px-2 py-1.5 whitespace-nowrap"><change-tags :types="ev.displayTypes" :small="true" /></td>
                        <td class="px-2 py-1.5">
                          <div v-if="ev.dryingResins && ev.dryingResins.length" class="flex flex-wrap gap-1">
                            <span v-for="res in ev.dryingResins" :key="res" class="px-2 py-0.5 rounded-full bg-red-600 text-white text-[11px] font-black">♨ {{ res }}</span>
                          </div>
                          <div v-if="ev.materialNames && ev.materialNames.length" class="text-xs text-slate-700 font-semibold mt-0.5">{{ ev.materialNames.join(' / ') }}</div>
                          <div v-else-if="ev.dryingMoldLabel" class="text-xs text-orange-900 font-bold">乾燥登録金型: {{ ev.dryingMoldLabel }}</div>
                        </td>
                        <td class="px-2 py-1.5 min-w-[180px]">
                          <template v-if="ev.types.includes('型替え')">
                            <span class="text-slate-600">{{ ev.prevMoldName || ev.prevMold || '—' }}</span>
                            <span class="text-slate-500 mx-0.5">→</span>
                            <b>{{ ev.mold_name || ev.mold_no }}</b>
                          </template>
                          <b v-else>{{ ev.mold_name || ev.mold_no }}</b>
                          <div class="text-xs text-slate-600">{{ ev.mold_no }}</div>
                        </td>
                        <td class="px-2 py-1.5">
                          <div v-if="ev.types.includes('色替え')">
                            <span class="text-slate-600">{{ ev.prevColor || '—' }}</span>
                            <span class="text-slate-500 mx-0.5">→</span>
                            <b class="text-amber-900 bg-amber-100 px-1.5 rounded border border-amber-300">{{ ev.curColor || '—' }}</b>
                          </div>
                          <span v-else class="px-2 py-0.5 rounded-full text-[11.5px] text-slate-900 bg-white border border-slate-900/20">{{ ev.curColor || '—' }}</span>
                          <div class="text-xs text-slate-600 mt-0.5">{{ ev.product_name }}</div>
                        </td>
                        <td class="px-2 py-1.5 font-mono text-xs text-slate-600 whitespace-nowrap">{{ ev.order_no }}</td>
                      </tr>
                    </tbody>
                  </table>
                </div>
                <div v-else class="py-8 px-4 text-center text-slate-600 text-[13.5px]">
                  本日の型替え・色替え予定に原材料乾燥が必要な機械はありません (0件)。
                </div>
              </div>
            </div>`,'drying subtab ui');
  return s;
 });
 const cssPath=resolve(root,'css/tailwind-custom.css');
 let css=readFileSync(cssPath,'utf8');
 if(css.includes('.chg.drying'))throw Error('CSS drying tag already exists');
 css+='\n/* Drying tag for actual change events only. */\n.chg.drying, .g-chg.drying { background:#fff7ed;color:#9a3412;border:1px solid #fb923c;font-weight:700;white-space:nowrap; }\n';
 writeFileSync(cssPath,css);
}
