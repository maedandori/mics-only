<script setup>
import {computed,onMounted,onUnmounted,ref,watch} from 'vue'
const order=[15,8,19,14,5,17,13,6,10,11,18,7,12,16]
const names={100:'段取り中',200:'稼働中',210:'サイクル注意',410:'金型異常',440:'色替え',490:'異常'}
const current=ref({machines:[]}),schedule=ref({rows:[]}),server=ref({mode:'STANDBY'})
const connected=ref(false),view=ref('dashboard'),week=ref(0),filter=ref('all'),menu=ref(false)
const fullscreen=ref(false),nodeWidth=ref(300),openRows=ref({})
const num=n=>(Number(n)||0).toLocaleString('ja-JP')
const status=m=>{
 const code=String(m?.status_code??''),label=String(m?.status_name||'')
 if(['410','440','490','300','0'].includes(code)||/停止|異常|クレーム/.test(label))return 'danger'
 if(code==='100'||/段取り/.test(label))return 'setup'
 if(code==='210'||/注意/.test(label))return 'warning'
 if(code==='200'||/稼働中/.test(label))return 'running'
 return ''
}
const machineMap=computed(()=>new Map((current.value.machines||[]).map(m=>[Number(m.machine_no),m])))
const machineCards=computed(()=>order.map(n=>{
 const m=machineMap.value.get(n)||{}
 const planned=Math.max(0,Number(m.qty_plan)||0),good=Math.max(0,Number(m.qty_ok)||0)
 const bad=Math.max(0,Number(m.qty_ng)||0)
 const remaining=Math.max(0,m.qty_remaining!=null&&Number.isFinite(Number(m.qty_remaining))?Number(m.qty_remaining):planned-good)
 return {n,m,has:machineMap.value.has(n),planned,good,bad,remaining,pct:planned?Math.max(0,Math.min(100,good/planned*100)):0,pred:prediction(m)}
}))
const warnings=computed(()=>machineCards.value.filter(c=>c.bad>20))
const running=computed(()=>machineCards.value.filter(c=>status(c.m)==='running').length)
const stopped=computed(()=>machineCards.value.filter(c=>status(c.m)==='danger').length)
function classify(prev,cur){
 if(!prev)return 'start'
 const mold=m=>String(m?.mold_no||m?.mold_name||'').replace(/-[A-Za-z0-9]+$/,'').trim()
 if(mold(prev)&&mold(cur)&&mold(prev)!==mold(cur))return 'mold'
 if(String(prev.color||'')!==String(cur.color||''))return 'color'
 return 'same'
}
const grouped=computed(()=>{
 const map=new Map(order.map(n=>[n,[]]))
 for(const row of (schedule.value.rows||[])){
  const n=Number(row.machine_no);if(map.has(n))map.get(n).push(row)
 }
 for(const rows of map.values())rows.sort((a,b)=>String(a.start_at||'').localeCompare(String(b.start_at||''))||String(a.sequence||'').localeCompare(String(b.sequence||'')))
 return map
})
function prediction(m){
 const same=(schedule.value.rows||[]).filter(r=>Number(r.machine_no)===Number(m.machine_no)).sort((a,b)=>String(a.start_at||'').localeCompare(String(b.start_at||'')))
 let i=m.order_no?same.findIndex(r=>String(r.order_no||'').trim()===String(m.order_no).trim()):-1
 if(i<0&&m.mold_no)i=same.findIndex(r=>String(r.mold_no||'').trim()===String(m.mold_no).trim())
 if(i<0)return {label:'—',text:'予定を確認'}
 let next,type
 for(let j=i+1;j<same.length;j++){const t=classify(same[j-1],same[j]);if(t==='color'||t==='mold'){next=same[j];type=t;break}}
 if(!next)return {label:'—',text:'変更予定なし'}
 const label=type==='mold'?'型替え':'色替え'
 const age=Date.parse(current.value.sampled_at||'')
 const c=Number(m.cycle_seconds),pick=Number(m.pick_qty),remaining=Number(m.qty_remaining)
 if(!Number.isFinite(age)||Math.abs(Date.now()-age)>600000||!['200','210'].includes(String(m.status_code))||!(c>0)||!(pick>0)||!(remaining>=0))return {type,label,text:'予測不可'}
 const sec=Math.ceil(remaining/pick)*c
 if(!Number.isFinite(sec)||sec>7776000)return {type,label,text:'予測不可'}
 return {type,label,text:new Intl.DateTimeFormat('ja-JP',{timeZone:'Asia/Tokyo',month:'2-digit',day:'2-digit',hour:'2-digit',minute:'2-digit',hourCycle:'h23'}).format(new Date(Date.now()+sec*1000))}
}
function mondayInJst(offset){
 const parts=new Intl.DateTimeFormat('en-CA',{timeZone:'Asia/Tokyo',year:'numeric',month:'2-digit',day:'2-digit'}).format(new Date())
 const d=new Date(parts+'T00:00:00+09:00')
 d.setUTCDate(d.getUTCDate()-(d.getUTCDay()+6)%7+offset*7)
 return d
}
const weekStart=computed(()=>mondayInJst(week.value))
const weekEnd=computed(()=>new Date(weekStart.value.getTime()+604800000))
const shortDate=d=>new Intl.DateTimeFormat('ja-JP',{timeZone:'Asia/Tokyo',year:'numeric',month:'2-digit',day:'2-digit'}).format(d)
const weekLabel=computed(()=>shortDate(weekStart.value)+' – '+shortDate(new Date(weekEnd.value.getTime()-86400000)))
function parseJst(v){
 const s=String(v||'').trim()
 if(!s)return new Date(NaN)
 if(/[zZ]$|[+-]\d\d:\d\d$/.test(s))return new Date(s)
 return new Date(s.replace(' ','T')+'+09:00')
}
function rowsFor(n,all=false){
 const rows=grouped.value.get(n)||[]
 if(all)return rows.map((r,i)=>({...r,change:classify(rows[i-1],r),rid:n+'-'+i}))
 return rows.map((r,i)=>({...r,change:classify(rows[i-1],r)})).filter(r=>{
  const a=parseJst(r.start_at),b=r.end_at?parseJst(r.end_at):a
  return a<weekEnd.value&&b>=weekStart.value&&(filter.value==='all'||r.change===filter.value)
 })
}
const dataFields=[['machine_no','機械番号'],['status_code','ステータス'],['mold_name','金型名'],['mold_no','金型番号'],['order_no','製造番号'],['product_name','品名'],['qty_plan','予定数'],['qty_ok','良品数'],['qty_ng','不良数'],['qty_remaining','残数'],['cycle_seconds','サイクル'],['pick_qty','取り数'],['staff_name','担当者'],['scheduled_end_at','完了予定']]
function allFields(m){const keys=new Set(dataFields.map(([k])=>k));return [...dataFields,...Object.keys(m||{}).filter(k=>!keys.has(k)).map(k=>[k,k])]}
const fmt=v=>v==null?'—':typeof v==='object'?JSON.stringify(v):String(v)
function toggleRow(key){openRows.value={...openRows.value,[key]:!openRows.value[key]}}
let timer=null,busy=false
async function refresh(){
 if(busy)return;busy=true
 try{
  const [a,b,c]=await Promise.all(['/api/machines','/api/status','/api/schedule'].map(p=>fetch(p,{cache:'no-store'})))
  if(!a.ok||!b.ok)throw Error('Local API unavailable')
  current.value=await a.json();server.value=await b.json()
  if(c.ok)schedule.value=await c.json()
  connected.value=true
 }catch{connected.value=false}
 finally{busy=false}
}
function saveWidth(v){
 nodeWidth.value=Math.max(220,Math.min(520,Math.round(Number(v)/10)*10))
 document.documentElement.style.setProperty('--node-width',nodeWidth.value+'px')
 try{localStorage.setItem('mics-only:node-width',String(nodeWidth.value))}catch{}
}
function toggleMenu(){
 menu.value=!menu.value
 try{localStorage.setItem('mics-only:menu-collapsed',menu.value?'0':'1')}catch{}
}
function chooseView(name){view.value=name;menu.value=false}
async function toggleFullscreen(){
 try{if(document.fullscreenElement)await document.exitFullscreen();else await document.documentElement.requestFullscreen()}catch{}
 fullscreen.value=!!document.fullscreenElement
}
function onFullscreen(){fullscreen.value=!!document.fullscreenElement}
onMounted(()=>{
 try{saveWidth(localStorage.getItem('mics-only:node-width')||300);menu.value=localStorage.getItem('mics-only:menu-collapsed')==='0'}catch{saveWidth(300)}
 document.addEventListener('fullscreenchange',onFullscreen)
 refresh();timer=setInterval(refresh,3000)
 if('serviceWorker' in navigator)navigator.serviceWorker.register('/sw.js').catch(()=>{})
})
onUnmounted(()=>{clearInterval(timer);document.removeEventListener('fullscreenchange',onFullscreen)})
</script>

<template>
<div class="layout" :class="{'menu-collapsed':!menu}">
 <main>
  <div class="app-top-layout"><div class="top-command-area">
   <div class="top-command-header">
    <button type="button" class="menu-toggle" :aria-expanded="menu" @click="toggleMenu">{{menu?'✕ 閉じる':'☰ メニュー'}}</button>
    <nav v-if="menu" class="top-command-nav" aria-label="Main menu">
     <button v-for="item in [{id:'dashboard',label:'◉ ダッシュボード'},{id:'schedule',label:'▦ 予定'},{id:'all',label:'☷ 全て'}]" :key="item.id" class="nav" :class="{active:view===item.id}" @click="chooseView(item.id)">{{item.label}}</button>
    </nav>
   </div>
   <button type="button" class="fullscreen-toggle" @click="toggleFullscreen">{{fullscreen?'⤢ 全画面終了':'⛶ 全画面'}}</button>
  </div></div>
  <div v-if="warnings.length" class="ng-alerts" role="status" aria-live="polite">
   <div v-for="c in warnings" :key="c.n" class="ng-warning-line"><strong>{{c.n}}号機：不良数 {{num(c.bad)}}個</strong><span>不良数が20個を超えています。現場で不良原因を確認してください。</span></div>
  </div>
  <section v-if="view==='dashboard'" class="view"><div class="machines">
   <article v-for="c in machineCards" :key="c.n" class="node" :class="status(c.m)">
    <div class="node-top"><span class="machine-number">{{c.n}}号機</span><span class="machine-status-top">{{c.has?(names[c.m.status_code]||c.m.status_name||c.m.status_code||'不明'):'データなし'}}</span><span class="staff">{{c.m.staff_name||c.m.operator_name?'担当者: '+(c.m.staff_name||c.m.operator_name):''}}</span></div>
    <div class="mold">{{c.m.mold_name||c.m.mold_no||'—'}}</div>
    <div class="mold-code">金型番号: {{c.m.mold_no||'—'}}</div>
    <div class="product">{{c.m.product_name||'—'}}</div>
    <div class="main-metrics"><div class="cycle">サイクル: <strong>{{c.m.cycle_seconds??'—'}}</strong><div class="pick">取り数: {{c.m.pick_qty??'—'}}</div></div>
     <div class="progress" role="progressbar" aria-label="生産進捗" aria-valuemin="0" aria-valuemax="100" :aria-valuenow="c.pct.toFixed(1)">
      <svg width="102" height="102" viewBox="0 0 102 102" aria-hidden="true"><circle class="track" cx="51" cy="51" r="42"/><circle class="arc" cx="51" cy="51" r="42" pathLength="100" :stroke-dasharray="c.pct.toFixed(1)+' 100'"/></svg>
      <b>{{c.pct.toFixed(0)}}%</b><small>生産進捗</small>
     </div>
    </div>
    <dl class="quantities">
     <div><dt>予定数</dt><dd>{{num(c.planned)}}</dd></div>
     <div><dt>良品数</dt><dd>{{num(c.good)}}</dd></div>
     <div><dt>不良数</dt><dd :class="{'ng-fire':c.bad>5}"><span :class="{'ng-value':c.bad>5}" :data-flame-number="c.bad>5?num(c.bad):undefined">{{num(c.bad)}}</span></dd></div>
     <div><dt>残数</dt><dd>{{num(c.remaining)}}</dd></div>
    </dl>
    <div class="next-change" :class="c.pred.type||'unknown'"><span>次の変更: <strong>{{c.pred.label}}</strong></span><span>変更予測: <strong>{{c.pred.text}}</strong></span></div>
   </article>
  </div></section>
  <section v-else-if="view==='schedule'" class="view">
   <div class="toolbar"><div class="week-controls"><button class="pill" :class="{selected:week===0}" @click="week=0">今週</button><button class="pill" :class="{selected:week===1}" @click="week=1">来週</button></div>
    <div class="filters"><button v-for="f in [{id:'all',label:'全て'},{id:'color',label:'色替え'},{id:'mold',label:'型替え'}]" :key="f.id" class="pill" :class="{selected:filter===f.id}" @click="filter=f.id">{{f.label}}</button></div>
   </div>
   <p class="muted">{{weekLabel}} · ローカル保存 {{(schedule.rows||[]).length}} 件</p>
   <div class="schedule-grid"><article v-for="n in order" :key="n" class="schedule-card">
    <header><strong>{{n}}号機</strong><span class="muted">{{rowsFor(n).length}} 件</span></header>
    <template v-if="rowsFor(n).length"><div v-for="(r,i) in rowsFor(n)" :key="i" class="schedule-entry">
     <div class="entry-time">{{r.start_at||'—'}} → {{r.end_at||'—'}}</div><div class="entry-name">{{r.mold_name||r.mold_no||'—'}}</div><div class="product">{{r.product_name||''}}　{{r.color||''}}</div>
     <span v-if="['color','mold'].includes(r.change)" class="tag" :class="r.change">{{r.change==='color'?'色替え':'型替え'}}</span>
    </div></template><div v-else class="empty">この週の保存予定なし</div>
   </article></div>
  </section>
  <section v-else class="view">
   <p class="muted">ローカル保存スケジュール: {{(schedule.rows||[]).length}} 件 · 全期間表示 <span v-if="schedule.from&&schedule.to">· {{schedule.from}} ～ {{schedule.to}}</span></p>
   <div class="all-grid"><article v-for="n in order" :key="n" class="all-card machine-schedule">
    <header><b class="machine-number">{{n}}号機</b><span class="count">{{rowsFor(n,true).length}} 件</span><span class="status">{{machineMap.has(n)?(names[machineMap.get(n).status_code]||machineMap.get(n).status_code||'不明'):'データなし'}}</span></header>
    <div v-if="rowsFor(n,true).length" class="schedule-table-scroll"><table class="schedule-table">
     <thead><tr><th>開始</th><th>終了</th><th>金型</th><th>品名</th><th>色</th><th>製造番号</th><th>変更</th></tr></thead>
     <tbody><template v-for="r in rowsFor(n,true)" :key="r.rid"><tr class="all-schedule-row" tabindex="0" role="button" :aria-expanded="!!openRows[r.rid]" @click="toggleRow(r.rid)" @keydown.enter.prevent="toggleRow(r.rid)" @keydown.space.prevent="toggleRow(r.rid)">
      <td class="when">{{r.start_at||'—'}}</td><td class="when">{{r.end_at||'—'}}</td>
      <td>{{r.mold_name||'—'}}<div class="product">{{r.mold_no||''}}</div></td><td>{{r.product_name||'—'}}</td><td>{{r.color||'—'}}</td><td>{{r.order_no||'—'}}</td>
      <td><span v-if="['color','mold'].includes(r.change)" class="tag" :class="r.change">{{r.change==='color'?'色替え':'型替え'}}</span><span v-else>—</span></td>
     </tr><tr v-if="openRows[r.rid]" class="all-schedule-detail"><td colspan="7"><dl class="all-row-fields"><template v-for="(value,key) in r" :key="key"><dt>{{key}}</dt><dd>{{fmt(value)}}</dd></template></dl></td></tr></template></tbody>
    </table></div><p v-else class="empty">保存された予定はありません</p>
    <details v-if="machineMap.has(n)" class="snapshot-details"><summary>現在の機械データを表示</summary><dl><template v-for="[key,label] in allFields(machineMap.get(n))" :key="key"><dt>{{label}}</dt><dd>{{fmt(machineMap.get(n)[key])}}</dd></template></dl></details>
   </article></div>
  </section>
  <footer class="page-footer" aria-label="生産サマリー">
   <section class="summary"><div class="stat"><span>機械</span><b>14</b></div><div class="stat"><span>稼働中</span><b>{{running}}</b></div><div class="stat"><span>停止 / 異常</span><b>{{stopped}}</b></div><div class="stat"><span>通信</span><b>{{connected?'LOCAL OK':'OFFLINE'}}</b></div><div class="stat server"><b>{{server.mode||'UNKNOWN'}}</b><span>{{current.sampled_at?'更新: '+new Date(current.sampled_at).toLocaleString('ja-JP'):'保存データなし'}}</span></div></section>
   <div class="footer-settings"><div class="size-dock"><label for="node-size">ノードサイズ</label><span>小</span><input id="node-size" type="range" min="220" max="520" step="10" :value="nodeWidth" @input="saveWidth($event.target.value)"><span>大</span><output>{{nodeWidth}}px</output></div></div>
   <small class="footer-note">127.0.0.1:8787 · ローカル専用 · Vue 3</small>
  </footer>
 </main>
</div>
</template>
