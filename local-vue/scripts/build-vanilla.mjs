// Preserve original uploaded HTML/CSS/JS templates, replacing Vue with native DOM JS.
// No Vite, Vue bundle, CDN dependency, or network fetches required by the UI.
import {readFileSync,writeFileSync,cpSync,mkdirSync,rmSync} from 'node:fs';
import {resolve,join} from 'node:path';
import {execFileSync} from 'node:child_process';
import './prepare-original.mjs';
const pkg=JSON.parse(readFileSync('package.json','utf8'));
if(!/^\d+\.\d+\.\d+$/.test(pkg.version))throw Error('Invalid version');
const base='vue-v'+pkg.version,output=resolve('../www',base);
rmSync(output,{force:true,recursive:true});
cpSync('original',output,{recursive:true});
mkdirSync(join(output,'assets'),{recursive:true});
writeFileSync(join(output,'assets','compat.js'),'/* Native MICS HTML/JS compatibility asset. No Vue runtime. */\n');
writeFileSync(join(output,'assets','compat.css'),'/* Native template, styles generated from original Tailwind classes. */\n');
writeFileSync(join(output,'js','vanilla-runtime.js'),readFileSync('scripts/vanilla-runtime.js','utf8'));
for(const page of ['index.html','change-schedule.html','status-colors.html','change-aliases.html']){
 const file=join(output,page);
 let html=readFileSync(file,'utf8');
 html=html.replaceAll(' (Vue 3)',' (JavaScript)').replaceAll(' v-cloak','');
 html=html.replace(/href="css\//g,'href="/'+base+'/css/');
 html=html.replace(/src="js\//g,'src="/'+base+'/js/');
 html=html.replace('</head>','<link rel="stylesheet" href="/'+base+'/assets/compat.css" />\n<script src="/'+base+'/assets/compat.js"></script>\n</head>');
 writeFileSync(file,html);
}
for(const name of ['dashboard-app.js','change-schedule-app.js','status-colors-app.js','change-aliases-app.js','components.js']){
 const file=join(output,'js',name);
 let source=readFileSync(file,'utf8').replace(/from\s+['"]vue['"]/g,"from './vanilla-runtime.js'");
 if(source.includes("from 'vue'"))throw Error('Vue import remained: '+name);
 writeFileSync(file,source);
}
for(const name of ['common.js','components.js','dashboard-app.js','change-schedule-app.js','status-colors-app.js','change-aliases-app.js','vanilla-runtime.js']){
 execFileSync(process.execPath,['--check',join(output,'js',name)],{stdio:'inherit'});
}
for(const page of ['index.html','change-schedule.html','status-colors.html','change-aliases.html']){
 const html=readFileSync(join(output,page),'utf8');
 if(html.includes('cdn.tailwindcss.com')||html.includes('vue.esm-browser')||!html.includes('/'+base+'/js/'))throw Error('Standalone template verification failed: '+page);
}
console.log('Original four-page template converted to native JavaScript: '+base);
