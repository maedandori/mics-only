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
let lastMachineText = null, lastMachineData = null;
let lastScheduleText = null, lastScheduleData = {rows:[]};
let lastNormalized = null, normalizedMachineText = null, normalizedScheduleText = null;
let schedulePromise = null, scheduleRequestedAt = 0;
async function refreshLocalSchedule() {
  if (schedulePromise) return schedulePromise;
  if (Date.now() - scheduleRequestedAt < 5000) return;
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
  })().catch(error => console.warn('Local schedule cache retained:', error))
    .finally(() => {schedulePromise = null;});
  return schedulePromise;
}
export async function loadData() {
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
  const loadingSchedule = refreshLocalSchedule();
  // The schedule page needs complete rows before its first render.
  if (location.pathname.endsWith('/change-schedule.html')) await loadingSchedule;
  // If both JSON files are unchanged, reuse the exact result object and skip
  // expensive normalization and unnecessary native-DOM redraws.
  if (lastNormalized && normalizedMachineText === lastMachineText &&
      normalizedScheduleText === lastScheduleText) return lastNormalized;
  const machines = lastMachineData.machines.map(m => ({
    ...m,
    qty_ng_total: m.qty_ng,
    qty_ng: m.qty_ng_shift_status === 'mics_today' && Number.isFinite(Number(m.qty_ng_shift))
      ? Number(m.qty_ng_shift) : m.qty_ng,
  }));
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
  lastNormalized = {...normalizeData(machines, windows), source:'MICS-only direct local JSON',
    loadedAt:new Date().toISOString(),live:true};
  normalizedMachineText = lastMachineText;
  normalizedScheduleText = lastScheduleText;
  return lastNormalized;
}

`;
common=common.slice(0,start)+localLoader+common.slice(end);

// D1 is authoritative: never display an unsynchronized alias edit as saved.
// Keep original layout/UI, only change the persistence behavior.
const saveStart=common.indexOf('export async function saveAliasConfig(cfg) {');
const saveEnd=common.indexOf('export async function resetAliasConfig()',saveStart);
if(saveStart<0||saveEnd<0)throw Error('Original alias save helper not found');
const cloudSave=`export async function saveAliasConfig(cfg) {
  const normalized=normalizeAliasConfig(cfg);
  let response;
  try {
    response=await fetch(ALIASES_JSON_URL,{
      method:'PUT', headers:{'Content-Type':'application/json'},
      body:JSON.stringify(normalized),
    });
    if(!response.ok)throw new Error('Cloudflare D1 sync HTTP '+response.status);
    if(typeof globalThis.window!=='undefined')globalThis.window.MICS_CHANGE_ALIASES=normalized;
    const ls=getStorage();
    try{if(ls)ls.setItem(LS_ALIASES,JSON.stringify(normalized))}catch{}
    return {savedToFile:true,path:'Cloudflare D1 (all machines)',cfg:normalized};
  } catch(e){
    console.warn('D1 alias sync failed',e);
    return {savedToFile:false,path:'Cloudflare D1',error:e.message,cfg:normalized};
  }
}

`;
common=common.slice(0,saveStart)+cloudSave+common.slice(saveEnd);

common=common.replace("export const ALIASES_JSON_URL = '/api/ui-template/aliases';","export const ALIASES_JSON_URL = '/api/ui-template/aliases';");
common=common.replace("export const STATUS_JSON_URL = '/api/ui-template/status-colors';","export const STATUS_JSON_URL = '/api/ui-template/status-colors';");
common=common.replaceAll('Machine-Control D1 JSON','MICS-only local JSON');
writeFileSync(commonPath,common);

const aliasAppPath=resolve(root,'js/change-aliases-app.js');
let aliasApp=readFileSync(aliasAppPath,'utf8');
aliasApp=aliasApp.replace(
  "if (msg) pushToast(res.savedToFile ? \`\${msg} (change-aliases.json に保存)\` : msg, 'ok');",
  "if (msg) pushToast(res.savedToFile ? \`\${msg} (Cloudflare D1 全端末に同期)\` : \`D1同期失敗: \${res.error || res.status || '通信エラー'}\`, res.savedToFile ? 'ok' : 'error');"
);
writeFileSync(aliasAppPath,aliasApp);

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
console.log('Prepared '+count+' original MICS Vue assets for local JSON and offline bundling');
