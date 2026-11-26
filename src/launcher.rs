#![cfg_attr(windows, windows_subsystem = "windows")]
// Stable Windows supervisor. Core executables live beside the launcher so
// existing INI/DPAPI/JSON paths and the local dashboard URL remain unchanged.
#[cfg(windows)]
mod credentials;
#[cfg(windows)]
mod windows {
    use super::credentials;
    use serde::{Deserialize,Serialize};
    use serde_json::{json,Value};
    use sha2::{Digest,Sha256};
    use std::{env,fs,io::{Read,Write},path::{Path,PathBuf},process::{Child,Command,Stdio},thread,time::{Duration,Instant},sync::mpsc};
    use std::os::windows::{fs::OpenOptionsExt,io::AsRawHandle,process::CommandExt};
    use windows_sys::Win32::{Foundation::{CloseHandle,HANDLE},System::JobObjects::{CreateJobObjectW,SetInformationJobObject,AssignProcessToJobObject,JobObjectExtendedLimitInformation,JOBOBJECT_EXTENDED_LIMIT_INFORMATION,JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE}};
    const NO_WINDOW:u32=0x08000000;
    const LIMIT:u64=120_000_000;
    const ASSET:&str="MICS-Core-Windows.zip";
    const LAUNCHER_VERSION:&str="1.0.0";
    #[derive(Clone,Debug,Serialize,Deserialize)]
    struct Slot { name:String, version:String }
    #[derive(Clone,Debug,Serialize,Deserialize)]
    struct State { active:Slot, previous:Option<Slot> }
    struct Staged { version:String, path:PathBuf }
    struct Job(HANDLE);
    impl Job {
        fn new()->Result<Self,String>{
            unsafe{
                let h=CreateJobObjectW(std::ptr::null(),std::ptr::null());
                if h.is_null(){return Err("CreateJobObjectW failed".into())}
                let mut info:JOBOBJECT_EXTENDED_LIMIT_INFORMATION=std::mem::zeroed();
                info.BasicLimitInformation.LimitFlags=JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                if SetInformationJobObject(h,JobObjectExtendedLimitInformation,(&info as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),std::mem::size_of_val(&info) as u32)==0{
                    CloseHandle(h);return Err("SetInformationJobObject failed".into())
                }
                Ok(Self(h))
            }
        }
        fn attach(&self,child:&Child)->Result<(),String>{
            if unsafe{AssignProcessToJobObject(self.0,child.as_raw_handle() as HANDLE)}==0{Err("AssignProcessToJobObject failed".into())}else{Ok(())}
        }
    }
    impl Drop for Job{fn drop(&mut self){unsafe{CloseHandle(self.0);}}}
    fn err(e:impl std::fmt::Display)->String{e.to_string()}
    fn version(s:&str)->Option<(u64,u64,u64)>{
        let v:Vec<_>=s.split('.').collect();
        if v.len()!=3||v.iter().any(|p|p.is_empty()||!p.bytes().all(|b|b.is_ascii_digit())){return None}
        Some((v[0].parse().ok()?,v[1].parse().ok()?,v[2].parse().ok()?))
    }
    fn slot_path(root:&Path,name:&str)->Result<PathBuf,String>{
        if !["a","b"].contains(&name){return Err("Invalid core slot".into())}
        Ok(root.join(format!("mics-core-{name}.exe")))
    }
    fn alternate(s:&str)->&'static str{if s=="a"{"b"}else{"a"}}
    fn atomic(path:&Path,bytes:&[u8])->Result<(),String>{
        let temp=path.with_extension("tmp");
        let mut f=fs::File::create(&temp).map_err(err)?;
        f.write_all(bytes).map_err(err)?;f.sync_all().map_err(err)?;drop(f);
        fs::rename(&temp,path).map_err(err)
    }
    fn log(root:&Path,msg:&str){
        if let Ok(mut f)=fs::OpenOptions::new().create(true).append(true).open(root.join("data/launcher.log")){
            let _=writeln!(f,"{} {}",chrono::Utc::now().to_rfc3339(),msg);
        }
    }
    fn publish_status(root:&Path,state:&State,phase:&str){
        let value=json!({"launcher_version":LAUNCHER_VERSION,"core_version":state.active.version,"core_slot":state.active.name,"previous_core_version":state.previous.as_ref().map(|s|&s.version),"phase":phase});
        let _=atomic(&root.join("data/launcher-status.json"),value.to_string().as_bytes());
    }
    fn persist(root:&Path,state:&State)->Result<(),String>{
        atomic(&root.join("data/core-state.json"),&serde_json::to_vec(state).map_err(err)?)
    }
    fn validate_slot(root:&Path,s:&Slot)->bool{version(&s.version).is_some()&&slot_path(root,&s.name).is_ok_and(|p|p.is_file())}
    fn binary_version(path:&Path)->Result<String,String>{
        let mut child=Command::new(path).arg("--core-version").creation_flags(NO_WINDOW).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().map_err(err)?;
        let started=Instant::now();
        loop{
            if let Some(status)=child.try_wait().map_err(err)?{
                let mut out=String::new();
                if let Some(mut stdout)=child.stdout.take(){stdout.read_to_string(&mut out).map_err(err)?;}
                let out=out.trim().to_owned();
                return if status.success()&&version(&out).is_some(){Ok(out)}else{Err("Core version probe failed".into())}
            }
            if started.elapsed()>Duration::from_secs(10){let _=child.kill();let _=child.wait();return Err("Core version probe timed out".into())}
            thread::sleep(Duration::from_millis(100));
        }
    }
    fn load_state(root:&Path)->Result<State,String>{
        let path=root.join("data/core-state.json");
        if path.is_file(){
            let s:State=serde_json::from_slice(&fs::read(path).map_err(err)?).map_err(err)?;
            if !validate_slot(root,&s.active)||s.previous.as_ref().is_some_and(|p|!validate_slot(root,p)||p.name==s.active.name){return Err("Invalid stored core state".into())}
            return Ok(s)
        }
        let slot=slot_path(root,"a")?;
        let s=State{active:Slot{name:"a".into(),version:binary_version(&slot)?},previous:None};
        persist(root,&s)?;Ok(s)
    }
    fn launch(root:&Path,job:&Job,slot:&Slot,trial:Option<&str>)->Result<Child,String>{
        let mut cmd=Command::new(slot_path(root,&slot.name)?);
        cmd.current_dir(root).env("MICS_SUPERVISED","1").env_remove("MICS_TRIAL_OLD_VERSION").creation_flags(NO_WINDOW);
        if let Some(old)=trial{cmd.env("MICS_TRIAL_OLD_VERSION",old);}
        let mut child=cmd.spawn().map_err(err)?;
        if let Err(e)=job.attach(&child){let _=child.kill();let _=child.wait();return Err(e)}
        Ok(child)
    }
    fn stop(child:&mut Child)->Result<(),String>{
        if child.try_wait().map_err(err)?.is_none(){child.kill().map_err(err)?;}
        child.wait().map_err(err)?;Ok(())
    }
    fn health()->Option<Value>{
        ureq::AgentBuilder::new().timeout(Duration::from_secs(2)).build().get("http://127.0.0.1:8787/api/status").call().ok()?.into_json().ok()
    }
    fn matching(v:&Value,child:&Child,slot:&Slot)->bool{
        v["ok"]==true&&v["pid"].as_u64()==Some(child.id() as u64)&&v["version"].as_str()==Some(slot.version.as_str())
    }
    fn safe_to_switch(child:&Child,slot:&Slot)->bool{
        health().is_some_and(|v|matching(&v,child,slot)&&matches!(v["mode"].as_str(),Some("STANDBY"|"MIRROR_ONLY")))
    }
    fn wait_trial(root:&Path,child:&mut Child,slot:&Slot)->bool{
        let started=Instant::now();
        let ack=root.join(format!("data/core-presence-{}",child.id()));
        while started.elapsed()<Duration::from_secs(100){
            if !matches!(child.try_wait(),Ok(None)){return false}
            if health().is_some_and(|v|matching(&v,child,slot))
                &&fs::read_to_string(&ack).ok().as_deref()==Some(&slot.version)
                &&started.elapsed()>=Duration::from_secs(5){return true}
            thread::sleep(Duration::from_secs(1));
        }
        false
    }
    fn download(root:&Path,installed:&str)->Result<Option<Staged>,String>{
        let agent=ureq::AgentBuilder::new().timeout(Duration::from_secs(180)).build();
        let r:Value=agent.get("https://api.github.com/repos/maedandori/mics-only/releases/latest").set("User-Agent","mics-only-launcher").call().map_err(err)?.into_json().map_err(err)?;
        let tag=r["tag_name"].as_str().and_then(|v|v.strip_prefix('v')).ok_or("Missing release version")?;
        let latest=version(tag).ok_or("Invalid release version")?;
        if latest<=version(installed).ok_or("Invalid installed version")?{return Ok(None)}
        if fs::read_to_string(root.join("data/update-blocked-version.txt")).ok().is_some_and(|s|s.trim().trim_start_matches('v')==tag){return Ok(None)}
        let asset=r["assets"].as_array().and_then(|a|a.iter().find(|v|v["name"]==ASSET)).ok_or("Independent Core asset missing")?;
        let url=asset["browser_download_url"].as_str().ok_or("Missing release URL")?;
        let expected_url=format!("https://github.com/maedandori/mics-only/releases/download/v{tag}/{ASSET}");
        if url!=expected_url{return Err("Untrusted release URL".into())}
        let size=asset["size"].as_u64().filter(|s|*s>0&&*s<=LIMIT).ok_or("Invalid archive size")?;
        let digest=asset["digest"].as_str().and_then(|d|d.strip_prefix("sha256:")).filter(|d|d.len()==64&&d.bytes().all(|b|b.is_ascii_hexdigit())).ok_or("SHA256 release digest required")?;
        let path=root.join(format!("data/core-stage-{tag}"));
        if path.exists(){fs::remove_dir_all(&path).map_err(err)?}
        fs::create_dir_all(&path).map_err(err)?;
        let archive=path.join("core.zip");
        let mut f=fs::File::create(&archive).map_err(err)?;
        let mut reader=agent.get(url).set("User-Agent","mics-only-launcher").call().map_err(err)?.into_reader().take(LIMIT+1);
        let mut hash=Sha256::new();let mut total=0u64;let mut buf=[0u8;65536];
        loop{let n=reader.read(&mut buf).map_err(err)?;if n==0{break}f.write_all(&buf[..n]).map_err(err)?;hash.update(&buf[..n]);total+=n as u64;}
        f.sync_all().map_err(err)?;drop(f);
        if total!=size||!format!("{:x}",hash.finalize()).eq_ignore_ascii_case(digest){return Err("Core ZIP size/SHA256 mismatch".into())}
        let output=path.join("extracted");
        let mut unzip=Command::new("powershell.exe").args(["-NoProfile","-NonInteractive","-Command","$ErrorActionPreference='Stop'; Expand-Archive -LiteralPath $env:MICS_ARCHIVE -DestinationPath $env:MICS_EXTRACT -Force"]).env("MICS_ARCHIVE",&archive).env("MICS_EXTRACT",&output).creation_flags(NO_WINDOW).spawn().map_err(err)?;
        let start=Instant::now();
        loop{if let Some(status)=unzip.try_wait().map_err(err)?{if !status.success(){return Err("Core extraction failed".into())}break}
            if start.elapsed()>Duration::from_secs(90){let _=unzip.kill();let _=unzip.wait();return Err("Core extraction timed out".into())}thread::sleep(Duration::from_millis(200));}
        if binary_version(&output.join("mics-core.exe"))?!=tag{return Err("Core binary version mismatch".into())}
        if !output.join(format!("www/vue-v{tag}/index.html")).is_file(){return Err("Core web template missing".into())}
        Ok(Some(Staged{version:tag.to_owned(),path:output}))
    }
    fn copy_dir(from:&Path,to:&Path)->Result<(),String>{
        fs::create_dir_all(to).map_err(err)?;
        for entry in fs::read_dir(from).map_err(err)?{
            let e=entry.map_err(err)?;let ty=e.file_type().map_err(err)?;let dest=to.join(e.file_name());
            if ty.is_dir(){copy_dir(&e.path(),&dest)?}else if ty.is_file(){fs::copy(e.path(),dest).map_err(err)?;}else{return Err("Unexpected web asset type".into())}
        }Ok(())
    }
    fn install(root:&Path,state:&State,staged:&Staged)->Result<Slot,String>{
        let candidate=Slot{name:alternate(&state.active.name).into(),version:staged.version.clone()};
        let dest=slot_path(root,&candidate.name)?;
        let temp=dest.with_extension("new");
        fs::copy(staged.path.join("mics-core.exe"),&temp).map_err(err)?;
        fs::rename(&temp,&dest).map_err(err)?;
        let web=format!("vue-v{}",candidate.version);
        let web_dest=root.join("www").join(&web);
        let web_temp=root.join("www").join(format!(".{web}-staged"));
        if web_temp.exists(){fs::remove_dir_all(&web_temp).map_err(err)?}
        copy_dir(&staged.path.join("www").join(&web),&web_temp)?;
        if web_dest.exists(){fs::remove_dir_all(&web_dest).map_err(err)?}
        fs::rename(&web_temp,&web_dest).map_err(err)?;
        Ok(candidate)
    }
    // Only when no Core is alive. Never request a Firebird lease. Existing
    // presence contract remains compatible; standby=false avoids advertising
    // an unavailable Core as a failover candidate.
    fn fallback_presence(root:&Path,state:&State)->bool{
        let Ok(raw)=fs::read_to_string(root.join("mics.ini")) else{return false};
        let mut fields=std::collections::BTreeMap::new();let mut in_mics=false;
        for l in raw.trim_start_matches('\u{feff}').lines(){
            let l=l.trim();if l.starts_with('['){in_mics=l.eq_ignore_ascii_case("[mics]");continue}
            if in_mics{if let Some((k,v))=l.split_once('='){fields.insert(k.trim().to_ascii_lowercase(),v.trim());}}
        }
        let Some(endpoint)=fields.get("coordinator_url") else{return false};
        let Some(id)=fields.get("agent_id") else{return false};
        let Ok(u)=url::Url::parse(endpoint) else{return false};
        if u.scheme()!="https"||u.host_str().is_none()||!u.username().is_empty()||u.password().is_some()||u.query().is_some()||u.fragment().is_some(){return false}
        let Ok(secret)=credentials::load() else{return false};
        if secret.coordinator_token.is_empty(){return false}
        let failed=fs::read_to_string(root.join("data/update-blocked-version.txt")).ok().map(|s|s.trim().trim_start_matches('v').to_owned());
        let body=json!({"agent_id":id,"display_name":fields.get("display_name").unwrap_or(id),"role":"backup","standby_enabled":false,"version":state.active.version,"update_failed_version":failed,"launcher_version":LAUNCHER_VERSION,"core_state":"offline"});
        ureq::AgentBuilder::new().timeout(Duration::from_secs(5)).build().post(&format!("{}/presence",endpoint.trim_end_matches('/'))).set("Authorization",&format!("Bearer {}",secret.coordinator_token)).send_json(body).ok().and_then(|r|r.into_json::<Value>().ok()).is_some_and(|r|r["update_requested"]==true)
    }
    pub fn run()->Result<i32,String>{
        let exe=env::current_exe().map_err(err)?;
        let root=exe.parent().ok_or("Launcher directory missing")?.to_owned();
        fs::create_dir_all(root.join("data")).map_err(err)?;
        let args:Vec<_>=env::args_os().skip(1).collect();
        if !args.is_empty(){
            let state=load_state(&root)?;
            let status=Command::new(slot_path(&root,&state.active.name)?).args(args).current_dir(&root).creation_flags(NO_WINDOW).status().map_err(err)?;
            return Ok(status.code().unwrap_or(1))
        }
        // Exclusive Windows file handle is automatically released on exit.
        let _lock=match fs::OpenOptions::new().create(true).read(true).write(true).share_mode(0).open(root.join("data/launcher.lock")){Ok(f)=>f,Err(e) if matches!(e.raw_os_error(),Some(32|33))=>return Ok(0),Err(e)=>return Err(err(e))};
        let job=Job::new()?;
        let mut state=load_state(&root)?;
        let mut child:Option<Child>=None;
        let mut failures=0u32;
        let mut start_at=Instant::now();
        let mut next_restart=Instant::now();
        let mut next_presence=Instant::now();
        let mut next_check=Instant::now()+Duration::from_secs(300);
        let request=root.join("data/core-update.request");
        let blocked=root.join("data/update-blocked-version.txt");
        let mut downloading:Option<mpsc::Receiver<Result<Option<Staged>,String>>>=None;
        let mut staged:Option<Staged>=None;
        loop{
            if let Some(c)=child.as_mut(){
                if !matches!(c.try_wait(),Ok(None)){
                    log(&root,"CORE_EXITED");let _=c.wait();child=None;
                    failures=if start_at.elapsed()>Duration::from_secs(120){1}else{failures.saturating_add(1)};
                    next_restart=Instant::now()+Duration::from_secs((5u64*failures as u64).min(60));
                }
            }
            if child.is_none(){
                if Instant::now()>=next_presence{
                    if fallback_presence(&root,&state){let _=fs::write(&request,"admin");}
                    next_presence=Instant::now()+Duration::from_secs(60);
                }
                if failures>=3{
                    if let Some(previous)=state.previous.clone(){
                        let failed=state.active.version.clone();
                        let restored=State{active:previous,previous:None};
                        persist(&root,&restored)?;state=restored;
                        let _=fs::write(&blocked,&failed);log(&root,"CORE_RECOVERED_PREVIOUS");
                        failures=0;
                    }
                }
                if Instant::now()>=next_restart{
                    match launch(&root,&job,&state.active,None){
                        Ok(c)=>{child=Some(c);start_at=Instant::now();log(&root,"CORE_STARTED");}
                        Err(e)=>{log(&root,&format!("CORE_START_FAILED {e}"));failures=failures.saturating_add(1);next_restart=Instant::now()+Duration::from_secs(30);}
                    }
                }
            }
            if let Some(rx)=downloading.as_ref(){
                match rx.try_recv(){
                    Ok(result)=>{downloading=None;match result{
                        Ok(Some(s))=>{log(&root,"CORE_STAGED");staged=Some(s);}
                        Ok(None)=>log(&root,"CORE_NO_UPDATE"),
                        Err(e)=>log(&root,&format!("CORE_DOWNLOAD_FAILED {e}")),
                    }}
                    Err(mpsc::TryRecvError::Disconnected)=>{downloading=None;log(&root,"CORE_DOWNLOAD_THREAD_FAILED");}
                    Err(mpsc::TryRecvError::Empty)=>{}
                }
            }
            let can_switch=child.as_ref().map_or(true,|c|safe_to_switch(c,&state.active));
            if downloading.is_none()&&staged.is_none()&&(request.is_file()||Instant::now()>=next_check)&&can_switch{
                let _=fs::remove_file(&request);
                next_check=Instant::now()+Duration::from_secs(21600);
                let dir=root.clone();let installed=state.active.version.clone();let (tx,rx)=mpsc::channel();
                thread::spawn(move||{let _=tx.send(download(&dir,&installed));});
                downloading=Some(rx);
            }
            if staged.is_some()&&can_switch{
                let update=staged.take().unwrap();
                // The inactive slot may contain the previous version. Clear its
                // persisted pointer before overwriting it; the current Core remains intact.
                if state.previous.is_some(){let next=State{active:state.active.clone(),previous:None};persist(&root,&next)?;state=next;}
                match install(&root,&state,&update){
                    Ok(candidate)=>{
                        if child.as_ref().is_some_and(|c|!safe_to_switch(c,&state.active)){staged=Some(update);}
                        else{
                            if let Some(c)=child.as_mut(){stop(c)?;}child=None;
                            publish_status(&root,&state,"UPDATING");
                            let mut trial=launch(&root,&job,&candidate,Some(&state.active.version)).ok();
                            let healthy=trial.as_mut().is_some_and(|c|wait_trial(&root,c,&candidate));
                            if healthy{
                                let c=trial.as_ref().unwrap();
                                let promoted=State{active:candidate.clone(),previous:Some(state.active.clone())};
                                if persist(&root,&promoted).is_ok(){
                                    state=promoted;
                                    let _=fs::write(root.join(format!("data/core-promoted-{}",c.id())),"ok");
                                    let _=fs::remove_file(&blocked);
                                    child=trial.take();failures=0;start_at=Instant::now();
                                    log(&root,"CORE_UPDATE_SUCCESS");
                                }
                            }
                            if let Some(mut c)=trial{stop(&mut c)?;}
                            if child.is_none(){
                                let _=fs::write(&blocked,&candidate.version);
                                log(&root,"CORE_UPDATE_FAILED_PREVIOUS_RETAINED");
                                next_restart=Instant::now();
                            }
                        }
                    }
                    Err(e)=>{let _=fs::write(&blocked,&update.version);log(&root,&format!("CORE_INSTALL_FAILED {e}"));}
                }
            }
            let phase=if downloading.is_some(){"DOWNLOADING"}else if staged.is_some(){"STAGED"}else if child.is_some(){"RUNNING"}else{"RECOVERING"};
            publish_status(&root,&state,phase);
            thread::sleep(Duration::from_secs(2));
        }
    }
    #[cfg(test)]
    mod tests{
        use super::*;
        #[test]fn version_and_slots(){assert!(version("0.2.66")>version("0.2.65"));assert_eq!(version("0.2.66.exe"),None);assert_eq!(alternate("a"),"b");assert!(slot_path(Path::new("."),"../other").is_err());}
    }
}
#[cfg(windows)]
fn main(){
    match windows::run(){Ok(code)=>std::process::exit(code),Err(e)=>{
        if let Ok(p)=std::env::current_exe(){if let Some(root)=p.parent(){let _=std::fs::write(root.join("launcher-startup.log"),e);}}
        std::process::exit(1)
    }}
}
#[cfg(not(windows))]
fn main(){eprintln!("MICS Launcher requires Windows.");}
