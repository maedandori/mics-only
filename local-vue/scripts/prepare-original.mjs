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
const localLoader=`export async function loadData() {
  // Each endpoint reads EXE_FOLDER/data/*.json through mics-only.exe.
  // Missing schedules must never blank the machine dashboard.
  const [machineResult, scheduleResult] = await Promise.allSettled([
    fetch('/api/machines', {cache: 'no-store'}),
    fetch('/api/schedule', {cache: 'no-store'}),
  ]);
  if(machineResult.status !== 'fulfilled' || !machineResult.value.ok) {
    throw new Error('Local machine JSON not available');
  }
  const machinesData = await machineResult.value.json();
  let scheduleData = {rows:[]};
  if(scheduleResult.status === 'fulfilled' && scheduleResult.value.ok) {
    try { scheduleData = await scheduleResult.value.json(); }
    catch(error) { console.warn('Invalid local schedule JSON',error); }
  }
  const machines = Array.isArray(machinesData.machines) ? machinesData.machines.map(m => ({
    ...m,
    qty_ng_total: m.qty_ng,
    qty_ng: m.qty_ng_shift_status === 'mics_today' && Number.isFinite(Number(m.qty_ng_shift))
      ? Number(m.qty_ng_shift) : m.qty_ng,
  })) : [];
  const byMachine = new Map();
  for(const row of (scheduleData.rows || [])){
    const key = Number(row.machine_no);
    if(!byMachine.has(key))byMachine.set(key,[]);
    byMachine.get(key).push(row);
  }
  const windows = [...byMachine].map(([machine_no,rows])=>({
    machine_no, rows_json: JSON.stringify(rows),
    window_start: scheduleData.from || null,
    window_end: scheduleData.to || null,
    updated_at: scheduleData.sampled_at || null,
  }));
  return {...normalizeData(machines,windows),source:'MICS-only local JSON',
    loadedAt:new Date().toISOString(),live:true};
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
