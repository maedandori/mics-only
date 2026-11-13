"use strict";
const order=[15,8,19,14,5,17,13,6,10,11,18,7,12,16];
const el=id=>document.getElementById(id);
const esc=x=>String(x??"").replace(/[&<>"']/g,c=>({"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;","'":"&#39;"}[c]));
const num=x=>Number.isFinite(Number(x))?Number(x):0;
const fmt=x=>num(x).toLocaleString("ja-JP");
const field=(m,keys)=>{for(const k of keys){if(m[k]!==null&&m[k]!==undefined&&m[k]!=="")return m[k]}return null};
const label={100:"段取り中",200:"稼働中",210:"サイクル注意",410:"金型異常",440:"色替え",490:"異常"};
function kind(code){const c=String(code??"");return ["410","440","490"].includes(c)?"danger":["100","210"].includes(c)?"warning":c==="200"?"running":""}
function metric(name,value,fire=false){
 const v=fmt(value);
 return '<div><dt>'+name+'</dt><dd class="'+(fire?"ng-fire":"")+'"><span class="'+(fire?"ng-value":"")+'"'+(fire?' data-flame-number="'+esc(v)+'"':"")+'>'+esc(v)+'</span></dd></div>';
}
function card(n,m,has){
 const state=kind(m.status_code),plan=num(field(m,["qty_plan","planned_qty"])),good=num(field(m,["qty_ok","good_qty"])),bad=num(field(m,["qty_ng","defect_qty"]));
 const remaining=Math.max(0,num(field(m,["qty_remaining","remaining_qty"])??(plan-good)));
 const pct=plan>0?Math.max(0,Math.min(100,100*good/plan)):0;
 const mold=field(m,["mold_name","tool_name"])||"—",code=field(m,["mold_no","tool_no","mold_code"])||"—";
 const staff=field(m,["staff_name","staff","operator_name"]);
 const cycle=field(m,["cycle_seconds","actual_cycle","cycle"]);
 const pick=field(m,["pick_qty","real_pick_qty"])??"—";
 const statusName=has?(label[m.status_code]||m.status_name||m.status_code||"不明"):"データなし";
 return '<article class="node '+state+'">'+
 '<div class="node-top"><span class="machine-number">#'+n+'</span><span class="staff">'+(staff?"担当者: "+esc(staff):"")+'</span></div>'+
 '<div class="mold">'+esc(mold)+'</div><div class="mold-code">金型番号: '+esc(code)+'</div>'+
 '<div class="product">'+esc(field(m,["product_name","product"])||"—")+'</div>'+
 '<div class="main-metrics"><div class="cycle">サイクル: <strong>'+esc(cycle??"—")+'</strong><div class="small-detail">取り数: '+esc(pick)+'</div></div>'+
 '<div class="progress" role="progressbar" aria-label="生産進捗" aria-valuemin="0" aria-valuemax="100" aria-valuenow="'+pct.toFixed(1)+'">'+
 '<svg width="102" height="102" viewBox="0 0 102 102" aria-hidden="true"><circle class="track" cx="51" cy="51" r="42"/><circle class="arc" cx="51" cy="51" r="42" pathLength="100" stroke-dasharray="'+pct.toFixed(1)+' 100"/></svg>'+
 '<span>'+pct.toFixed(0)+'%</span><small>生産進捗</small></div></div>'+
 '<dl class="quantities">'+metric("予定数",plan)+metric("良品数",good)+metric("不良数",bad,bad>5)+metric("残数",remaining)+'</dl>'+
 '<span class="status">'+esc(statusName)+'</span><span class="status-code">'+esc(m.status_code??"")+'</span></article>';
}
function render(data,status){
 const machines=Array.isArray(data.machines)?data.machines:[];
 const map=new Map(machines.map(m=>[Number(m.machine_no),m]));
 el("machines").innerHTML=order.map(n=>card(n,map.get(n)||{},map.has(n))).join("");
 el("total").textContent=machines.length||order.length;
 el("running").textContent=machines.filter(m=>kind(m.status_code)==="running").length;
 el("stopped").textContent=machines.filter(m=>kind(m.status_code)==="danger").length;
 const t=data.sampled_at?new Date(data.sampled_at):null;
 const stamp=t&&!isNaN(t)?"更新: "+t.toLocaleString("ja-JP"):"保存データなし";
 el("updated").textContent=stamp;el("menu-updated").textContent=stamp;
 const stale=t&&Date.now()-t.getTime()>120000&&status.mode!=="ACTIVE";
 el("banner").textContent=!machines.length?"保存されたデータはありません。Agent が ACTIVE になると更新されます。":stale?"注意: 最終データは2分以上前です。表示は保存された情報です。":"";
}
function stored(k,d){try{return localStorage.getItem(k)??d}catch{return d}}
function save(k,v){try{localStorage.setItem(k,v)}catch{}}
const slider=el("node-size");
function setSize(raw){
 const w=Math.max(220,Math.min(520,Math.round((Number(raw)||300)/10)*10));
 document.documentElement.style.setProperty("--node-width",w+"px");
 slider.value=String(w);el("size-output").textContent=w+"px";save("mics-only:node-width",String(w));
}
function setMenu(hidden){
 el("app-shell").classList.toggle("menu-collapsed",hidden);
 const b=el("menu-toggle");b.textContent=hidden?"❮":"❯";
 b.setAttribute("aria-expanded",String(!hidden));
 b.setAttribute("aria-label",hidden?"メニューを開く":"メニューを閉じる");
 b.title=hidden?"メニューを開く":"メニューを閉じる";save("mics-only:menu-collapsed",hidden?"1":"0");
}
setSize(stored("mics-only:node-width","300"));
setMenu(stored("mics-only:menu-collapsed",window.innerWidth<=650?"1":"0")==="1");
slider.addEventListener("input",e=>setSize(e.target.value));
el("menu-toggle").addEventListener("click",()=>setMenu(!el("app-shell").classList.contains("menu-collapsed")));
el("reset-size").addEventListener("click",()=>setSize(300));
async function refresh(){
 try{
  const [data,status]=await Promise.all([
   fetch("/api/machines",{cache:"no-store"}).then(r=>{if(!r.ok)throw Error(r.status);return r.json()}),
   fetch("/api/status",{cache:"no-store"}).then(r=>{if(!r.ok)throw Error(r.status);return r.json()})
  ]);
  render(data,status);el("server-mode").textContent=status.mode||"UNKNOWN";el("connection").textContent="LOCAL OK";
 }catch(e){el("connection").textContent="OFFLINE";el("banner").textContent="ローカルサーバーに接続できません。最後の画面を表示しています。"}
}
refresh();setInterval(refresh,3000);
if("serviceWorker" in navigator)navigator.serviceWorker.register("/sw.js").catch(()=>{});
