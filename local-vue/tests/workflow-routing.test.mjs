import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
const root=new URL('../../',import.meta.url);
const windows=readFileSync(new URL('.github/workflows/build.yml',root),'utf8');
const web=readFileSync(new URL('.github/workflows/web-release.yml',root),'utf8');
// Extract the deliberately simple quoted path lists from the actual workflow.
function paths(source,event){
 const match=source.match(new RegExp('^  '+event+':\\n([\\s\\S]*?)(?=^  [a-z_]+:|^concurrency:|^jobs:|$(?![\\s\\S]))','m'));
 assert.ok(match,'Missing '+event);
 return [...match[1].matchAll(/^      - '([^']+)'$/gm)].map(m=>m[1]);
}
function matches(pattern,file){
 if(pattern==='**')return true;
 if(pattern.endsWith('/**'))return file.startsWith(pattern.slice(0,-2));
 assert.ok(!pattern.includes('*'),'Unexpected glob requires a test update: '+pattern);
 return file===pattern;
}
function triggers(patterns,files){
 return files.some(file=>{
  let included=false;
  for(const pattern of patterns){
   const negative=pattern.startsWith('!');
   if(matches(negative?pattern.slice(1):pattern,file))included=!negative;
  }
  return included;
 });
}
test('Windows CI skips Web-only changes but retains shared and mixed Core builds',()=>{
 for(const event of ['push','pull_request']){
  const p=paths(windows,event);
  assert.ok(p.length);
  for(const file of ['local-vue/scripts/mics-pastel-nodes.css','local-vue/scripts/build-vanilla.mjs','local-vue/tests/workflow-routing.test.mjs','local-vue/web-release-version.txt','.github/workflows/web-release.yml','AGENTS.md']){
   assert.equal(triggers(p,[file]),false,event+' '+file);
  }
  for(const file of ['Cargo.toml','src/main.rs','src/bin/mics-core.rs','settings-tauri/ui/app.js','scripts/apply-update.ps1','www/style.css','local-vue/package.json']){
   assert.equal(triggers(p,[file]),true,event+' '+file);
  }
  assert.equal(triggers(p,['local-vue/scripts/mics-pastel-nodes.css','src/main.rs']),true);
 }
 assert.equal(triggers(paths(windows,'pull_request'),['.github/workflows/build.yml']),true);
 assert.equal(triggers(paths(windows,'push'),['.github/workflows/build.yml','AGENTS.md']),false);
 assert.match(windows,/^  workflow_dispatch:/m);
});
test('Web CI validates Web and Windows-workflow PRs; only version bump publishes Web on main',()=>{
 const pr=paths(web,'pull_request'),push=paths(web,'push');
 assert.equal(triggers(pr,['local-vue/scripts/mics-pastel-nodes.css']),true);
 assert.equal(triggers(pr,['.github/workflows/build.yml']),true);
 assert.equal(triggers(pr,['.github/workflows/web-release.yml']),true);
 assert.equal(triggers(push,['local-vue/web-release-version.txt']),true);
 assert.equal(triggers(push,['local-vue/scripts/mics-pastel-nodes.css']),false);
 assert.match(web,/Verify actual native card in Chromium/);
 assert.match(web,/node --test local-vue\/tests\/\*\.test\.mjs/);
 assert.match(web,/Package independently versioned UI/);
});
