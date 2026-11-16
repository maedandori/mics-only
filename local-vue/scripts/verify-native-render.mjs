// Release regression guard for the original four-page MICS template.
// Comments separating v-if branches must not hide machine nodes or schedule rows.
import {readFileSync} from 'node:fs';
import {resolve} from 'node:path';
import assert from 'node:assert/strict';
const {version}=JSON.parse(readFileSync('package.json','utf8'));
const web=resolve('../www','vue-v'+version);
const runtime=readFileSync(resolve('scripts/vanilla-runtime.js'),'utf8');
assert.match(runtime,/n\.nodeType===8/, 'HTML comments must be ignored when chaining v-if/v-else-if');
const dashboard=readFileSync(resolve(web,'js/dashboard-app.js'),'utf8');
const schedule=readFileSync(resolve(web,'js/change-schedule-app.js'),'utf8');
assert.match(dashboard,/<!-- Grid view[\s\S]{0,1600}?v-else-if="state\.view === 'grid'"/,'Machine grid conditional branch was lost');
assert.match(dashboard,/v-for="m in filteredMachines"/,'Machine node loop was lost');
assert.match(schedule,/<!-- TAB 1:[\s\S]{0,200}?v-else-if="state\.tab === 'today' && todayChronologicalModel"/,'Today schedule conditional branch was lost');
assert.match(schedule,/<!-- TAB 2:[\s\S]{0,220}?v-else-if="state\.tab === 'day' && dayViewModel"/,'Day schedule conditional branch was lost');
assert.match(schedule,/v-for="row in dayViewModel\.rows"/,'Day schedule machine loop was lost');
for(const page of ['index.html','change-schedule.html','status-colors.html','change-aliases.html']){
 const html=readFileSync(resolve(web,page),'utf8');
 assert.ok(!html.includes('vue.esm-browser')&&!html.includes('cdn.tailwindcss.com'),'External Vue/CDN reference: '+page);
}
const css=readFileSync(resolve(web,'css/tailwind-generated.css'),'utf8');
assert.ok(css.length>4000 && css.includes('.grid'), 'Current-version Tailwind stylesheet was not built');
for(const page of ['index.html','change-schedule.html','status-colors.html','change-aliases.html']){
 const html=readFileSync(resolve(web,page),'utf8');
 assert.match(html, /css\/tailwind-generated\.css/, 'Original template stylesheet reference missing: '+page);
}
const loader = readFileSync(resolve(web,'js/common.js'),'utf8');
assert.match(loader,/fetch\('\/data\/machines\.json'/,'Browser must read the local machine JSON directly');
assert.match(loader,/fetch\('\/data\/schedule-local\.json'/,'Browser must read the local schedule JSON directly');
assert.ok(!loader.includes("fetch('/api/machines'")&&!loader.includes("fetch('/api/schedule'"),'Database-like API polling must not reappear');
assert.match(loader,/lastNormalized/,'Unchanged JSON should reuse normalized state');
console.log('PASS original nodes, local JSON-only loader, schedule deferral, CSS');
