import {computed} from './vanilla-runtime.js';
// Target cycle remains independent of the live production forecast.
export function cycleGauge(actual,target){
  const valid=v=>v!==null&&v!==''&&Number.isFinite(Number(v))&&Number(v)>0;
  const a=valid(actual)?Number(actual):null, t=valid(target)?Number(target):null;
  if(t===null)return {actual:a,target:t,angle:null,state:'unknown',color:'#64748b',min:null,max:null,delta:null};
  const min=Math.max(0,t-20),max=t+20;
  const delta=a===null?null:a-t;
  const angle=delta===null?null:Math.max(-90,Math.min(90,delta<0?90*delta/(t-min):90*delta/20));
  const state=delta===null?'unknown':delta<=0?'normal':delta<=10?'warning':'over';
  const color={normal:'#16a34a',warning:'#eab308',over:'#dc2626',unknown:'#64748b'}[state];
  return {actual:a,target:t,angle,state,color,min,max,delta};
}
export const CycleGauge={
  props:['actual','target'],
  setup(props){return {g:computed(()=>cycleGauge(props.actual,props.target)),fmt:v=>v===null?'—':v.toFixed(1)}},
  template:`
  <div class="cycle-gauge">
    <div class="mics-cycle-title"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true"><circle cx="12" cy="13" r="8"/><path d="M12 9v4l3 2M9 2h6m-3 0v3m6 1 2-2"/></svg>サイクルタイム</div>
    <svg v-if="g.target!==null" viewBox="0 -7 150 90" class="mics-cycle-dial" role="img" aria-label="サイクルタイム">
      <path d="M 19 66 A 56 56 0 0 1 75 10" fill="none" stroke="#079c46" stroke-width="9"/>
      <path d="M 75 10 A 56 56 0 0 1 114.6 26.4" fill="none" stroke="#f0ab12" stroke-width="9"/>
      <path d="M 114.6 26.4 A 56 56 0 0 1 131 66" fill="none" stroke="#dc222c" stroke-width="9"/>
      <path d="M 19 66 A 56 56 0 0 1 131 66" fill="none" stroke="#fff" stroke-opacity=".18" stroke-width="1" transform="translate(0 -3)"/>
      <path d="M75 5 V15" stroke="#334155" stroke-width="1.3"/>
      <text x="75" y="3" font-size="8.5" fill="#253746" text-anchor="middle">{{ g.target }}</text>
      <text x="13" y="79" font-size="9" fill="#253746" text-anchor="middle">{{ g.min }}</text>
      <text x="124" y="20" font-size="9" fill="#253746" text-anchor="middle">{{ g.target+10 }}</text>
      <text x="137" y="79" font-size="9" fill="#253746" text-anchor="middle">{{ g.max }}</text>
      <g v-if="g.angle!==null" :transform="'rotate('+g.angle+' 75 66)'">
        <path d="M75 16 L81 65 Q82 74 75 75 Q68 74 69 65Z" fill="#243541" stroke="#fff" stroke-width="1.1" style="filter:drop-shadow(1px 3px 2px #122d4966)"/>
        <path d="M75 16 L75 73 Q69 73 70.2 65Z" fill="#a8b8c5"/>
        <path d="M75 17 L77.5 64 Q78 69 75 71Z" fill="#455967"/>
        <path d="M74.5 21 L70.7 65" stroke="#f8fafc" stroke-opacity=".8" stroke-width=".8"/>
      </g>
    </svg>
    <div v-else class="mics-cycle-missing">指定値なし</div>
    <div class="mics-cycle-actual" :style="{color:g.color}">実績 {{ fmt(g.actual) }}s</div>
    <div class="mics-cycle-target">指定 {{ fmt(g.target) }}s</div>
  </div>`
};
