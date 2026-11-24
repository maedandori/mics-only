import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';
import { resolve } from 'node:path';

const pkg = JSON.parse(readFileSync('local-vue/package.json', 'utf8'));
const vueDir = `www/vue-v${pkg.version}`;
const commonPath = resolve(vueDir, 'js/common.js');
const scheduleAppPath = resolve(vueDir, 'js/change-schedule-app.js');
const localWebPath = resolve('src/local_web.rs');

test('common.js provides getWeekDaysMonToSun with all 7 days Mon..Sun', async () => {
  const mod = await import(pathToFileURL(commonPath).href);
  assert.equal(typeof mod.getWeekDaysMonToSun, 'function');
  const days = mod.getWeekDaysMonToSun('2026-11-25'); // Wed -> Mon 2026-11-23..Sun 2026-11-29
  assert.equal(days.length, 7);
  assert.equal(mod.dateKey(days[0]), '2026-11-23');
  assert.equal(days[0].getDay(), 1);
  assert.equal(mod.dateKey(days[6]), '2026-11-29');
  assert.equal(days[6].getDay(), 0);
});

test('change-schedule-app.js renders 7-day (月〜日) weekly tables offline and watches change-schedule', () => {
  const app = readFileSync(scheduleAppPath, 'utf8');
  assert.ok(app.includes('getWeekDaysMonToSun(selectedDate.value)'));
  assert.ok(app.includes('月〜日'));
  assert.ok(!app.includes('月〜土'));
  assert.ok(app.includes('wd[6]'));
  assert.ok(!app.includes('wd[5]'));
  assert.ok(app.includes('isSun: d.getDay() === 0'));
  assert.ok(app.includes("kind==='schedule'||kind==='change-schedule'"));
  assert.ok(app.includes('next.change_schedule!==previous.change_schedule'));
});

test('local_web.rs serves offline 7-day change-schedule-week.json and version metadata', () => {
  const rs = readFileSync(localWebPath, 'utf8');
  assert.ok(rs.includes('"/data/change-schedule-week.json"'));
  assert.ok(rs.includes('"/api/change-schedule"'));
  assert.ok(rs.includes('"change_schedule":json_version(&state.data_path.with_file_name("change-schedule-week.json"))'));
  assert.ok(rs.includes('(state.data_path.with_file_name("change-schedule-week.json"),"change-schedule")'));
});
