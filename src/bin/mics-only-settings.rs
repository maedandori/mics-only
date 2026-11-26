#![cfg_attr(windows, windows_subsystem = "windows")]
#[path = "../credentials.rs"] mod credentials;
#[cfg(windows)]
#[path = "../updater.rs"] mod updater;
#[cfg(windows)]
#[path = "../traffic.rs"] mod traffic;

#[cfg(windows)]
fn main() {
    use native_windows_gui as nwg;
    use std::{cell::RefCell, collections::BTreeMap, fs, path::PathBuf, process::Command, rc::Rc, sync::mpsc, thread};
    use std::os::windows::process::CommandExt;
    nwg::init().expect("設定画面を初期化できません。");
    nwg::Font::set_global_family("Meiryo UI").expect("フォントを設定できません。");

    let home = std::env::current_exe().ok().and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));
    let ini = home.join("mics.ini");
    let diagnostics=home.join("mics-only-settings.log");
    let report_error=move |message:&str| {
        use std::io::Write;
        if let Ok(mut f)=fs::OpenOptions::new().create(true).append(true).open(&diagnostics){
            let _=writeln!(f,"{} {}",chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),message);
        }
    };
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
    nwg::Window::builder().size((1000,710)).position((90,35))
        .title("MICS-only｜設定")
        .build(&mut window).expect("設定画面を作成できません。");

    // Fixed layout with two clearly separated functional columns.
    let mut section_labels=Vec::<nwg::Label>::new();
    for (title,x,y,w) in [
        ("接続先・同期間隔",24,98,565),
        ("認証情報",24,429,565),
        ("端末・起動設定",620,98,345),
        ("エージェント操作",620,280,345),
        ("保存後は自動的に再起動します。",620,590,345)
    ] {
        let mut label=nwg::Label::default();
        nwg::Label::builder().text(title).parent(&window).position((x,y)).size((w,25)).build(&mut label).unwrap();
        section_labels.push(label);
    }
    let mut tab_main=nwg::Button::default();
    nwg::Button::builder().text("一般設定").parent(&window).position((24,55)).size((145,32)).build(&mut tab_main).unwrap();
    let mut tab_sql=nwg::Button::default();
    nwg::Button::builder().text("データベース調査").parent(&window).position((180,55)).size((190,32)).build(&mut tab_sql).unwrap();
    let mut sql_window=nwg::Window::default();
    nwg::Window::builder().size((790,570)).position((230,110)).title("データベース調査（読み取り専用）")
        .flags(nwg::WindowFlags::WINDOW).build(&mut sql_window).unwrap();
    let mut sql_label=nwg::Label::default();
    nwg::Label::builder().text("読み取り専用：SELECT 文のみ実行可能（最大10件）").parent(&sql_window)
        .position((20,15)).size((720,25)).build(&mut sql_label).unwrap();
    let mut sql_input=nwg::TextBox::default();
    nwg::TextBox::builder().text("SELECT FIRST 10 * FROM PARTS").parent(&sql_window)
        .position((20,45)).size((740,110)).build(&mut sql_input).unwrap();
    let mut sql_run=nwg::Button::default();
    nwg::Button::builder().text("SQL を実行").parent(&sql_window)
        .position((20,170)).size((230,36)).build(&mut sql_run).unwrap();
    let mut sql_export=nwg::Button::default();
    nwg::Button::builder().text("全テーブルの先頭10件を保存").parent(&sql_window)
        .position((270,170)).size((260,36)).build(&mut sql_export).unwrap();
    let mut sql_output=nwg::TextBox::default();
    nwg::TextBox::builder().text("実行結果と保存状況がここに表示されます。")
        .parent(&sql_window).position((20,220)).size((740,290)).readonly(true)
        .build(&mut sql_output).unwrap();
    let mut heading=nwg::Label::default();
    nwg::Label::builder().text("MICS-only　予備機設定").parent(&window)
        .position((24,14)).size((440,30)).build(&mut heading).unwrap();

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
        "設定 v{}　｜　稼働版 {}",
        env!("CARGO_PKG_VERSION"),
        if running_version=="offline" {"停止中".to_owned()}
        else {format!("v{running_version}")}
    );
    let mut version_label=nwg::Label::default();
    nwg::Label::builder().text(&version_caption).parent(&window)
        .position((580,18)).size((390,26))
        .build(&mut version_label).unwrap();

    let mut enable=nwg::CheckBox::default();
    nwg::CheckBox::builder().text("予備機を有効にする").parent(&window)
        .position((620,126)).size((340,30))
        .check_state(if get("enabled","0")=="1" {nwg::CheckBoxState::Checked}else{nwg::CheckBoxState::Unchecked})
        .build(&mut enable).unwrap();

    // A human-readable label, distinct from the immutable Coordinator Agent ID.
    let mut display_name_label=nwg::Label::default();
    nwg::Label::builder().text("端末名").parent(&window)
        .position((620,171)).size((340,22)).build(&mut display_name_label).unwrap();
    let mut display_name=nwg::TextInput::default();
    nwg::TextInput::builder().text(&get("display_name","")).parent(&window)
        .position((620,195)).size((345,29)).build(&mut display_name).unwrap();

    let mut labels=Vec::<nwg::Label>::new();
    let mut inputs=Vec::<nwg::TextInput>::new();
    let names=[
        ("予備機識別番号（ID）",get("agent_id","mics-backup-01")),
        ("Firebird サーバー",get("host","")),
        ("接続ポート",get("port","3050")),
        ("データベースのパス",get("database","")),
        ("接続ユーザー",get("user","READONLY")),
        ("調整サーバーの URL",get("coordinator_url","")),
        ("基本同期間隔（秒）",get("interval_seconds","30")),
    ];
    for (index,(name,value)) in names.iter().enumerate() {
        let y=127+(index as i32)*42;
        let mut label=nwg::Label::default();
        nwg::Label::builder().text(name).parent(&window).position((24,y))
            .size((195,28)).build(&mut label).unwrap();
        let mut input=nwg::TextInput::default();
        nwg::TextInput::builder().text(value).parent(&window).position((222,y))
            .size((367,29)).build(&mut input).unwrap();
        labels.push(label);inputs.push(input);
    }
    let mut secrets=Vec::<nwg::TextInput>::new();
    let secret_names=[
        "データベースのパスワード",
        "データ同期用トークン",
        "予備機認証用トークン"
    ];
    let mut secret_labels=Vec::<nwg::Label>::new();
    for (index,name) in secret_names.iter().enumerate() {
        let y=458+(index as i32)*42;
        let mut label=nwg::Label::default();
        nwg::Label::builder().text(name).parent(&window).position((24,y))
            .size((195,28)).build(&mut label).unwrap();
        let mut input=nwg::TextInput::default();
        nwg::TextInput::builder().password(Some('*')).parent(&window)
            .position((222,y)).size((367,29)).build(&mut input).unwrap();
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
        .text("Windows 起動時に自動起動")
        .parent(&window).position((620,238)).size((345,28))
        .check_state(if startup_enabled {nwg::CheckBoxState::Checked}else{nwg::CheckBoxState::Unchecked})
        .build(&mut autostart).unwrap();

    let mut note=nwg::Label::default();
    nwg::Label::builder().text(
        "認証情報を空欄にすると保存済みの情報を保持します。"
    ).parent(&window).position((24,592)).size((565,25)).build(&mut note).unwrap();

    let mut status=nwg::Label::default();
    nwg::Label::builder().text(if credentials::configured() {
        "認証情報：Windows の暗号化機能で保存済み"
    }else{
        "認証情報：未設定"
    }).parent(&window).position((24,620)).size((565,25)).build(&mut status).unwrap();

    let mut test=nwg::Button::default();
    nwg::Button::builder().text("接続テスト・端末登録").parent(&window)
        .position((620,309)).size((168,36)).build(&mut test).unwrap();
    let mut open_web=nwg::Button::default();
    nwg::Button::builder().text("ローカル画面を開く").parent(&window)
        .position((799,309)).size((166,36)).build(&mut open_web).unwrap();

    // Agent management uses only mics-only.exe; printing processes stay untouched.
    let mut start_agent=nwg::Button::default();
    nwg::Button::builder().text("起動").parent(&window)
        .position((620,355)).size((106,35)).build(&mut start_agent).unwrap();
    let mut stop_agent=nwg::Button::default();
    nwg::Button::builder().text("停止").parent(&window)
        .position((738,355)).size((106,35)).build(&mut stop_agent).unwrap();
    let mut restart_agent=nwg::Button::default();
    nwg::Button::builder().text("再起動").parent(&window)
        .position((856,355)).size((109,35)).build(&mut restart_agent).unwrap();
    let mut update_agent=nwg::Button::default();
    nwg::Button::builder().text("最新版を確認").parent(&window)
        .position((620,480)).size((345,35)).build(&mut update_agent).unwrap();

    let mut save=nwg::Button::default();
    nwg::Button::builder().text("保存して再起動").parent(&window)
        .position((620,620)).size((220,38)).build(&mut save).unwrap();
    let mut close=nwg::Button::default();
    nwg::Button::builder().text("閉じる").parent(&window)
        .position((851,620)).size((114,38)).build(&mut close).unwrap();

    let mut alias_sync=nwg::Button::default();
    nwg::Button::builder().text("別名・状態色を同期").parent(&window)
        .position((620,401)).size((345,35)).build(&mut alias_sync).unwrap();
    let mut alias_label=nwg::Label::default();
    nwg::Label::builder().text("同期元：共有データベース").parent(&window)
        .position((620,439)).size((345,30)).build(&mut alias_label).unwrap();
    let mut alias_notice=nwg::Notice::default();
    nwg::Notice::builder().parent(&window).build(&mut alias_notice).unwrap();
    let (alias_sender,alias_receiver)=mpsc::channel::<Result<String,String>>();
    let alias_sender=Rc::new(alias_sender);
    let alias_receiver=Rc::new(RefCell::new(alias_receiver));

    // Update downloading runs off the UI thread so the progress bar stays responsive.
    let mut update_text=nwg::Label::default();
    nwg::Label::builder().text("更新：待機中").parent(&window)
        .position((620,527)).size((345,28)).build(&mut update_text).unwrap();
    let mut update_bar=nwg::ProgressBar::default();
    nwg::ProgressBar::builder().parent(&window).position((620,557))
        .size((345,20)).range(0..100).pos(0).build(&mut update_bar).unwrap();
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
        let sql_window=sql_window.clone();
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
                    sql_output.set_text(&format!("SQL 文を保存できませんでした。"));return
                }
            }
            let mut cmd=Command::new(&sql_agent);
            if query {cmd.arg("--sql-query").arg(&path);}else{cmd.arg("--dump-tables");}
            let output=cmd.creation_flags(0x08000000).output();
            match output {
                Ok(out) if out.status.success()=>{
                    let data=String::from_utf8_lossy(&out.stdout);
                    sql_output.set_text(&format!("実行完了\r\n{}",if data.trim().is_empty(){"該当データはありません。"}else{data.trim()}));
                },
                Ok(_)=>sql_output.set_text("実行に失敗しました。SQL 文、接続設定、認証情報をご確認ください。"),
                Err(e)=>sql_output.set_text(&format!("実行プログラムを起動できませんでした。"))
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
                    update_text_ui.set_text(&format!("更新：{percent}%　{message}"));
                    if let Some(result)=done {
                        update_agent.set_enabled(true);
                        match result {
                            Ok(summary)=>{
                                if summary.starts_with("UPDATE_STAGED ") {
                                    // Exit immediately without a blocking modal dialog so
                                    // apply-update.ps1 can overwrite mics-only-settings.exe.
                                    update_text_ui.set_text("更新の準備が完了しました。適用のため設定画面を閉じます。");
                                    nwg::stop_thread_dispatch();
                                }else{nwg::simple_message("バージョン更新",&summary);}
                            },
                            Err(_error)=>{update_text_ui.set_text("更新に失敗しました。");nwg::error_message("バージョン更新","更新に失敗しました。ネットワークと更新ログを確認してください。");}
                        }
                    }
                }
                return
            }
            if event==nwg::Event::OnNotice && handle==alias_notice_handle {
                while let Ok(result)=alias_receiver.borrow().try_recv(){
                    alias_sync.set_enabled(true);
                    match result{
                        Ok(s)=>alias_label.set_text(&format!("同期：{s}")),
                        Err(e)=>alias_label.set_text(&format!("同期に失敗しました。")),
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
                alias_label.set_text("共有データベースから取得中…");
                let tx=alias_sender.as_ref().clone();
                let sender=alias_notice_sender.clone();
                thread::spawn(move||{
                    let result=(||->Result<String,String>{
                        let response=ureq::AgentBuilder::new().timeout(std::time::Duration::from_secs(12)).build()
                            .put("http://127.0.0.1:8787/api/aliases/sync")
                            .call().map_err(|e|format!("ローカル接続または同期に失敗しました。"))?;
                        let data:serde_json::Value=response.into_json().map_err(|e|e.to_string())?;
                        if data["ok"]!=true{return Err("同期に失敗しました。".to_owned())}
                        Ok(format!("同期完了｜{}",data["synced_at"].as_str().unwrap_or("完了")))
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
                        let detail=if o.status.success(){
                            format!("接続テストが完了しました。\nデータベース接続：{}\n調整サーバー：{}\n端末登録：{}\n※ SQLの実行や予備機権限の取得は行っていません。",
                                if stdout.contains("FIREBIRD_TCP_OK"){"成功"}else{"未確認"},
                                if stdout.contains("COORDINATOR_HTTP_OK"){"成功"}else{"未確認"},
                                if stdout.contains("D1_PRESENCE_OK"){"成功"}else{"未確認"})
                        }else{
                            // Do not display driver output or credentials in a user-facing dialog.
                            let _=stderr;
                            "接続テストに失敗しました。保存済みの接続設定、認証情報とネットワークをご確認ください。".to_owned()
                        };
                        if o.status.success(){nwg::simple_message("接続テスト", &detail);}
                        else{nwg::error_message("接続テスト失敗", &detail);}
                    }
                    Err(e)=>{nwg::error_message("接続テスト", &format!("実行プログラムを起動できませんでした。"));}
                }
                return
            }
            if handle==start_agent.handle {
                if Command::new(&agent_exe).creation_flags(0x08000000).spawn().is_ok(){
                    nwg::simple_message("MICS-only","起動を要求しました。ローカル画面で状態を確認してください。");
                }else{nwg::error_message("MICS-only","実行プログラムを起動できませんでした。");}
                return
            }
            if handle==stop_agent.handle {
                let o=Command::new("taskkill").args(["/F","/IM","mics-only.exe"])
                    .creation_flags(0x08000000).output();
                match o {
                    Ok(v) if v.status.success()=>{nwg::simple_message("MICS-only","停止しました。ローカル画面は接続できません。");},
                    _=>{nwg::simple_message("MICS-only","起動していないか、停止できませんでした。");}
                }
                return
            }
            if handle==restart_agent.handle {
                let _=Command::new("taskkill").args(["/F","/IM","mics-only.exe"])
                    .creation_flags(0x08000000).status();
                match Command::new(&agent_exe).creation_flags(0x08000000).spawn(){
                    Ok(_)=>{nwg::simple_message("MICS-only","再起動を要求しました。");},
                    Err(e)=>{nwg::error_message("MICS-only",&format!("再起動に失敗しました。"));}
                }
                return
            }
            if handle==update_agent.handle {
                update_agent.set_enabled(false);
                update_bar_ui.set_pos(1);
                update_text_ui.set_text("更新：1%　最新版を確認中…");
                let tx=update_sender.as_ref().clone();
                let sender=update_notice_sender.clone();
                let exe=std::env::current_exe().unwrap_or_else(|_|agent_exe.clone());
                thread::spawn(move || {
                    let outcome=updater::install_with_progress(&exe,|pct,msg|{
                        let _=tx.send((pct,msg.to_string(),None));
                        sender.notice();
                    });
                    let _=tx.send((100,"更新確認が完了しました。".to_string(),Some(outcome)));
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
                    if value.contains(['\r','\n']) {return Err(format!("入力欄に改行を含めることはできません。"))}
                    cfg.insert(*k,value);
                }
                let device_name=display_name.text().trim().to_owned();
                if device_name.chars().count()>60||device_name.chars().any(|c|c.is_control()){return Err("端末名は60文字以内で、改行や制御文字は使用できません。".into())}
                cfg.insert("display_name",device_name);
                let enabled=enable.check_state()==nwg::CheckBoxState::Checked;
                let number=cfg["port"].parse::<u16>().map_err(|_|"接続ポートが正しくありません。")?;
                if number==0{return Err("接続ポートが正しくありません。".into())}
                let interval=cfg["interval_seconds"].parse::<u64>().map_err(|_|"同期間隔が正しくありません。")?;
                if !(15..=3600).contains(&interval) {return Err("同期間隔は15～3600秒で指定してください。".into())}
                let id=&cfg["agent_id"];
                if id.len()<3||id.len()>100||!id.bytes().all(|b|b.is_ascii_alphanumeric()||b"-_".contains(&b)){
                    return Err("予備機IDは半角英数字、ハイフン、アンダーバーで3～100文字にしてください。".into())
                }
                if enabled {
                    for name in ["host","database","user","coordinator_url"] {
                        if cfg[name].is_empty(){return Err(format!("接続設定の必須項目が入力されていません。"))}
                    }
                    let url=url::Url::parse(&cfg["coordinator_url"]).map_err(|_|"調整サーバーのURLが正しくありません。")?;
                    if url.scheme()!="https"||url.host_str().is_none()||url.username()!=""
                        ||url.password().is_some()||url.query().is_some()||url.fragment().is_some() {
                        return Err("調整サーバーのURLはHTTPSで、認証情報やクエリを含めないでください。".into())
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
                    return Err("パスワードと2種類の認証トークンを入力してください。".into())
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
                let run_value=format!("\"{}\"",agent_exe.display());
                let mut reg=Command::new("reg");
                if autostart.check_state()==nwg::CheckBoxState::Checked {
                    reg.args(["add",run_key,"/v","MICS-Only-Agent","/t","REG_SZ","/d",&run_value,"/f"]);
                } else {
                    reg.args(["delete",run_key,"/v","MICS-Only-Agent","/f"]);
                }
                let output=reg.creation_flags(0x08000000).output().map_err(|e|format!("Windows の自動起動設定に失敗しました。"))?;
                if !output.status.success() && autostart.check_state()==nwg::CheckBoxState::Checked {
                    return Err("自動起動の登録に失敗しました。".to_owned());
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
                            nwg::simple_message("MICS-only", "設定を保存し、新しい設定で再起動しました。");
                            nwg::stop_thread_dispatch();
                        }
                        Err(err) => { nwg::error_message("MICS-only", &format!("設定を保存しましたが、起動に失敗しました。")); },
                    }
                },
                Err(e)=>{report_error(&e);let message=if e.chars().any(|c|matches!(c,'\u{3040}'..='\u{30ff}'|'\u{4e00}'..='\u{9fff}')){e.as_str()}else{"保存に失敗しました。入力内容を確認してください。詳細は設定ログに記録しました。"};nwg::error_message("設定",message);},
            }
        }
    });
    // Keep window-owned controls alive for the duration of the event loop.
    let _controls=(section_labels,display_name_label,tab_main,tab_sql,sql_label,sql_input,sql_run,sql_export,sql_output,sql_window,sql_window_handle,_sql_handler,heading,version_label,labels,secret_labels,note,status,test,open_web,start_agent,stop_agent,restart_agent,update_agent,alias_sync,alias_label,alias_notice,save,close,enable,autostart,state,update_notice,update_text,update_bar);
    nwg::dispatch_thread_events();
}
#[cfg(not(windows))]
fn main() {
    eprintln!("設定 requires Windows.");
}

