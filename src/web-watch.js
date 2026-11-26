// Local only. Loaded by the Core on every modern page; no cloud polling.
(()=>{
 const script=document.currentScript;
 const loaded=new URL(script.src).searchParams.get('v');
 let busy=false;
 async function check(){
   if(busy||!loaded)return;
   busy=true;
   try{
     const response=await fetch('/api/web-version',{cache:'no-store'});
     if(!response.ok)return;
     const info=await response.json();
     if(info.generation&&info.generation!==loaded){
       if('serviceWorker' in navigator){
         for(const registration of await navigator.serviceWorker.getRegistrations()){
           if(new URL(registration.scope).origin===location.origin)await registration.unregister();
         }
       }
       if('caches' in window){for(const key of await caches.keys()){if(key.startsWith('mics-local-ui-'))await caches.delete(key);}}
       const next=new URL(location.href);
       if(next.pathname.startsWith('/web-v')||next.pathname.startsWith('/vue-v'))next.pathname='/'+next.pathname.split('/').slice(2).join('/');
       next.searchParams.set('web_generation',info.generation);
       location.replace(next.href);
     }
   }catch{}finally{busy=false;}
 }
 setInterval(check,8000);
 check();
})();
