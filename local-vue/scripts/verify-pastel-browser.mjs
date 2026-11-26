// Exercise the actual generated card with the actual native renderer, not Vue.
import {readFileSync} from 'node:fs';
import {resolve,join} from 'node:path';
import {createServer} from 'node:http';
import assert from 'node:assert/strict';
import {chromium} from 'playwright';
const version=JSON.parse(readFileSync('package.json','utf8')).version;
const root=resolve('../www/vue-v'+version);
const source=readFileSync(join(root,'js/dashboard-app.js'),'utf8');
const at=source.indexOf('class="card mics-pastel-node');
const article=source.slice(source.lastIndexOf('<article',at),source.indexOf('</article>',at)+10);
assert.ok(article.startsWith('<article'));
const server=createServer((req,res)=>{
 const path=new URL(req.url,'http://localhost').pathname;
 if(path==='/'){res.setHeader('Content-Type','text/html');return res.end('<!doctype html><meta charset="utf-8"><style>body{margin:0;width:365px}*{box-sizing:border-box}</style><link rel="stylesheet" href="/css/tailwind-generated.css"><link rel="stylesheet" href="/css/tailwind-custom.css"><div id="app"></div><script src="/assets/compiled-expressions.js"></script>')}
 if(!/^\/(?:js|css|assets)\/[a-zA-Z0-9_.-]+$/.test(path)){res.statusCode=404;return res.end()}
 try{res.setHeader('Content-Type',path.endsWith('.css')?'text/css':'application/javascript');res.end(readFileSync(join(root,path)))}catch{res.statusCode=404;res.end()}
});
await new Promise(r=>server.listen(0,'127.0.0.1',r));
let browser;
try{
 browser=await chromium.launch({headless:true});
 const page=await browser.newPage({viewport:{width:365,height:850}});
 const errors=[];page.on('pageerror',e=>errors.push(e.message));const warnings=[];page.on('console',m=>{if(m.type()==='warning'&&m.text().includes('MICS expression'))warnings.push(m.text())});
 await page.goto('http://127.0.0.1:'+server.address().port);
 await page.evaluate(async article=>{
  const {createApp,reactive}=await import('/js/vanilla-runtime.js');
  const {CycleGauge}=await import('/js/mics-cycle-gauge.js');
  const m={machine_no:8,status_code:'210',staff_name:'ゴイ',mold_name:'救急箱本体',product_name:'クリア',mold_no:'H-52',pick_qty:1,cycle_seconds:54.8,target_cycle_seconds:50};
  const metrics={plan:960,ok:180,ng:5,ngRate:0.5,remaining:775,pct:18.75,ngPct:.5,curColor:'クリア',idle:false,change:null};
  createApp({
   components:{CycleGauge,StatusBadge:{props:['code','cfg'],template:'<span>サイクル注意</span>'},DonutChart:{props:['pct','ngPct','ok','plan'],template:'<div class="donut"><div class="donut-in"><b>19%</b></div></div>'}},
   setup(){const data=reactive(metrics);window.updateMetric=()=>{data.ok=181;data.remaining=774};return {filteredMachines:[m],state:{cfg:{},aliasCfg:{}},metricsByMachine:{8:data},statusStyle:()=>({color:'#efc43d'}),machineAnimationEffect:()=>'',productColorStyle:()=>({bg:'#fff',text:'#123',border:'#abc'}),fmtNum:n=>String(n),openMachine:()=>{window.machineClicked=true}}},
   template:article
  }).mount('#app');
 },article);
 const card=page.locator('article[data-machine="8"]');
 await card.waitFor();
 const stats=card.locator('.mics-pastel-stats');
 await stats.waitFor({state:'visible'});
 assert.equal(await stats.locator('dt').count(),4);
 assert.deepEqual(await stats.locator('dt').allTextContents(),['予定数','良品数','不良数','残数']);
 assert.deepEqual((await stats.locator('dd').allTextContents()).map(s=>s.trim()),['960','180','5 (0.5%)','775']);
 assert.equal(await card.locator('.mics-pastel-metrics > .cycle-gauge').count(),1);
 const box=await stats.boundingBox(),dial=await card.locator('.cycle-gauge').boundingBox();
 assert.ok(box&&dial&&box.width>0&&box.height>0&&box.x>=dial.x+dial.width-1,'Stats panel must be visible beside dial');
 await page.evaluate(()=>window.updateMetric());
 await page.waitForFunction(()=>document.querySelector('.mics-pastel-stats')?.textContent.includes('181'));
 assert.match(await stats.innerText(),/774/);
 await card.locator('.mics-pastel-machine').click();
 assert.equal(await page.evaluate(()=>window.machineClicked),true);
 assert.deepEqual(errors,[]);
 assert.deepEqual(warnings,[]);
 console.log('PASS actual native card: 4 production counts, 19% progress, cycle, reactive refresh, layout and click');
}finally{if(browser)await browser.close();await new Promise(r=>server.close(r))}
