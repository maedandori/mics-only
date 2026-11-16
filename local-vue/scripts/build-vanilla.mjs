// Preserve original uploaded HTML/CSS/JS templates, replacing Vue with native DOM JS.
// No Vite, Vue bundle, CDN dependency, or network fetches required by the UI.
import {readFileSync,writeFileSync,cpSync,mkdirSync,rmSync} from 'node:fs';
import {resolve,join} from 'node:path';
import {execFileSync} from 'node:child_process';
import {Script} from 'node:vm';
import './prepare-original.mjs';
const pkg=JSON.parse(readFileSync('package.json','utf8'));
if(!/^\d+\.\d+\.\d+$/.test(pkg.version))throw Error('Invalid version');
const base='vue-v'+pkg.version,output=resolve('../www',base);
rmSync(output,{force:true,recursive:true});
cpSync('original',output,{recursive:true});
mkdirSync(join(output,'assets'),{recursive:true});
writeFileSync(join(output,'assets','compat.js'),'/* Native MICS HTML/JS compatibility asset. No Vue runtime. */\n');
writeFileSync(join(output,'assets','compat.css'),'/* Native template, styles generated from original Tailwind classes. */\n');

// Compile template expressions at build time as classic JavaScript functions.
// This preserves the original declarative HTML without runtime eval/new Function
// and works under the existing Content-Security-Policy script-src 'self'.
const expressions=new Set(),setters=new Set();
for(const name of ['dashboard-app.js','change-schedule-app.js','status-colors-app.js','change-aliases-app.js','components.js']){
 const source=readFileSync(join(output,'js',name),'utf8');
 const attr=/(?:v-if|v-else-if|v-for|v-model|:[\w-]+|@[\w.-]+)\s*=\s*"([^"]*)"/g;
 for(const match of source.matchAll(attr)){
  const value=match[1].trim();
  if(match[0].startsWith('v-model'))setters.add(value);
  const loop=value.match(/^\s*(?:\(([^)]+)\)|[\w$]+)\s+(?:in|of)\s+([\s\S]+)$/);
  expressions.add(loop?loop[2].trim():value);
 }
 for(const match of source.matchAll(/{{([\s\S]*?)}}/g))expressions.add(match[1].trim());
}
const exprCode=['/* Compiled from trusted original MICS templates at build time. */',
 'window.MICS_EXPRESSIONS=Object.create(null);','window.MICS_SETTERS=Object.create(null);'];
for(const value of expressions){
 // Every expression remains local and must parse before release.
 exprCode.push('window.MICS_EXPRESSIONS['+JSON.stringify(value)+']=function(s,$event){with(s){return ('+value+')}};');
}
for(const value of setters)exprCode.push('window.MICS_SETTERS['+JSON.stringify(value)+']=function(s,v){with(s){'+value+'=v}};');
const compiled=exprCode.join('\n')+'\n';
new Script(compiled,{filename:'compiled-expressions.js'});
writeFileSync(join(output,'assets','compiled-expressions.js'),compiled);

writeFileSync(join(output,'js','vanilla-runtime.js'),readFileSync('scripts/vanilla-runtime.js','utf8'));
for(const page of ['index.html','change-schedule.html','status-colors.html','change-aliases.html']){
 const file=join(output,page);
 let html=readFileSync(file,'utf8');
 html=html.replaceAll(' (Vue 3)',' (JavaScript)').replaceAll(' v-cloak','');
 html=html.replace(/href="css\//g,'href="/'+base+'/css/');
 html=html.replace(/src="js\//g,'src="/'+base+'/js/');
 html=html.replace('</head>','<link rel="stylesheet" href="/'+base+'/assets/compat.css" />\n<script src="/'+base+'/assets/compiled-expressions.js"></script>\n<script src="/'+base+'/assets/compat.js"></script>\n</head>');
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
// Generate Tailwind into this release's own directory, not an older version.
const tailwind=resolve('node_modules','tailwindcss','lib','cli.js');
execFileSync(process.execPath,[tailwind,'-c','tailwind.config.cjs','-i','original/tailwind-input.css',
 '-o',join(output,'css','tailwind-generated.css'),'--minify'],{stdio:'inherit'});
const generatedCss=readFileSync(join(output,'css','tailwind-generated.css'),'utf8');
if(generatedCss.length<4000||!generatedCss.includes('.grid'))throw Error('Current-version Tailwind CSS missing or incomplete: '+base);
console.log('Original four-page template converted to native JavaScript with current-version CSS: '+base);
