import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
const settings=readFileSync('src/bin/mics-only-settings.rs','utf8');
const version=JSON.parse(readFileSync('local-vue/package.json','utf8')).version;
const tabs=readFileSync('www/vue-v'+version+'/js/local-horizontal-tabs.js','utf8');
test('settings labels are Japanese and retain core actions',()=>{
 for(const label of ['予備機を有効にする','データベース調査','端末名','接続テスト・端末登録','別名・状態色を同期','保存して再起動','Windows 起動時に自動起動','全テーブルの先頭10件を保存'])assert.ok(settings.includes(label),label);
 for(const old of ['Chạy SQL','Tên máy','Lưu 10 dòng','Enable standby agent /','Open Local Web','Start Agent /','Settings v{}  |  Agent','Results and export status'])assert.ok(!settings.includes(old),old);
 assert.ok(settings.includes('credentials::save'));
 assert.ok(settings.includes('updater::install_with_progress'));
});
test('local header uses current polling mode and interval',()=>{
 assert.ok(tabs.includes("mode==='STANDBY'"));
 assert.ok(tabs.includes('poll_interval_seconds'));
 assert.ok(!tabs.includes('約5分間隔'));
});
