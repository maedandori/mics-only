//! Loopback-only HTTP server. Never bind to a LAN/public interface.
use serde_json::{json, Value};
use std::{fs, io::{Read,Write}, net::{TcpListener,TcpStream}, path::{Path,PathBuf}, sync::{Arc,Mutex}, thread, time::Duration};

#[derive(Clone)]
pub struct LocalState {
    pub snapshot: Arc<Mutex<Value>>,
    pub mode: Arc<Mutex<String>>,
    pub data_path: PathBuf,
    pub sync_status: Arc<Mutex<String>>,
    pub last_sync_at: Arc<Mutex<Option<String>>>,
}
impl LocalState {
    pub fn new(data_path:PathBuf)->Self {
        let initial=fs::read(&data_path).ok()
            .and_then(|raw|serde_json::from_slice::<Value>(&raw).ok())
            .filter(|v|v.get("machines").and_then(Value::as_array).is_some())
            .unwrap_or_else(||json!({"source":"mics-only","machines":[],"sampled_at":null}));
        Self{snapshot:Arc::new(Mutex::new(initial)),mode:Arc::new(Mutex::new("STANDBY".into())),
            sync_status:Arc::new(Mutex::new("NOT_SYNCED".into())),last_sync_at:Arc::new(Mutex::new(None)),data_path}
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
    let headers=format!("HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nCache-Control: {cache}\r\nX-Content-Type-Options: nosniff\r\nX-Frame-Options: DENY\r\nContent-Security-Policy: default-src 'self'; style-src 'self'; script-src 'self'; img-src 'self' data:; connect-src 'self'\r\nConnection: close\r\n\r\n",body.len());
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
    if method!="GET"&&method!="HEAD" {
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
    let snapshot=||state.snapshot.lock().map(|v|v.clone()).unwrap_or(json!({}));
    let (status,kind,body,cache)=match path.split('?').next().unwrap_or("") {
        "/"|"/index.html"=>("200 OK","text/html; charset=utf-8",load_www_file("index.html").unwrap_or_default(),"no-cache"),
        "/app.js"=>("200 OK","application/javascript; charset=utf-8",load_www_file("app.js").unwrap_or_default(),"no-cache"),
        "/style.css"=>("200 OK","text/css; charset=utf-8",load_www_file("style.css").unwrap_or_default(),"no-cache"),
        "/sw.js"=>("200 OK","application/javascript; charset=utf-8",load_www_file("sw.js").unwrap_or_default(),"no-cache"),
        "/manifest.webmanifest"=>("200 OK","application/manifest+json",load_www_file("manifest.webmanifest").unwrap_or_default(),"no-cache"),
        "/icon.svg"=>("200 OK","image/svg+xml",load_www_file("icon.svg").unwrap_or_default(),"public, max-age=86400"),
        "/api/machines"=>("200 OK","application/json; charset=utf-8",serde_json::to_vec(&snapshot()).unwrap_or_default(),"no-store"),
        "/api/status"=>{
            let mode=state.mode.lock().map(|s|s.clone()).unwrap_or("UNKNOWN".into());
            let v=snapshot();
            ("200 OK","application/json; charset=utf-8",serde_json::to_vec(&json!({
                "ok":true,"mode":mode,"sampled_at":v.get("sampled_at"),"machine_count":v["machines"].as_array().map(|a|a.len()).unwrap_or(0),
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
    let base=std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
        .or_else(||std::env::current_exe().ok().and_then(|p|p.parent().map(Path::to_path_buf)))
        .unwrap_or_else(||PathBuf::from("."));
    base.join("MICS-Only").join("data").join("machines.json")
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
