// Expands local weekly mold/color change schedule from 6 days (Mon-Sat) to 7 days (Mon-Sun).
import { readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';

export function patchWeekly7Days(root) {
  const once = (s, a, b, tag) => {
    const n = s.split(a).length - 1;
    if (n !== 1) throw Error(`Weekly 7-day patch ${tag}: expected 1 match, got ${n}`);
    return s.replace(a, b);
  };
  const exactCount = (s, a, b, expected, tag) => {
    const n = s.split(a).length - 1;
    if (n !== expected) throw Error(`Weekly 7-day patch ${tag}: expected ${expected} matches, got ${n}`);
    return s.split(a).join(b);
  };

  // 1. Add getWeekDaysMonToSun helper to common.js while preserving getWeekDaysMonToSat.
  const commonPath = resolve(root, 'js/common.js');
  let common = readFileSync(commonPath, 'utf8');
  common = once(
    common,
    `export function getWeekDaysMonToSat(monday) {
  const mon = getMonday(monday);
  if (!mon) return [];
  const days = [];
  for (let i = 0; i < 6; i++) {
    days.push(new Date(mon.getFullYear(), mon.getMonth(), mon.getDate() + i));
  }
  return days;
}`,
    `export function getWeekDaysMonToSat(monday) {
  const mon = getMonday(monday);
  if (!mon) return [];
  const days = [];
  for (let i = 0; i < 6; i++) {
    days.push(new Date(mon.getFullYear(), mon.getMonth(), mon.getDate() + i));
  }
  return days;
}

export function getWeekDaysMonToSun(monday) {
  const mon = getMonday(monday);
  if (!mon) return [];
  const days = [];
  for (let i = 0; i < 7; i++) {
    days.push(new Date(mon.getFullYear(), mon.getMonth(), mon.getDate() + i));
  }
  return days;
}`,
    'common getWeekDaysMonToSun',
  );
  common = once(
    common,
    'dateKey, parseDateKey, getWorkdayAndShift, getMonday, getWeekDaysMonToSat,',
    'dateKey, parseDateKey, getWorkdayAndShift, getMonday, getWeekDaysMonToSat, getWeekDaysMonToSun,',
    'common window.MICS export',
  );
  writeFileSync(commonPath, common);

  // 2. Update change-schedule-app.js to render Mon-Sun (月〜日) 7-day schedules.
  const appPath = resolve(root, 'js/change-schedule-app.js');
  let app = readFileSync(appPath, 'utf8');
  app = once(
    app,
    'dateKey, parseDateKey, getWorkdayAndShift, getWeekDaysMonToSat,',
    'dateKey, parseDateKey, getWorkdayAndShift, getWeekDaysMonToSun,',
    'app import',
  );
  app = once(
    app,
    'const weekDays = computed(() => getWeekDaysMonToSat(selectedDate.value));',
    'const weekDays = computed(() => getWeekDaysMonToSun(selectedDate.value));',
    'app weekDays',
  );
  app = once(
    app,
    '        isSat: d.getDay() === 6,\n        isSelected: dateKey(d) === state.selectedDateKey,',
    '        isSat: d.getDay() === 6,\n        isSun: d.getDay() === 0,\n        isSelected: dateKey(d) === state.selectedDateKey,',
    'app isSun flag',
  );
  app = exactCount(
    app,
    'fmtShortDay(wd[5])',
    'fmtShortDay(wd[6])',
    2,
    'app week end label wd[6]',
  );
  app = once(
    app,
    '12・16・18・7号機は1日表示タブでも月〜土の1週間予定を常時表示します',
    '12・16・18・7号機は1日表示タブでも月〜日の1週間予定を常時表示します',
    'app alwaysWeek subtitle',
  );
  app = once(
    app,
    '(全14台 · 月〜土)',
    '(全14台 · 月〜日)',
    'app weekViewModel title',
  );
  app = once(
    app,
    '>🗓 週間予定 (月〜土)</button>',
    '>🗓 週間予定 (月〜日)</button>',
    'app tab button label',
  );
  app = once(
    app,
    '<!-- TAB 3: 週間予定 (月〜土 全14台) -->',
    '<!-- TAB 3: 週間予定 (月〜日 全14台) -->',
    'app tab comment',
  );
  app = exactCount(
    app,
    ":class=\"d.isSelected ? 'bg-blue-100 text-blue-800' : d.isSat ? 'bg-blue-50 text-blue-700' : 'bg-slate-100 text-slate-900'\"",
    ":class=\"d.isSelected ? 'bg-blue-100 text-blue-800' : d.isSat ? 'bg-blue-50 text-blue-700' : d.isSun ? 'bg-rose-50 text-rose-700' : 'bg-slate-100 text-slate-900'\"",
    2,
    'app Sunday header styling',
  );
  writeFileSync(appPath, app);
}
