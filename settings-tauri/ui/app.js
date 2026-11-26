'use strict';
const $=id=>document.getElementById(id),invoke=(name,args)=>window.__TAURI__.core.invoke(name,args);
const fields=['enabled','display_name','agent_id','host','port','database','user','coordinator_url','interval_seconds','autostart','password','sync_token','coordinator_token'];
const checks=new Set(['enabled','autostart']),secretKeys=['password','sync_token','coordinator_token'];
const actions=['save','saveRestart','start','stop','restart','web','test','aliases','update','runSql','exportSql'];
const headings={general:['一般設定','端末と予備機の動作を設定します。'],connection:['接続設定','データベースと調整サーバーを設定します。'],secrets:['認証情報','接続に必要な認証情報を安全に管理します。'],sync:['同期・更新','同期間隔、別名・状態色、バージョンを管理します。'],traffic:['通信統計','Worker・WebSocket・Turso・Firebirdの時間別通信を確認します。'],sql:['データベース調査','読み取り専用でデータを確認・保存します。']};
const modes={ACTIVE:'稼働中',STANDBY:'待機中',MIRROR_ONLY:'閲覧専用',OFFLINE:'未接続'};
let ready=false,busy=false,updating=false,checking=false;
function say(message,error=false){$('message').textContent=String(message);$('message').classList.toggle('error',error)}
function controls(){for(const key of [...actions,...fields,'query','reveal'])$(key).disabled=!ready||busy||updating}
function dirty(value){$('dirty').textContent=value?'未保存の変更があります':'保存済み'}
function getForm(){return Object.fromEntries(fields.map(k=>[k,checks.has(k)?$(k).checked:$(k).value]))}
async function status(){if(checking||busy||updating)return;checking=true;try{const s=await invoke('agent_status'),label=modes[s.mode]||'不明';$('agentState').textContent=label;$('sideState').textContent=label;$('agentVersion').textContent=s.version?'mo'+s.version:label;$('modeDetail').textContent=s.poll_interval_seconds?'同期間隔：'+s.poll_interval_seconds+'秒':'ローカル稼働状況';const css='dot '+(s.mode==='ACTIVE'?'on':['STANDBY','MIRROR_ONLY'].includes(s.mode)?'wait':'');$('sideDot').className=css;$('statusDot').className=css}catch{$('agentState').textContent='確認できません'}finally{checking=false}}
async function run(work){if(!ready||busy||updating)return;busy=true;controls();try{await work()}catch(e){say(e,true)}finally{busy=false;controls();void status()}}
async function save(){await invoke('save_settings',{form:getForm()});for(const key of secretKeys)$(key).value='';const data=await invoke('load_settings');$('vaultState').textContent=data.credentials_configured?'保存済み':'未設定';dirty(false)}
for(const field of fields)$(field).addEventListener('input',()=>dirty(true));
for(const item of document.querySelectorAll('.nav'))item.addEventListener('click',()=>{const tab=item.dataset.tab;for(const b of document.querySelectorAll('.nav'))b.classList.toggle('active',b===item);for(const p of document.querySelectorAll('.panel'))p.hidden=p.id!==tab;$('heading').textContent=headings[tab][0];$('subtitle').textContent=headings[tab][1];if(tab==='traffic')void refreshTraffic()});
$('reveal').addEventListener('change',()=>{for(const key of secretKeys)$(key).type=$('reveal').checked?'text':'password'});
$('save').addEventListener('click',()=>run(async()=>{await save();say('設定を保存しました。再起動すると適用されます。')}));
$('saveRestart').addEventListener('click',()=>run(async()=>{await save();await invoke('agent_action',{action:'restart'});say('設定を保存し、再起動を要求しました。')}));
for(const [id,label] of [['start','起動を要求しました。'],['stop','停止しました。'],['restart','再起動を要求しました。'],['web','ローカル画面を開きました。']])$(id).addEventListener('click',()=>run(async()=>{await invoke('agent_action',{action:id});say(label)}));
$('test').addEventListener('click',()=>run(async()=>{say('保存済みの設定で接続を確認しています…');const r=await invoke('connection_test');$('testResult').hidden=false;$('testResult').textContent=['データベース疎通：'+(r.firebird?'成功':'未確認'),'調整サーバー：'+(r.coordinator?'成功':'未確認'),'端末登録：'+(r.registration?'成功':'未確認')].join('\n');say(r.ok?'接続テストが完了しました。':'接続テストに失敗しました。保存済みの設定をご確認ください。',!r.ok)}));
$('aliases').addEventListener('click',()=>run(async()=>{say('別名・状態色を同期しています…');say(await invoke('sync_aliases'))}));
for(const id of ['runSql','exportSql'])$(id).addEventListener('click',()=>run(async()=>{say('データベースを読み取り中…');$('sqlResult').value='処理中…';try{$('sqlResult').value=await invoke('inspect_sql',{query:id==='runSql'?$('query').value:null});say('処理が完了しました。')}catch(e){$('sqlResult').value=String(e);throw e}}));
$('update').addEventListener('click',()=>run(async()=>{await invoke('start_update');updating=true;controls();say('最新版を確認しています…')}));
let polling=false;
async function pollUpdate(){if(!updating||polling)return;polling=true;try{const r=await invoke('update_state');$('progress').value=r.percent;$('updateMessage').textContent=r.message;say(r.message,r.failed);if(r.staged){await invoke('close_after_update');return}if(!r.running){updating=false;controls()}}catch(e){say(e,true)}finally{polling=false}}
async function init(){controls();try{const data=await invoke('load_settings');$('version').textContent='設定 mo'+data.version;$('vaultState').textContent=data.credentials_configured?'保存済み':'未設定';for(const key of fields){if(checks.has(key))$(key).checked=!!data.form[key];else $(key).value=data.form[key]??''}ready=true;dirty(false);say('準備完了');controls();void status()}catch(e){say(e,true)}}
void init();setInterval(status,15000);setInterval(pollUpdate,750);

const trafficCols=['worker_http','ws_sent','ws_received','turso_requests','turso_receipt_missing','firebird_reads','avg_ms','github_http'];
const trafficLabels={worker_http:'Worker HTTP試行（WS接続含む）',worker_http_response:'Worker HTTP応答',ws_attempt:'WebSocket送信試行',ws_sent:'WebSocket送信',ws_received:'WebSocket受信',ws_connected:'WebSocket接続成功',ws_handshake:'WebSocket接続試行',turso_requests:'Turso HTTP試行（確認済み）',turso_responses:'Turso HTTP応答',turso_http_ok:'Turso HTTP成功',turso_pipeline_ok:'Tursoパイプライン成功',turso_receipt_missing:'Turso通信数不明（操作数）',firebird_reads:'Firebird読取処理',avg_ms:'Firebird平均応答 ms',github_http:'GitHub HTTP試行',http_error:'HTTPエラー',ws_error:'WSエラー',firebird_error:'Firebirdエラー'};
const opNames={lease:'稼働権限',presence:'端末状態',sync:'生産データ',schedule:'生産予定',cache:'待機キャッシュ',aliases:'別名',status_colors:'状態色',socket:'WS接続',production:'生産読取',materials:'原料読取',health:'接続確認',update_check:'更新確認',update_download:'更新取得'};
let trafficData=null,trafficBusy=false;
function stat(c,k){if(k==='avg_ms')return c.firebird_reads?Math.round((c.firebird_ms||0)/c.firebird_reads):0;return c[k]||0}
function cell(tag,value){const e=document.createElement(tag);e.textContent=String(value);return e}
function rows(id,items,cols){const el=$(id);el.replaceChildren();for(const [name,c] of items){const tr=document.createElement('tr');tr.append(cell('td',name));for(const k of cols)tr.append(cell('td',k==='errors'?(c.http_error||0)+(c.ws_error||0)+(c.firebird_error||0):stat(c,k)));el.append(tr)}}
async function refreshTraffic(){if($('traffic').hidden||trafficBusy)return;trafficBusy=true;try{const data=await invoke('traffic_stats',{days:Number($('trafficDays').value)});trafficData=data;const totals=$('trafficTotals');totals.replaceChildren();for(const [k,label] of Object.entries(trafficLabels)){const box=document.createElement('div');box.append(cell('small',label),cell('strong',stat(data.totals,k).toLocaleString('ja-JP')));totals.append(box)}rows('trafficHours',data.hours.slice().reverse().map(x=>[x.hour,x.counts]),trafficCols);rows('trafficOps',Object.entries(data.operations).map(([k,c])=>[opNames[k]||k,c]),['worker_http','ws_sent','ws_received','turso_requests','turso_receipt_missing','errors']);$('trafficStatus').textContent='更新：'+new Date().toLocaleTimeString('ja-JP',{timeZone:'Asia/Tokyo'})+' JST ・ DO換算の目安：'+Math.ceil((data.totals.ws_received||0)/20)+'（受信したACKを基にした参考値、請求値ではありません）';}catch(e){$('trafficStatus').textContent=String(e)}finally{trafficBusy=false}}
$('trafficDays').addEventListener('change',()=>void refreshTraffic());
$('trafficExport').addEventListener('click',async()=>{if(!trafficData)return;try{const csv=['hour,'+trafficCols.join(','),...trafficData.hours.map(x=>[x.hour,...trafficCols.map(k=>stat(x.counts,k))].join(','))].join('\r\n');const path=await invoke('save_traffic_csv',{csv});$('trafficStatus').textContent='CSV保存：'+path;}catch(e){$('trafficStatus').textContent=String(e)}});
setInterval(refreshTraffic,5000);
