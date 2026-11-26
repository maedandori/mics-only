import {readFileSync,writeFileSync} from 'node:fs';
import {resolve} from 'node:path';
const icons={
 clock:'<circle cx="12" cy="13" r="8"/><path d="M12 9v4l3 2M9 2h6m-3 0v3m6 1 2-2"/>',
 plan:'<rect x="3" y="5" width="18" height="16" rx="2"/><path d="M7 3v4m10-4v4M3 10h18m-13 4h1m3 0h1m3 0h1m-9 3h1m3 0h1"/>',
 ok:'<circle cx="12" cy="12" r="9"/><path d="m7.5 12 3 3 6-6"/>',
 ng:'<path d="M10.3 4.5 2.8 18a2 2 0 0 0 1.8 3h14.8a2 2 0 0 0 1.8-3L13.7 4.5a2 2 0 0 0-3.4 0ZM12 9v4m0 4h.01"/>',
 remaining:'<path d="m12 3 9 5-9 5-9-5 9-5Zm-9 9 9 5 9-5M3 16l9 5 9-5"/>',
 change:'<path d="M3 10a9 9 0 0 1 15.4-5.7L21 7m0-5v5h-5M21 14a9 9 0 0 1-15.4 5.7L3 17m0 5v-5h5"/>',
 photo:'<path d="M3 7h4l2-3h6l2 3h4a1 1 0 0 1 1 1v11a1 1 0 0 1-1 1H3a1 1 0 0 1-1-1V8a1 1 0 0 1 1-1Z"/><circle cx="12" cy="13" r="4"/>',
 note:'<rect x="5" y="5" width="14" height="17" rx="2"/><rect x="9" y="2" width="6" height="5" rx="1"/><path d="M8 11h8m-8 4h8m-8 4h6"/>'
};
function icon(name){return '<svg class="mics-pastel-icon icon-'+name+'" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">'+icons[name]+'</svg>'}

export function patchPastelLocal(root){
 const path=resolve(root,'js/dashboard-app.js');
 let s=readFileSync(path,'utf8');
 const marker='v-for="m in filteredMachines"';
 const at=s.indexOf(marker),start=s.lastIndexOf('<article',at),end=s.indexOf('</article>',at)+10;
 if(at<0||start<0||end<=at)throw Error('Local machine article missing');
 const old=s.slice(start,end);
 const headEnd=old.indexOf('>',old.indexOf('@click="openMachine(m.machine_no)"'))+1;
 if(headEnd<=0||!old.includes('mics-node-anim-blink'))throw Error('Local machine header mismatch');
 let opening=old.slice(0,headEnd).replace('class="card ','class="card mics-pastel-node ').replace(':data-machine="m.machine_no"',':data-machine="m.machine_no" :data-status="String(m.status_code)"');
 if(!opening.includes('mics-pastel-node')||!opening.includes(':data-status'))throw Error('Local card anchors');
 const nextAt=old.indexOf('<div class="sec-border border-t pt-1.5 mt-1.5 mics-next-compact">');
 if(nextAt<0)throw Error('Local next change not found');
 const next=old.slice(nextAt,old.lastIndexOf('</article>')).replace('mics-next-compact','mics-next-compact mics-pastel-next');
 const metric='metricsByMachine[m.machine_no]';
 const body="\n  <header class=\"mics-pastel-header\">\n    <div class=\"mics-pastel-machine\">{{ m.machine_no }}<span class=\"mno-unit\">号機</span></div>\n    <div class=\"mics-pastel-status-wrap\"><status-badge class=\"mics-pastel-status\" :code=\"m.status_code\" :cfg=\"state.cfg\"></status-badge></div>\n    <div class=\"mics-pastel-operator\"><small>担当者</small><b>{{ m.staff_name || '—' }}</b></div>\n  </header>\n  <div class=\"mics-pastel-identity\">\n    <div class=\"mics-pastel-product-info\">\n      <div class=\"mics-pastel-kicker\">金型／製品</div>\n      <div class=\"mics-pastel-mold mold-title\" :title=\"m.mold_name\"><span v-if=\"m.mold_name\">{{ m.mold_name }}</span><i v-else>(金型名なし)</i></div>\n      <div class=\"mics-pastel-product prod-title\" :title=\"m.product_name\">{{ m.product_name || '—' }}</div>\n      <div class=\"mics-pastel-product-meta\">\n        <span class=\"mics-pastel-color color-tag\" :style=\"{backgroundColor: productColorStyle(METRIC.curColor,state.aliasCfg).bg,color: productColorStyle(METRIC.curColor,state.aliasCfg).text,borderColor: productColorStyle(METRIC.curColor,state.aliasCfg).border}\">色: <b>{{ METRIC.curColor || '—' }}</b></span>\n        <span>{{ m.mold_no || '—' }}</span><span>{{ m.pick_qty ?? '—' }}個取</span>\n      </div>\n    </div>\n    <donut-chart :pct=\"METRIC.pct\" :ng-pct=\"METRIC.ngPct\" :ok=\"METRIC.ok\" :plan=\"METRIC.plan\"></donut-chart>\n  </div>\n  <div class=\"mics-pastel-metrics\">\n    GAUGE\n    <dl class=\"mics-pastel-stats\">\n      <div><dt>ICON_PLAN予定数</dt><dd>{{ fmtNum(METRIC.plan) }}</dd></div>\n      <div><dt>ICON_OK良品数</dt><dd>{{ fmtNum(METRIC.ok) }}</dd></div>\n      <div><dt>ICON_NG不良数</dt><dd :class=\"{ 'text-red-600': METRIC.ng }\">{{ fmtNum(METRIC.ng) }}<small> ({{ METRIC.ngRate.toFixed(1) }}%)</small></dd></div>\n      <div><dt>ICON_REMAINING残数</dt><dd>{{ fmtNum(METRIC.remaining) }}</dd></div>\n    </dl>\n  </div>\n  LOCAL_NEXT\n".replaceAll('METRIC',metric).replaceAll('GAUGE','<cycle-gauge :actual="m.cycle_seconds" :target="m.target_cycle_seconds"></cycle-gauge>').replaceAll('LOCAL_NEXT',next).replace(/ICON_([A-Z]+)/g,(_,name)=>icon(name.toLowerCase()));
 s=s.slice(0,start)+opening+'\n'+body+'\n</article>'+s.slice(end);
 const registration='components: { AppShell, StatusBadge, ChangeTags, DonutChart, ToastContainer },';
 if(s.split(registration).length!==2)throw Error('Local components registration changed');
 s="import {CycleGauge} from './mics-cycle-gauge.js';\n"+s.replace(registration,'components: { AppShell, StatusBadge, ChangeTags, DonutChart, ToastContainer, CycleGauge },');
 writeFileSync(path,s);
 writeFileSync(resolve(root,'js/mics-cycle-gauge.js'),readFileSync('scripts/mics-cycle-gauge.js','utf8'));
 const cssPath=resolve(root,'css/tailwind-custom.css');
 writeFileSync(cssPath,readFileSync(cssPath,'utf8')+'\n'+readFileSync('scripts/mics-pastel-nodes.css','utf8'));
}
