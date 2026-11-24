import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
const source=readFileSync('local-vue/scripts/build-vanilla.mjs','utf8');
const a=source.indexOf('function materialEstimate('), b=source.indexOf('function titleText(',a);
assert.ok(a>=0&&b>a);
const estimate=new Function(source.slice(a,b)+'return materialEstimate;')();
test('confirmed per-piece weight and optional cavity count',()=>{
 for(const [qty,pick,grams,expected] of [[10001,4,117,1170.117],[10000,2,110,1100],[10000,undefined,110,1100],[1,8,25,0.025],[0,4,117,0],[-1,4,117,null],['',4,117,null],[100,4,null,null]]){
  assert.equal(estimate(qty,pick,grams)?.totalKg??null,expected);
 }
});
test('schedule is authoritative including zero, legacy live value is only fallback',()=>{
 assert.ok(source.includes('qty_remaining:r.qty_remaining??m.qty_remaining'));
 assert.ok(source.includes('remaining_from_live:r.qty_remaining==null&&m.qty_remaining!=null'));
 assert.equal(0??20,0);
 assert.equal(null??20,20);
 const schedule=readFileSync('src/schedule.rs','utf8');
 assert.ok(schedule.includes('S.CALCQTYMREMAIN'));
 assert.ok(schedule.includes('"qty_remaining":remaining.trim().parse::<i64>()'));
});
