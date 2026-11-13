"use strict";
const order=[15,8,19,14,5,17,13,6,10,11,18,7,12,16],names={100:"段取り中",200:"稼働中",210:"サイクル注意",410:"金型異常",440:"色替え",490:"異常"};
const $=id=>document.getElementById(id), esc=x=>String(x??"").replace(/[&<>"']/g,c=>({"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;","'":"&#39;"}[c]));
let current={machines:[]},schedule={rows:[]},week=0,filter="all",view="dashboard";
function status(code){const s=String(code??"");return["410","440","490"].includes(s)?"danger":["100","210"].includes(s)?"warning":s==="200"?"running":""}
function num(n){return(Number(n)||0).toLocaleString("ja-JP")}
function drawDashboard(){
 const map=new Map((current.machines||[]).map(m=>[Number(m.machine_no),m]));
 const format=v=>(Number(v)||0).toLocaleString("ja-JP");
 const cell=(name,value,flame=false)=>{
  const text=format(value);
  return '<div><dt>'+name+'</dt><dd class="'+(flame?'ng-fire':'')+'"><span class="'+(flame?'ng-value':'')+'"'+(flame?' data-flame-number="'+esc(text)+'"':'')+'>'+text+'</span></dd></div>';
 };
 $("machines").innerHTML=order.map(n=>{
  const m=map.get(n)||{},has=map.has(n),planned=Math.max(0,+m.qty_plan||0),good=Math.max(0,+m.qty_ok||0),bad=Math.max(0,+m.qty_ng||0);
  const remaining=Math.max(0,Number.isFinite(Number(m.qty_remaining))&&m.qty_remaining!==null?+m.qty_remaining:planned-good);
  const pct=planned?Math.max(0,Math.min(100,100*good/planned)):0;
  const staff=m.staff_name||m.operator_name||"",cycle=m.cycle_seconds??"—",pick=m.pick_qty??"—";
  const statusLabel=has?(names[m.status_code]||m.status_name||m.status_code||"不明"):"データなし";
  return '<article class="node '+status(m.status_code)+'">'+
   '<div class="node-top"><span class="machine-number">#'+n+'</span><span class="staff">'+(staff?'担当者: '+esc(staff):'')+'</span></div>'+
   '<div class="mold">'+esc(m.mold_name||m.mold_no||"—")+'</div>'+
   '<div class="mold-code">金型番号: '+esc(m.mold_no||"—")+'</div>'+
   '<div class="product">'+esc(m.product_name||"—")+'</div>'+
   '<div class="main-metrics"><div class="cycle">サイクル: <strong>'+esc(cycle)+'</strong><div class="pick">取り数: '+esc(pick)+'</div></div>'+
   '<div class="progress" role="progressbar" aria-label="生産進捗" aria-valuemin="0" aria-valuemax="100" aria-valuenow="'+pct.toFixed(1)+'">'+
   '<svg width="102" height="102" viewBox="0 0 102 102" aria-hidden="true"><circle class="track" cx="51" cy="51" r="42"/><circle class="arc" cx="51" cy="51" r="42" pathLength="100" stroke-dasharray="'+pct.toFixed(1)+' 100"/></svg>'+
   '<b>'+pct.toFixed(0)+'%</b><small>生産進捗</small></div></div>'+
   '<dl class="quantities">'+cell("予定数",planned)+cell("良品数",good)+cell("不良数",bad,bad>5)+cell("残数",remaining)+'</dl>'+
   '<span class="status">'+esc(statusLabel)+'</span><span class="status-code">'+esc(m.status_code??"")+'</span></article>';
 }).join("");
}
const dataFields=[["machine_no","機械番号"],["status_code","ステータス"],["mold_name","金型名"],["mold_no","金型番号"],["order_no","製造番号"],["product_name","品名"],["qty_plan","予定数"],["qty_ok","良品数"],["qty_ng","不良数"],["qty_remaining","残数"],["cycle_seconds","サイクル"],["pick_qty","取り数"],["staff_name","担当者"],["scheduled_end_at","完了予定"]];
let lastAllSignature="";
function drawAll(){
 // ALL shows every stored schedule row per machine, across all available dates.
 const signature=JSON.stringify({machines:current.machines,rows:schedule.rows,from:schedule.from,to:schedule.to});
 if(signature===lastAllSignature)return;
 lastAllSignature=signature;
 // The current machine snapshot is kept in a collapsible secondary section.
 const snapshotMap=new Map((current.machines||[]).map(m=>[Number(m.machine_no),m]));
 const rows=Array.isArray(schedule.rows)?schedule.rows:[];
 const byMachine=new Map(order.map(n=>[n,[]]));
 for(const r of rows){
   const n=Number(r.machine_no);
   if(byMachine.has(n))byMachine.get(n).push(r);
 }
 for(const list of byMachine.values()){
   list.sort((a,b)=>String(a.start_at||"").localeCompare(String(b.start_at||""))||
     String(a.sequence||"").localeCompare(String(b.sequence||"")));
 }
 const total=rows.length;
 const meta=$("all-meta");
 if(meta){
   meta.textContent=`ローカル保存スケジュール: ${total} 件 · 全期間表示`+
     (schedule.from&&schedule.to?` · ${schedule.from} ～ ${schedule.to}`:"");
 }
 $("all-machines").innerHTML=order.map(n=>{
   const m=snapshotMap.get(n);
   const entries=byMachine.get(n);
   const details=m?`<details class="snapshot-details"><summary>現在の機械データを表示</summary><dl>${
     [...dataFields,...Object.keys(m).filter(key=>!dataFields.some(([known])=>known===key)).map(key=>[key,key])]
      .map(([key,label])=>`<dt>${esc(label)}</dt><dd>${esc(typeof m[key]==="object"?JSON.stringify(m[key]):m[key]??"—")}</dd>`).join("")
   }</dl></details>`:"";
   const list=entries.length?entries.map((r,i)=>{
     const change=classify(entries[i-1],r);
     const tag=change==="mold"?"型替え":change==="color"?"色替え":"";
     return `<tr>
      <td class="when">${esc(r.start_at||"—")}</td>
      <td class="when">${esc(r.end_at||"—")}</td>
      <td>${esc(r.mold_name||"—")}<div class="product">${esc(r.mold_no||"")}</div></td>
      <td>${esc(r.product_name||"—")}</td>
      <td>${esc(r.color||"—")}</td>
      <td>${esc(r.order_no||"—")}</td>
      <td>${tag?`<span class="tag ${change}">${tag}</span>`:"—"}</td>
     </tr>`;
   }).join(""):"";
   return `<article class="all-card machine-schedule"><header>
     <b class="machine-number">#${n}</b>
     <span class="count">${entries.length} 件</span>
     <span class="status">${m?esc(names[m.status_code]||m.status_code||"不明"):"データなし"}</span>
   </header>
   ${entries.length?`<div class="schedule-table-scroll"><table class="schedule-table">
     <thead><tr><th>開始</th><th>終了</th><th>金型</th><th>品名</th><th>色</th><th>製造番号</th><th>変更</th></tr></thead>
     <tbody>${list}</tbody>
   </table></div>`:'<p class="empty">保存された予定はありません</p>'}
   ${details}</article>`;
 }).join("");
}
const monday=(offset)=>{const d=new Date();d.setHours(0,0,0,0);d.setDate(d.getDate()-(d.getDay()+6)%7+offset*7);return d};
const parseDate=x=>new Date(String(x||"").replace(" ","T")+"+09:00");
function classify(prev,cur){
 if(!prev)return "start";
 const moldKey=m=>String(m.mold_no||m.mold_name||"").replace(/-[A-Za-z0-9]+$/,"").trim();
 if(moldKey(prev)&&moldKey(cur)&&moldKey(prev)!==moldKey(cur))return "mold";
 if(String(prev.color||"")!==String(cur.color||""))return "color";
 return "same";
}
function drawSchedule(){
 const start=monday(week),end=new Date(start.getTime()+7*86400000),rows=Array.isArray(schedule.rows)?schedule.rows:[];
 const label=d=>d.toLocaleDateString("ja-JP",{year:"numeric",month:"2-digit",day:"2-digit"});
 $("schedule-meta").textContent=`${label(start)} – ${label(new Date(end.getTime()-86400000))} · ローカル保存 ${rows.length} 件`;
 const by=new Map(order.map(n=>[n,[]]));
 for(const r of rows)if(by.has(+r.machine_no))by.get(+r.machine_no).push(r);
 for(const entries of by.values())entries.sort((a,b)=>String(a.start_at).localeCompare(String(b.start_at)));
 $("schedule-machines").innerHTML=order.map(n=>{
 const all=by.get(n),entries=all.map((r,i)=>({...r,change:classify(all[i-1],r)})).filter(r=>{
 const a=parseDate(r.start_at),b=r.end_at?parseDate(r.end_at):a;
 return a<end&&b>=start&&(filter==="all"||r.change===filter)
 });
 return `<article class="schedule-card"><header><strong>#${n}</strong><span class="muted">${entries.length} 件</span></header>${entries.length?entries.map(r=>`<div class="schedule-entry"><div class="entry-time">${esc(r.start_at)} → ${esc(r.end_at||"—")}</div><div class="entry-name">${esc(r.mold_name||r.mold_no||"—")}</div><div class="product">${esc(r.product_name||"")}　${esc(r.color||"")}</div>${["color","mold"].includes(r.change)?`<span class="tag ${r.change}">${r.change==="color"?"色替え":"型替え"}</span>`:""}</div>`).join(""):'<div class="empty">この週の保存予定なし</div>'}</article>`;
 }).join("");
}
function show(viewName){view=viewName;for(const v of ["dashboard","schedule","all"])$(v+"-view").hidden=v!==viewName;document.querySelectorAll(".nav").forEach(b=>b.classList.toggle("active",b.dataset.view===viewName));$("page-title").textContent={dashboard:"ダッシュボード",schedule:"予定",all:"全て"}[viewName];if(viewName==="schedule")drawSchedule();if(viewName==="all")drawAll()}
document.querySelectorAll(".nav").forEach(b=>b.onclick=()=>show(b.dataset.view));
$("current-week").onclick=()=>{week=0;weekButtons();drawSchedule()};
$("next-week").onclick=()=>{week=1;weekButtons();drawSchedule()};
function weekButtons(){$("current-week").classList.toggle("selected",week===0);$("next-week").classList.toggle("selected",week===1)}
document.querySelectorAll("[data-filter]").forEach(b=>b.onclick=()=>{filter=b.dataset.filter;document.querySelectorAll("[data-filter]").forEach(x=>x.classList.toggle("selected",x===b));drawSchedule()});
async function refresh(){
 try{
 const [a,b,c]=await Promise.all([fetch("/api/machines",{cache:"no-store"}),fetch("/api/status",{cache:"no-store"}),fetch("/api/schedule",{cache:"no-store"})]);
 if(!a.ok||!b.ok)throw Error("Local API unavailable");
 current=await a.json();const stat=await b.json();
 if(c.ok)schedule=await c.json();
 $("total").textContent=order.length;$("running").textContent=(current.machines||[]).filter(m=>status(m.status_code)==="running").length;
 $("stopped").textContent=(current.machines||[]).filter(m=>status(m.status_code)==="danger").length;
 $("connection").textContent="LOCAL OK";$("server-mode").textContent=stat.mode||"UNKNOWN";
 $("updated").textContent=current.sampled_at?"更新: "+new Date(current.sampled_at).toLocaleString("ja-JP"):"保存データなし";
 if(view==="dashboard")drawDashboard();if(view==="schedule")drawSchedule();if(view==="all")drawAll();
 }catch(e){$("connection").textContent="OFFLINE"}
}
function safeRead(key,otherwise){try{return localStorage.getItem(key)??otherwise}catch{return otherwise}}
function safeStore(key,value){try{localStorage.setItem(key,value)}catch{}}
function setNodeSize(value){
 const v=Math.max(220,Math.min(520,Math.round((Number(value)||300)/10)*10));
 document.documentElement.style.setProperty("--node-width",v+"px");
 $("node-size").value=String(v);$("size-output").textContent=v+"px";
 safeStore("mics-only:node-width",String(v));
}
function setSidebar(collapsed){
 $("app-shell").classList.toggle("menu-collapsed",collapsed);
 const button=$("menu-toggle");
 button.textContent=collapsed?"❮":"❯";
 button.setAttribute("aria-expanded",String(!collapsed));
 button.setAttribute("aria-label",collapsed?"メニューを開く":"メニューを閉じる");
 button.title=collapsed?"メニューを開く":"メニューを閉じる";
 safeStore("mics-only:menu-collapsed",collapsed?"1":"0");
}
setNodeSize(safeRead("mics-only:node-width","300"));
setSidebar(safeRead("mics-only:menu-collapsed",window.innerWidth<=800?"1":"0")==="1");
$("node-size").addEventListener("input",e=>setNodeSize(e.target.value));
$("menu-toggle").addEventListener("click",()=>setSidebar(!$("app-shell").classList.contains("menu-collapsed")));
refresh();setInterval(refresh,3000);if("serviceWorker"in navigator)navigator.serviceWorker.register("/sw.js").catch(()=>{});
