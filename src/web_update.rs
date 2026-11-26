//! Independent, opt-in web releases. No Firebird, lease or Core restart.
use serde::{Deserialize,Serialize};
use serde_json::{json,Value};
use std::{fs,path::{Path,PathBuf}};
const API:u64=1;
const REQUIRED:[&str;9]=["index.html","change-schedule.html","status-colors.html","change-aliases.html","assets/compat.js","assets/compiled-expressions.js","css/tailwind-generated.css","js/vanilla-runtime.js","js/dashboard-app.js"];
#[derive(Default,Debug,Clone,Serialize,Deserialize)]
struct Selection {current:Option<String>,previous:Option<String>}
fn version(s:&str)->Option<(u64,u64,u64)>{
    if s.len()>30{return None}
    let parts:Vec<_>=s.split('.').collect();
    if parts.len()!=3||parts.iter().any(|p|p.is_empty()||!p.bytes().all(|b|b.is_ascii_digit())||(p.len()>1&&p.starts_with('0'))){return None}
    Some((parts[0].parse().ok()?,parts[1].parse().ok()?,parts[2].parse().ok()?))
}
fn root()->PathBuf{std::env::current_exe().ok().and_then(|p|p.parent().map(Path::to_path_buf)).unwrap_or_else(||PathBuf::from("."))}
fn read_json(path:&Path)->Value{fs::read(path).ok().and_then(|b|serde_json::from_slice(&b).ok()).unwrap_or_default()}
fn state(root:&Path)->Selection{serde_json::from_value(read_json(&root.join("data/web-active.json"))).unwrap_or_default()}
fn valid_bundle(dir:&Path,v:&str)->bool{
    if version(v).is_none(){return false}
    let m=read_json(&dir.join("web-manifest.json"));
    m["web_version"].as_str()==Some(v)&&m["api_version"].as_u64()==Some(API)
        &&m["min_core"].as_str().and_then(version).zip(version(env!("CARGO_PKG_VERSION"))).is_some_and(|(a,b)|a<=b)
        &&REQUIRED.iter().all(|f|fs::metadata(dir.join(f)).is_ok_and(|m|m.is_file()&&m.len()>0))
}
pub fn explicit_dir(www:&Path,prefix:&str)->Option<PathBuf>{
    let v=prefix.strip_prefix("web-v")?;
    version(v)?;
    let p=www.join(prefix);
    valid_bundle(&p,v).then_some(p)
}
fn selected(root:&Path)->Option<String>{
    let s=state(root);
    [s.current,s.previous].into_iter().flatten().find(|v|explicit_dir(&root.join("www"),&format!("web-v{v}")).is_some())
}
pub fn active_dir(www:&Path)->Option<(String,PathBuf)>{
    let v=selected(www.parent()?)?;
    let prefix=format!("web-v{v}");
    let dir=explicit_dir(www,&prefix)?;
    Some((prefix,dir))
}
pub fn generation()->String{selected(&root()).map(|v|format!("web-v{v}")).unwrap_or_else(||format!("vue-v{}",env!("CARGO_PKG_VERSION")))}
pub fn status()->Value{
    let root=root();
    let s=state(&root);
    let mut out=read_json(&root.join("data/web-update-status.json"));
    if !out.is_object(){out=json!({})}
    out["web_version"]=json!(selected(&root));
    out["web_previous_version"]=json!(s.previous);
    out["web_update_supported"]=json!(cfg!(windows));
    out["generation"]=json!(generation());
    out
}
#[cfg(windows)]
mod windows {
    use super::*;
    use sha2::{Digest,Sha256};
    use std::{io::{Read,Write},os::windows::process::CommandExt,process::Command,sync::Mutex,thread,time::{Duration,Instant}};
    const LIMIT:u64=50_000_000;
    const ASSET:&str="MICS-Web.zip";
    static REQUEST_LOCK:Mutex<()>=Mutex::new(());
    fn err(e:impl std::fmt::Display)->String{e.to_string()}
    fn atomic(path:&Path,value:&impl Serialize)->Result<(),String>{
        let tmp=path.with_extension("tmp");
        let mut f=fs::File::create(&tmp).map_err(err)?;
        f.write_all(&serde_json::to_vec(value).map_err(err)?).map_err(err)?;
        f.sync_all().map_err(err)?;drop(f);
        fs::rename(tmp,path).map_err(err)
    }
    #[derive(Deserialize,Serialize,Debug)]
    struct Request{target:String,#[serde(default)]id:Option<i64>}
    fn target_valid(v:&str)->bool{v=="rollback"||version(v).is_some()}
    pub fn queue_remote(response:&Value){
        let cmd=&response["web_update_command"];
        let Some(id)=cmd["id"].as_i64().filter(|v|*v>0) else{return};
        let Some(target)=cmd["target"].as_str().filter(|v|target_valid(v)) else{return};
        let Ok(_guard)=REQUEST_LOCK.lock() else{return};
        let dir=root().join("data");
        let request=dir.join("web-update.request");
        let status=read_json(&dir.join("web-update-status.json"));
        if request.exists()||status["command_id"].as_i64().is_some_and(|n|n>=id){return}
        let _=fs::create_dir_all(&dir);
        let _=atomic(&request,&Request{target:target.to_owned(),id:Some(id)});
    }
    fn record(root:&Path,r:&Request,phase:&str,message:Option<&str>){
        let _=atomic(&root.join("data/web-update-status.json"),&json!({"command_id":r.id,"target":r.target,"status":phase,"message":message}));
    }
    fn download(root:&Path,v:&str)->Result<PathBuf,String>{
        let agent=ureq::AgentBuilder::new().timeout(Duration::from_secs(120)).build();
        let tag=format!("web-v{v}");
        let release:Value=agent.get(&format!("https://api.github.com/repos/maedandori/mics-only/releases/tags/{tag}")).set("User-Agent","mics-web-updater").call().map_err(err)?.into_json().map_err(err)?;
        if release["tag_name"].as_str()!=Some(tag.as_str())||release["draft"]==true{return Err("Invalid web release".into())}
        let asset=release["assets"].as_array().and_then(|a|a.iter().find(|a|a["name"]==ASSET)).ok_or("Web asset missing")?;
        let url=format!("https://github.com/maedandori/mics-only/releases/download/{tag}/{ASSET}");
        if asset["browser_download_url"].as_str()!=Some(url.as_str()){return Err("Invalid web asset URL".into())}
        let size=asset["size"].as_u64().filter(|s|*s>0&&*s<=LIMIT).ok_or("Invalid web ZIP size")?;
        let digest=asset["digest"].as_str().and_then(|s|s.strip_prefix("sha256:")).filter(|s|s.len()==64&&s.bytes().all(|b|b.is_ascii_hexdigit())).ok_or("SHA256 required")?;
        let stage=root.join("www").join(format!(".web-stage-{v}"));
        if stage.exists(){fs::remove_dir_all(&stage).map_err(err)?}
        fs::create_dir_all(&stage).map_err(err)?;
        let zip=stage.join("web.zip");let out=stage.join("unpacked");
        let mut file=fs::File::create(&zip).map_err(err)?;
        let mut reader=agent.get(&url).set("User-Agent","mics-web-updater").call().map_err(err)?.into_reader().take(LIMIT+1);
        let mut hash=Sha256::new();let mut total=0u64;let mut buf=[0u8;65536];
        loop{let n=reader.read(&mut buf).map_err(err)?;if n==0{break}total+=n as u64;hash.update(&buf[..n]);file.write_all(&buf[..n]).map_err(err)?;}
        file.sync_all().map_err(err)?;drop(file);
        if total!=size||!format!("{:x}",hash.finalize()).eq_ignore_ascii_case(digest){return Err("Web ZIP SHA256/size mismatch".into())}
        // Reject traversal, ambiguous Windows names, executables and oversized expansion before writing files.
        let ps=r#"$ErrorActionPreference='Stop'; Add-Type -AssemblyName System.IO.Compression.FileSystem; $z=[IO.Compression.ZipFile]::OpenRead($env:MICS_WEB_ZIP); try { $total=0L; $seen=@{}; foreach($e in $z.Entries) { $n=$e.FullName; if($n -notmatch '^[A-Za-z0-9_.-]+(/[A-Za-z0-9_.-]+)*/?$' -or $n -match '(^|/)\.{1,2}(/|$)' -or $n -match '(^|/)(CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9])(\.|/|$)' -or $n -match '[. ](/|$)' -or $seen.ContainsKey($n)) {throw 'Invalid ZIP entry'}; $seen[$n]=1; $total+=$e.Length; if($total -gt 150000000 -or $seen.Count -gt 5000){throw 'ZIP expansion limit'}; if($e.Name -and $e.Name -notmatch '\.(html|js|css|json|svg|png|jpg|jpeg|webp|woff|woff2|ico|txt|webmanifest|map|gif|avif|ttf|otf)$'){throw 'Unexpected web asset type'} } } finally { $z.Dispose() }; Expand-Archive -LiteralPath $env:MICS_WEB_ZIP -DestinationPath $env:MICS_WEB_OUT"#;
        let mut child=Command::new("powershell.exe").args(["-NoProfile","-NonInteractive","-Command",ps]).env("MICS_WEB_ZIP",&zip).env("MICS_WEB_OUT",&out).creation_flags(0x08000000).spawn().map_err(err)?;
        let start=Instant::now();
        loop{if let Some(s)=child.try_wait().map_err(err)?{if !s.success(){return Err("Web extraction failed".into())}break}if start.elapsed()>Duration::from_secs(90){let _=child.kill();let _=child.wait();return Err("Web extraction timeout".into())}thread::sleep(Duration::from_millis(200));}
        if !valid_bundle(&out,v){return Err("Web bundle incompatible or incomplete".into())}
        for page in &REQUIRED[..4]{
            let html=fs::read_to_string(out.join(page)).map_err(err)?;
            let prefix=format!("/web-v{v}/");
            let mut count=0;
            for part in html.split(['"','\'']){if let Some(rel)=part.strip_prefix(&prefix){if rel.contains("..")||!out.join(rel).is_file(){return Err("Missing referenced web asset".into())}count+=1;}}
            if count==0{return Err("Missing versioned asset references".into())}
        }
        let dest=root.join("www").join(tag);
        // Never overwrite a published generation, including one still used by a browser.
        if dest.exists(){return Err("Web version already exists; publish a new version".into())}
        fs::rename(&out,&dest).map_err(err)?;
        let _=fs::remove_dir_all(stage);
        Ok(dest)
    }
    fn apply(root:&Path,r:&Request)->Result<(),String>{
        let old=state(root);let current=selected(root);
        let next=if r.target=="rollback"{
            if let Some(v)=old.previous.as_deref(){if explicit_dir(&root.join("www"),&format!("web-v{v}")).is_none(){return Err("Previous web is unavailable".into())}}
            if current==old.previous{return Err("No different previous version".into())}
            Selection{current:old.previous,previous:current}
        }else{
            if version(&r.target).is_none(){return Err("Invalid web version".into())}
            if current.as_deref()==Some(&r.target){return Ok(())}
            let dest=root.join("www").join(format!("web-v{}",r.target));
            if !dest.exists(){download(root,&r.target)?;}
            if !valid_bundle(&dest,&r.target){return Err("Invalid installed web version".into())}
            Selection{current:Some(r.target.clone()),previous:current}
        };
        // Atomic pointer switch: old directory and live data never modified.
        atomic(&root.join("data/web-active.json"),&next)
    }
    pub fn start(){
        thread::spawn(||{
            let root=root();let dir=root.join("data");
            if fs::create_dir_all(&dir).is_err(){return}
            loop{
                let req=dir.join("web-update.request");
                let request=if let Ok(_guard)=REQUEST_LOCK.lock(){
                    fs::read_to_string(&req).ok().and_then(|s|{
                        let r=serde_json::from_str::<Request>(&s).ok().unwrap_or(Request{target:s.trim().to_owned(),id:None});
                        if target_valid(&r.target){record(&root,&r,"updating",None);}
                        let _=fs::remove_file(&req);
                        target_valid(&r.target).then_some(r)
                    })
                }else{None};
                if let Some(r)=request{
                    match apply(&root,&r){Ok(())=>record(&root,&r,"success",None),Err(e)=>record(&root,&r,"failed",Some(&e))}
                }
                thread::sleep(Duration::from_secs(2));
            }
        });
    }
}
#[cfg(windows)]pub use windows::{queue_remote,start};
#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn versions_are_path_safe(){assert_eq!(version("1.2.3"),Some((1,2,3)));for v in ["../1.2.3","1.2.3.exe","01.2.3","1.2","1.2.3/"]{assert!(version(v).is_none())}}
    #[test]fn invalid_prefix_is_not_a_web_directory(){assert!(explicit_dir(Path::new("."),"web-v../").is_none());assert!(explicit_dir(Path::new("."),"vue-v1.0.0").is_none())}
}
