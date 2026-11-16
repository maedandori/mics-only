/* Framework-free renderer for the user's original MICS HTML templates.
 * Uses native DOM, JavaScript Proxies and requestAnimationFrame. No Vue runtime. */
let active=null;
const REF=Symbol('ref'), proxies=new WeakMap(), jobs=new WeakSet();
const later=app=>{if(!app||jobs.has(app))return;jobs.add(app);requestAnimationFrame(()=>{jobs.delete(app);app.render()})};
export function ref(value){const app=active;return {[REF]:1,get value(){return value},set value(v){if(value!==v){value=v;if(!(v instanceof Node))later(app)}}}}
// Memoize computed expressions for one render. The original template accesses
// metricsByMachine dozens of times per card; recomputing it each time is costly.
export function computed(fn){
 const app=active;let lastCycle=-1,cached;
 return {[REF]:1,get value(){
  if(!app||!app.rendering)return fn();
  if(lastCycle!==app.renderCycle){cached=fn();lastCycle=app.renderCycle;}
  return cached;
 }};
}
export function reactive(obj){const app=active;const wrap=x=>{
 if(!x||typeof x!=='object'||x instanceof Date||x instanceof Map||x instanceof Set||x[REF])return x;
 if(proxies.has(x))return proxies.get(x);
 const p=new Proxy(x,{get(t,k){return wrap(Reflect.get(t,k))},set(t,k,v){const prev=t[k];const ok=Reflect.set(t,k,v);if(prev!==v)later(app);return ok},deleteProperty(t,k){const ok=Reflect.deleteProperty(t,k);later(app);return ok}});
 proxies.set(x,p);return p;
};return wrap(obj)}
export function onMounted(fn){active?.mounted.push(fn)}
export function onUnmounted(fn){active?.cleanup.push(fn)}
export function nextTick(fn){return new Promise(done=>requestAnimationFrame(()=>{fn?.();done()}))}
const scope=(parent,fields)=>new Proxy(Object.assign(Object.create(parent||null),fields),{has(t,k){return k!==Symbol.unscopables&&k in t},get(t,k){if(k===Symbol.unscopables)return;const x=t[k];return x?.[REF]?x.value:x}});
function evaluate(code,ctx,event){
 const fn=window.MICS_EXPRESSIONS?.[code];
 if(!fn){console.warn('MICS uncompiled template expression',code);return undefined}
 try{return fn(ctx,event)}catch(e){console.warn('MICS expression failed',code,e.message);return undefined}
}
function run(code,ctx,event){const result=evaluate(code,ctx,event);if(typeof result==='function')result(event)}
function assign(code,ctx,value){
 try{const fn=window.MICS_SETTERS?.[code];if(!fn)throw Error('Expression missing');fn(ctx,value)}
 catch(e){console.warn('MICS input update failed',code,e.message)}
}
const interpolate=(s,ctx)=>s.replace(/{{([\s\S]*?)}}/g,(_,code)=>String(evaluate(code.trim(),ctx)??''));
const cssClass=x=>Array.isArray(x)?x.map(cssClass).filter(Boolean).join(' '):x&&typeof x==='object'?Object.keys(x).filter(k=>x[k]).join(' '):String(x??'');
const kebab=s=>s.replace(/[A-Z]/g,c=>'-'+c.toLowerCase());
const camel=s=>s.replace(/-([a-z])/g,(_,c)=>c.toUpperCase());
const templateCache=new Map();
function parse(html){let t=templateCache.get(html);if(!t){t=document.createElement('template');t.innerHTML=html;templateCache.set(html,t)}return t.content}
function forEntries(value,ctx){const m=value.match(/^\s*(?:\(([^,]+),\s*([^)]+)\)|([\w$]+))\s+(?:in|of)\s+([\s\S]+)$/);
 if(!m)return[];let v=evaluate(m[4],ctx);if(typeof v==='number')v=Array.from({length:v},(_,i)=>i+1);
 return v&&v[Symbol.iterator]?Array.from(v, (x,i)=>scope(ctx,{[m[1]||m[3]]:x,...(m[2]?{[m[2]]:i}:{})})):[];
}
function children(nodes,ctx,app,path,slot){const out=document.createDocumentFragment(),all=Array.from(nodes);
 for(let i=0;i<all.length;i++){const node=all[i];
  if(node.nodeType!==1){out.appendChild(render(node,ctx,app,path+'/'+i,slot));continue}
  if(node.hasAttribute('v-else-if')||node.hasAttribute('v-else'))continue;
  if(node.hasAttribute('v-if')){
   const group=[node];let j=i+1;
   for(;j<all.length;j++){let n=all[j];
    // HTML comments between v-if / v-else-if are whitespace in the original
    // template. They must not terminate the conditional chain.
    if(n.nodeType===8||(n.nodeType===3&&!n.textContent.trim()))continue;
    if(n.nodeType===1&&(n.hasAttribute('v-else-if')||n.hasAttribute('v-else'))){group.push(n);continue}break}
   const selected=group.find(n=>n.hasAttribute('v-else')||Boolean(evaluate(n.getAttribute('v-if')||n.getAttribute('v-else-if'),ctx)));
   if(selected){const copy=selected.cloneNode(true);for(const k of ['v-if','v-else-if','v-else'])copy.removeAttribute(k);out.appendChild(render(copy,ctx,app,path+'/'+i,slot))}
   i=j-1;continue;
  }
  out.appendChild(render(node,ctx,app,path+'/'+i,slot));
 }return out;
}
function render(node,ctx,app,path,slot){
 if(node.nodeType===3)return document.createTextNode(interpolate(node.textContent,ctx));
 if(node.nodeType!==1)return document.createDocumentFragment();
 if(node.hasAttribute('v-for')){const out=document.createDocumentFragment();
  for(const [i,item] of forEntries(node.getAttribute('v-for'),ctx).entries()){
   const copy=node.cloneNode(true);copy.removeAttribute('v-for');out.appendChild(render(copy,item,app,path+'/for'+i,slot))}
  return out;
 }
 const tag=node.tagName.toLowerCase();
 if(tag==='template')return children(node.content?.childNodes||node.childNodes,ctx,app,path,slot);
 if(tag==='slot'){
  if(!slot)return children(node.childNodes,ctx,app,path,null);
  const values={};for(const a of node.attributes)if(a.name.startsWith(':'))values[camel(a.name.slice(1))]=evaluate(a.value,ctx);
  const match=slot.vars.match(/^\s*{([^}]+)}\s*$/);const fields={};
  for(const name of (match?.[1]||'').split(',').map(s=>s.trim()))if(name)fields[name]=values[name];
  return children(slot.nodes,scope(slot.ctx,fields),app,path+'/slot',null);
 }
 const component=app.components[tag];
 if(component){
  const props={};for(const a of node.attributes){if(a.name.startsWith(':'))props[camel(a.name.slice(1))]=evaluate(a.value,ctx);
   else if(!a.name.startsWith('@')&&!a.name.startsWith('v-'))props[camel(a.name)]=a.value;}
  // Native components need the same declared default props as the original
  // template; otherwise omitted values such as AppShell.narrow are undefined.
  for(const [key,spec] of Object.entries(component.props||{})){
   if(!(key in props)&&spec&&Object.prototype.hasOwnProperty.call(spec,'default')){
    props[key]=typeof spec.default==='function'&&spec.type!==Function?spec.default():spec.default;
   }
  }
  const slotNode=Array.from(node.children).find(n=>n.hasAttribute('#default'));
  const slotInfo={nodes:slotNode?(slotNode.content?.childNodes||slotNode.childNodes):node.childNodes,vars:slotNode?.getAttribute('#default')||'{}',ctx};
  let instance=app.instances.get(path);
  const emit=(name,...args)=>{const handler=node.getAttribute('@'+name);if(handler)run(handler,ctx,args[0]);later(app)};
  if(!instance){instance={props:{...props},setup:{}};app.instances.set(path,instance);
   if(component.setup){const saved=active;active=app;try{instance.setup=component.setup(instance.props,{emit})||{}}finally{active=saved}}}
  Object.assign(instance.props,props);
  return children(parse(component.template).childNodes,scope(null,{...instance.props,...instance.setup,$emit:emit}),app,path+'/component',slotInfo);
 }
 const el=document.createElement(tag);
 for(const a of node.attributes){const name=a.name,val=a.value;
  if(name==='ref'){let obj=ctx;let item;while(obj&&!item){item=Object.getOwnPropertyDescriptor(obj,val)?.value;obj=Object.getPrototypeOf(obj)}if(item?.[REF])item.value=el;continue}
  if(name==='v-model')continue;
  if(name.startsWith('@')){const [kind,...mods]=name.slice(1).split('.');
   el.addEventListener(kind,event=>{if(mods.includes('enter')&&event.key!=='Enter')return;
    if(mods.includes('prevent'))event.preventDefault();if(mods.includes('stop'))event.stopPropagation();
    run(val,ctx,event);later(app)});continue}
  if(name.startsWith(':')){const key=name.slice(1),value=evaluate(val,ctx);
   if(key==='key')continue;
   if(key==='class'){el.className+=' '+cssClass(value);continue}
   if(key==='style'&&value&&typeof value==='object'){for(const [k,v] of Object.entries(value))el.style.setProperty(k.startsWith('--')?k:kebab(k),v??'');continue}
   if(key==='value'){el.value=value??'';continue}
   if(key==='disabled'){el.disabled=!!value;continue}
   if(key==='checked'){el.checked=!!value;continue}
   if(value!==false&&value!=null)el.setAttribute(key,String(value));continue}
  if(name.startsWith('v-')||name.startsWith('#'))continue;el.setAttribute(name,val);
 }
 el.appendChild(children(node.childNodes,ctx,app,path,slot));
 if(node.hasAttribute('v-model')){
  const model=node.getAttribute('v-model'),current=evaluate(model,ctx);
  if(el.type==='checkbox')el.checked=!!current;else el.value=current??'';
  el.addEventListener(el.tagName==='SELECT'||el.type==='checkbox'?'change':'input',()=>{
   assign(model,ctx,el.type==='checkbox'?el.checked:el.value);later(app);
  });
 }
 return el;
}
export function createApp(component){
 const app={components:{},instances:new Map(),mounted:[],cleanup:[],render:()=>{},renderCycle:0,rendering:false};
 for(const [name,definition] of Object.entries(component.components||{}))app.components[name.replace(/([a-z])([A-Z])/g,'$1-$2').toLowerCase()]=definition;
 return {mount(selector){
  const root=document.querySelector(selector);if(!root)throw Error('MICS template root missing');
  const saved=active;active=app;let state;try{state=component.setup?.()||{}}finally{active=saved}
  const ctx=scope(null,state),markup=parse(component.template);
  app.render=()=>{
   app.renderCycle++;
   app.rendering=true;
   try{
   const focus=document.activeElement,focusId=focus?.id,position=focus&&typeof focus.selectionStart==='number'?focus.selectionStart:null;
   const path=[];let cursor=focus;
   while(cursor&&cursor!==root&&root.contains(cursor)){
    const parent=cursor.parentElement;if(!parent)break;
    path.unshift(Array.prototype.indexOf.call(parent.children,cursor));cursor=parent;
   }
   const x=window.scrollX,y=window.scrollY;
   root.replaceChildren(children(markup.childNodes,ctx,app,'root',null));
   let next=focusId?document.getElementById(focusId):null;
   if(!next&&cursor===root&&path.length){
    next=root;for(const i of path){next=next?.children[i];if(!next)break}
   }
   if(next&&typeof next.focus==='function'){next.focus({preventScroll:true});if(position!=null)try{next.setSelectionRange(position,position)}catch{}}
   window.scrollTo(x,y);
   }finally{app.rendering=false;}
  };
  app.render();for(const fn of app.mounted.splice(0)){try{fn()}catch(e){console.error('MICS mount',e)}}
  window.addEventListener('beforeunload',()=>{for(const fn of app.cleanup)try{fn()}catch{}});
  return app;
 }};
}
