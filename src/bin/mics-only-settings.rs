#![cfg_attr(windows, windows_subsystem = "windows")]
#[path = "../credentials.rs"] mod credentials;

#[cfg(windows)]
fn main() {
    use native_windows_gui as nwg;
    use std::{cell::RefCell, collections::BTreeMap, fs, path::PathBuf, process::Command, rc::Rc};
    use std::os::windows::process::CommandExt;
    nwg::init().expect("Cannot initialize Windows GUI");
    nwg::Font::set_global_family("Segoe UI").expect("Font error");

    let home = std::env::current_exe().ok().and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));
    let ini = home.join("mics.ini");
    let mut original = BTreeMap::<String,String>::new();
    if let Ok(contents) = fs::read_to_string(&ini) {
        let mut section = false;
        for line in contents.trim_start_matches('\u{feff}').lines() {
            let line = line.trim();
            if line.starts_with('[') {section = line.eq_ignore_ascii_case("[mics]");continue}
            if !section || line.starts_with(';') || line.starts_with('#') {continue}
            if let Some((k,v))=line.split_once('=') {original.insert(k.trim().to_lowercase(),v.trim().into());}
        }
    }
    let defaults = [
        ("enabled","0"), ("agent_id","mics-backup-01"),
        ("host",""),("port","3050"),("database",""),("user","READONLY"),
        ("coordinator_url",""),("interval_seconds","30")
    ];
    let get = |k:&str,d:&str| original.get(k).cloned().unwrap_or_else(||d.into());

    let mut window=nwg::Window::default();
    nwg::Window::builder().size((790,790)).position((190,65))
        .title("MICS Only Settings / MICS 予備機設定")
        .build(&mut window).expect("Window error");

    let mut heading=nwg::Label::default();
    nwg::Label::builder().text("MICS Only | BACKUP Firebird Reader").parent(&window)
        .position((24,15)).size((730,30)).build(&mut heading).unwrap();

    let mut enable=nwg::CheckBox::default();
    nwg::CheckBox::builder().text("Enable standby agent / 予備機を有効化").parent(&window)
        .position((25,52)).size((510,30))
        .check_state(if get("enabled","0")=="1" {nwg::CheckBoxState::Checked}else{nwg::CheckBoxState::Unchecked})
        .build(&mut enable).unwrap();

    let mut labels=Vec::<nwg::Label>::new();
    let mut inputs=Vec::<nwg::TextInput>::new();
    let names=[
        ("Agent ID / 予備機ID",get("agent_id","mics-backup-01")),
        ("Firebird Host / サーバー",get("host","")),
        ("Firebird Port",get("port","3050")),
        ("Database Server Path / DB パス",get("database","")),
        ("Firebird User / ユーザー",get("user","READONLY")),
        ("Cloudflare Coordinator URL",get("coordinator_url","")),
        ("Sync interval (seconds) / 同期間隔",get("interval_seconds","30")),
    ];
    for (index,(name,value)) in names.iter().enumerate() {
        let y=94+(index as i32)*48;
        let mut label=nwg::Label::default();
        nwg::Label::builder().text(name).parent(&window).position((25,y))
            .size((300,28)).build(&mut label).unwrap();
        let mut input=nwg::TextInput::default();
        nwg::TextInput::builder().text(value).parent(&window).position((335,y))
            .size((425,29)).build(&mut input).unwrap();
        labels.push(label);inputs.push(input);
    }
    let mut secrets=Vec::<nwg::TextInput>::new();
    let secret_names=[
        "Firebird Password / DB パスワード",
        "Backup MICS Sync Token / 専用同期トークン",
        "Backup Coordinator Token / 予備機専用トークン"
    ];
    let mut secret_labels=Vec::<nwg::Label>::new();
    for (index,name) in secret_names.iter().enumerate() {
        let y=417+(index as i32)*50;
        let mut label=nwg::Label::default();
        nwg::Label::builder().text(name).parent(&window).position((25,y))
            .size((305,28)).build(&mut label).unwrap();
        let mut input=nwg::TextInput::default();
        nwg::TextInput::builder().password(Some('*')).parent(&window)
            .position((335,y)).size((425,29)).build(&mut input).unwrap();
        secret_labels.push(label);secrets.push(input);
    }
    let mut autostart=nwg::CheckBox::default();
    // HKCU Run starts the agent at Windows sign-in, without administrator rights.
    let run_key=r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
    let startup_enabled=Command::new("reg")
        .args(["query",run_key,"/v","MICS-Only-Agent"])
        .creation_flags(0x08000000).output()
        .map(|o|o.status.success()).unwrap_or(false);
    nwg::CheckBox::builder()
        .text("Start agent with Windows / Windows起動時に実行")
        .parent(&window).position((25,565)).size((680,28))
        .check_state(if startup_enabled {nwg::CheckBoxState::Checked}else{nwg::CheckBoxState::Unchecked})
        .build(&mut autostart).unwrap();

    let mut note=nwg::Label::default();
    nwg::Label::builder().text(
        "Blank secret fields retain DPAPI values. Tokens are separate from mics-print."
    ).parent(&window).position((25,594)).size((735,25)).build(&mut note).unwrap();

    let mut status=nwg::Label::default();
    nwg::Label::builder().text(if credentials::configured() {
        "Credentials: stored separately with Windows DPAPI"
    }else{
        "Credentials: not configured"
    }).parent(&window).position((25,621)).size((735,25)).build(&mut status).unwrap();

    let mut test=nwg::Button::default();
    nwg::Button::builder().text("Test Connection / 接続テスト").parent(&window)
        .position((25,658)).size((190,36)).build(&mut test).unwrap();
    let mut open_web=nwg::Button::default();
    nwg::Button::builder().text("Open Local Web").parent(&window)
        .position((230,658)).size((190,36)).build(&mut open_web).unwrap();

    // Agent management uses only mics-only.exe; printing processes stay untouched.
    let mut start_agent=nwg::Button::default();
    nwg::Button::builder().text("Start Agent / 起動").parent(&window)
        .position((25,704)).size((155,36)).build(&mut start_agent).unwrap();
    let mut stop_agent=nwg::Button::default();
    nwg::Button::builder().text("Stop Agent / 停止").parent(&window)
        .position((190,704)).size((155,36)).build(&mut stop_agent).unwrap();
    let mut restart_agent=nwg::Button::default();
    nwg::Button::builder().text("Restart Agent / 再起動").parent(&window)
        .position((355,704)).size((175,36)).build(&mut restart_agent).unwrap();
    let mut update_agent=nwg::Button::default();
    nwg::Button::builder().text("GitHub Update / 更新").parent(&window)
        .position((540,704)).size((225,36)).build(&mut update_agent).unwrap();

    let mut save=nwg::Button::default();
    nwg::Button::builder().text("Save / 保存").parent(&window)
        .position((470,658)).size((140,36)).build(&mut save).unwrap();
    let mut close=nwg::Button::default();
    nwg::Button::builder().text("Close / 閉じる").parent(&window)
        .position((620,658)).size((140,36)).build(&mut close).unwrap();

    let enable=Rc::new(enable);
    let autostart=Rc::new(autostart);
    let test=Rc::new(test);
    let open_web=Rc::new(open_web);
    let start_agent=Rc::new(start_agent);
    let stop_agent=Rc::new(stop_agent);
    let restart_agent=Rc::new(restart_agent);
    let update_agent=Rc::new(update_agent);
    let save=Rc::new(save);
    let close=Rc::new(close);
    let fields=Rc::new(inputs);
    let secret_fields=Rc::new(secrets);
    let state=Rc::new(RefCell::new(false));
    let state2=state.clone();

    let _handler=nwg::full_bind_event_handler(&window.handle,{
        let fields=fields.clone();
        let secret_fields=secret_fields.clone();
        let enable=enable.clone();
        let autostart=autostart.clone();
        let test=test.clone();
        let open_web=open_web.clone();
        let start_agent=start_agent.clone();
        let stop_agent=stop_agent.clone();
        let restart_agent=restart_agent.clone();
        let update_agent=update_agent.clone();
        let save=save.clone();
        let close=close.clone();
        let ini=ini.clone();
        let agent_exe=home.join("mics-only.exe");
        move |event,_,handle| {
            if event==nwg::Event::OnWindowClose {
                nwg::stop_thread_dispatch();
            }
            if event!=nwg::Event::OnButtonClick {return}
            if handle==close.handle {nwg::stop_thread_dispatch();return}
            if handle==open_web.handle {
                let _=Command::new("explorer.exe").arg("http://127.0.0.1:8787")
                    .creation_flags(0x08000000).spawn();
                return
            }
            if handle==test.handle {
                // Checks only the saved INI configuration, TCP port and coordinator health.
                // It never connects to the Firebird database, executes SQL or requests a lease.
                let output=Command::new(&agent_exe).arg("--test-connection")
                    .creation_flags(0x08000000).output();
                match output {
                    Ok(o)=>{
                        let stdout=String::from_utf8_lossy(&o.stdout);
                        let stderr=String::from_utf8_lossy(&o.stderr);
                        let detail=format!("Saved configuration only. No SQL performed.\n{}\n{}",stdout,stderr);
                        if o.status.success(){nwg::simple_message("Connection Test", &detail);}
                        else{nwg::error_message("Connection Test FAILED", &detail);}
                    }
                    Err(e)=>{nwg::error_message("Connection Test", &format!("Could not run mics-only.exe: {e}"));}
                }
                return
            }
            if handle==start_agent.handle {
                if Command::new(&agent_exe).creation_flags(0x08000000).spawn().is_ok(){
                    nwg::simple_message("MICS Only","Start requested. Check Local Web at 127.0.0.1:8787.");
                }else{nwg::error_message("MICS Only","Cannot start mics-only.exe");}
                return
            }
            if handle==stop_agent.handle {
                let o=Command::new("taskkill").args(["/F","/IM","mics-only.exe"])
                    .creation_flags(0x08000000).output();
                match o {
                    Ok(v) if v.status.success()=>{nwg::simple_message("MICS Only","Agent stopped. Local web is now offline.");},
                    _=>{nwg::simple_message("MICS Only","Agent was not running or could not be stopped.");}
                }
                return
            }
            if handle==restart_agent.handle {
                let _=Command::new("taskkill").args(["/F","/IM","mics-only.exe"])
                    .creation_flags(0x08000000).status();
                match Command::new(&agent_exe).creation_flags(0x08000000).spawn(){
                    Ok(_)=>{nwg::simple_message("MICS Only","Agent restart requested.");},
                    Err(e)=>{nwg::error_message("MICS Only",&format!("Restart failed: {e}"));}
                }
                return
            }
            if handle==update_agent.handle {
                match github_update(&agent_exe) {
                    Ok(result)=>{nwg::simple_message("GitHub Update",&result);if result.starts_with("Portable ZIP "){nwg::stop_thread_dispatch();}},
                    Err(e)=>{nwg::error_message("GitHub Update",&e);}
                }
                return
            }
            if handle!=save.handle{return}
            if *state2.borrow(){return}
            *state2.borrow_mut()=true;
            let result=(|| -> Result<(),String> {
                let keys=["agent_id","host","port","database","user","coordinator_url","interval_seconds"];
                let mut cfg=BTreeMap::new();
                for (k,input) in keys.iter().zip(fields.iter()) {
                    let value=input.text().trim().to_owned();
                    if value.contains(['\r','\n']) {return Err(format!("Invalid newline in {k}"))}
                    cfg.insert(*k,value);
                }
                let enabled=enable.check_state()==nwg::CheckBoxState::Checked;
                let number=cfg["port"].parse::<u16>().map_err(|_|"Invalid Firebird port")?;
                if number==0{return Err("Invalid Firebird port".into())}
                let interval=cfg["interval_seconds"].parse::<u64>().map_err(|_|"Invalid sync interval")?;
                if !(15..=3600).contains(&interval) {return Err("Sync interval: 15..3600 seconds".into())}
                let id=&cfg["agent_id"];
                if id.len()<3||id.len()>100||!id.bytes().all(|b|b.is_ascii_alphanumeric()||b"-_".contains(&b)){
                    return Err("Agent ID must contain 3..100 letters/numbers/-/_".into())
                }
                if enabled {
                    for name in ["host","database","user","coordinator_url"] {
                        if cfg[name].is_empty(){return Err(format!("Missing {name}"))}
                    }
                    let url=url::Url::parse(&cfg["coordinator_url"]).map_err(|e|e.to_string())?;
                    if url.scheme()!="https"||url.host_str().is_none()||url.username()!=""
                        ||url.password().is_some()||url.query().is_some()||url.fragment().is_some() {
                        return Err("Coordinator URL must be HTTPS without query/credentials".into())
                    }
                }
                let previous=credentials::load().ok();
                let password=secret_fields[0].text();
                let sync=secret_fields[1].text();
                let coordinator=secret_fields[2].text();
                let vault=credentials::Credentials {
                    db_password: if password.is_empty(){previous.as_ref().map(|c|c.db_password.clone()).unwrap_or_default()}else{password},
                    mics_sync_token: if sync.is_empty(){previous.as_ref().map(|c|c.mics_sync_token.clone()).unwrap_or_default()}else{sync},
                    coordinator_token: if coordinator.is_empty(){previous.as_ref().map(|c|c.coordinator_token.clone()).unwrap_or_default()}else{coordinator},
                };
                if enabled && (vault.db_password.is_empty()||vault.mics_sync_token.is_empty()||vault.coordinator_token.is_empty()){
                    return Err("Enter Firebird password, Sync token and Coordinator token".into())
                }
                let mut lines=vec!["[mics]".to_string(),format!("enabled={}",if enabled {1}else{0})];
                for key in keys {lines.push(format!("{key}={}",cfg[key]));}
                let tmp=ini.with_extension("ini.tmp");
                fs::write(&tmp,format!("{}\n",lines.join("\n"))).map_err(|e|e.to_string())?;
                // Persist the vault before changing enabled state; never write passwords into INI.
                if !vault.db_password.is_empty()&&!vault.mics_sync_token.is_empty()&&!vault.coordinator_token.is_empty(){
                    if let Err(err)=credentials::save(&vault){let _=fs::remove_file(&tmp);return Err(err)}
                }
                if ini.exists(){
                    let backup=ini.with_extension("ini.bak");
                    fs::copy(&ini,&backup).map_err(|e|e.to_string())?;
                    fs::remove_file(&ini).map_err(|e|e.to_string())?;
                }
                fs::rename(&tmp,&ini).map_err(|e|e.to_string())?;
                let run_key=r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
                // Quote the full executable path so paths containing spaces work.
                let run_value=format!(r#"\"{}\""#,agent_exe.display());
                let mut reg=Command::new("reg");
                if autostart.check_state()==nwg::CheckBoxState::Checked {
                    reg.args(["add",run_key,"/v","MICS-Only-Agent","/t","REG_SZ","/d",&run_value,"/f"]);
                } else {
                    reg.args(["delete",run_key,"/v","MICS-Only-Agent","/f"]);
                }
                let output=reg.creation_flags(0x08000000).output().map_err(|e|format!("Windows startup setting failed: {e}"))?;
                if !output.status.success() && autostart.check_state()==nwg::CheckBoxState::Checked {
                    return Err(format!("Startup registry update failed: {}",String::from_utf8_lossy(&output.stderr)));
                }
                Ok(())
            })();
            *state2.borrow_mut()=false;
            match result {
                Ok(())=> {
                    // Only restart our own agent, never mics-print or the print-only agent.
                    // Stop-then-start avoids two mics-only instances competing for Firebird.
                    let _ = Command::new("taskkill").args(["/F", "/IM", "mics-only.exe"])
                        .creation_flags(0x08000000).status();
                    match Command::new(&agent_exe).creation_flags(0x08000000).spawn() {
                        Ok(_) => {
                            nwg::simple_message("MICS Only", "Saved. MICS-only agent restarted with new settings.");
                            nwg::stop_thread_dispatch();
                        }
                        Err(err) => { nwg::error_message("MICS Only", &format!("Saved, but could not start agent: {err}")); },
                    }
                },
                Err(e)=>{nwg::error_message("MICS Only Settings", &e);},
            }
        }
    });
    // Keep window-owned controls alive for the duration of the event loop.
    let _controls=(heading,labels,secret_labels,note,status,test,open_web,start_agent,stop_agent,restart_agent,update_agent,save,close,enable,autostart,state);
    nwg::dispatch_thread_events();
}
#[cfg(not(windows))]
fn main() {
    eprintln!("MICS Only Settings requires Windows.");
}

#[cfg(windows)]
fn github_update(agent_exe:&std::path::Path)->Result<String,String>{
    use std::{fs,io::{Read,Write},process::Command};
    use std::os::windows::process::CommandExt;
    use sha2::{Digest,Sha256};
    let api="https://api.github.com/repos/maedandori/mics-only/releases/latest";
    let response=ureq::AgentBuilder::new().timeout(std::time::Duration::from_secs(20))
        .build().get(api).set("User-Agent","mics-only-settings-portable")
        .call().map_err(|e|format!("GitHub Release check failed: {e}"))?;
    let info:serde_json::Value=response.into_json().map_err(|e|e.to_string())?;
    let tag=info["tag_name"].as_str().ok_or("Release tag missing")?;
    if !tag.starts_with('v')||tag.len()>32||!tag[1..].bytes().all(|c|c.is_ascii_digit()||c==b'.'){
        return Err("Invalid release tag".into())
    }
    let current=format!("v{}",env!("CARGO_PKG_VERSION"));
    if tag==current{return Ok(format!("Already up to date ({current})."))}
    let assets=info["assets"].as_array().ok_or("Release assets missing")?;
    let asset=assets.iter().find(|a|a["name"]=="MICS-Only-Portable-Windows.zip")
        .ok_or("Portable ZIP asset missing from GitHub Release")?;
    let url=asset["browser_download_url"].as_str().ok_or("Asset URL missing")?;
    if !url.starts_with("https://github.com/maedandori/mics-only/releases/download/")
        || !url.ends_with("/MICS-Only-Portable-Windows.zip") {
        return Err("Untrusted GitHub Release URL".into())
    }
    let size=asset["size"].as_u64().unwrap_or(0);
    if size==0||size>120_000_000{return Err("Unexpected ZIP size".into())}
    let response=ureq::AgentBuilder::new().timeout(std::time::Duration::from_secs(180))
        .build().get(url).set("User-Agent","mics-only-settings-portable")
        .call().map_err(|e|format!("ZIP download failed: {e}"))?;
    let dir=std::env::temp_dir().join("MICS-Only-Update");
    fs::create_dir_all(&dir).map_err(|e|e.to_string())?;
    let zip=dir.join("MICS-Only-Portable-Windows.zip");
    let mut file=fs::File::create(&zip).map_err(|e|e.to_string())?;
    let mut reader=response.into_reader().take(120_000_001);
    let mut hasher=Sha256::new();
    let mut count=0u64;
    let mut buffer=[0u8;65536];
    loop{
        let n=reader.read(&mut buffer).map_err(|e|e.to_string())?;
        if n==0{break}
        file.write_all(&buffer[..n]).map_err(|e|e.to_string())?;
        hasher.update(&buffer[..n]);
        count+=n as u64;
    }
    file.flush().map_err(|e|e.to_string())?;
    if count!=size{return Err("Downloaded ZIP size mismatch".into())}
    // GitHub Release asset digest is provided by GitHub and checked when available.
    if let Some(expected)=asset["digest"].as_str().and_then(|v|v.strip_prefix("sha256:")){
        let actual=format!("{:x}",hasher.finalize());
        if !actual.eq_ignore_ascii_case(expected){return Err("GitHub ZIP SHA256 mismatch".into())}
    }
    let installation=agent_exe.parent().ok_or("Agent directory unavailable")?;
    // A separate PowerShell process waits for Settings to exit before replacing EXEs.
    // It preserves mics.ini, Windows DPAPI credentials, and any existing www files.
    let script=dir.join("apply-portable-update.ps1");
    let script_content=r#"
param(
    [Parameter(Mandatory=$true)][string]$Archive,
    [Parameter(Mandatory=$true)][string]$Target,
    [Parameter(Mandatory=$true)][int]$ParentId
)
$ErrorActionPreference='Stop'
try {
    try { Wait-Process -Id $ParentId -Timeout 60 -ErrorAction Stop } catch {}
    $stage=Join-Path $env:TEMP 'MICS-Only-Update\extracted'
    if(Test-Path $stage){Remove-Item -LiteralPath $stage -Recurse -Force}
    New-Item -ItemType Directory -Path $stage -Force | Out-Null
    Expand-Archive -LiteralPath $Archive -DestinationPath $stage -Force
    $names=@('mics-only.exe','mics-only-settings.exe')
    foreach($name in $names){
        if(!(Test-Path -LiteralPath (Join-Path $stage $name))){throw "ZIP missing $name"}
    }
    Get-Process -Name 'mics-only' -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction Stop
    Start-Sleep -Milliseconds 700
    foreach($name in $names){
        $dest=Join-Path $Target $name
        if(Test-Path $dest){Copy-Item -LiteralPath $dest -Destination ($dest+'.bak') -Force}
        Copy-Item -LiteralPath (Join-Path $stage $name) -Destination $dest -Force
    }
    $web=Join-Path $stage 'www'
    if(Test-Path $web){
        $destWeb=Join-Path $Target 'www'
        New-Item -ItemType Directory -Path $destWeb -Force | Out-Null
        Get-ChildItem -LiteralPath $web -File -Recurse | ForEach-Object {
            $relative=$_.FullName.Substring($web.Length).TrimStart('\')
            $dst=Join-Path $destWeb $relative
            $parent=Split-Path -Parent $dst
            New-Item -ItemType Directory -Path $parent -Force | Out-Null
            if(!(Test-Path -LiteralPath $dst)){Copy-Item -LiteralPath $_.FullName -Destination $dst}
        }
    }
    Start-Process -FilePath (Join-Path $Target 'mics-only.exe') -WorkingDirectory $Target
} catch {
    $msg="MICS Only update failed: $($_.Exception.Message)"
    Add-Content -LiteralPath (Join-Path $env:TEMP 'MICS-Only-Update\update-error.log') -Value $msg
}
"#;
    fs::write(&script,script_content).map_err(|e|e.to_string())?;
    Command::new("powershell.exe").args(["-NoProfile","-NonInteractive","-ExecutionPolicy","Bypass","-File"])
        .arg(&script).arg("-Archive").arg(&zip).arg("-Target").arg(installation)
        .arg("-ParentId").arg(std::process::id().to_string())
        .creation_flags(0x08000000).spawn().map_err(|e|e.to_string())?;
    Ok(format!("Portable ZIP {tag} verified. Settings will close and install the update."))
}
