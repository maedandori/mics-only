#![cfg_attr(windows, windows_subsystem = "windows")]
#[path = "../credentials.rs"] mod credentials;

#[cfg(windows)]
fn main() {
    use native_windows_gui as nwg;
    use std::{cell::RefCell, collections::BTreeMap, fs, path::PathBuf, rc::Rc};
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
    nwg::Window::builder().size((790,730)).position((190,65))
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
        let y=441+(index as i32)*50;
        let mut label=nwg::Label::default();
        nwg::Label::builder().text(name).parent(&window).position((25,y))
            .size((305,28)).build(&mut label).unwrap();
        let mut input=nwg::TextInput::default();
        nwg::TextInput::builder().password(Some('*')).parent(&window)
            .position((335,y)).size((425,29)).build(&mut input).unwrap();
        secret_labels.push(label);secrets.push(input);
    }
    let mut note=nwg::Label::default();
    nwg::Label::builder().text(
        "Blank secret fields retain DPAPI values. Tokens are separate from mics-print."
    ).parent(&window).position((25,599)).size((735,25)).build(&mut note).unwrap();

    let mut status=nwg::Label::default();
    nwg::Label::builder().text(if credentials::configured() {
        "Credentials: stored separately with Windows DPAPI"
    }else{
        "Credentials: not configured"
    }).parent(&window).position((25,626)).size((735,25)).build(&mut status).unwrap();

    let mut save=nwg::Button::default();
    nwg::Button::builder().text("Save / 保存").parent(&window)
        .position((470,665)).size((140,36)).build(&mut save).unwrap();
    let mut close=nwg::Button::default();
    nwg::Button::builder().text("Close / 閉じる").parent(&window)
        .position((620,665)).size((140,36)).build(&mut close).unwrap();

    let fields=Rc::new(inputs);
    let secret_fields=Rc::new(secrets);
    let form_window=window.clone();
    let state=Rc::new(RefCell::new(false));
    let state2=state.clone();

    let _handler=nwg::full_bind_event_handler(&window.handle,{
        let fields=fields.clone();
        let secret_fields=secret_fields.clone();
        let enable=enable.clone();
        let save=save.clone();
        let close=close.clone();
        let ini=ini.clone();
        let window=form_window.clone();
        move |event,_,handle| {
            if event==nwg::Event::OnWindowClose {
                nwg::stop_thread_dispatch();
            }
            if event!=nwg::Event::OnButtonClick {return}
            if handle==close.handle {nwg::stop_thread_dispatch();return}
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
                Ok(())
            })();
            *state2.borrow_mut()=false;
            match result {
                Ok(())=> {nwg::simple_message("MICS Only", "Saved. Restart mics-only.exe to apply changes.");nwg::stop_thread_dispatch();},
                Err(e)=>nwg::error_message("MICS Only Settings", &e),
            }
        }
    });
    // Keep window-owned controls alive for the duration of the event loop.
    let _controls=(heading,labels,secret_labels,note,status,save,close,enable,state,form_window);
    nwg::dispatch_thread_events();
}
#[cfg(not(windows))]
fn main() {
    eprintln!("MICS Only Settings requires Windows.");
}
