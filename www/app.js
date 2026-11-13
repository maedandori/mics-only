"use strict";
const order=[15,8,19,14,5,17,13,6,10,11,18,7,12,16],names={100:"段取り中",200:"稼働中",210:"サイクル注意",410:"金型異常",440:"色替え",490:"異常"};
const $=id=>document.getElementById(id), esc=x=>String(x??"").replace(/[&<>"']/g,c=>({"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;","'":"&#39;"}[c]));
let current={machines:[]},schedule={rows:[]},week=0,filter="all",view="dashboard";
function status(code){const s=String(code??"");return["410","440","490"].includes(s)?"danger":["100","210"].includes(s)?"warning":s==="200"?"running":""}
function num(n){return(Number(n)||0).toLocaleString("ja-JP")}
function drawDashboard(){
 const map=new Map((current.machines||[]).map(m=>[+m.machine_no,m]));
 $("machines").innerHTML=order.map(n=>{
 const m=map.get(n)||{},has=map.has(n),planned=+m.qty_plan||0,good=+m.qty_ok||0,pct=planned?Math.min(100,100*good/planned):0;
 return `<article class="node ${status(m.status_code)}"><header><span class="machine-number">#${n}</span><span class="status">${has?esc(names[m.status_code]||m.status_code||"不明"):"データなし"}</span></header><div class="mold">${esc(m.mold_name||m.mold_no||"—")}</div><div class="product">${esc(m.product_name||"—")}</div><div class="bar"><div class="fill" style="width:${pct.toFixed(1)}%"></div></div><div class="numbers"><span>良品 <b>${num(good)}</b></span><span>予定 <b>${num(planned)}</b></span></div><div class="meta"><span>不良: ${num(m.qty_ng)}</span><span>${pct.toFixed(1)}%</span></div><div class="meta"><span>Cycle: ${esc(m.cycle_seconds??"—")}</span><span>取り数: ${esc(m.pick_qty??"—")}</span></div></article>`;
 }).join("");
}
const dataFields=[["machine_no","機械番号"],["status_code","ステータス"],["mold_name","金型名"],["mold_no","金型番号"],["order_no","製造番号"],["product_name","品名"],["qty_plan","予定数"],["qty_ok","良品数"],["qty_ng","不良数"],["qty_remaining","残数"],["cycle_seconds","実績サイクル"],["pick_qty","取り数"],["staff_name","担当者"],["scheduled_end_at","完了予定"]];
function drawAll(){
 // ALL shows every stored schedule row per machine, across all available dates.
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
refresh();setInterval(refresh,3000);if("serviceWorker"in navigator)navigator.serviceWorker.register("/sw.js").catch(()=>{});
