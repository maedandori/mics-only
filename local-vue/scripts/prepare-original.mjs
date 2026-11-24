// Unpack the original user's Vue 3 HTML/CSS/JS, preserving its layout and components.
// Changes are limited to data endpoints, offline CSS/JS bundling, and local settings paths.
import {readFileSync,mkdirSync,rmSync,writeFileSync} from 'node:fs';
import {resolve,dirname,sep} from 'node:path';
import {brotliDecompressSync} from 'node:zlib';
const root=resolve('original');
const archive=readFileSync('original-source.tar.br.b64','utf8').trim();
const tar=brotliDecompressSync(Buffer.from(archive,'base64'));
rmSync(root,{recursive:true,force:true});mkdirSync(root,{recursive:true});
let offset=0,count=0;
while(offset+512<=tar.length){
 const header=tar.subarray(offset,offset+512);offset+=512;
 if(header.every(b=>b===0))break;
 const name=header.subarray(0,100).toString('utf8').replace(/\0.*$/s,'');
 const size=parseInt(header.subarray(124,136).toString('ascii').replace(/\0.*$/s,'').trim(),8)||0;
 if(!name||name.startsWith('/')||name.includes('..')||name.includes('\\'))throw Error('Unsafe template path: '+name);
 const dest=resolve(root,name);
 if(!dest.startsWith(root+sep))throw Error('Unsafe destination: '+name);
 mkdirSync(dirname(dest),{recursive:true});
 writeFileSync(dest,tar.subarray(offset,offset+size));
 offset+=Math.ceil(size/512)*512;count++;
}
if(count<14)throw Error('Incomplete MICS Vue archive: '+count);
const commonPath=resolve(root,'js/common.js');
let common=readFileSync(commonPath,'utf8');
const start=common.indexOf('export async function loadData() {');
const end=common.indexOf('export async function loadFromFile(file)',start);
if(start<0||end<0)throw Error('Original common.js loader not found');
const localLoader=`// Fast local JSON loader. Browser never requests a production database.
const SS_MACHINES = 'mics.fastMachines.v1';
const SS_SCHEDULE = 'mics.fastSchedule.v1';
let lastMachineText = null, lastMachineData = null;
let lastScheduleText = null, lastScheduleData = {rows:[]};
let lastSchedulesText = undefined, lastSchedules = Object.create(null);
let scheduleTimer = null;
let lastNormalized = null, normalizedMachineText = null, normalizedScheduleText = null;
let schedulePromise = null, scheduleRequestedAt = 0;
try {
  if (typeof sessionStorage !== 'undefined') {
    const cachedM = sessionStorage.getItem(SS_MACHINES);
    if (cachedM) {
      const parsedM = JSON.parse(cachedM);
      if (Array.isArray(parsedM.machines) && parsedM.machines.length) {
        lastMachineText = cachedM;
        lastMachineData = parsedM;
      }
    }
    const cachedS = sessionStorage.getItem(SS_SCHEDULE);
    if (cachedS) {
      const parsedS = JSON.parse(cachedS);
      if (Array.isArray(parsedS.rows)) {
        lastScheduleText = cachedS;
        lastScheduleData = parsedS;
      }
    }
  }
} catch {}
function buildNormalizedSnapshot() {
  if (!lastMachineData) return null;
  if (lastNormalized && normalizedMachineText === lastMachineText &&
      normalizedScheduleText === lastScheduleText) return lastNormalized;
  const machines = lastMachineData.machines.map(m => ({
    ...m,
    qty_ng_total: m.qty_ng,
    qty_ng: m.qty_ng_shift_status === 'mics_today' && Number.isFinite(Number(m.qty_ng_shift))
      ? Number(m.qty_ng_shift) : m.qty_ng,
  }));
  // Expensive grouping/sorting of the 30-day schedule is only needed when
  // schedule-local.json actually changes, not on every machine update.
  if (lastSchedulesText !== lastScheduleText) {
    const byMachine = new Map();
    for (const row of lastScheduleData.rows) {
      const key = Number(row.machine_no);
      if (!byMachine.has(key)) byMachine.set(key, []);
      byMachine.get(key).push(row);
    }
    const windows = [...byMachine].map(([machine_no, rows]) => ({
      machine_no, rows_json: JSON.stringify(rows),
      window_start: lastScheduleData.from || null,
      window_end: lastScheduleData.to || null,
      updated_at: lastScheduleData.sampled_at || null,
    }));
    lastSchedules = normalizeData([], windows).schedules;
    lastSchedulesText = lastScheduleText;
  }
  lastNormalized = {machines: machines.slice().sort(compareMachineNo),
    schedules: lastSchedules, source:'MICS-only direct local JSON',
    loadedAt:new Date().toISOString(), live:true};
  normalizedMachineText = lastMachineText;
  normalizedScheduleText = lastScheduleText;
  return lastNormalized;
}
if (typeof window !== 'undefined') {
  window.__micsCachedData = (requireSchedule = false) => {
    if (!lastMachineData) return null;
    if (requireSchedule && !lastScheduleText) return null;
    return buildNormalizedSnapshot();
  };
}
async function refreshLocalSchedule() {
  if (schedulePromise) return schedulePromise;
  if (Date.now() - scheduleRequestedAt < 5000 && lastScheduleText) return;
  scheduleRequestedAt = Date.now();
  schedulePromise = (async () => {
    const response = await fetch('/data/schedule-local.json', {cache: 'no-store'});
    if (!response.ok) throw new Error('Local schedule JSON HTTP ' + response.status);
    const text = await response.text();
    if (text === lastScheduleText) return;
    const parsed = JSON.parse(text);
    if (!Array.isArray(parsed.rows)) throw new Error('Invalid local schedule JSON rows');
    lastScheduleText = text;
    lastScheduleData = parsed;
    lastNormalized = null;
    try { if (typeof sessionStorage !== 'undefined') sessionStorage.setItem(SS_SCHEDULE, text); } catch {}
    // Update next-change forecasts after initial cards are already visible.
    window.dispatchEvent(new Event('mics-local-schedule-ready'));
  })().catch(error => console.warn('Local schedule cache retained:', error))
    .finally(() => {schedulePromise = null;});
  return schedulePromise;
}
export async function loadData(waitForSchedule = false) {
  const needSchedule = waitForSchedule || location.pathname.endsWith('/change-schedule.html');
  // Start schedule request in parallel when required by schedule/Gantt view.
  const parallelSchedule = needSchedule ? refreshLocalSchedule() : null;
  // Render machine nodes as soon as the small local JSON arrives.
  // Do not wait for the potentially large schedule file on Dashboard.
  try {
    const response = await fetch('/data/machines.json', {cache: 'no-store'});
    if (!response.ok) throw new Error('Local machines JSON HTTP ' + response.status);
    const text = await response.text();
    if (text !== lastMachineText) {
      const parsed = JSON.parse(text);
      if (!Array.isArray(parsed.machines) || !parsed.machines.length)
        throw new Error('Local machines JSON is empty');
      lastMachineText = text;
      lastMachineData = parsed;
      lastNormalized = null;
      try { if (typeof sessionStorage !== 'undefined') sessionStorage.setItem(SS_MACHINES, text); } catch {}
    }
  } catch(error) {
    console.warn('Local machines JSON unavailable, preserving last snapshot:', error);
    if (!lastMachineData) {
      // Show machine placeholders rather than removing every node on initial failure.
      lastMachineData = {machines:[15,8,19,14,5,17,13,6,10,11,18,7,12,16]
        .map(machine_no => ({machine_no, status_name:'データなし'}))};
      lastMachineText = '__placeholder__';
    }
  }
  // Delay large schedule parsing until after the first dashboard paint.
  // A dedicated schedule page still awaits its local JSON immediately.
  if (parallelSchedule) {
    await parallelSchedule;
  } else if (scheduleTimer === null) {
    scheduleTimer = setTimeout(() => {
      scheduleTimer = null;
      void refreshLocalSchedule();
    }, 16);
  }
  // If both JSON files are unchanged, reuse the exact result object and skip
  // expensive normalization and unnecessary native-DOM redraws.
  return buildNormalizedSnapshot();
}

`;
common=common.slice(0,start)+localLoader+common.slice(end);

// Precompile and memoize alias matching so 30-day schedule scans do not
// recreate and sort candidate arrays or recompile RegExps on every row.
const matchStart=common.indexOf('function aliasTokenMatches(text, lowerText, alias) {');
const matchEnd=common.indexOf('export function extractColor(rowOrProd, aliasCfg) {',matchStart);
if(matchStart<0||matchEnd<=matchStart)throw Error('Original matchAliasRule not found');
const fastAliasMatcher=`const compiledRuleCache = new WeakMap();
function getCompiledRules(rules) {
  let entry = compiledRuleCache.get(rules);
  if (entry) return entry;
  const candidates = [];
  for (let i = 0; i < rules.length; i++) {
    const rule = rules[i];
    if (!rule || !rule.canonical) continue;
    for (const a of rule.aliases || []) {
      if (!a) continue;
      const isAsciiWord = /^[A-Za-z]+$/.test(a);
      candidates.push({
        alias: a,
        lowerAlias: a.toLowerCase(),
        re: isAsciiWord ? new RegExp(\`(?:^|[^A-Za-z0-9])\${escapeRegExp(a)}(?=$|[^A-Za-z0-9])\`, 'i') : null,
        canonical: rule.canonical,
        idx: i,
      });
    }
  }
  candidates.sort((x, y) => (y.alias.length - x.alias.length) || (x.idx - y.idx));
  entry = { candidates, hits: new Map() };
  compiledRuleCache.set(rules, entry);
  return entry;
}

export function matchAliasRule(text, rules) {
  const s = String(text || '').trim();
  if (!s || !Array.isArray(rules) || !rules.length) return '';
  const { candidates, hits } = getCompiledRules(rules);
  const cached = hits.get(s);
  if (cached !== undefined) return cached;
  const lower = s.toLowerCase();
  let matched = '';
  for (let i = 0; i < candidates.length; i++) {
    const c = candidates[i];
    if (c.re ? c.re.test(s) : lower.includes(c.lowerAlias)) {
      matched = c.canonical;
      break;
    }
  }
  if (hits.size < 4000) hits.set(s, matched);
  return matched;
}

`;
common=common.slice(0,matchStart)+fastAliasMatcher+common.slice(matchEnd);

// D1 is authoritative: never display an unsynchronized alias edit as saved.
// Keep original layout/UI, only change the persistence behavior.
const saveStart=common.indexOf('export async function saveAliasConfig(cfg) {');
const saveEnd=common.indexOf('export async function resetAliasConfig()',saveStart);
if(saveStart<0||saveEnd<0)throw Error('Original alias save helper not found');
const cloudSave="export async function saveAliasConfig(cfg) {\n  return {savedToFile:false,savedLocal:false,cfg:normalizeAliasConfig(cfg),\n    error:'ローカルは読み取り専用です。Machine-Control Admin で編集し、D1から同期してください'};\n}\n";
common=common.slice(0,saveStart)+cloudSave+common.slice(saveEnd);

common=common.replace("export const ALIASES_JSON_URL = '/api/ui-template/aliases';","export const ALIASES_JSON_URL = '/api/ui-template/aliases';");
common=common.replace("export const STATUS_JSON_URL = '/api/ui-template/status-colors';","export const STATUS_JSON_URL = '/api/ui-template/status-colors';");
common=common.replaceAll('Machine-Control D1 JSON','MICS-only local JSON');

// Local status colors are always read from the agent's data/status-colors.json
// instead of any browser-stored settings or online D1 requests.
const statusStart=common.indexOf('export function getStatusConfig() {');
const statusEnd=common.indexOf('export function statusStyle(',statusStart);
if(statusStart<0||statusEnd<=statusStart)throw Error('Missing original status helpers');
const statusReadOnly=[
"export function getStatusConfig() {",
"  const data=typeof window!=='undefined' ? window.MICS_STATUS_COLORS : null;",
"  return normalizeStatusConfig(data?.codes ? data : DEFAULT_CONFIG);",
"}",
"export async function loadStatusConfig() {",
"  const r=await fetch('/data/status-colors.json',{cache:'no-store'});",
"  if(!r.ok)throw new Error('Local status-colors.json HTTP '+r.status);",
"  const data=await r.json();",
"  if(!data?.default||!data?.codes)throw new Error('Invalid local status-colors.json');",
"  const cfg=normalizeStatusConfig(data);",
"  if(typeof window!=='undefined')window.MICS_STATUS_COLORS=cfg;",
"  return cfg;",
"}",
"export async function saveStatusConfig(cfg) {",
"  return {savedToFile:false,cfg:normalizeStatusConfig(cfg),",
"   error:'読み取り専用です。Machine-Controlで編集してD1から同期してください'};",
"}",
"export async function resetStatusConfig() {",
"  return saveStatusConfig(normalizeStatusConfig(DEFAULT_CONFIG));",
"}",
"",
].join('\n');
common=common.slice(0,statusStart)+statusReadOnly+common.slice(statusEnd);
common=common.replace("export const STATUS_JSON_URL = '/api/ui-template/status-colors';",
 "export const STATUS_JSON_URL = '/data/status-colors.json';");

writeFileSync(commonPath,common);

// Preserve layout but make dashboard cards the default on every new page.
const dashboardPath=resolve(root,'js/dashboard-app.js');
let dashboard=readFileSync(dashboardPath,'utf8');
const replaceDashboard=(from,to)=>{
 const count=dashboard.split(from).length-1;
 if(count!==1)throw Error('Original dashboard patch mismatch: '+from.slice(0,55));
 dashboard=dashboard.replace(from,to);
};
replaceDashboard("data: null,","data: (typeof window !== 'undefined' && typeof window.__micsCachedData === 'function' ? window.__micsCachedData(false) : null),");
replaceDashboard("view: localStorage.getItem('mics.view') || 'grid',","view: 'grid',");
replaceDashboard(`    function setView(v) {
      state.view = v;
      localStorage.setItem('mics.view', v);
    }`,`    function setView(v) {
      state.view = v;
      // Only the schedule view must wait for local schedule JSON.
      if (v === 'gantt') load();
    }`);
replaceDashboard('file ? loadFromFile(file) : loadData(),',"file ? loadFromFile(file) : loadData(state.view === 'gantt'),");
replaceDashboard('日程データ (mics_schedule_week) がありません。',
 '日程データ (data/schedule-local.json) がありません。保存された予定を確認してください。');
// Replace the original blocking loader: node data must not wait for alias/status
// requests, and unchanged JSON must not redraw all fourteen cards.
const loadStart=dashboard.indexOf('    async function load(file = null) {');
const loadEnd=dashboard.indexOf('    function onStorage(e) {',loadStart);
if(loadStart<0||loadEnd<=loadStart)throw Error('Original dashboard load function not found');
const optimizedLoad="    let configLoading = false, lastConfigFingerprint = '';\n    async function refreshConfiguration() {\n      if (configLoading) return;\n      configLoading = true;\n      try {\n        const [cfg, aliasCfg] = await Promise.all([loadStatusConfig(), loadAliasConfig()]);\n        const fingerprint = JSON.stringify([cfg, aliasCfg]);\n        if (fingerprint !== lastConfigFingerprint) {\n          lastConfigFingerprint = fingerprint;\n          state.cfg = cfg;\n          state.aliasCfg = aliasCfg;\n        }\n      } catch (error) {\n        console.warn('Local settings retained:', error);\n      } finally { configLoading = false; }\n    }\n    let loadBusy = false;\n    async function load(file = null) {\n      if (loadBusy) return;\n      loadBusy = true;\n      // Do not hide existing nodes while polling unchanged JSON.\n      if (!state.data) state.sourceText = '読み込み中…';\n      if (state.errorMsg) state.errorMsg = '';\n      // The tiny machine JSON takes priority; settings load in parallel.\n      void refreshConfiguration();\n      try {\n        const data = await (file ? loadFromFile(file) : loadData(state.view === 'gantt'));\n        if (data !== state.data) {\n          state.data = data;\n          state.sourceText = data.source + ' · ' + fmtDT(new Date(data.loadedAt));\n        }\n        clearInterval(refreshTimer);\n        if (data.live) refreshTimer = setInterval(() => load(), 60000);\n        if (file) pushToast(file.name + ' を読み込みました', 'ok');\n      } catch (e) {\n        console.error(e);\n        state.sourceText = '読み込みエラー';\n        state.errorMsg = e.message;\n        pushToast(e.message, 'err');\n      } finally { loadBusy = false; }\n    }\n";
dashboard=dashboard.slice(0,loadStart)+optimizedLoad+dashboard.slice(loadEnd);
const scheduleReadyHandler="    function onLocalScheduleReady() { if (state.view === 'grid' || state.view === 'table') void load(); }\n";
const handlerTarget='    function onStorage(e) {';
if(!dashboard.includes(handlerTarget))throw Error('Dashboard storage handler not found');
dashboard=dashboard.replace(handlerTarget,scheduleReadyHandler+handlerTarget);
dashboard=dashboard.replace("      window.addEventListener('storage', onStorage);",
 "      window.addEventListener('storage', onStorage);\n      window.addEventListener('mics-local-schedule-ready', onLocalScheduleReady);");
dashboard=dashboard.replace("      window.removeEventListener('storage', onStorage);",
 "      window.removeEventListener('storage', onStorage);\n      window.removeEventListener('mics-local-schedule-ready', onLocalScheduleReady);");
// Trim passed/deleted hours in hourly Gantt views (日程) so no empty leading gap appears.
replaceDashboard(`      ws = new Date(ws.getFullYear(), ws.getMonth(), ws.getDate());
      we = new Date(we.getFullYear(), we.getMonth(), we.getDate() + 1);
      const now = new Date();
      const today = new Date(now.getFullYear(), now.getMonth(), now.getDate());
      if (today < we) ws = today;

      const dayWidth = state.ganttDayWidth;
      const scale = ganttScaleInfo(dayWidth);
      const span = we - ws;
      const pos = (d) => ((d - ws) / span) * 100;

      const days = [];
      for (let d = new Date(ws); d < we; d.setDate(d.getDate() + 1)) {
        const copy = new Date(d);
        days.push({
          key: copy.toISOString(),
          label: \`\${copy.getMonth() + 1}/\${copy.getDate()}\`,
          dow: DOW[copy.getDay()],
          isSun: copy.getDay() === 0,
          isSat: copy.getDay() === 6,
        });
      }

      const hourLabels = [];
      if (scale.step < 24) {
        const slots = 24 / scale.step;
        const slotW = dayWidth / slots;
        for (let h = 0; h < 24; h += scale.step) {
          hourLabels.push(slotW >= 36 ? \`\${h}:00\` : slotW >= 24 ? \`\${h}時\` : String(h));
        }
      }

      const trackWidth = days.length * dayWidth;`,
`      ws = new Date(ws.getFullYear(), ws.getMonth(), ws.getDate());
      we = new Date(we.getFullYear(), we.getMonth(), we.getDate() + 1);
      const now = new Date();
      const today = new Date(now.getFullYear(), now.getMonth(), now.getDate());
      let earliestActive = null;
      for (const m of list) {
        const s = state.data.schedules[m.machine_no];
        if (!s) continue;
        for (const r of s.rows) {
          if (!r.mold_no) continue;
          const rs = parseDate(r.start_at);
          const re = parseDate(r.end_at);
          if (!rs || !re || re <= today) continue;
          const visibleStart = rs < now && re > now ? now : rs;
          if (!earliestActive || visibleStart < earliestActive) earliestActive = visibleStart;
        }
      }

      const dayWidth = state.ganttDayWidth;
      const scale = ganttScaleInfo(dayWidth);
      if (today < we) ws = today;
      if (earliestActive) {
        const earliestDay = new Date(earliestActive.getFullYear(), earliestActive.getMonth(), earliestActive.getDate());
        if (earliestDay > ws && earliestDay < we) ws = earliestDay;
      }
      if (scale.step < 24) {
        let anchor = now > ws ? now : ws;
        if (earliestActive && earliestActive > anchor) anchor = earliestActive;
        const slotHour = Math.floor(anchor.getHours() / scale.step) * scale.step;
        const slotStart = new Date(anchor.getFullYear(), anchor.getMonth(), anchor.getDate(), slotHour, 0, 0, 0);
        if (slotStart < we) ws = slotStart;
      }

      const span = we - ws;
      const pos = (d) => ((d - ws) / span) * 100;

      const days = [];
      const slots = scale.step < 24 ? 24 / scale.step : 0;
      const slotW = slots ? dayWidth / slots : dayWidth;
      for (let cursor = new Date(ws); cursor < we; ) {
        const nextMidnight = new Date(cursor.getFullYear(), cursor.getMonth(), cursor.getDate() + 1);
        const segEnd = nextMidnight < we ? nextMidnight : we;
        const startHour = scale.step < 24 ? cursor.getHours() : 0;
        const endHour = segEnd.getDate() !== cursor.getDate() || segEnd.getMonth() !== cursor.getMonth()
          ? 24
          : segEnd.getHours();
        const dayHourLabels = [];
        if (scale.step < 24) {
          for (let h = startHour; h < endHour; h += scale.step) {
            dayHourLabels.push(slotW >= 36 ? \`\${h}:00\` : slotW >= 24 ? \`\${h}時\` : String(h));
          }
        }
        const width = scale.step < 24 ? dayHourLabels.length * slotW : dayWidth;
        days.push({
          key: cursor.toISOString(),
          label: \`\${cursor.getMonth() + 1}/\${cursor.getDate()}\`,
          dow: DOW[cursor.getDay()],
          isSun: cursor.getDay() === 0,
          isSat: cursor.getDay() === 6,
          width,
          hourLabels: dayHourLabels,
        });
        cursor = nextMidnight;
      }

      const hourLabels = days.find((d) => d.hourLabels.length)?.hourLabels || [];
      const trackWidth = days.reduce((sum, d) => sum + d.width, 0);`);
replaceDashboard(`                      <div
                        v-for="d in ganttModel.days"
                        :key="d.key"
                        class="flex flex-col justify-center text-center text-xs font-semibold border-l border-slate-300"
                        :class="{ 'bg-red-500/5': d.isSun, 'bg-blue-500/5': d.isSat }"
                        :style="{ width: ganttModel.dayWidth + 'px', flex: '1 0 ' + ganttModel.dayWidth + 'px' }"
                      >
                        <div class="g-day-title px-1 pt-1 pb-0.5 leading-tight whitespace-nowrap">
                          {{ d.label }}<small class="font-normal text-slate-600 text-[10px] ml-0.5">({{ d.dow }})</small>
                        </div>
                        <div v-if="ganttModel.hourLabels.length" class="flex border-t border-slate-300 text-[10px] font-medium text-slate-600">
                          <span
                            v-for="(hl, idx) in ganttModel.hourLabels"
                            :key="idx"
                            class="flex-1 text-left px-0.5 py-0.5 whitespace-nowrap overflow-hidden"
                            :class="{ 'border-l border-slate-200': idx > 0 }"
                          >{{ hl }}</span>
                        </div>
                      </div>`,
`                      <div
                        v-for="d in ganttModel.days"
                        :key="d.key"
                        class="flex flex-col justify-center text-center text-xs font-semibold border-l border-slate-300"
                        :class="{ 'bg-red-500/5': d.isSun, 'bg-blue-500/5': d.isSat }"
                        :style="{ width: d.width + 'px', flex: d.width + ' 0 ' + d.width + 'px' }"
                      >
                        <div class="g-day-title px-1 pt-1 pb-0.5 leading-tight whitespace-nowrap overflow-hidden text-ellipsis">
                          {{ d.label }}<small v-if="d.width >= 64" class="font-normal text-slate-600 text-[10px] ml-0.5">({{ d.dow }})</small>
                        </div>
                        <div v-if="d.hourLabels.length" class="flex border-t border-slate-300 text-[10px] font-medium text-slate-600">
                          <span
                            v-for="(hl, idx) in d.hourLabels"
                            :key="idx"
                            class="flex-1 text-left px-0.5 py-0.5 whitespace-nowrap overflow-hidden"
                            :class="{ 'border-l border-slate-200': idx > 0 }"
                          >{{ hl }}</span>
                        </div>
                      </div>`);
replaceDashboard(`                      <div
                        v-for="d in ganttModel.days"
                        :key="d.key"
                        class="flex border-l border-slate-300 h-full"
                        :class="{ 'bg-red-500/5': d.isSun, 'bg-blue-500/5': d.isSat }"
                        :style="{ width: ganttModel.dayWidth + 'px', flex: '1 0 ' + ganttModel.dayWidth + 'px' }"
                      >
                        <div
                          v-for="idx in ganttModel.hourLabels.length"
                          :key="idx"
                          class="flex-1 h-full"
                          :class="{ 'border-l border-dashed border-slate-300': idx > 1 }"
                        ></div>
                      </div>`,
`                      <div
                        v-for="d in ganttModel.days"
                        :key="d.key"
                        class="flex border-l border-slate-300 h-full"
                        :class="{ 'bg-red-500/5': d.isSun, 'bg-blue-500/5': d.isSat }"
                        :style="{ width: d.width + 'px', flex: d.width + ' 0 ' + d.width + 'px' }"
                      >
                        <div
                          v-for="idx in d.hourLabels.length"
                          :key="idx"
                          class="flex-1 h-full"
                          :class="{ 'border-l border-dashed border-slate-300': idx > 1 }"
                        ></div>
                      </div>`);
writeFileSync(dashboardPath,dashboard);

const scheduleAppPath=resolve(root,'js/change-schedule-app.js');
let scheduleApp=readFileSync(scheduleAppPath,'utf8');
scheduleApp=scheduleApp.replace("data: null,","data: (typeof window !== 'undefined' && typeof window.__micsCachedData === 'function' ? window.__micsCachedData(true) : null),");
writeFileSync(scheduleAppPath,scheduleApp);

const aliasAppPath=resolve(root,'js/change-aliases-app.js');
let aliasApp=readFileSync(aliasAppPath,'utf8');
aliasApp=aliasApp.replace(
  "if (msg) pushToast(res.savedToFile ? \`\${msg} (change-aliases.json に保存)\` : msg, 'ok');",
  "if (msg) pushToast(res.savedToFile ? \`\${msg} (D1 → ローカル同期)\` : \`${res.savedLocal ? 'ローカルJSON保存済み・' : ''}保存不可: \${res.error || '通信エラー'}\`, res.savedToFile ? 'ok' : 'err');"
);
writeFileSync(aliasAppPath,aliasApp);


// Local status page shows the synced file; editing belongs to online Admin.
const statusPage=resolve(root,'js/status-colors-app.js');
let statusUi=readFileSync(statusPage,'utf8');
statusUi=statusUi.replaceAll('vue/data/status-colors.json','data/status-colors.json');
statusUi=statusUi.replace(
 '変更は <b>data/status-colors.json</b> に自動保存され、全画面に即時反映されます。',
 '表示色は <b>data/status-colors.json</b> から読み込みます。変更は Machine-Controlで保存し、D1 → Local 同期してください。');
statusUi=statusUi.replace('💾 status-colors.json に保存','🔒 読み取り専用');
statusUi=statusUi.replace('@click="saveToFile"','disabled title="D1 → Local 同期専用"');
writeFileSync(statusPage,statusUi);


for(const page of ['index.html','change-schedule.html','status-colors.html','change-aliases.html']){
 const file=resolve(root,page);let html=readFileSync(file,'utf8');
 html=html.replace(/<script src="https:\/\/cdn\.tailwindcss\.com"><\/script>\s*/g,'');
 html=html.replace(/<script type="importmap">[\s\S]*?<\/script>\s*/g,'');
 html=html.replace(/<script src="data\/data\.js"><\/script>\s*/g,'');
 html=html.replace(/<script src="data\/(?:status-colors|change-aliases)\.js"><\/script>\s*/g,'');
 html=html.replace('<link rel="stylesheet" href="css/tailwind-custom.css" />','<link rel="stylesheet" href="css/tailwind-generated.css" />\n  <link rel="stylesheet" href="css/tailwind-custom.css" />');
 writeFileSync(file,html);
}
writeFileSync(resolve(root,'tailwind-input.css'),'@tailwind base;\n@tailwind components;\n@tailwind utilities;\n');
// Reuse the same 乾燥 mold aliases and tags as Machine-Control online.
const {patchDryingTemplate}=await import('./patch-drying.mjs');
patchDryingTemplate(root);
// Keep 刻印替え only for same-mold stamp changes and use 刻印確認 for mold changes.
const {patchStampChanges}=await import('./patch-stamp-change.mjs');
patchStampChanges(root);
// Status animations are read-only locally; edits remain on Cloudflare D1.
const {patchStatusAnimations}=await import('./patch-status-animation.mjs');
patchStatusAnimations(root,{readOnly:true});
// Agent-owned SSE stream notifies local UI immediately when JSON changes.
const {patchLocalJsonLive}=await import('./patch-local-json-live.mjs');
patchLocalJsonLive(root);
// Show event-only 乾燥 and 刻印 tags in machine nodes and 区分 columns.
const {patchEventTags}=await import('./patch-event-tags.mjs');
patchEventTags(root);
// Mirror online next-change type/time and product code on local machine cards.
const {patchNextChange}=await import('./patch-local-next-change.mjs');
patchNextChange(root);
// Expand local weekly change schedule from Mon-Sat (6d) to Mon-Sun (7d).
const {patchWeekly7Days}=await import('./patch-weekly-7days.mjs');
patchWeekly7Days(root);
console.log('Prepared '+count+' original MICS Vue assets for local JSON and offline bundling');
