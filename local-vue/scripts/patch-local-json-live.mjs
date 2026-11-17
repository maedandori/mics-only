// One server-sent event connection handles all local machine/schedule JSON changes.
// Retain a 60s local-only polling fallback if the agent is unavailable.
import {readFileSync,writeFileSync} from 'node:fs';
import {resolve} from 'node:path';

export function patchLocalJsonLive(root){
 const once=(s,a,b,tag)=>{
  const n=s.split(a).length-1;
  if(n!==1)throw Error('Local live patch '+tag+': '+n);
  return s.replace(a,b);
 };
 const update=(path,fn)=>{
  const full=resolve(root,path),before=readFileSync(full,'utf8'),after=fn(before);
  if(after===before)throw Error('Local live patch did not alter '+path);
  writeFileSync(full,after);
 };
 update('js/common.js',s=>once(s,
  'async function refreshLocalSchedule() {',
  'export async function refreshLocalSchedule() {',
  'export schedule refresher'));
 for(const [file,isDashboard] of [['dashboard-app.js',true],['change-schedule-app.js',false]]){
  update('js/'+file,s=>{
   if(isDashboard){
    s=once(s,'loadData, loadFromFile,','loadData, loadFromFile, refreshLocalSchedule,','dashboard import schedule refresh');
   }
   s=once(s,'    let refreshTimer = null;',
    '    let refreshTimer = null;\n    let liveJsonStream = null;\n    let liveJsonConnected = false;',
    'SSE connection state');
   // The polling interval remains a fallback only, while the SSE is unavailable.
   s=once(s,'if (data.live) refreshTimer = setInterval(() => load(), 60000);',
    'if (data.live && !liveJsonConnected) refreshTimer = setInterval(() => load(), 60000);',
    'offline fallback');
   const eventHandler=isDashboard?[
    '    function onLocalJsonEvent(event) {',
    '      let kind;',
    '      try { kind=JSON.parse(event.data).kind; } catch { return; }',
    "      if (kind==='machines') { void load(); return; }",
    "      if (kind==='schedule') {",
    '        void refreshLocalSchedule().then(() => {',
    "          if (state.view==='gantt') void load();",
    '        });',
    '        return;',
    '      }',
    "      if (kind==='settings') void refreshConfiguration();",
    '    }',
   ]:[
    '    function onLocalJsonEvent(event) {',
    '      let kind;',
    '      try { kind=JSON.parse(event.data).kind; } catch { return; }',
    "      if (kind==='machines'||kind==='schedule') { void load(); return; }",
    "      if (kind==='settings') void load();",
    '    }',
   ];
   const shared=[
    '    function startLocalJsonLive() {',
    "      if (typeof EventSource!=='function') return;",
    "      liveJsonStream=new EventSource('/api/live');",
    "      liveJsonStream.addEventListener('mics-json',onLocalJsonEvent);",
    '      liveJsonStream.onopen=()=>{',
    '        liveJsonConnected=true;',
    '        clearInterval(refreshTimer);refreshTimer=null;',
    '      };',
    '      liveJsonStream.onerror=()=>{',
    '        liveJsonConnected=false;',
    '        if(!refreshTimer)refreshTimer=setInterval(()=>load(),60000);',
    '      };',
    '    }',
    '',
   ];
   s=once(s,'    function onStorage(e) {',
     [...eventHandler,...shared].join('\n')+'    function onStorage(e) {',
     'SSE event receiver');
   s=once(s,'    onMounted(() => {',
     '    onMounted(() => {\n      startLocalJsonLive();',
     'subscribe in mount');
   s=once(s,'    onUnmounted(() => {\n      clearInterval(refreshTimer);',
     '    onUnmounted(() => {\n      liveJsonStream?.close();\n      liveJsonStream=null;\n      clearInterval(refreshTimer);',
     'SSE cleanup');
   return s;
  });
 }
}
