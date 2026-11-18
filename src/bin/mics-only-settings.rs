#![cfg_attr(windows, windows_subsystem = "windows")]
#[path = "../credentials.rs"] mod credentials;
#[cfg(windows)]
#[path = "../updater.rs"] mod updater;

#[cfg(windows)]
fn main() {
    use native_windows_gui as nwg;
    use std::{cell::RefCell, collections::BTreeMap, fs, path::PathBuf, process::Command, rc::Rc, sync::mpsc, thread};
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
    let _defaults = [
        ("enabled","0"), ("agent_id","mics-backup-01"),
        ("host",""),("port","3050"),("database",""),("user","READONLY"),
        ("coordinator_url",""),("interval_seconds","30")
    ];
    let get = |k:&str,d:&str| original.get(k).cloned().unwrap_or_else(||d.into());

    let mut window=nwg::Window::default();
    nwg::Window::builder().size((790,915)).position((190,65))
        .title("MICS Only Settings / MICS 予備機設定")
        .build(&mut window).expect("Window error");

    let mut tab_main=nwg::Button::default();
    nwg::Button::builder().text("一般設定").parent(&window).position((25,880)).size((140,28)).build(&mut tab_main).unwrap();
    let mut tab_sql=nwg::Button::default();
    nwg::Button::builder().text("SQL / DB調査").parent(&window).position((175,880)).size((160,28)).build(&mut tab_sql).unwrap();
    let mut sql_window=nwg::Window::default();
    nwg::Window::builder().size((790,570)).position((230,110)).title("SQL / DB調査 (Read Only)")
        .flags(nwg::WindowFlags::WINDOW).build(&mut sql_window).unwrap();
    let mut sql_label=nwg::Label::default();
    nwg::Label::builder().text("SELECT only - max 10 rows. Database is read-only.").parent(&sql_window)
        .position((20,15)).size((720,25)).build(&mut sql_label).unwrap();
    let mut sql_input=nwg::TextBox::default();
    nwg::TextBox::builder().text("SELECT FIRST 10 * FROM PARTS").parent(&sql_window)
        .position((20,45)).size((740,110)).build(&mut sql_input).unwrap();
    let mut sql_run=nwg::Button::default();
    nwg::Button::builder().text("SQL実行 / Chạy SQL").parent(&sql_window)
        .position((20,170)).size((230,36)).build(&mut sql_run).unwrap();
    let mut sql_export=nwg::Button::default();
    nwg::Button::builder().text("全テーブル保存 / Lưu 10 dòng").parent(&sql_window)
        .position((270,170)).size((260,36)).build(&mut sql_export).unwrap();
    let mut sql_output=nwg::TextBox::default();
    nwg::TextBox::builder().text("Results and export status will appear here.")
        .parent(&sql_window).position((20,220)).size((740,290)).readonly(true)
        .build(&mut sql_output).unwrap();
    let mut heading=nwg::Label::default();
    nwg::Label::builder().text("MICS Only | BACKUP Firebird Reader").parent(&window)
        .position((24,15)).size((380,30)).build(&mut heading).unwrap();

    // Display both binary versions: after a rollback the Settings EXE may not
    // match the currently running agent. Do not infer the agent version from
    // the Settings binary. Probe the local endpoint with a strict timeout.
    let running_version=ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_millis(800))
        .build()
        .get("http://127.0.0.1:8787/api/status")
        .call().ok()
        .and_then(|response|response.into_json::<serde_json::Value>().ok())
        .and_then(|body|body["version"].as_str().map(str::to_owned))
        .unwrap_or_else(||"offline".to_string());
    let version_caption=format!(
        "Settings v{}  |  Agent {}",
        env!("CARGO_PKG_VERSION"),
        if running_version=="offline" {running_version}
        else {format!("v{running_version}")}
    );
    let mut version_label=nwg::Label::default();
    nwg::Label::builder().text(&version_caption).parent(&window)
        .position((413,15)).size((352,30))
        .build(&mut version_label).unwrap();

    let mut enable=nwg::CheckBox::default();
    nwg::CheckBox::builder().text("Enable standby agent / 予備機を有効化").parent(&window)
        .position((25,52)).size((510,30))
        .check_state(if get("enabled","0")=="1" {nwg::CheckBoxState::Checked}else{nwg::CheckBoxState::Unchecked})
        .build(&mut enable).unwrap();

    // A human-readable label, distinct from the immutable Coordinator Agent ID.
    let mut display_name_label=nwg::Label::default();
    nwg::Label::builder().text("PC名 / Tên máy").parent(&window)
        .position((540,43)).size((220,22)).build(&mut display_name_label).unwrap();
    let mut display_name=nwg::TextInput::default();
    nwg::TextInput::builder().text(&get("display_name","")).parent(&window)
        .position((540,66)).size((220,25)).build(&mut display_name).unwrap();

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
    nwg::Button::builder().text("Test + Register D1 / 接続テスト").parent(&window)
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

    let mut alias_sync=nwg::Button::default();
    nwg::Button::builder().text("↓ Alias + Status Color / 別名・状態色同期").parent(&window)
        .position((25,751)).size((240,36)).build(&mut alias_sync).unwrap();
    let mut alias_label=nwg::Label::default();
    nwg::Label::builder().text("Alias + Status Color: D1 → local").parent(&window)
        .position((279,753)).size((485,32)).build(&mut alias_label).unwrap();
    let mut alias_notice=nwg::Notice::default();
    nwg::Notice::builder().parent(&window).build(&mut alias_notice).unwrap();
    let (alias_sender,alias_receiver)=mpsc::channel::<Result<String,String>>();
    let alias_sender=Rc::new(alias_sender);
    let alias_receiver=Rc::new(RefCell::new(alias_receiver));

    // Update downloading runs off the UI thread so the progress bar stays responsive.
    let mut update_text=nwg::Label::default();
    nwg::Label::builder().text("Update: idle / 更新待機").parent(&window)
        .position((25,811)).size((735,24)).build(&mut update_text).unwrap();
    let mut update_bar=nwg::ProgressBar::default();
    nwg::ProgressBar::builder().parent(&window).position((25,840))
        .size((735,23)).range(0..100).pos(0).build(&mut update_bar).unwrap();
    let mut update_notice=nwg::Notice::default();
    nwg::Notice::builder().parent(&window).build(&mut update_notice).unwrap();
    let (update_sender,update_receiver)=mpsc::channel::<(u32,String,Option<Result<String,String>>)>();
    let update_receiver=Rc::new(RefCell::new(update_receiver));
    let update_sender=Rc::new(update_sender);
    let update_text=Rc::new(update_text);
    let update_bar=Rc::new(update_bar);

    let enable=Rc::new(enable);
    let autostart=Rc::new(autostart);
    let test=Rc::new(test);
    let open_web=Rc::new(open_web);
    let start_agent=Rc::new(start_agent);
    let stop_agent=Rc::new(stop_agent);
    let restart_agent=Rc::new(restart_agent);
    let update_agent=Rc::new(update_agent);
    let alias_sync=Rc::new(alias_sync);
    let alias_label=Rc::new(alias_label);
    let save=Rc::new(save);
    let close=Rc::new(close);
    let fields=Rc::new(inputs);
    let display_name=Rc::new(display_name);
    let secret_fields=Rc::new(secrets);
    let state=Rc::new(RefCell::new(false));
    let state2=state.clone();

    let sql_input=Rc::new(sql_input);
    let sql_output=Rc::new(sql_output);
    let sql_run=Rc::new(sql_run);
    let sql_export=Rc::new(sql_export);
    let sql_window=Rc::new(sql_window);
    let sql_agent=home.join("mics-only.exe");
    let sql_home=home.clone();
    let sql_window_handle=sql_window.handle.clone();
    let _sql_handler=nwg::full_bind_event_handler(&sql_window_handle,{
        let sql_input=sql_input.clone();let sql_output=sql_output.clone();
        let sql_run=sql_run.clone();let sql_export=sql_export.clone();
        move |event,_,handle|{
            if event==nwg::Event::OnWindowClose {sql_window.set_visible(false);return}
            if event!=nwg::Event::OnButtonClick{return}
            if handle!=sql_run.handle && handle!=sql_export.handle{return}
            let query=handle==sql_run.handle;
            let path=sql_home.join("sql-inspector-query.sql");
            if query {
                if let Err(e)=fs::write(&path,sql_input.text()){
                    sql_output.set_text(&format!("Cannot save query: {e}"));return
                }
            }
            let mut cmd=Command::new(&sql_agent);
            if query {cmd.arg("--sql-query").arg(&path);}else{cmd.arg("--dump-tables");}
            let output=cmd.creation_flags(0x08000000).output();
            match output {
                Ok(out)=>sql_output.set_text(&format!("{}\n{}",String::from_utf8_lossy(&out.stdout),String::from_utf8_lossy(&out.stderr))),
                Err(e)=>sql_output.set_text(&format!("Failed to launch agent: {e}"))
            }
        }
    });
    let _handler=nwg::full_bind_event_handler(&window.handle,{
        let sql_window=sql_window.clone();
        let tab_sql_handle=tab_sql.handle.clone();
        let fields=fields.clone();
        let display_name=display_name.clone();
        let secret_fields=secret_fields.clone();
        let enable=enable.clone();
        let autostart=autostart.clone();
        let test=test.clone();
        let open_web=open_web.clone();
        let start_agent=start_agent.clone();
        let stop_agent=stop_agent.clone();
        let restart_agent=restart_agent.clone();
        let update_agent=update_agent.clone();
        let alias_sync=alias_sync.clone();
        let alias_label=alias_label.clone();
        let alias_sender=alias_sender.clone();
        let alias_receiver=alias_receiver.clone();
        let alias_notice_sender=alias_notice.sender();
        let alias_notice_handle=alias_notice.handle.clone();
        let save=save.clone();
        let close=close.clone();
        let update_sender=update_sender.clone();
        let update_receiver=update_receiver.clone();
        let update_notice_sender=update_notice.sender();
        let update_notice_handle=update_notice.handle.clone();
        let update_text_ui=update_text.clone();
        let update_bar_ui=update_bar.clone();
        let ini=ini.clone();
        let agent_exe=home.join("mics-only.exe");
        move |event,_,handle| {
            if event==nwg::Event::OnWindowClose {
                nwg::stop_thread_dispatch();
            }
            if event==nwg::Event::OnNotice && handle==update_notice_handle {
                while let Ok((percent,message,done))=update_receiver.borrow().try_recv() {
                    update_bar_ui.set_pos(percent);
                    update_text_ui.set_text(&format!("Update: {percent}% - {message}"));
                    if let Some(result)=done {
                        update_agent.set_enabled(true);
                        match result {
                            Ok(summary)=>{
                                if summary.starts_with("UPDATE_STAGED ") {
                                    // Exit immediately without a blocking modal dialog so
                                    // apply-update.ps1 can overwrite mics-only-settings.exe.
                                    update_text_ui.set_text(&summary);
                                    nwg::stop_thread_dispatch();
                                }else{nwg::simple_message("GitHub Update",&summary);}
                            },
                            Err(error)=>{update_text_ui.set_text("Update failed / 更新失敗");nwg::error_message("GitHub Update",&error);}
                        }
                    }
                }
                return
            }
            if event==nwg::Event::OnNotice && handle==alias_notice_handle {
                while let Ok(result)=alias_receiver.borrow().try_recv(){
                    alias_sync.set_enabled(true);
                    match result{
                        Ok(s)=>alias_label.set_text(&format!("Alias + Status Color: {s}")),
                        Err(e)=>alias_label.set_text(&format!("Alias + Status Color FAILED: {e}")),
                    }
                }
                return
            }
            if event!=nwg::Event::OnButtonClick {return}
            if handle==tab_sql_handle {sql_window.set_visible(true);return}
            if handle==close.handle {nwg::stop_thread_dispatch();return}
            if handle==open_web.handle {
                let _=Command::new("explorer.exe").arg("http://127.0.0.1:8787")
                    .creation_flags(0x08000000).spawn();
                return
            }
            if handle==alias_sync.handle {
                alias_sync.set_enabled(false);
                alias_label.set_text("Alias + Status Color: D1 から取得中...");
                let tx=alias_sender.as_ref().clone();
                let sender=alias_notice_sender.clone();
                thread::spawn(move||{
                    let result=(||->Result<String,String>{
                        let response=ureq::AgentBuilder::new().timeout(std::time::Duration::from_secs(12)).build()
                            .put("http://127.0.0.1:8787/api/aliases/sync")
                            .call().map_err(|e|format!("Local agent unavailable or D1 failed: {e}"))?;
                        let data:serde_json::Value=response.into_json().map_err(|e|e.to_string())?;
                        if data["ok"]!=true{return Err(format!("Sync failed: {}",data["error"]))}
                        Ok(format!("SYNCED D1 → local | {}",data["synced_at"].as_str().unwrap_or("OK")))
                    })();
                    let _=tx.send(result);
                    sender.notice();
                });
                return
            }
            if handle==test.handle {
                // Checks saved INI, Firebird TCP and Coordinator; then registers\n                // this Agent ID in D1 via authenticated /presence. No Firebird SQL or lease.
                let output=Command::new(&agent_exe).arg("--test-connection")
                    .creation_flags(0x08000000).output();
                match output {
                    Ok(o)=>{
                        let stdout=String::from_utf8_lossy(&o.stdout);
                        let stderr=String::from_utf8_lossy(&o.stderr);
                        let detail=format!("Saved configuration only. No SQL or lease. D1 registration result below.\n{}\n{}",stdout,stderr);
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
                update_agent.set_enabled(false);
                update_bar_ui.set_pos(1);
                update_text_ui.set_text("Update: 1% - Starting GitHub check");
                let tx=update_sender.as_ref().clone();
                let sender=update_notice_sender.clone();
                let exe=std::env::current_exe().unwrap_or_else(|_|agent_exe.clone());
                thread::spawn(move || {
                    let outcome=updater::install_with_progress(&exe,|pct,msg|{
                        let _=tx.send((pct,msg.to_string(),None));
                        sender.notice();
                    });
                    let _=tx.send((100,"Finished download check".to_string(),Some(outcome)));
                    sender.notice();
                });
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
                let device_name=display_name.text().trim().to_owned();
                if device_name.chars().count()>60||device_name.chars().any(|c|c.is_control()){return Err("Tên máy phải dưới 60 ký tự, không chứa xuống dòng".into())}
                cfg.insert("display_name",device_name);
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
                lines.push(format!("display_name={}",cfg["display_name"]));
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
    let _controls=(tab_main,tab_sql,sql_label,sql_input,sql_run,sql_export,sql_output,sql_window,sql_window_handle,_sql_handler,heading,version_label,labels,secret_labels,note,status,test,open_web,start_agent,stop_agent,restart_agent,update_agent,alias_sync,alias_label,alias_notice,save,close,enable,autostart,state,update_notice,update_text,update_bar);
    nwg::dispatch_thread_events();
}
#[cfg(not(windows))]
fn main() {
    eprintln!("MICS Only Settings requires Windows.");
}

