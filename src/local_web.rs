//! Loopback-only HTTP server. Never bind to a LAN/public interface.
use serde_json::{json, Value};
use std::{fs, io::{Read,Write}, net::{TcpListener,TcpStream}, path::{Path,PathBuf}, sync::{Arc,Mutex}, thread, time::{Duration,Instant}};

#[derive(Clone)]
pub struct LocalState {
    pub snapshot: Arc<Mutex<Value>>,
    pub mode: Arc<Mutex<String>>,
    pub data_path: PathBuf,
    pub sync_status: Arc<Mutex<String>>,
    pub last_sync_at: Arc<Mutex<Option<String>>>,
    pub alias_cloud: Arc<Mutex<Option<(String,String)>>>,
    pub alias_last_check: Arc<Mutex<Option<Instant>>>,
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
            alias_cloud:Arc::new(Mutex::new(None)),alias_last_check:Arc::new(Mutex::new(None)),data_path}
    }
    pub fn set_alias_sync(&self,coordinator_url:&str,token:&str){
        if coordinator_url.starts_with("https://") && !token.is_empty(){
            if let Ok(mut conn)=self.alias_cloud.lock(){
                *conn=Some((format!("{}/aliases",coordinator_url.trim_end_matches('/')),token.to_owned()));
            }
        }
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
    let Ok(index)=fs::read_to_string(dir.join("index.html")) else{return false};
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
    found>0
}

fn template_settings_file(state:&LocalState,route:&str)->Option<(PathBuf,&'static str)> {
    let name=match route {
        "/api/ui-template/aliases"=>"change-aliases.json",
        "/api/ui-template/status-colors"=>"status-colors.json",
        _=>return None
    };
    Some((state.data_path.with_file_name(name),name))
}

fn alias_cloud_credentials(state:&LocalState)->Option<(String,String)> {
    state.alias_cloud.lock().ok().and_then(|c|c.clone())
}
fn fetch_cloud_alias(state:&LocalState) {
    // Only a UI request triggers the 60-second refresh. No extra Firebird or
    // machine snapshot request; agent coordinator heartbeat is unaffected.
    let refresh=state.alias_last_check.lock().ok().map(|mut last|{
        if last.as_ref().is_some_and(|t|t.elapsed()<Duration::from_secs(60)){false}
        else{*last=Some(Instant::now());true}
    }).unwrap_or(false);
    if !refresh{return}
    let Some((url,token))=alias_cloud_credentials(state) else{return};
    let response=ureq::AgentBuilder::new().timeout(Duration::from_secs(6)).build()
        .get(&url).set("Authorization",&format!("Bearer {token}")).call();
    if let Ok(resp)=response{
        let mut bytes=Vec::new();
        let reader=resp.into_reader();
        let _=reader.take(128*1024+1).read_to_end(&mut bytes);
        if bytes.len()<=128*1024 && !bytes.is_empty(){
            let path=state.data_path.with_file_name("change-aliases.json");
            if fs::read(&path).ok().as_deref()!=Some(bytes.as_slice()){
                let _=save_template_json(&path,&bytes,"change-aliases.json");
            }
        }
    }
}
fn put_cloud_alias(state:&LocalState,body:&[u8])->Result<(),String>{
    let (url,token)=alias_cloud_credentials(state)
        .ok_or("Cloud alias sync not configured: save Coordinator Token in MICS-only Settings")?;
    // Validate locally before sending. Cloudflare also validates the same schema.
    let value:Value=serde_json::from_slice(body).map_err(|_|"Invalid alias JSON")?;
    if !["color","mold","stamp"].iter().all(|kind|value.get(kind).and_then(Value::as_array).is_some())
       {return Err("Invalid alias configuration".into())}
    let result=ureq::AgentBuilder::new().timeout(Duration::from_secs(10)).build()
        .put(&url).set("Authorization",&format!("Bearer {token}"))
        .set("Content-Type","application/json").send_bytes(body);
    match result{
        Ok(resp) if resp.status()==200=>{
            let path=state.data_path.with_file_name("change-aliases.json");
            save_template_json(&path,body,"change-aliases.json")?;
            if let Ok(mut last)=state.alias_last_check.lock(){*last=Some(Instant::now());}
            Ok(())
        }
        Ok(resp)=>Err(format!("Cloudflare alias sync HTTP {}",resp.status())),
        Err(e)=>Err(format!("Cloudflare alias sync unavailable: {e}"))
    }
}
fn save_template_json(dest:&Path,bytes:&[u8],name:&str)->Result<(),String>{
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
        value.get("default").and_then(Value::as_object).is_some() &&
        value.get("codes").and_then(Value::as_object).is_some_and(|m|m.len()<=250)
    };
    if !valid{return Err("Invalid JSON settings schema".into())}
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
    let res=if name=="change-aliases.json" {
        put_cloud_alias(state,&body[..length])
    }else{save_template_json(&file,&body[..length],name)};
    match res{
        Ok(())=>reply(stream,"200 OK","application/json; charset=utf-8",b"{\"ok\":true}","no-store"),
        Err(e)=>reply(stream,"400 Bad Request","text/plain",e.as_bytes(),"no-store")
    }
}

fn serve(mut stream:TcpStream,state:&LocalState) {
    let _=stream.set_read_timeout(Some(Duration::from_secs(3)));
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
    if let Some((file,name))=template_settings_file(state,route){
        if method=="PUT"{
            receive_template_json(&mut stream,&buf[..n],state,route);
            return
        }
        if name=="change-aliases.json"{fetch_cloud_alias(state);}
        let fallback=www_dir().join(concat!("vue-v",env!("CARGO_PKG_VERSION"))).join("data").join(name);
        let raw=fs::read(file).or_else(|_|fs::read(fallback)).unwrap_or_else(|_|b"{}".to_vec());
        reply(&mut stream,"200 OK","application/json; charset=utf-8",if method=="HEAD"{&[]}else{&raw},"no-store");
        return
    }
    if method=="PUT"{
        reply(&mut stream,"405 Method Not Allowed","text/plain",b"PUT only for JSON settings","no-store");return
    }
    // Versioned Vue bundle. The v0.2.5 updater preserves existing www files:
    // a new versioned directory is copied without replacing the legacy UI.
    let route=path.split('?').next().unwrap_or("");
    let vue_dir=www_dir().join(concat!("vue-v",env!("CARGO_PKG_VERSION")));
    let vue_entry=vue_dir.join("index.html");
    let legacy_ui=path.split("?").nth(1).is_some_and(|q|q.split("&").any(|v|v=="legacy=1"));

    if !legacy_ui && vue_bundle_ready(&vue_dir) {
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
                else {"text/html; charset=utf-8"};
            let found=fs::read(file).ok();
            let is_found=found.is_some();
            let body=found.unwrap_or_default();
            reply(&mut stream,if is_found{"200 OK"}else{"404 Not Found"},kind,
                if method=="HEAD"{&[]}else{&body},"no-cache");
            return
        }
    }
    let snapshot=||state.snapshot.lock().map(|v|v.clone()).unwrap_or(json!({}));
    let (status,kind,body,cache)=match path.split('?').next().unwrap_or("") {
        "/"|"/index.html"=>("200 OK","text/html; charset=utf-8",load_www_file("index.html").unwrap_or_default(),"no-cache"),
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
        "/api/status"=>{
            let mode=state.mode.lock().map(|s|s.clone()).unwrap_or("UNKNOWN".into());
            let v=snapshot();
            ("200 OK","application/json; charset=utf-8",serde_json::to_vec(&json!({
                "ok":true,"version":env!("CARGO_PKG_VERSION"),"mode":mode,"sampled_at":v.get("sampled_at"),"machine_count":v["machines"].as_array().map(|a|a.len()).unwrap_or(0),
                "server":"127.0.0.1:8787",
                "sync_status":state.sync_status.lock().map(|s|s.clone()).unwrap_or("UNKNOWN".into()),
                "last_sync_at":state.last_sync_at.lock().ok().and_then(|v|v.clone())
            })).unwrap_or_default(),"no-store")
        }
        _=>("404 Not Found","text/plain",b"Not Found".to_vec(),"no-store")
    };
    // Missing or unreadable WWW files are never silently served as 200.
    let is_static=matches!(path.split('?').next().unwrap_or(""),
        "/"|"/index.html"|"/app.js"|"/style.css"|"/sw.js"|"/manifest.webmanifest"|"/icon.svg");
    let final_status=if is_static&&body.is_empty(){"404 Not Found"}else{status};
    reply(&mut stream,final_status,kind,if method=="HEAD"{&[]}else{&body},cache);
}
pub fn start(state:LocalState)->Result<(),String>{
    // Settings restarts the agent immediately; the old process may still be
    // releasing the local port for a brief moment.
    let mut listener=None;
    for _ in 0..20 {
        match TcpListener::bind("127.0.0.1:8787"){
            Ok(l)=>{listener=Some(l);break},
            Err(_)=>thread::sleep(Duration::from_millis(150))
        }
    }
    let listener=listener.ok_or("LOCAL_WEB_BIND_FAILED: port 8787 busy after restart")?;
    thread::spawn(move ||{
        println!("LOCAL_WEB_READY http://127.0.0.1:8787");
        for client in listener.incoming(){
            match client{
                Ok(stream)=>serve(stream,&state),
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
