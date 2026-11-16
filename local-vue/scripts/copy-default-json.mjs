import {mkdirSync,copyFileSync,readFileSync,existsSync} from 'node:fs';
import {resolve} from 'node:path';

// Keep the output in sync with vite.config.js and package.json on each release.
const {version}=JSON.parse(readFileSync(resolve('package.json'),'utf8'));
if(!/^\d+\.\d+\.\d+$/.test(version))throw new Error('Invalid package version: '+version);
const dest=resolve('../www/vue-v'+version+'/data');
mkdirSync(dest,{recursive:true});
for(const n of ['change-aliases.json','status-colors.json']){
 const source=resolve('original/data',n);
 if(!existsSync(source))throw new Error('Missing original UI JSON: '+source);
 copyFileSync(source,resolve(dest,n));
}
console.log('Copied default UI JSON settings to versioned bundle: v'+version);
