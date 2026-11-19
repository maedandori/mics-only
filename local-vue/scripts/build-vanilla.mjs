// Preserve original uploaded HTML/CSS/JS templates, replacing Vue with native DOM JS.
// No Vite, Vue bundle, CDN dependency, or network fetches required by the UI.
import {readFileSync,writeFileSync,cpSync,mkdirSync,rmSync} from 'node:fs';
import {resolve,join} from 'node:path';
import {execFileSync} from 'node:child_process';
import {Script} from 'node:vm';
import './prepare-original.mjs';
const pkg=JSON.parse(readFileSync('package.json','utf8'));
if(!/^\d+\.\d+\.\d+$/.test(pkg.version))throw Error('Invalid version');
const base='vue-v'+pkg.version,output=resolve('../www',base);
rmSync(output,{force:true,recursive:true});
cpSync('original',output,{recursive:true});
mkdirSync(join(output,'assets'),{recursive:true});
writeFileSync(join(output,'assets','compat.js'),'/* Native MICS HTML/JS compatibility asset. No Vue runtime. */\n');
writeFileSync(join(output,'assets','compat.css'),'/* Native template, styles generated from original Tailwind classes. */\n');

// Compile template expressions at build time as classic JavaScript functions.
// This preserves the original declarative HTML without runtime eval/new Function
// and works under the existing Content-Security-Policy script-src 'self'.
const expressions=new Set(),setters=new Set();
for(const name of ['dashboard-app.js','change-schedule-app.js','status-colors-app.js','change-aliases-app.js','components.js']){
 const source=readFileSync(join(output,'js',name),'utf8');
 const attr=/(?:v-if|v-else-if|v-for|v-model|:[\w-]+|@[\w.-]+)\s*=\s*"([^"]*)"/g;
 for(const match of source.matchAll(attr)){
  const value=match[1].trim();
  if(match[0].startsWith('v-model'))setters.add(value);
  const loop=value.match(/^\s*(?:\(([^)]+)\)|[\w$]+)\s+(?:in|of)\s+([\s\S]+)$/);
  expressions.add(loop?loop[2].trim():value);
 }
 for(const match of source.matchAll(/{{([\s\S]*?)}}/g))expressions.add(match[1].trim());
}
const exprCode=['/* Compiled from trusted original MICS templates at build time. */',
 'window.MICS_EXPRESSIONS=Object.create(null);','window.MICS_SETTERS=Object.create(null);'];
for(const value of expressions){
 // Every expression remains local and must parse before release.
 exprCode.push('window.MICS_EXPRESSIONS['+JSON.stringify(value)+']=function(s,$event){with(s){return ('+value+')}};');
}
for(const value of setters)exprCode.push('window.MICS_SETTERS['+JSON.stringify(value)+']=function(s,v){with(s){'+value+'=v}};');
const compiled=exprCode.join('\n')+'\n';
new Script(compiled,{filename:'compiled-expressions.js'});
writeFileSync(join(output,'assets','compiled-expressions.js'),compiled);

writeFileSync(join(output,'js','vanilla-runtime.js'),readFileSync('scripts/vanilla-runtime.js','utf8'));
for(const page of ['index.html','change-schedule.html','status-colors.html','change-aliases.html']){
 const file=join(output,page);
 let html=readFileSync(file,'utf8');
 html=html.replaceAll(' (Vue 3)',' (JavaScript)').replaceAll(' v-cloak','');
 html=html.replace(/href="css\//g,'href="/'+base+'/css/');
 html=html.replace(/src="js\//g,'src="/'+base+'/js/');
 html=html.replace('</head>','<link rel="stylesheet" href="/'+base+'/assets/compat.css" />\n<script src="/'+base+'/assets/compiled-expressions.js"></script>\n<script src="/'+base+'/assets/compat.js"></script>\n</head>');
 writeFileSync(file,html);
}
for(const name of ['dashboard-app.js','change-schedule-app.js','status-colors-app.js','change-aliases-app.js','components.js']){
 const file=join(output,'js',name);
 let source=readFileSync(file,'utf8').replace(/from\s+['"]vue['"]/g,"from './vanilla-runtime.js'");
 if(source.includes("from 'vue'"))throw Error('Vue import remained: '+name);
 writeFileSync(file,source);
}
for(const name of ['common.js','components.js','dashboard-app.js','change-schedule-app.js','status-colors-app.js','change-aliases-app.js','vanilla-runtime.js']){
 execFileSync(process.execPath,['--check',join(output,'js',name)],{stdio:'inherit'});
}
for(const page of ['index.html','change-schedule.html','status-colors.html','change-aliases.html']){
 const html=readFileSync(join(output,page),'utf8');
 if(html.includes('cdn.tailwindcss.com')||html.includes('vue.esm-browser')||!html.includes('/'+base+'/js/'))throw Error('Standalone template verification failed: '+page);
}
// Generate Tailwind into this release's own directory, not an older version.
const tailwind=resolve('node_modules','tailwindcss','lib','cli.js');
execFileSync(process.execPath,[tailwind,'-c','tailwind.config.cjs','-i','original/tailwind-input.css',
 '-o',join(output,'css','tailwind-generated.css'),'--minify'],{stdio:'inherit'});
const generatedCss=readFileSync(join(output,'css','tailwind-generated.css'),'utf8');
if(generatedCss.length<4000||!generatedCss.includes('.grid'))throw Error('Current-version Tailwind CSS missing or incomplete: '+base);
// Add local-only horizontal navigation without rewriting the user's original template.
const localTabsJS=`/* MICS local horizontal navigation and all-machine order list. Read-only JSON. */
(()=>{
'use strict';
const $=(s,r=document)=>r.querySelector(s), $$=(s,r=document)=>Array.from(r.querySelectorAll(s));
const esc=s=>String(s??'').replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
const order=[15,8,19,14,5,17,13,6,10,11,18,7,12,16], byOrder=n=>{const i=order.indexOf(Number(n));return i<0?999:i};
const dashboard=location.pathname==='/'||location.pathname.endsWith('/index.html');
function titleText(node){return (node?.textContent||'').replace(/\\s+/g,' ').trim()}
const nav=document.createElement('nav');
nav.id='mics-local-tabs';nav.setAttribute('aria-label','画面切り替え');
nav.innerHTML='<a data-page="dashboard" href="/">▦ ダッシュボード</a><a data-page="schedule" href="/change-schedule.html">▤ 型替え・色替え予定</a><button type="button" data-page="products">☷ 全機械の品番一覧</button><button type="button" id="mics-local-refresh" title="クリックでローカル表示を再読込">データ更新: —</button>';
document.body.prepend(nav);
const dashboardLink=$('[data-page="dashboard"]',nav);
const productButton=$('[data-page="products"]',nav);
const refreshButton=$('#mics-local-refresh',nav);
let refreshStatus=null,refreshStatusBusy=false;
function renderRefreshClock(){
  const mode=refreshStatus?.mode||'';
  const at=Date.parse(refreshStatus?.next_poll_at||'');
  const remaining=Number.isFinite(at)?Math.ceil((at-Date.now())/1000):NaN;
  let label='待機中';
  if(!refreshStatus)label='接続確認中';
  else if((mode==='ACTIVE'||mode==='MIRROR_ONLY')&&Number.isFinite(remaining)){
    if(remaining<=0)label='更新中…';
    else {
      const minutes=Math.floor(remaining/60),seconds=String(remaining%60).padStart(2,'0');
      label=(minutes>=60?Math.floor(minutes/60)+'時間'+String(minutes%60).padStart(2,'0')+'分':String(minutes).padStart(2,'0')+':')+seconds+(minutes>=60?'秒':'');
      if(mode==='MIRROR_ONLY')label='ミラー '+label;
    }
  }else if(mode==='ACTIVE')label='更新中…';
  else if(mode==='OFFLINE')label='オフライン';
  refreshButton.textContent='データ更新: '+label;
  const interval=refreshStatus?.poll_interval_seconds;
  refreshButton.title='クリック: ローカル画面を再読込（Firebirdの強制同期ではありません）'+(interval?' / MICS設定: '+interval+'秒':'')+(mode==='MIRROR_ONLY'?' / ミラーは約5分間隔':'');
}
async function readRefreshStatus(){
  if(refreshStatusBusy)return;
  refreshStatusBusy=true;
  try{
    const response=await fetch('/api/status',{cache:'no-store'});
    if(!response.ok)throw Error('Local agent unavailable');
    refreshStatus=await response.json();
  }catch{refreshStatus=null}
  finally{refreshStatusBusy=false;renderRefreshClock()}
}
refreshButton.addEventListener('click',()=>location.reload());
setInterval(renderRefreshClock,1000);
setInterval(()=>void readRefreshStatus(),15000);
document.addEventListener('visibilitychange',()=>{if(!document.hidden)void readRefreshStatus()});
void readRefreshStatus();
if(dashboard){dashboardLink.classList.add('selected')}
else if(location.pathname.includes('change-schedule')){$('[data-page="schedule"]',nav).classList.add('selected')}
const page=document.createElement('section');page.id='mics-products';page.hidden=true;
page.innerHTML='<div class="mics-products-head"><div><h2>全機械の品番一覧</h2><p id="mics-products-count">ローカルJSONを読み込み中</p></div><div class="mics-products-actions"><input id="mics-products-search" placeholder="受注番号・品名・金型・号機で検索…" aria-label="品番を検索"><select id="mics-products-sort" aria-label="並び替え"><option value="machine">号機順</option><option value="start">開始日時順</option><option value="order">受注番号順</option></select></div></div><div class="mics-products-scroll"><table><thead><tr><th>#</th><th>号機</th><th>品番</th><th>品名</th><th>金型</th><th>開始</th><th>終了</th><th>時間</th></tr></thead><tbody id="mics-products-rows"></tbody></table></div>';
document.body.append(page);
let rows=[], showing=false, loading=false, visibleRows=[];
const details=document.createElement('div');
details.id='mics-products-detail';
details.hidden=true;
details.style.cssText='position:fixed;inset:0;z-index:15000;background:rgba(15,23,42,.75);display:none;align-items:center;justify-content:center;padding:12px;box-sizing:border-box';
page.append(details);
function closeDetails(){details.hidden=true;details.style.display='none';details.innerHTML='';}
function showDetails(r){
 const item=(label,value)=>'<dt style="font-weight:700;color:#64748b">'+esc(label)+'</dt><dd style="margin:0;overflow-wrap:anywhere">'+esc(value??'—')+'</dd>';
 const names=Array.isArray(r.material_names)?r.material_names.filter(Boolean):[];
 const materials=names.length?'<ul style="margin:0;padding-left:22px;overflow-wrap:anywhere">'+names.map(v=>'<li style="padding:4px 0">'+esc(v)+'</li>').join('')+'</ul>':'<p style="margin:0;color:#64748b">原料データがありません</p>';
 details.innerHTML='<section role="dialog" aria-modal="true" aria-labelledby="mics-products-detail-title" style="box-sizing:border-box;width:100%;max-width:540px;max-height:calc(100dvh - 24px);overflow-y:auto;border-radius:14px;padding:18px;background:#fff;color:#0f172a;box-shadow:0 20px 60px rgba(0,0,0,.25)">'
 +'<header style="display:flex;justify-content:space-between;align-items:flex-start;gap:12px;margin-bottom:14px"><div><h2 id="mics-products-detail-title" style="margin:0 0 6px;font-size:19px">品番の詳細</h2><strong style="color:#1d4ed8;font-size:16px;overflow-wrap:anywhere">'+esc(r.parts_no||r.order_no||'—')+'</strong></div><button type="button" id="mics-products-close-detail" aria-label="閉じる" style="flex:none;border:1px solid #cbd5e1;border-radius:9px;padding:7px 13px;font-size:18px;background:#f1f5f9;color:#0f172a;cursor:pointer">×</button></header>'
 +'<dl style="margin:0;display:grid;grid-template-columns:minmax(90px,34%) minmax(0,1fr);gap:9px 12px;font-size:14px">'
 +item('号機',machineKey(r)+'号機')+item('品番',r.parts_no||'—')+item('受注番号',r.order_no||'—')
 +item('品名',r.product_name||'—')+item('金型',[(r.mold_name||''),(r.mold_no||'')].filter(Boolean).join(' / ')||'—')
 +item('色',r.color||'—')+item('開始',formatDate(r.start_at))+item('終了',formatDate(r.end_at))
 +(r.qty_plan!=null?item('予定数',Number(r.qty_plan).toLocaleString('ja-JP')):'')+'</dl>'
 +'<div style="margin-top:16px;border-top:1px solid #e2e8f0;padding-top:13px"><strong style="display:block;margin-bottom:8px">使用原料</strong>'+materials+'</div></section>';
 details.hidden=false;
 details.style.display='flex';
 const close=$('#mics-products-close-detail',details);
 close?.addEventListener('click',closeDetails);
 close?.focus();
}
details.addEventListener('click',e=>{if(e.target===details)closeDetails()});
document.addEventListener('keydown',e=>{if(e.key==='Escape'&&!details.hidden)closeDetails()});
$('#mics-products-rows',page).addEventListener('click',e=>{
 const button=e.target.closest('button[data-mics-detail]');
 if(!button)return;
 const row=visibleRows[Number(button.dataset.micsDetail)];
 if(row)showDetails(row);
});
function closeProducts(){
 if(!showing)return;
 showing=false;page.hidden=true;closeDetails();document.body.classList.remove('mics-products-open');
 $$('[data-page]',nav).forEach(el=>el.classList.toggle('selected',dashboard?el.dataset.page==='dashboard':el.dataset.page==='schedule'));
}
function removeOldChrome(){
  // Mark the old dashboard filter/search/sort toolbar so it stays hidden.
  $$('#app #status-chips, #app #search, #app #sort').forEach(el=>{
    const sec=el.closest('section');
    if(sec && sec!==document.body && !sec.querySelector('.card[data-machine],article[data-machine]')){
      sec.dataset.micsOldToolbar='1';
    }
  });
  // Locate the original three-mode controller (#view-seg: カード / 一覧 / 日程)
  // inside #app and relocate it intact to the top-right of #mics-local-tabs.
  let group=$('#app #view-seg');
  if(!group){
    const originalButtons=$$('#app button').filter(b=>!b.closest('#mics-local-tabs') && !b.closest('#mics-products'));
    const mode=re=>originalButtons.find(b=>re.test(titleText(b).replace(/\\s+/g,'')));
    const buttons=[/カード$/, /一覧$/, /日程$/].map(mode);
    if(buttons.every(Boolean)){
      let candidate=buttons[0].parentElement;
      for(let i=0;candidate&&i<7&&!buttons.every(b=>candidate.contains(b));i++)candidate=candidate.parentElement;
      if(candidate && candidate!==document.body && candidate.id!=='app')group=candidate;
    }
  }
  if(group){
    const sec=group.closest('section');
    if(sec && sec!==document.body && !sec.querySelector('.card[data-machine],article[data-machine]')){
      sec.dataset.micsOldToolbar='1';
    }
    let dest=$('#mics-local-view-modes',nav);
    if(!dest){
      dest=document.createElement('div');
      dest.id='mics-local-view-modes';
      dest.addEventListener('click',e=>{
        if(e.target.closest('button') && showing)closeProducts();
      });
      nav.append(dest);
    }
    if(group.parentElement!==dest){
      dest.replaceChildren(group);
    }
  }
  // Independently hide deprecated settings links and old menu toggle.
  $$('a,button').forEach(el=>{
    if(el.closest('#mics-local-tabs'))return;
    if(el.id==='menu-toggle' || /^(Status Setting|Status Settings|Alias Setting|Alias Settings|ステータス設定|エイリアス設定)$/i.test(titleText(el))){
      el.hidden=true;
    }
  });
}
const machineKey=r=>Number(r.machine_no??r.machine??r.machine_number);
const parseDate=v=>{if(!v)return NaN;const s=String(v);return Date.parse(/[zZ]$|[+-]\\d\\d:\\d\\d$/.test(s)?s:s.replace(' ','T')+'+09:00')};
const formatDate=v=>{const n=parseDate(v);return Number.isFinite(n)?new Intl.DateTimeFormat('ja-JP',{timeZone:'Asia/Tokyo',month:'2-digit',day:'2-digit',weekday:'short',hour:'2-digit',minute:'2-digit'}).format(new Date(n)):String(v||'—')};
const duration=r=>{const a=parseDate(r.start_at),b=parseDate(r.end_at);if(!Number.isFinite(a)||!Number.isFinite(b)||b<a)return '—';const mins=Math.round((b-a)/60000);return mins>=1440?Math.floor(mins/1440)+'日'+Math.floor(mins%1440/60)+'時間':Math.floor(mins/60)+'時間'+mins%60+'分'};
function render(){
 const q=$('#mics-products-search',page).value.normalize('NFKC').toLowerCase().trim();
 const sort=$('#mics-products-sort',page).value;
 const selected=rows.filter(r=>!q||[r.machine_no,r.order_no,r.parts_no,r.product_name,r.mold_no,r.mold_name,r.color,(Array.isArray(r.material_names)?r.material_names.join(" "):"")].some(v=>String(v??'').normalize('NFKC').toLowerCase().includes(q)));
 selected.sort((a,b)=>sort==='start'?(parseDate(a.start_at)||0)-(parseDate(b.start_at)||0)||byOrder(machineKey(a))-byOrder(machineKey(b)):sort==='order'?String(a.order_no||'').localeCompare(String(b.order_no||''),'ja'):byOrder(machineKey(a))-byOrder(machineKey(b))||(parseDate(a.start_at)||0)-(parseDate(b.start_at)||0));
 visibleRows=selected;
 $('#mics-products-count',page).textContent=selected.length+' 件 / 全'+rows.length+' 件 · ローカル保存データ';
 $('#mics-products-rows',page).innerHTML=selected.length?selected.map((r,i)=>'<tr><td>'+ (i+1) +'</td><td><b>'+esc(machineKey(r))+'号機</b></td><td><button type="button" data-mics-detail="'+i+'" aria-label="品番の詳細" style="background:none;border:0;padding:3px 0;color:#1d4ed8;text-decoration:underline;font:700 13px system-ui,sans-serif;text-align:left;cursor:pointer;overflow-wrap:anywhere">'+esc(r.parts_no||r.order_no||'詳細')+'</button></td><td>'+esc(r.product_name||'—')+'</td><td><b>'+esc(r.mold_no||'—')+'</b><small>'+esc(r.mold_name||'')+'</small></td><td>'+esc(formatDate(r.start_at))+'</td><td>'+esc(formatDate(r.end_at))+'</td><td>'+esc(duration(r))+'</td></tr>').join(''):'<tr><td colspan="8" class="mics-empty">該当する品番はありません</td></tr>';
}
async function load(){
 if(loading)return;loading=true;
 try{
  const [a,b]=await Promise.all([fetch('/data/schedule-local.json',{cache:'no-store'}),fetch('/data/machines.json',{cache:'no-store'})]);
  if(!a.ok)throw Error('schedule-local.json HTTP '+a.status);
  const schedule=await a.json(), machines=b.ok?await b.json():{machines:[]};
  const scheduled=Array.isArray(schedule.rows)?schedule.rows:[];
  const active=Array.isArray(machines.machines)?machines.machines:[];
  // Every scheduled order for every machine; include a running machine only if no matching order is scheduled.
  const known=new Set(scheduled.map(r=>machineKey(r)+'|'+String(r.order_no??'')));
  rows=scheduled.slice();
  for(const m of active)if(m.order_no&&!known.has(machineKey(m)+'|'+String(m.order_no)))rows.unshift({...m,start_at:m.start_at||m.started_at||'',end_at:m.end_at||m.scheduled_end_at||''});
  render();
 }catch(err){$('#mics-products-count',page).textContent='ローカルJSONの読み込みに失敗しました: '+err.message}
 finally{loading=false}
}
function openProducts(){
 showing=true;page.hidden=false;document.body.classList.add('mics-products-open');
 $$('[data-page]',nav).forEach(el=>el.classList.toggle('selected',el.dataset.page==='products'));
 void load();
}
dashboardLink.addEventListener('click',e=>{
 if(dashboard && showing){e.preventDefault();closeProducts();}
});
productButton.addEventListener('click',openProducts);
$('#mics-products-search',page).addEventListener('input',render);
$('#mics-products-sort',page).addEventListener('change',render);
if('EventSource' in window){try{const sse=new EventSource('/api/live');sse.addEventListener('mics-json',e=>{if(!showing)return;try{const kind=JSON.parse(e.data).kind;if(kind==='machines'||kind==='schedule')void load()}catch{}})}catch{}}
document.addEventListener('DOMContentLoaded',removeOldChrome);
if(document.readyState!=='loading')removeOldChrome();
// The original runtime mounts its controls asynchronously and may replace them
// when its view rerenders. Reattach the real buttons only when DOM nodes change.
let chromeScheduled=false;
const chromeObserver=new MutationObserver(records=>{
 if(!records.some(record=>record.addedNodes.length||record.removedNodes.length))return;
 if(chromeScheduled)return;
 chromeScheduled=true;
 queueMicrotask(()=>{chromeScheduled=false;removeOldChrome()});
});
chromeObserver.observe(document.body,{childList:true,subtree:true});
})();`;
const localTabsCSS="\n/* Compact horizontal menu – the existing upper-right card/list/calendar control is untouched. */\n#mics-local-tabs{position:sticky;top:0;z-index:100;display:flex!important;flex-wrap:wrap;gap:5px;align-items:center!important;min-height:48px;width:100%;box-sizing:border-box;padding:8px 12px;background:#fff;border-bottom:1px solid #dce3ef;font:600 13px/1.3 system-ui,\"Noto Sans JP\",sans-serif}\n#mics-local-tabs>[data-page]{display:inline-flex;align-items:center;text-decoration:none;color:#31445f;background:transparent;border:1px solid transparent;border-radius:7px;padding:9px 13px;font:inherit;cursor:pointer}\n#mics-local-tabs>[data-page].selected,#mics-local-tabs>[data-page]:hover{background:#eaf1ff;border-color:#b7cdfb;color:#1451cb}\n#mics-local-refresh{display:inline-flex;align-items:center;padding:8px 12px;border:1px solid #bfdbfe;border-radius:8px;background:#eff6ff;color:#1e40af;cursor:pointer;font:700 12px/1.3 system-ui,sans-serif;white-space:nowrap;font-variant-numeric:tabular-nums}\n#mics-local-refresh:hover{background:#dbeafe}\nbody.mics-products-open> :not(#mics-local-tabs):not(#mics-products):not(script){display:none!important}\n#mics-products{padding:18px 14px 32px;background:#fff;min-height:80vh;color:#12233f;font:13px/1.5 system-ui,\"Noto Sans JP\",sans-serif}\n#mics-products[hidden]{display:none!important}\n.mics-products-head{display:flex;align-items:center;justify-content:space-between;flex-wrap:wrap;gap:12px;margin-bottom:12px}\n.mics-products-head h2{font-size:19px;font-weight:800;margin:0}.mics-products-head p{font-size:12px;color:#64748b;margin:2px 0}\n.mics-products-actions{display:flex;gap:8px;flex-wrap:wrap}\n.mics-products-actions input,.mics-products-actions select{border:1px solid #cbd5e1;border-radius:8px;padding:9px 11px;background:#fff;min-width:175px}\n.mics-products-actions input{min-width:min(320px,80vw)}\n.mics-products-scroll{overflow-x:auto;border:1px solid #cbd5e1;border-radius:10px}\n#mics-products table{border-collapse:collapse;width:100%;min-width:880px;text-align:left}\n#mics-products th{background:#eff4fa;font-weight:700;color:#45566e;font-size:12px;padding:10px;border-bottom:1px solid #cbd5e1}\n#mics-products td{padding:9px 10px;border-bottom:1px solid #e0e7f1;vertical-align:top}\n#mics-products tr:nth-child(even){background:#f7f9fc}\n#mics-products tr:hover td{background:#ebf2ff}\n#mics-products td small{display:block;font-size:11px;color:#64748b}\n#mics-products .mics-empty{text-align:center;padding:22px}\n#mics-local-view-modes{display:flex;align-items:center;justify-content:flex-end;margin-left:auto;flex:0 0 auto}\n#mics-local-view-modes>div{display:inline-flex;align-items:center}\n#mics-local-view-modes button{white-space:nowrap}\n[data-mics-old-toolbar=\"1\"],#app #menu-toggle,#app #status-chips,#app #search,#app #sort,#app section:has(#status-chips),#app section:has(#search){display:none!important}\n@media(max-width:750px){#mics-local-tabs{flex-wrap:wrap}#mics-local-view-modes{margin-left:auto}}\n@media(max-width:650px){#mics-local-tabs{padding:5px;gap:1px}#mics-local-tabs>[data-page]{padding:8px;font-size:11px}#mics-products{padding:10px 6px}}\n";
writeFileSync(join(output,'js','local-horizontal-tabs.js'),localTabsJS);
writeFileSync(join(output,'css','local-horizontal-tabs.css'),localTabsCSS);
execFileSync(process.execPath,['--check',join(output,'js','local-horizontal-tabs.js')],{stdio:'inherit'});
for(const page of ['index.html','change-schedule.html']){
 const file=join(output,page);let html=readFileSync(file,'utf8');
 html=html.replace('</head>','<link rel="stylesheet" href="/'+base+'/css/local-horizontal-tabs.css" /></head>');
 html=html.replace('</body>','<script src="/'+base+'/js/local-horizontal-tabs.js" defer></script></body>');
 writeFileSync(file,html);
}
console.log('Original four-page template converted to native JavaScript with current-version CSS: '+base);
