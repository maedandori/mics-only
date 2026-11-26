import {readFileSync,writeFileSync,cpSync,rmSync,readdirSync,statSync} from 'node:fs';
import {resolve,join} from 'node:path';
import {fileURLToPath} from 'node:url';
const root=fileURLToPath(new URL('../..',import.meta.url));
const v=process.argv[2];
if(!/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(v||'')||v.length>30)throw Error('Invalid web version');
const core=JSON.parse(readFileSync(join(root,'local-vue/package.json'),'utf8')).version;
const old='vue-v'+core,next='web-v'+v;
const out=resolve(root,'web-package');
rmSync(out,{force:true,recursive:true});
cpSync(join(root,'www',old),out,{recursive:true});
function walk(dir){
 for(const ent of readdirSync(dir,{withFileTypes:true})){
  const p=join(dir,ent.name);
  if(ent.isDirectory())walk(p);
  else if(ent.isFile()&&/\.(html|js|css|json|webmanifest)$/i.test(p))writeFileSync(p,readFileSync(p,'utf8').replaceAll('/'+old+'/','/'+next+'/'));
  else if(!ent.isFile())throw Error('Unexpected asset type: '+p);
 }
}
walk(out);
for(const page of ['index.html','change-schedule.html','status-colors.html','change-aliases.html']){
 const html=readFileSync(join(out,page),'utf8');
 const refs=[...html.matchAll(new RegExp('["\\\']/'+next.replaceAll('.','\\.')+'/([^"\\\']+)["\\\']','g'))];
 if(!refs.length)throw Error('Missing versioned links '+page);
 for(const [,p] of refs)if(p.includes('..')||!statSync(join(out,p)).isFile())throw Error('Missing asset '+p);
}
writeFileSync(join(out,'web-manifest.json'),JSON.stringify({web_version:v,api_version:1,min_core:'0.2.69'},null,2));
console.log('Web package '+next);
