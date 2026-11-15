import {mkdirSync,copyFileSync} from 'node:fs';
import {resolve} from 'node:path';
const dest=resolve('../www/vue-v0.2.9/data');
mkdirSync(dest,{recursive:true});
for(const n of ['change-aliases.json','status-colors.json']){
 copyFileSync(resolve('original/data',n),resolve(dest,n));
}
console.log('Copied default UI JSON settings to versioned bundle');
