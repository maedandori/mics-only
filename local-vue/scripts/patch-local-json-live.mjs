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
    '    let refreshTimer = null;\n    let liveJsonStream = null;\n    let liveJsonConnected = false;\n    let jsonVersionTimer = null;\n    let knownJsonVersions = null;\n    let versionRequestBusy = false;',
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
    '    function ensureLocalFallback() {',
    '      if (!refreshTimer) refreshTimer=setInterval(()=>void load(),15000);',
    '    }',
    '    async function checkLocalJsonVersions() {',
    '      if (versionRequestBusy) return;',
    '      versionRequestBusy=true;',
    '      try {',
    "        const response=await fetch('/api/json-versions',{cache:'no-store'});",
    "        if (!response.ok) throw Error('JSON version status HTTP '+response.status);",
    '        const next=await response.json();',
    "        if (!next?.ok) throw Error('Local JSON version status unavailable');",
    '        const previous=knownJsonVersions;',
    '        knownJsonVersions=next;',
    '        if(previous){',
    "          if(next.machines!==previous.machines)onLocalJsonEvent({data:'{\"kind\":\"machines\"}'});",
    "          if(next.schedule!==previous.schedule)onLocalJsonEvent({data:'{\"kind\":\"schedule\"}'});",
    "          if(next.aliases!==previous.aliases||next.colors!==previous.colors)onLocalJsonEvent({data:'{\"kind\":\"settings\"}'});",
    '        }',
    '      } catch(err) {',
    '        // A missing SSE/version endpoint must never freeze a local dashboard.',
    '        if(!liveJsonConnected)ensureLocalFallback();',
    '        else if(!refreshTimer)refreshTimer=setInterval(()=>void load(),20000);',
    '      } finally { versionRequestBusy=false; }',
    '    }',
    '    function startLocalJsonLive() {',
    "      if (new URLSearchParams(location.search).has('ci-smoke')) return;",
    "      // Independent metadata watchdog: GETs only this PC, never D1/Turso.",
    '      jsonVersionTimer=setInterval(()=>void checkLocalJsonVersions(),8000);',
    '      void checkLocalJsonVersions();',
    "      if (typeof EventSource!=='function') {ensureLocalFallback();return;}",
    "      liveJsonStream=new EventSource('/api/live');",
    "      liveJsonStream.addEventListener('mics-json',onLocalJsonEvent);",
    '      liveJsonStream.onopen=()=>{',
    '        liveJsonConnected=true;',
    '        clearInterval(refreshTimer);refreshTimer=null;',
    '        // Reconcile immediately after SSE reconnect, even if an event was lost.',
    '        void load();',
    '        void checkLocalJsonVersions();',
    '      };',
    '      liveJsonStream.onerror=()=>{',
    '        liveJsonConnected=false;',
    '        ensureLocalFallback();',
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
     '    onUnmounted(() => {\n      liveJsonStream?.close();\n      liveJsonStream=null;\n      clearInterval(jsonVersionTimer);jsonVersionTimer=null;\n      clearInterval(refreshTimer);',
     'SSE cleanup');
   if(isDashboard){
    // Never drop an update arriving while the initial machine JSON is loading.
    s=once(s,
      '    let loadBusy = false;\n    async function load(file = null) {\n      if (loadBusy) return;',
      '    let loadBusy = false, loadQueued = false;\n    async function load(file = null) {\n      if (loadBusy) {loadQueued=true;return;}',
      'busy reload queue');
    s=once(s,
      '      } finally { loadBusy = false; }\n    }',
      '      } finally { loadBusy = false; if(loadQueued){loadQueued=false;queueMicrotask(()=>void load());} }\n    }',
      'drain latest update');
   }
   return s;
  });
 }
}
