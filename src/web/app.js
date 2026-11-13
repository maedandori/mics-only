"use strict";
const order=[15,8,19,14,5,17,13,6,10,11,18,7,12,16];
const el=id=>document.getElementById(id);
const escapeHtml=s=>String(s??"").replace(/[&<>"']/g,c=>({"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;","'":"&#39;"}[c]));
function kind(code){code=String(code??"");if(["410","440","490"].includes(code))return "danger";if(["100","210"].includes(code))return "warning";if(["200"].includes(code))return "running";return "";}
const label={100:"段取り中",200:"稼働中",210:"サイクル注意",410:"金型異常",440:"色替え",490:"異常"};
function render(data){
  const machines=Array.isArray(data.machines)?data.machines:[];
  const map=new Map(machines.map(m=>[Number(m.machine_no),m]));
  el("running").textContent=machines.filter(m=>kind(m.status_code)==="running").length;
  el("stopped").textContent=machines.filter(m=>kind(m.status_code)==="danger").length;
  el("machines").innerHTML=order.map(n=>{
    const m=map.get(n)||{},has=map.has(n),state=kind(m.status_code);
    const plan=Number(m.qty_plan)||0,good=Number(m.qty_ok)||0,bad=Number(m.qty_ng)||0;
    const progress=plan?Math.min(100,Math.max(0,good/plan*100)):0;
    return `<article class="node ${state}"><header><span class="machine-number">#${n}</span><span class="status">${has?escapeHtml(label[m.status_code]||m.status_code||"不明"):"データなし"}</span></header>
      <div class="mold">${escapeHtml(m.mold_name||m.mold_no||"—")}</div>
      <div class="product">${escapeHtml(m.product_name||"—")}</div>
      <div class="bar"><div class="fill" style="width:${progress.toFixed(1)}%"></div></div>
      <div class="numbers"><span>良品 <b>${good.toLocaleString()}</b></span><span>予定 <b>${plan.toLocaleString()}</b></span></div>
      <div class="meta"><span>不良: ${bad.toLocaleString()}</span><span>${progress.toFixed(1)}%</span></div>
      <div class="meta"><span>Cycle: ${escapeHtml(m.cycle_seconds??"—")}</span><span>取り数: ${escapeHtml(m.pick_qty??"—")}</span></div></article>`;
  }).join("");
  const stamp=data.sampled_at?new Date(data.sampled_at):null;
  el("updated").textContent=stamp&&!isNaN(stamp)?"更新: "+stamp.toLocaleString("ja-JP"):"保存データなし";
  const stale=stamp&&Date.now()-stamp.getTime()>120000;
  el("banner").textContent=!machines.length?"保存されたデータはありません。Agent が ACTIVE になると更新されます。":stale?"注意: 最終データは2分以上前です。表示は保存された情報です。":"";
}
async function refresh(){
  try{
    const [snapshot,status]=await Promise.all([
      fetch("/api/machines",{cache:"no-store"}).then(r=>{if(!r.ok)throw Error(r.status);return r.json()}),
      fetch("/api/status",{cache:"no-store"}).then(r=>r.json())]);
    render(snapshot);
    el("server-mode").textContent=status.mode||"UNKNOWN";
    el("connection").textContent="LOCAL OK";
  }catch(error){
    el("connection").textContent="OFFLINE";
    el("banner").textContent="ローカルサーバーに接続できません。最後の画面を表示しています。";
  }
}
refresh();setInterval(refresh,3000);
if("serviceWorker" in navigator)navigator.serviceWorker.register("/sw.js").catch(()=>{});
