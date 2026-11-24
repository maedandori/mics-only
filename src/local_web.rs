//! Loopback-only HTTP server. Never bind to a LAN/public interface.
use serde_json::{json, Value};
use std::{fs, io::{Read,Write},  net::{TcpListener,TcpStream}, path::{Path,PathBuf}, sync::{mpsc,Arc,Mutex,atomic::{AtomicU8,AtomicUsize,Ordering}}, thread, time::{Duration,Instant,SystemTime}};

static VUE_ASSETS_STATE: AtomicU8 = AtomicU8::new(0);
static ACTIVE_WEB_CLIENTS: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone)]
pub struct LocalState {
    pub snapshot: Arc<Mutex<Value>>,
    pub mode: Arc<Mutex<String>>,
    pub data_path: PathBuf,
    pub sync_status: Arc<Mutex<String>>,
    pub last_sync_at: Arc<Mutex<Option<String>>>,
    // Agent-scheduled scan clock; independent of whether snapshot contents changed.
    pub poll_interval_seconds: Arc<Mutex<Option<u64>>>,
    pub next_poll_at: Arc<Mutex<Option<String>>>,
    pub alias_cloud: Arc<Mutex<Option<(String,String)>>>,
    pub alias_last_check: Arc<Mutex<Option<Instant>>>,
    pub alias_sync_status: Arc<Mutex<String>>,
    pub alias_last_sync_at: Arc<Mutex<Option<String>>>,
    pub alias_file_lock: Arc<Mutex<()>>,
    pub alias_sync_ack: Arc<Mutex<Option<i64>>>,
    pub machines_cache: Arc<Mutex<Option<(SystemTime,Vec<u8>)>>>,
    pub schedule_cache: Arc<Mutex<Option<(SystemTime,Vec<u8>)>>>,
    pub change_schedule_cache: Arc<Mutex<Option<(SystemTime,Vec<u8>)>>>,
    pub live_subscribers: Arc<Mutex<Vec<mpsc::Sender<&'static str>>>>,
}
impl LocalState {
    pub fn new(data_path:PathBuf)->Self {
        // Migrate old LocalAppData JSON once, only for the real application path.
        // Copy rather than move: a failed copy never destroys the original cache.
        if data_path==default_snapshot_path() {
            if let (Some(old_root), Some(dest))=(std::env::var_os("LOCALAPPDATA"),data_path.parent()) {
                let old_dir=PathBuf::from(old_root).join("MICS-Only").join("data");
                if old_dir!=dest && old_dir.is_dir() {
                    for name in ["machines.json","schedule-local.json","sync-status.json","schedule-last-ack.sha256"] {
                        let src=old_dir.join(name);
                        let dst=dest.join(name);
                        if src.is_file() && !dst.exists() {
                            if fs::create_dir_all(dest).is_ok() {
                                let _=fs::copy(&src,&dst);
                            }
                        }
                    }
                }
            }
        }
        let initial=fs::read(&data_path).ok()
            .and_then(|raw|serde_json::from_slice::<Value>(&raw).ok())
            .filter(|v|v.get("machines").and_then(Value::as_array).is_some())
            .unwrap_or_else(||json!({"source":"mics-only","machines":[],"sampled_at":null}));
        Self{snapshot:Arc::new(Mutex::new(initial)),mode:Arc::new(Mutex::new("STANDBY".into())),
            sync_status:Arc::new(Mutex::new("NOT_SYNCED".into())),last_sync_at:Arc::new(Mutex::new(None)),
            poll_interval_seconds:Arc::new(Mutex::new(None)),next_poll_at:Arc::new(Mutex::new(None)),
            alias_cloud:Arc::new(Mutex::new(None)),alias_last_check:Arc::new(Mutex::new(None)),
            alias_sync_status:Arc::new(Mutex::new("NOT_CHECKED".into())),
            alias_last_sync_at:Arc::new(Mutex::new(None)),
            alias_file_lock:Arc::new(Mutex::new(())),
            alias_sync_ack:Arc::new(Mutex::new(None)),
            machines_cache:Arc::new(Mutex::new(None)),
            schedule_cache:Arc::new(Mutex::new(None)),
            change_schedule_cache:Arc::new(Mutex::new(None)),
            live_subscribers:Arc::new(Mutex::new(Vec::new())),
            data_path}
    }
    pub fn set_alias_sync(&self,coordinator_url:&str,token:&str){
        if coordinator_url.starts_with("https://") && !token.is_empty(){
            if let Ok(mut conn)=self.alias_cloud.lock(){
                *conn=Some((format!("{}/aliases",coordinator_url.trim_end_matches('/')),token.to_owned()));
            }
        }
    }
    pub fn set_poll_interval(&self, seconds:u64) {
        if let Ok(mut value)=self.poll_interval_seconds.lock(){*value=Some(seconds);}
    }
    pub fn schedule_next_poll(&self, seconds:Option<u64>) {
        // The clock reflects the next agent cycle, not the last changed JSON.
        let next=seconds.map(|s|(chrono::Utc::now()+chrono::Duration::seconds(s as i64)).to_rfc3339());
        if let Ok(mut value)=self.next_poll_at.lock(){*value=next;}
    }
    pub fn set_mode(&self,mode:&str) {
        if let Ok(mut m)=self.mode.lock(){*m=mode.to_owned()}
    }
    pub fn record_sync(&self,status:&str){
        if let Ok(mut s)=self.sync_status.lock(){*s=status.to_owned();}
        if status=="SYNCED"{
            if let Ok(mut t)=self.last_sync_at.lock(){*t=Some(chrono::Utc::now().to_rfc3339());}
        }
        // Status is informational; never store credentials or raw error messages.
        let time=self.last_sync_at.lock().ok().and_then(|v|v.clone());
        let summary=json!({"status":status,"last_sync_at":time});
        if let Some(parent)=self.data_path.parent(){
            let _=fs::create_dir_all(parent);
            let _=fs::write(parent.join("sync-status.json"),summary.to_string());
        }
    }
    pub fn update(&self,mut data:Value)->Result<(),String>{
        if data.get("machines").and_then(Value::as_array).is_none_or(|a|a.is_empty()){
            return Err("Refusing empty snapshot".into())
        }
        data["source"]=json!("mics-only");
        // Avoid rewriting identical machine values on failed cloud retries.
        let unchanged=self.snapshot.lock().ok().map(|old|
            old.get("machines")==data.get("machines")).unwrap_or(false);
        if unchanged{return Ok(())}
        let bytes=serde_json::to_vec_pretty(&data).map_err(|e|e.to_string())?;
        if let Some(parent)=self.data_path.parent(){fs::create_dir_all(parent).map_err(|e|e.to_string())?}
        let tmp=self.data_path.with_extension("json.tmp");
        fs::write(&tmp,bytes).map_err(|e|e.to_string())?;
        // Windows cannot replace an existing destination using fs::rename directly.
        // Move old snapshot aside first, restoring it if the replacement fails.
        let backup=self.data_path.with_extension("json.bak");
        if self.data_path.exists(){
            let _=fs::remove_file(&backup);
            fs::rename(&self.data_path,&backup).map_err(|e|e.to_string())?;
        }
        if let Err(e)=fs::rename(&tmp,&self.data_path){
            if backup.exists(){let _=fs::rename(&backup,&self.data_path);}
            return Err(e.to_string())
        }
        if let Ok(mut cache)=self.machines_cache.lock(){*cache=None;}
        if let Ok(mut v)=self.snapshot.lock(){*v=data}
        Ok(())
    }
}
fn reply(stream:&mut TcpStream,status:&str,kind:&str,body:&[u8],cache:&str){
    let headers=format!("HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nCache-Control: {cache}\r\nX-Content-Type-Options: nosniff\r\nX-Frame-Options: DENY\r\nContent-Security-Policy: default-src 'self'; style-src 'self' 'unsafe-inline'; script-src 'self'; img-src 'self' data:; connect-src 'self'\r\nConnection: close\r\n\r\n",body.len());
    let _=stream.write_all(headers.as_bytes());let _=stream.write_all(body);
}
fn www_dir()->PathBuf{
    std::env::current_exe().ok().and_then(|p|p.parent().map(|d|d.join("www")))
        .unwrap_or_else(||PathBuf::from("www"))
}
fn load_www_file(name:&str)->Option<Vec<u8>>{
    // Paths are static constants passed from the router, never user-provided.
    fs::read(www_dir().join(name)).ok()
}

// An HTML file alone does not mean that the Vue app can boot. Check the
// referenced Vite JS/CSS files before preferring it to the legacy dashboard.
fn vue_bundle_ready(dir:&Path)->bool{
    let entry=dir.join("index.html");
    if !entry.is_file(){return false}
    if VUE_ASSETS_STATE.load(Ordering::Relaxed)==1 {return true}
    let Ok(index)=fs::read_to_string(&entry) else{return false};
    let prefix=format!("/vue-v{}/",env!("CARGO_PKG_VERSION"));
    let mut found=0usize;
    for quoted in index.split(['"', '\'']) {
        if let Some(relative)=quoted.strip_prefix(&prefix) {
            if relative.starts_with("assets/") {
                if relative.contains("..") || !dir.join(relative).is_file(){return false}
                found+=1;
            }
        }
    }
    if found>0 {
        VUE_ASSETS_STATE.store(1,Ordering::Relaxed);
        true
    } else {
        false
    }
}

fn template_settings_file(state:&LocalState,route:&str)->Option<(PathBuf,&'static str)> {
    let name=match route {
        "/api/ui-template/aliases"|"/data/change-aliases.json"=>"change-aliases.json",
        "/api/ui-template/status-colors"|"/data/status-colors.json"=>"status-colors.json",
        _=>return None
    };
    Some((state.data_path.with_file_name(name),name))
}

fn alias_cloud_credentials(state:&LocalState)->Option<(String,String)> {
    state.alias_cloud.lock().ok().and_then(|c|c.clone())
}
// The executable owns all Cloudflare D1 access. Browser requests always use
// the on-disk cache and never wait for login/token/network.
fn record_alias_status(state:&LocalState,status:&str){
    if let Ok(mut v)=state.alias_sync_status.lock(){*v=status.to_owned();}
    if status=="SYNCED" {
        if let Ok(mut t)=state.alias_last_sync_at.lock(){
            *t=Some(chrono::Utc::now().to_rfc3339());
        }
    }
}
// D1 is the ONLY writable alias source; agent may read but never upload aliases.
// One authenticated pull replaces BOTH configurations (D1 -> local only).
pub fn sync_aliases_now(state:&LocalState)->Result<(),String>{
 let _lock=state.alias_file_lock.lock().map_err(|_|"LOCK_ERROR".to_string())?;
 let result=(||->Result<(),String>{
  let (url,token)=alias_cloud_credentials(state).ok_or("NOT_CONFIGURED")?;
  let base=url.strip_suffix("/aliases").ok_or("INVALID_CONFIG")?;
  let status_url=format!("{base}/status-colors");
  let get=|endpoint:&str,name:&str|->Result<Vec<u8>,String>{
   let response=ureq::AgentBuilder::new().timeout(Duration::from_secs(8)).build()
    .get(endpoint).set("Authorization",&format!("Bearer {token}")).call()
    .map_err(|e|match e{
      ureq::Error::Status(401|403,_)=>"AUTH_FAILED".to_string(),
      ureq::Error::Status(code,_)=>format!("HTTP_{code}"),
      _=>"NETWORK_ERROR".to_string()
    })?;
   let mut bytes=Vec::new();
   response.into_reader().take(128*1024+1).read_to_end(&mut bytes)
    .map_err(|_|"NETWORK_ERROR".to_string())?;
   validate_template_json(&bytes,name).map_err(|e|format!("INVALID_{name}: {e}"))?;
   Ok(bytes)
  };
  // Download and validate both before modifying either file.
  let aliases=get(&url,"change-aliases.json")?;
  let colors=get(&status_url,"status-colors.json")?;
  let alias_path=state.data_path.with_file_name("change-aliases.json");
  let color_path=state.data_path.with_file_name("status-colors.json");
  let old_alias=fs::read(&alias_path).ok();
  let old_color=fs::read(&color_path).ok();
  let alias_changed=old_alias.as_deref()!=Some(aliases.as_slice());
  let color_changed=old_color.as_deref()!=Some(colors.as_slice());
  if alias_changed{
   save_template_json(&alias_path,&aliases,"change-aliases.json")
    .map_err(|e|format!("ALIAS_CACHE_ERROR: {e}"))?;
   if let Some(dir)=state.data_path.parent(){
    if state.data_path.with_file_name("schedule-local.json").is_file(){
     let _=super::change_schedule::sync_for_data_dir(dir);
    }
   }
  }
  if color_changed{
   if let Err(e)=save_template_json(&color_path,&colors,"status-colors.json"){
    // A second-file write failure must not report a successful combined sync.
    if alias_changed{
     if let Some(prev)=old_alias{
      let _=save_template_json(&alias_path,&prev,"change-aliases.json");
     }else{let _=fs::remove_file(&alias_path);}
    }
    return Err(format!("STATUS_COLORS_CACHE_ERROR: {e}"))
   }
  }
  // Old change-aliases.pending.json files are never uploaded.
  Ok(())
 })();
 match result{
  Ok(())=>{
   if let Ok(mut t)=state.alias_last_check.lock(){*t=Some(Instant::now());}
   record_alias_status(state,"SYNCED");
   Ok(())
  },
  Err(e)=>{
   record_alias_status(state,&e);
   Err(e)
  }
 }
}
fn fetch_cloud_alias(state:&LocalState){
 let refresh=state.alias_last_check.lock().ok().map(|mut last|{
  if last.as_ref().is_some_and(|t|t.elapsed()<Duration::from_secs(300)){false}
  else{*last=Some(Instant::now());true}
 }).unwrap_or(false);
 if refresh{
  if let Err(e)=sync_aliases_now(state){eprintln!("ALIAS_D1_PULL_FAILED {e}")}
 }
}
pub fn start_alias_sync(state:LocalState){
 thread::spawn(move ||loop{
  fetch_cloud_alias(&state);
  thread::sleep(Duration::from_secs(300));
 });
}

fn validate_template_json(bytes:&[u8],name:&str)->Result<(),String>{
    if bytes.len()>128*1024 || bytes.is_empty() {return Err("Invalid settings size".into())}
    let value:Value=serde_json::from_slice(bytes).map_err(|_|"Malformed JSON".to_string())?;
    let valid=if name=="change-aliases.json" {
        ["color","mold","stamp"].iter().all(|kind|{
            value.get(kind).and_then(Value::as_array).is_some_and(|a|
                a.len()<=200 && a.iter().all(|r|{
                    r.get("canonical").and_then(Value::as_str).is_some_and(|v|v.len()<=200) &&
                    r.get("aliases").and_then(Value::as_array).is_some_and(|b|b.len()<=100 &&
                        b.iter().all(|v|v.as_str().is_some_and(|s|s.len()<=200)))
                }))
        })
    } else {
        let hex=|s:&str|s.len()==7&&s.starts_with('#')
            &&s.as_bytes()[1..].iter().all(|c|c.is_ascii_hexdigit());
        value.get("default").and_then(Value::as_object).is_some_and(|d|
            d.get("color").and_then(Value::as_str).is_some_and(hex) &&
            d.get("label").and_then(Value::as_str).is_some_and(|s|s.len()<=300)
        ) && value.get("codes").and_then(Value::as_object).is_some_and(|m|
            m.len()<=250 && m.iter().all(|(name,entry)|
                !name.is_empty()&&name.len()<=30 &&
                entry.get("color").and_then(Value::as_str).is_some_and(hex) &&
                entry.get("label").and_then(Value::as_str).is_some_and(|s|s.len()<=300)
            )
        )
    };
    if !valid{return Err("Invalid JSON settings schema".into())}
    Ok(())
}
fn save_template_json(dest:&Path,bytes:&[u8],name:&str)->Result<(),String>{
    validate_template_json(bytes,name)?;
    if let Some(parent)=dest.parent(){fs::create_dir_all(parent).map_err(|e|e.to_string())?;}
    let next=dest.with_extension("tmp");
    fs::write(&next,bytes).map_err(|e|e.to_string())?;
    let backup=dest.with_extension("json.bak");
    if dest.exists(){
        let _=fs::remove_file(&backup);
        fs::rename(dest,&backup).map_err(|e|e.to_string())?;
    }
    if let Err(e)=fs::rename(&next,dest){
        if backup.exists(){let _=fs::rename(&backup,dest);}
        return Err(e.to_string())
    }
    Ok(())
}
fn receive_template_json(stream:&mut TcpStream,first_bytes:&[u8],state:&LocalState,route:&str) {
    let Some((file,name))=template_settings_file(state,route) else {return};
    let Some(split)=first_bytes.windows(4).position(|w|w==b"\r\n\r\n") else {
        reply(stream,"400 Bad Request","text/plain",b"Malformed headers","no-store");return
    };
    let header=String::from_utf8_lossy(&first_bytes[..split]);
    // Same-origin requests only; Host must already be validated by serve().
    if let Some(origin)=header.lines().find_map(|l|l.split_once(':').and_then(|(k,v)|
        k.eq_ignore_ascii_case("Origin").then_some(v.trim().to_ascii_lowercase()))) {
        if !["http://127.0.0.1:8787","http://localhost:8787"].contains(&origin.as_str()){
            reply(stream,"403 Forbidden","text/plain",b"Origin rejected","no-store");return
        }
    }
    let length=header.lines().find_map(|l|l.split_once(':').and_then(|(k,v)|
        k.eq_ignore_ascii_case("Content-Length").then(||v.trim().parse::<usize>().ok()).flatten()));
    let Some(length)=length else {
        reply(stream,"411 Length Required","text/plain",b"Content-Length required","no-store");return
    };
    if length>128*1024{
        reply(stream,"413 Payload Too Large","text/plain",b"JSON too large","no-store");return
    }
    let mut body=first_bytes[split+4..].to_vec();
    let mut chunk=[0u8;8192];
    while body.len()<length {
        match stream.read(&mut chunk){
            Ok(0)|Err(_)=>break,
            Ok(n)=>body.extend_from_slice(&chunk[..n])
        }
    }
    if body.len()<length {
        reply(stream,"400 Bad Request","text/plain",b"Incomplete request body","no-store");return
    }
    // One-way D1 -> local: local browser must never save or upload alias edits.
    if name=="change-aliases.json"||name=="status-colors.json"{
        reply(stream,"405 Method Not Allowed","application/json; charset=utf-8",
            br#"{"ok":false,"error":"Read-only aliases. Edit in Machine-Control Admin and pull D1 -> local."}"#,"no-store");
        return
    }
    match save_template_json(&file,&body[..length],name){
        Ok(())=>reply(stream,"200 OK","application/json; charset=utf-8",br#"{"ok":true,"saved_local":true}"#,"no-store"),
        Err(e)=>reply(stream,"400 Bad Request","text/plain",e.as_bytes(),"no-store")
    }
}

// One loopback HTTP streaming connection per browser tab, NOT a D1/WebSocket read.
// Local file metadata is checked by a single agent-side watcher. No JSON polling
// requests are made by the browser while this stream remains connected.
fn serve_live_json(mut stream:TcpStream,state:&LocalState){
 let head=b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream; charset=utf-8\r\nCache-Control: no-cache, no-store\r\nConnection: keep-alive\r\nX-Content-Type-Options: nosniff\r\nX-Frame-Options: DENY\r\nContent-Security-Policy: default-src 'self'; connect-src 'self'\r\n\r\n";
 if stream.write_all(head).is_err()||stream.write_all(b"retry: 2500\n: connected\n\n").is_err(){return}
 let (tx,rx)=mpsc::channel::<&'static str>();
 match state.live_subscribers.lock(){
  Ok(mut clients)=>clients.push(tx),
  Err(_)=>return
 }
 loop{
  let message=match rx.recv_timeout(Duration::from_secs(15)){
   Ok(kind)=>format!("event: mics-json\ndata: {{\"kind\":\"{kind}\"}}\n\n"),
   Err(mpsc::RecvTimeoutError::Timeout)=>": heartbeat\n\n".to_string(),
   Err(mpsc::RecvTimeoutError::Disconnected)=>break
  };
  if stream.write_all(message.as_bytes()).is_err(){break}
 }
}
fn json_file_signature(path:&Path)->Option<(SystemTime,u64)>{
 let meta=fs::metadata(path).ok()?;
 Some((meta.modified().ok()?,meta.len()))
}
// Stable, cheap local-only version tokens for browser consistency checks.
// Metadata only: no JSON parsing, Firebird queries or Cloudflare requests.
fn json_version(path:&Path)->Option<String>{
 let (mtime,len)=json_file_signature(path)?;
 let nanos=mtime.duration_since(SystemTime::UNIX_EPOCH).ok()?.as_nanos();
 Some(format!("{nanos}:{len}"))
}
fn start_local_json_watcher(state:LocalState){
 thread::spawn(move||{
  let targets=[
   (state.data_path.clone(),"machines"),
   (state.data_path.with_file_name("schedule-local.json"),"schedule"),
   (state.data_path.with_file_name("change-schedule-week.json"),"change-schedule"),
   (state.data_path.with_file_name("change-aliases.json"),"settings"),
   (state.data_path.with_file_name("status-colors.json"),"settings")
  ];
  let mut known=targets.iter().map(|(path,_)|json_file_signature(path)).collect::<Vec<_>>();
  loop{
   // A single inexpensive metadata check; no D1/Turso requests, no JSON parsing.
   thread::sleep(Duration::from_secs(1));
   for (i,(path,kind)) in targets.iter().enumerate(){
    let next=json_file_signature(path);
    if next==known[i]{continue}
    known[i]=next;
    if next.is_none(){continue}
    if let Ok(mut clients)=state.live_subscribers.lock(){
     clients.retain(|tx|tx.send(kind).is_ok());
    }
   }
  }
 });
}

fn serve(mut stream:TcpStream,state:&LocalState) {
    let _=stream.set_nodelay(true);
    let _=stream.set_read_timeout(Some(Duration::from_millis(400)));
    let _=stream.set_write_timeout(Some(Duration::from_secs(3)));
    let mut buf=[0u8;8192];
    let Ok(n)=stream.read(&mut buf) else{return};
    let req=String::from_utf8_lossy(&buf[..n]);
    let first=req.lines().next().unwrap_or("");
    let mut fields=first.split_ascii_whitespace();
    let method=fields.next().unwrap_or("");
    let path=fields.next().unwrap_or("");
    if method!="GET"&&method!="HEAD"&&method!="PUT" {
        reply(&mut stream,"405 Method Not Allowed","text/plain",b"GET only","no-store");return
    }
    // Loopback binding protects network access. Host validation additionally
    // prevents other websites from using DNS rebinding to read local production data.
    let host=req.lines().find_map(|line|{
        let (name,value)=line.split_once(':')?;
        if name.eq_ignore_ascii_case("Host"){Some(value.trim().to_ascii_lowercase())}else{None}
    }).unwrap_or_default();
    if !matches!(host.as_str(),"127.0.0.1:8787"|"localhost:8787"|"[::1]:8787") {
        reply(&mut stream,"403 Forbidden","text/plain",b"Localhost only","no-store");return
    }

    let route=path.split('?').next().unwrap_or("");
    if route=="/api/json-versions"{
        if method!="GET" && method!="HEAD"{
            reply(&mut stream,"405 Method Not Allowed","text/plain",b"GET or HEAD required","no-store");return
        }
        let versions=json!({
            "ok":true,
            "machines":json_version(&state.data_path),
            "schedule":json_version(&state.data_path.with_file_name("schedule-local.json")),
            "change_schedule":json_version(&state.data_path.with_file_name("change-schedule-week.json")),
            "aliases":json_version(&state.data_path.with_file_name("change-aliases.json")),
            "colors":json_version(&state.data_path.with_file_name("status-colors.json")),
            "live_connections":state.live_subscribers.lock().map(|v|v.len()).unwrap_or(0)
        }).to_string();
        reply(&mut stream,"200 OK","application/json; charset=utf-8",
          if method=="HEAD"{&[]}else{versions.as_bytes()},"no-store");
        return
    }
    if route=="/api/live"{
        if method!="GET"{
            reply(&mut stream,"405 Method Not Allowed","text/plain",b"GET required","no-store");return
        }
        // Reject cross-site EventSource requests, even when Host is localhost.
        let origin=req.lines().find_map(|l|l.split_once(':').and_then(|(k,v)|
            k.eq_ignore_ascii_case("Origin").then_some(v.trim().to_ascii_lowercase())));
        if origin.as_ref().is_some_and(|v|v!="http://127.0.0.1:8787"&&v!="http://localhost:8787"){
            reply(&mut stream,"403 Forbidden","text/plain",b"Origin rejected","no-store");return
        }
        let _=stream.set_write_timeout(Some(Duration::from_secs(8)));
        serve_live_json(stream,state);
        return
    }
    if route=="/api/aliases/sync"{
        if method!="PUT"{
            reply(&mut stream,"405 Method Not Allowed","text/plain",b"PUT required","no-store");return
        }
        let allowed=req.lines().find_map(|l|l.split_once(':').and_then(|(k,v)|
            k.eq_ignore_ascii_case("Origin").then_some(v.trim().to_ascii_lowercase())))
            .is_none_or(|v|v=="http://127.0.0.1:8787"||v=="http://localhost:8787");
        if !allowed{
            reply(&mut stream,"403 Forbidden","text/plain",b"Origin rejected","no-store");return
        }
        match sync_aliases_now(state){
            Ok(())=>{
                let data=json!({"ok":true,"direction":"D1_TO_LOCAL","files":["change-aliases.json","status-colors.json"],"status":"SYNCED",
                   "synced_at":state.alias_last_sync_at.lock().ok().and_then(|v|v.clone())}).to_string();
                reply(&mut stream,"200 OK","application/json; charset=utf-8",data.as_bytes(),"no-store");
            },
            Err(e)=>{
                let data=json!({"ok":false,"direction":"D1_TO_LOCAL","error":e}).to_string();
                reply(&mut stream,"502 Bad Gateway","application/json; charset=utf-8",data.as_bytes(),"no-store");
            }
        }
        return
    }
    if let Some((file,name))=template_settings_file(state,route){
        if method=="PUT"{
            receive_template_json(&mut stream,&buf[..n],state,route);
            return
        }
        // Browser reads only local JSON; Cloudflare is handled by the agent thread.
        let fallback=www_dir().join(concat!("vue-v",env!("CARGO_PKG_VERSION"))).join("data").join(name);
        let raw=fs::read(file).or_else(|_|fs::read(fallback)).unwrap_or_else(|_|
            if name=="change-aliases.json"{b"{\"color\":[],\"mold\":[],\"stamp\":[]}".to_vec()}
            else{b"{\"default\":{},\"codes\":{}}".to_vec()});
        reply(&mut stream,"200 OK","application/json; charset=utf-8",if method=="HEAD"{&[]}else{&raw},"no-store");
        return
    }
    if method=="PUT"{
        reply(&mut stream,"405 Method Not Allowed","text/plain",b"PUT only for JSON settings","no-store");return
    }
    // Serve the two on-disk JSON snapshots directly over loopback.
    // Reuse validated in-memory bytes when the file timestamp is unchanged.
    if route=="/data/machines.json" {
        let mtime=fs::metadata(&state.data_path).and_then(|m|m.modified()).ok();
        if let Some(t)=mtime {
            if let Ok(guard)=state.machines_cache.lock() {
                if let Some((cached_t,cached_bytes))=guard.as_ref() {
                    if *cached_t==t {
                        reply(&mut stream,"200 OK","application/json; charset=utf-8",
                            if method=="HEAD"{&[]}else{cached_bytes},"no-store");
                        return
                    }
                }
            }
        }
        let valid=|bytes:&[u8]|->bool {
            serde_json::from_slice::<Value>(bytes).ok()
                .and_then(|v|v.get("machines").and_then(Value::as_array).cloned())
                .is_some_and(|rows|!rows.is_empty())
        };
        let from_main=fs::read(&state.data_path).ok().filter(|v|valid(v));
        if let (Some(t),Some(bytes))=(mtime,from_main.as_ref()) {
            if let Ok(mut guard)=state.machines_cache.lock() {
                *guard=Some((t,bytes.clone()));
            }
        }
        let raw=from_main
            .or_else(||fs::read(state.data_path.with_extension("json.bak")).ok().filter(|v|valid(v)))
            .unwrap_or_else(||serde_json::to_vec(&state.snapshot.lock()
                .map(|v|v.clone()).unwrap_or_else(|_|json!({"machines":[]}))).unwrap_or_default());
        reply(&mut stream,"200 OK","application/json; charset=utf-8",
            if method=="HEAD"{&[]}else{&raw},"no-store");
        return
    }
    if route=="/data/schedule-local.json" {
        let path=state.data_path.with_file_name("schedule-local.json");
        let mtime=fs::metadata(&path).and_then(|m|m.modified()).ok();
        if let Some(t)=mtime {
            if let Ok(guard)=state.schedule_cache.lock() {
                if let Some((cached_t,cached_bytes))=guard.as_ref() {
                    if *cached_t==t {
                        reply(&mut stream,"200 OK","application/json; charset=utf-8",
                            if method=="HEAD"{&[]}else{cached_bytes},"no-store");
                        return
                    }
                }
            }
        }
        let from_disk=fs::read(&path).ok().filter(|bytes|{
            serde_json::from_slice::<Value>(bytes).ok()
                .and_then(|v|v.get("rows").and_then(Value::as_array).cloned()).is_some()
        });
        if let (Some(t),Some(bytes))=(mtime,from_disk.as_ref()) {
            if let Ok(mut guard)=state.schedule_cache.lock() {
                *guard=Some((t,bytes.clone()));
            }
        }
        let raw=from_disk.unwrap_or_else(||b"{\"rows\":[],\"from\":null,\"to\":null}".to_vec());
        reply(&mut stream,"200 OK","application/json; charset=utf-8",
            if method=="HEAD"{&[]}else{&raw},"no-store");
        return
    }
    if route=="/data/change-schedule-week.json" {
        let path=state.data_path.with_file_name("change-schedule-week.json");
        let mtime=fs::metadata(&path).and_then(|m|m.modified()).ok();
        if let Some(t)=mtime {
            if let Ok(guard)=state.change_schedule_cache.lock() {
                if let Some((cached_t,cached_bytes))=guard.as_ref() {
                    if *cached_t==t {
                        reply(&mut stream,"200 OK","application/json; charset=utf-8",
                            if method=="HEAD"{&[]}else{cached_bytes},"no-store");
                        return
                    }
                }
            }
        }
        let from_disk=fs::read(&path).ok().filter(|bytes|{
            serde_json::from_slice::<Value>(bytes).ok()
                .and_then(|v|v.get("machines").and_then(Value::as_array).cloned()).is_some()
        });
        if let (Some(t),Some(bytes))=(mtime,from_disk.as_ref()) {
            if let Ok(mut guard)=state.change_schedule_cache.lock() {
                *guard=Some((t,bytes.clone()));
            }
        }
        let raw=from_disk.unwrap_or_else(super::change_schedule::empty_weekly_payload_bytes);
        reply(&mut stream,"200 OK","application/json; charset=utf-8",
            if method=="HEAD"{&[]}else{&raw},"no-store");
        return
    }
    // Versioned Vue bundle. The v0.2.5 updater preserves existing www files:
    // a new versioned directory is copied without replacing the legacy UI.
    let route=path.split('?').next().unwrap_or("");
    let vue_dir=www_dir().join(concat!("vue-v",env!("CARGO_PKG_VERSION")));
    let vue_entry=vue_dir.join("index.html");
    // Original uploaded template is the default; manual legacy=1 remains available.
    let legacy_opt_in=path.split("?").nth(1).is_some_and(|q|q.split("&").any(|v|v=="legacy=1"));
    let explicit_vue_asset=route.starts_with(concat!("/vue-v",env!("CARGO_PKG_VERSION"),"/"));

    if (!legacy_opt_in || explicit_vue_asset) && vue_bundle_ready(&vue_dir) {
        let requested=if matches!(route,"/"|"/index.html"){
            Some(vue_entry.clone())
        }else if matches!(route,"/change-schedule.html"|"/status-colors.html"|"/change-aliases.html"){
            Some(vue_dir.join(route.trim_start_matches('/')))
        }else if route.starts_with(concat!("/vue-v",env!("CARGO_PKG_VERSION"),"/")){
            let relative=route.trim_start_matches('/').strip_prefix(concat!("vue-v",env!("CARGO_PKG_VERSION"),"/")).unwrap_or("");
            if relative.len()<220 && !relative.is_empty() &&
                relative.bytes().all(|b|b.is_ascii_alphanumeric()||b"-_./".contains(&b)) &&
                !relative.contains("..") && !relative.contains("//") &&
                (relative.starts_with("assets/") || relative.starts_with("data/") ||
                 relative.starts_with("css/") || relative.starts_with("js/") ||
                 relative.starts_with("img/") || relative.starts_with("images/") ||
                 matches!(relative,"index.html"|"change-schedule.html"|"status-colors.html"|"change-aliases.html")) {
                Some(vue_dir.join(relative))
            }else{None}
        }else if route.starts_with("/css/") || route.starts_with("/js/") ||
                 route.starts_with("/data/") || route.starts_with("/images/") ||
                 route.starts_with("/img/") {
            let relative=route.trim_start_matches('/');
            if relative.len()<220 && !relative.contains("..") && !relative.contains("//") &&
                relative.bytes().all(|b|b.is_ascii_alphanumeric()||b"-_./".contains(&b)){
                Some(vue_dir.join(relative))
            }else{None}
        }else{None};
        if let Some(file)=requested{
            let kind=if route.ends_with(".js") {"application/javascript; charset=utf-8"}
                else if route.ends_with(".css") {"text/css; charset=utf-8"}
                else if route.ends_with(".json") {"application/json; charset=utf-8"}
                else if route.ends_with(".svg") {"image/svg+xml"}
                else if route.ends_with(".png") {"image/png"}
                else if route.ends_with(".jpg") || route.ends_with(".jpeg") {"image/jpeg"}
                else if route.ends_with(".webp") {"image/webp"}
                else if route.ends_with(".woff2") {"font/woff2"}
                else if route.ends_with(".woff") {"font/woff"}
                else if route.ends_with(".ico") {"image/x-icon"}
                else {"text/html; charset=utf-8"};
            let cache_policy=if explicit_vue_asset && (route.ends_with(".js") || route.ends_with(".css") || route.ends_with(".svg") || route.ends_with(".png") || route.ends_with(".woff2") || route.ends_with(".woff") || route.ends_with(".ico")) {
                "public, max-age=3600"
            } else {
                "no-cache"
            };
            let found=fs::read(file).ok();
            let is_found=found.is_some();
            let mut body=found.unwrap_or_default();
            if is_found && matches!(route,"/"|"/index.html") {
                if let Ok(html)=String::from_utf8(body.clone()) {
                    body=html.replace("</head>","<script src=\"/vue-diagnostics.js\" defer></script></head>").into_bytes();
                }
            }
            reply(&mut stream,if is_found{"200 OK"}else{"404 Not Found"},kind,
                if method=="HEAD"{&[]}else{&body},cache_policy);
            return
        }
    }
    let snapshot=||state.snapshot.lock().map(|v|v.clone()).unwrap_or(json!({}));
    let (status,kind,body,cache)=match path.split('?').next().unwrap_or("") {
        "/"|"/index.html"=>{
            // Keep the release marker for existing installed updaters which verify
            // that this version's bundled assets exist after replacing the EXE.
            let mut html=load_www_file("index.html").unwrap_or_default();
            let marker=format!("<!-- vue-v{}/ installed; stable HTML interface active -->",env!("CARGO_PKG_VERSION"));
            html.extend_from_slice(marker.as_bytes());
            ("200 OK","text/html; charset=utf-8",html,"no-cache")
        },
        "/vue-diagnostics.js"=>("200 OK","application/javascript; charset=utf-8",load_www_file("vue-diagnostics.js").unwrap_or_default(),"no-cache"),
        "/app.js"=>("200 OK","application/javascript; charset=utf-8",load_www_file("app.js").unwrap_or_default(),"no-cache"),
        "/style.css"=>("200 OK","text/css; charset=utf-8",load_www_file("style.css").unwrap_or_default(),"no-cache"),
        "/sw.js"=>("200 OK","application/javascript; charset=utf-8",load_www_file("sw.js").unwrap_or_default(),"no-cache"),
        "/manifest.webmanifest"=>("200 OK","application/manifest+json",load_www_file("manifest.webmanifest").unwrap_or_default(),"no-cache"),
        "/icon.svg"=>("200 OK","image/svg+xml",load_www_file("icon.svg").unwrap_or_default(),"public, max-age=86400"),
        "/api/machines"=>("200 OK","application/json; charset=utf-8",serde_json::to_vec(&snapshot()).unwrap_or_default(),"no-store"),
        "/api/schedule"=>{
            let path=state.data_path.with_file_name("schedule-local.json");
            let bytes=fs::read(path).unwrap_or_else(|_|b"{\"rows\":[],\"from\":null,\"to\":null}".to_vec());
            ("200 OK","application/json; charset=utf-8",bytes,"no-store")
        },
        "/api/change-schedule"=>{
            let path=state.data_path.with_file_name("change-schedule-week.json");
            let bytes=fs::read(path).unwrap_or_else(|_|super::change_schedule::empty_weekly_payload_bytes());
            ("200 OK","application/json; charset=utf-8",bytes,"no-store")
        },
        "/api/status"=>{
            let mode=state.mode.lock().map(|s|s.clone()).unwrap_or("UNKNOWN".into());
            let v=snapshot();
            ("200 OK","application/json; charset=utf-8",serde_json::to_vec(&json!({
                "ok":true,"version":env!("CARGO_PKG_VERSION"),"mode":mode,"sampled_at":v.get("sampled_at"),"machine_count":v["machines"].as_array().map(|a|a.len()).unwrap_or(0),
                "server":"127.0.0.1:8787",
                "sync_status":state.sync_status.lock().map(|s|s.clone()).unwrap_or("UNKNOWN".into()),
                "last_sync_at":state.last_sync_at.lock().ok().and_then(|v|v.clone()),
                "poll_interval_seconds":state.poll_interval_seconds.lock().ok().and_then(|v|*v),
                "next_poll_at":state.next_poll_at.lock().ok().and_then(|v|v.clone()),
                "alias_sync_status":state.alias_sync_status.lock().map(|s|s.clone()).unwrap_or("UNKNOWN".into()),
                "alias_last_sync_at":state.alias_last_sync_at.lock().ok().and_then(|v|v.clone())
            })).unwrap_or_default(),"no-store")
        }
        _=>("404 Not Found","text/plain",b"Not Found".to_vec(),"no-store")
    };
    // Missing or unreadable WWW files are never silently served as 200.
    let is_static=matches!(path.split('?').next().unwrap_or(""),
        "/"|"/index.html"|"/vue-diagnostics.js"|"/app.js"|"/style.css"|"/sw.js"|"/manifest.webmanifest"|"/icon.svg");
    let final_status=if is_static&&body.is_empty(){"404 Not Found"}else{status};
    reply(&mut stream,final_status,kind,if method=="HEAD"{&[]}else{&body},cache);
}
pub fn start(state:LocalState)->Result<(),String>{
    // Settings restarts the agent immediately; the old process may still be
    // releasing the local port for a brief moment.
    let mut listener=None;
    // Windows may retain the old listener while Stop-Process completes.
    // Retry for 20 seconds rather than exiting after only 3 seconds.
    for _ in 0..100 {
        match TcpListener::bind("127.0.0.1:8787"){
            Ok(l)=>{listener=Some(l);break},
            Err(_)=>thread::sleep(Duration::from_millis(200))
        }
    }
    let listener=listener.ok_or("LOCAL_WEB_BIND_FAILED: port 8787 busy after 20 second restart retry")?;
    start_local_json_watcher(state.clone());
    thread::spawn(move ||{
        println!("LOCAL_WEB_READY http://127.0.0.1:8787");
        for client in listener.incoming(){
            match client{
                Ok(stream)=>{
                    if ACTIVE_WEB_CLIENTS.fetch_add(1,Ordering::AcqRel)<32 {
                        let st=state.clone();
                        thread::spawn(move ||{
                            serve(stream,&st);
                            ACTIVE_WEB_CLIENTS.fetch_sub(1,Ordering::Release);
                        });
                    } else {
                        ACTIVE_WEB_CLIENTS.fetch_sub(1,Ordering::Release);
                        reply(&mut {let mut s=stream; s},"503 Service Unavailable","text/plain",b"Too many local clients","no-store");
                    }
                },
                Err(e)=>eprintln!("LOCAL_WEB_CLIENT_FAILED {e}")
            }
        }
    });
    Ok(())
}
pub fn default_snapshot_path()->PathBuf{
    // Portable data lives beside mics-only.exe, never in %LOCALAPPDATA%.
    // This also keeps machines, schedule, and sync status in one directory.
    let base=std::env::current_exe().ok()
        .and_then(|p|p.parent().map(Path::to_path_buf))
        .unwrap_or_else(||PathBuf::from("."));
    base.join("data").join("machines.json")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn local_state_starts_with_empty_snapshot(){
        let path=std::env::temp_dir().join("mics-only-test-nonexistent-snapshot.json");
        let state=LocalState::new(path);
        assert!(state.snapshot.lock().unwrap()["machines"].as_array().is_some());
    }
}

// Verify file-signature based notifications work without reading the JSON body.
#[cfg(test)]
mod live_json_tests {
    use super::{json_file_signature,json_version};
    use std::{fs,time::Duration};
    #[test]
    fn notices_detect_atomic_json_replacement() {
        let dir=std::env::temp_dir().join(format!("mics-json-live-{}",std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path=dir.join("machines.json");
        fs::write(&path,b"{\"machines\":[]}").unwrap();
        let old=json_file_signature(&path);
        assert!(old.is_some());
        // Different byte length is also detected if a filesystem has coarse timestamps.
        fs::write(&path,b"{\"machines\":[{\"machine_no\":19}]}").unwrap();
        let changed=json_file_signature(&path);
        assert_ne!(old,changed);
        fs::remove_file(&path).unwrap();
        assert_eq!(json_file_signature(&path),None);
        let _=fs::remove_dir(&dir);
        let _=Duration::from_millis(1);
    }

    #[test]
    fn json_versions_change_on_atomic_json_replacement(){
        let root=std::env::temp_dir().join(format!("mics-version-{}",std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let file=root.join("machines.json");
        assert!(json_version(&file).is_none());
        fs::write(&file,b"{\"machines\":[]}").unwrap();
        let old=json_version(&file).unwrap();
        fs::write(&file,b"{\"machines\":[{\"machine_no\":19}]}").unwrap();
        assert_ne!(json_version(&file).unwrap(),old);
        let _=fs::remove_file(&file);
        let _=fs::remove_dir(&root);
    }

    #[test]
    fn change_schedule_version_and_empty_payload_work_offline(){
        let root=std::env::temp_dir().join(format!("mics-chg-ver-{}",std::process::id()));
        let _=fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let file=root.join("change-schedule-week.json");
        assert!(json_version(&file).is_none());
        let empty: serde_json::Value=serde_json::from_slice(&crate::change_schedule::empty_weekly_payload_bytes()).unwrap();
        assert_eq!(empty["source"],"mics-only-offline");
        assert_eq!(empty["days"].as_array().unwrap().len(),7);
        fs::write(&file,b"{\"ok\":true,\"machines\":[]}").unwrap();
        assert!(json_version(&file).is_some());
        let _=fs::remove_dir_all(&root);
    }

}
