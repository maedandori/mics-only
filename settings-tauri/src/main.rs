#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#[path="../../src/credentials.rs"] mod credentials;
#[path="../../src/updater.rs"] mod updater;
#[path="../../src/traffic.rs"] mod traffic;
use serde::{Deserialize,Serialize};
use serde_json::{json,Value};
use std::{collections::BTreeMap,env,fs,path::{Path,PathBuf},process::{Command,Stdio},os::windows::process::CommandExt,sync::{Mutex,Arc},time::Duration};
use winreg::{RegKey,enums::HKEY_CURRENT_USER};
const VERSION:&str=env!("CARGO_PKG_VERSION");
const NO_WINDOW:u32=0x08000000;
const RUN_KEY:&str=r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_NAME:&str="MICS-Only-Agent";
const BRIDGE:&str=include_str!("process.ps1");
#[derive(Clone,Serialize,Deserialize)]
struct Form{
 enabled:bool,display_name:String,agent_id:String,host:String,port:String,database:String,user:String,
 coordinator_url:String,interval_seconds:String,autostart:bool,password:String,sync_token:String,coordinator_token:String,
}
fn root()->Result<PathBuf,String>{env::current_exe().map_err(|_|"実行ファイルの場所を取得できません。".to_owned())?.parent().map(Path::to_owned).ok_or_else(||"設定の保存先が見つかりません。".into())}
fn parse_ini(raw:&str)->BTreeMap<String,String>{
 let mut result=BTreeMap::new();let mut active=false;
 for line in raw.trim_start_matches('\u{feff}').lines(){
  let line=line.trim();
  if line.starts_with('['){active=line.eq_ignore_ascii_case("[mics]");continue}
  if !active||line.starts_with('#')||line.starts_with(';'){continue}
  if let Some((k,v))=line.split_once('='){result.insert(k.trim().to_ascii_lowercase(),v.trim().to_owned());}
 }
 result
}
fn read_ini(dir:&Path)->Result<String,String>{
 match fs::read_to_string(dir.join("mics.ini")){Ok(s)=>Ok(s),Err(e) if e.kind()==std::io::ErrorKind::NotFound=>Ok(String::new()),Err(_)=>Err("設定ファイルを読み込めません。".into())}
}
fn startup_enabled(dir:&Path)->bool{
 let expected=format!("\"{}\"",dir.join("mics-only.exe").display());
 RegKey::predef(HKEY_CURRENT_USER).open_subkey(RUN_KEY).ok().and_then(|k|k.get_value::<String,_>(RUN_NAME).ok()).is_some_and(|v|v.eq_ignore_ascii_case(&expected))
}
fn form_from(raw:&str,autostart:bool)->Form{
 let m=parse_ini(raw);let get=|key:&str,default:&str|m.get(key).cloned().unwrap_or_else(||default.to_owned());
 Form{enabled:matches!(get("enabled","0").to_ascii_lowercase().as_str(),"1"|"true"),display_name:get("display_name",""),agent_id:get("agent_id","mics-backup-01"),host:get("host",""),port:get("port","3050"),database:get("database",""),user:get("user","READONLY"),coordinator_url:get("coordinator_url",""),interval_seconds:get("interval_seconds","30"),autostart,password:String::new(),sync_token:String::new(),coordinator_token:String::new()}
}
fn validate(f:&Form)->Result<(),String>{
 for v in [&f.display_name,&f.agent_id,&f.host,&f.port,&f.database,&f.user,&f.coordinator_url,&f.interval_seconds]{
  if v.chars().any(|c|c.is_control()){return Err("入力欄に改行や制御文字は使用できません。".into())}
 }
 if f.display_name.chars().count()>60{return Err("端末名は60文字以内で指定してください。".into())}
 if f.agent_id.len()<3||f.agent_id.len()>100||!f.agent_id.bytes().all(|b|b.is_ascii_alphanumeric()||b"-_".contains(&b)){return Err("予備機IDは半角英数字・ハイフン・アンダーバーで3～100文字にしてください。".into())}
 if f.port.trim().parse::<u16>().ok().filter(|p|*p>0).is_none(){return Err("接続ポートが正しくありません。".into())}
 if !f.interval_seconds.trim().parse::<u64>().ok().is_some_and(|n|(15..=3600).contains(&n)){return Err("基本同期間隔は15～3600秒で指定してください。".into())}
 if f.enabled{
  if [&f.host,&f.database,&f.user,&f.coordinator_url].iter().any(|s|s.trim().is_empty()){return Err("接続先の必須項目を入力してください。".into())}
  let u=url::Url::parse(f.coordinator_url.trim()).map_err(|_|"調整サーバーのURLが正しくありません。")?;
  if u.scheme()!="https"||u.host_str().is_none()||!u.username().is_empty()||u.password().is_some()||u.query().is_some()||u.fragment().is_some(){return Err("調整サーバーは認証情報やクエリを含まないHTTPSのURLを指定してください。".into())}
 }
 Ok(())
}
// Preserve unknown keys and sections; replace all known keys in [mics].
fn merge_ini(raw:&str,f:&Form)->String{
 let values:BTreeMap<&str,String>=BTreeMap::from([
 ("enabled",if f.enabled{"1"}else{"0"}.into()),("display_name",f.display_name.trim().into()),("agent_id",f.agent_id.trim().into()),("host",f.host.trim().into()),("port",f.port.trim().into()),("database",f.database.trim().into()),("user",f.user.trim().into()),("coordinator_url",f.coordinator_url.trim().into()),("interval_seconds",f.interval_seconds.trim().into())]);
 let mut lines=Vec::new();let mut active=false;let mut found=false;
 for original in raw.trim_start_matches('\u{feff}').lines(){
  let line=original.trim();
  if line.starts_with('['){active=line.eq_ignore_ascii_case("[mics]");lines.push(original.to_owned());
   if active&&!found{found=true;for (k,v) in &values{lines.push(format!("{k}={v}"));}}continue
  }
  if active&&!line.starts_with(';')&&!line.starts_with('#'){
   if let Some((k,_))=line.split_once('='){if values.contains_key(k.trim().to_ascii_lowercase().as_str()){continue}}
  }
  lines.push(original.to_owned());
 }
 if !found{lines.push("[mics]".into());for (k,v) in &values{lines.push(format!("{k}={v}"));}}
 format!("{}\n",lines.join("\n"))
}
fn save(dir:&Path,f:&Form)->Result<(),String>{
 validate(f)?;let ini=dir.join("mics.ini");let raw=read_ini(dir)?;let previous=credentials::load().ok();
 let secrets=credentials::Credentials{
  db_password:if f.password.is_empty(){previous.as_ref().map(|c|c.db_password.clone()).unwrap_or_default()}else{f.password.clone()},
  mics_sync_token:if f.sync_token.is_empty(){previous.as_ref().map(|c|c.mics_sync_token.clone()).unwrap_or_default()}else{f.sync_token.clone()},
  coordinator_token:if f.coordinator_token.is_empty(){previous.as_ref().map(|c|c.coordinator_token.clone()).unwrap_or_default()}else{f.coordinator_token.clone()}};
 let changed=!f.password.is_empty()||!f.sync_token.is_empty()||!f.coordinator_token.is_empty();
 let complete=!secrets.db_password.trim().is_empty()&&!secrets.mics_sync_token.trim().is_empty()&&!secrets.coordinator_token.trim().is_empty();
 if (f.enabled||changed)&&!complete{return Err("パスワードと2種類の認証トークンを入力してください。".into())}
 let tmp=dir.join(format!("mics.{}.ini.tmp",std::process::id()));
 fs::write(&tmp,merge_ini(&raw,f)).map_err(|_|"設定ファイルを書き込めません。")?;
 if ini.exists()&&fs::copy(&ini,dir.join("mics.ini.bak")).is_err(){let _=fs::remove_file(&tmp);return Err("設定のバックアップを作成できません。".into())}
 if changed&&credentials::save(&secrets).is_err(){let _=fs::remove_file(&tmp);return Err("認証情報を暗号化して保存できません。".into())}
 if fs::rename(&tmp,&ini).is_err(){let _=fs::remove_file(&tmp);return Err("設定を保存できませんでした。".into())}
 let k=RegKey::predef(HKEY_CURRENT_USER).create_subkey(RUN_KEY).map_err(|_|"設定は保存されましたが、自動起動の登録を開けません。")?.0;
 let expected=format!("\"{}\"",dir.join("mics-only.exe").display());
 if f.autostart{k.set_value(RUN_NAME,&expected).map_err(|_|"設定は保存されましたが、自動起動の登録に失敗しました。")?}
 else if let Ok(v)=k.get_value::<String,_>(RUN_NAME){if v.eq_ignore_ascii_case(&expected){k.delete_value(RUN_NAME).map_err(|_|"設定は保存されましたが、自動起動の解除に失敗しました。")?;}}
 Ok(())
}
fn status()->Value{
 ureq::AgentBuilder::new().timeout(Duration::from_secs(3)).build().get("http://127.0.0.1:8787/api/status").call().ok().and_then(|r|r.into_json::<Value>().ok()).filter(|v|v["ok"]==true).unwrap_or_else(||json!({"ok":false,"mode":"OFFLINE"}))
}
#[tauri::command]
fn load_settings()->Result<Value,String>{let dir=root()?;Ok(json!({"version":VERSION,"form":form_from(&read_ini(&dir)?,startup_enabled(&dir)),"credentials_configured":credentials::configured()}))}
#[tauri::command]
fn save_settings(form:Form)->Result<(),String>{save(&root()?,&form)}
#[tauri::command]
async fn agent_status()->Result<Value,String>{tauri::async_runtime::spawn_blocking(status).await.map_err(|_|"稼働状況を取得できません。".into())}
fn run_process(action:&str)->Result<(),String>{
 let dir=root()?;
 let output=Command::new("powershell.exe").args(["-NoProfile","-NonInteractive","-ExecutionPolicy","Bypass","-Command"]).arg(BRIDGE).env("MICS_SETTINGS_ROOT",&dir).env("MICS_SETTINGS_ACTION",action).current_dir(&dir).creation_flags(NO_WINDOW).stdin(Stdio::null()).output().map_err(|_|"操作を開始できません。")?;
 if !output.status.success(){return Err("操作に失敗しました。実行プログラムの状態とアクセス権限を確認してください。".into())}Ok(())
}
#[tauri::command]
async fn agent_action(action:String)->Result<(),String>{
 if !["start","stop","restart","web"].contains(&action.as_str()){return Err("不明な操作です。".into())}
 tauri::async_runtime::spawn_blocking(move||run_process(&action)).await.map_err(|_|"操作に失敗しました。".to_owned())?
}
#[tauri::command]
async fn connection_test()->Result<Value,String>{
 tauri::async_runtime::spawn_blocking(||->Result<Value,String>{
  let dir=root()?;let out=Command::new(dir.join("mics-only.exe")).arg("--test-connection").current_dir(&dir).creation_flags(NO_WINDOW).output().map_err(|_|"接続テストを開始できません。")?;
  let text=String::from_utf8_lossy(&out.stdout);
  Ok(json!({"ok":out.status.success(),"firebird":text.contains("FIREBIRD_TCP_OK"),"coordinator":text.contains("COORDINATOR_HTTP_OK"),"registration":text.contains("D1_PRESENCE_OK")}))
 }).await.map_err(|_|"接続テストに失敗しました。".to_owned())?
}
#[tauri::command]
async fn sync_aliases()->Result<String,String>{
 tauri::async_runtime::spawn_blocking(||->Result<String,String>{
  let response=ureq::AgentBuilder::new().timeout(Duration::from_secs(15)).build().put("http://127.0.0.1:8787/api/aliases/sync").call().map_err(|_|"別名・状態色の同期に失敗しました。ローカルの稼働状況をご確認ください。")?;
  let data:Value=response.into_json().map_err(|_|"同期結果を取得できません。")?;
  if data["ok"]!=true{return Err("別名・状態色の同期に失敗しました。".into())}Ok("別名・状態色の同期が完了しました。".into())
 }).await.map_err(|_|"同期に失敗しました。".to_owned())?
}
#[tauri::command]
async fn inspect_sql(query:Option<String>)->Result<String,String>{
 tauri::async_runtime::spawn_blocking(move||->Result<String,String>{
  let dir=root()?;let mut cmd=Command::new(dir.join("mics-only.exe"));cmd.current_dir(&dir).creation_flags(NO_WINDOW);
  let path=dir.join(format!("sql-inspector-{}.sql",std::process::id()));
  if let Some(q)=&query{
   if q.trim().is_empty()||q.len()>65536{return Err("SQL 文を入力してください（最大64KB）。".into())}
   fs::write(&path,q).map_err(|_|"SQL 文を保存できません。")?;cmd.arg("--sql-query").arg(&path);
  }else{cmd.arg("--dump-tables");}
  let result=cmd.output();if query.is_some(){let _=fs::remove_file(&path);}
  let out=result.map_err(|_|"実行プログラムを起動できません。")?;
  if !out.status.success(){return Err("実行に失敗しました。読み取り専用のSQL文（ENTRYNO列を含む）、接続設定、認証情報をご確認ください。".into())}
  let text=String::from_utf8_lossy(&out.stdout).trim().to_owned();
  Ok(if text.is_empty(){"該当データはありません。".into()}else{text})
 }).await.map_err(|_|"データベース調査に失敗しました。".to_owned())?
}
#[derive(Clone,Serialize)]
struct UpdateState{running:bool,percent:u32,message:String,staged:bool,failed:bool}
impl Default for UpdateState{fn default()->Self{Self{running:false,percent:0,message:"更新：待機中".into(),staged:false,failed:false}}}
#[derive(Default)]
struct Tracker(Arc<Mutex<UpdateState>>);
#[tauri::command]
fn update_state(tracker:tauri::State<'_,Tracker>)->Result<UpdateState,String>{tracker.0.lock().map(|s|s.clone()).map_err(|_|"更新状況を取得できません。".into())}
#[tauri::command]
fn start_update(tracker:tauri::State<'_,Tracker>)->Result<(),String>{
 let exe=env::current_exe().map_err(|_|"実行ファイルの場所を取得できません。")?;let shared=tracker.0.clone();
 {let mut s=shared.lock().map_err(|_|"更新状況を取得できません。")?;if s.running||s.staged{return Err("更新処理中です。".into())}*s=UpdateState{running:true,percent:1,message:"最新版を確認中…".into(),staged:false,failed:false};}
 std::thread::spawn(move||{
  let result=updater::install_with_progress(&exe,|p,m|{if let Ok(mut s)=shared.lock(){s.percent=p;s.message=m.into()}});
  if let Ok(mut s)=shared.lock(){s.running=false;match result{
   Ok(msg) if msg.starts_with("UPDATE_STAGED ")=>{s.staged=true;s.percent=100;s.message="更新の準備が完了しました。適用のため設定画面を閉じます。".into()},
   Ok(_)=>{s.percent=100;s.message="最新版を使用しています。".into()},
   Err(_)=>{s.failed=true;s.message="更新に失敗しました。ネットワークと更新ログをご確認ください。".into()}
  }}
 });Ok(())
}
#[tauri::command]
fn close_after_update(app:tauri::AppHandle,tracker:tauri::State<'_,Tracker>)->Result<(),String>{
 if tracker.0.lock().map_err(|_|"更新状態を取得できません。")?.staged{app.exit(0);Ok(())}else{Err("更新の準備が完了していません。".into())}
}
#[tauri::command]
fn save_traffic_csv(csv:String)->Result<String,String>{if csv.len()>1_000_000{return Err("CSVが大きすぎます。".into())}let dir=root()?.join("data");fs::create_dir_all(&dir).map_err(|e|e.to_string())?;let path=dir.join("traffic-export.csv");fs::write(&path,format!("\u{feff}{csv}")).map_err(|e|e.to_string())?;Ok(path.display().to_string())}
#[tauri::command]
async fn traffic_stats(days:u64)->Result<Value,String>{tauri::async_runtime::spawn_blocking(move||traffic::snapshot(days)).await.map_err(|_|"通信統計を読み込めません。".into())}
fn main(){
 let result=tauri::Builder::default().manage(Tracker::default()).invoke_handler(tauri::generate_handler![traffic_stats,save_traffic_csv,load_settings,save_settings,agent_status,agent_action,connection_test,sync_aliases,inspect_sql,update_state,start_update,close_after_update]).run(tauri::generate_context!());
 if result.is_err(){if let Ok(dir)=root(){let _=fs::write(dir.join("mics-only-settings-startup.log"),"設定画面を起動できません。Microsoft Edge WebView2 Runtime を確認してください。\n");}}
}
#[cfg(test)]
mod tests{
 use super::*;
 #[test]fn ini_preserves_config_and_other_sections(){
  let raw="[mics]\nenabled=0\nagent_id=abc\nunknown=保留\nport=3050\n[other]\nvalue=42\n";
  let mut f=form_from(raw,true);f.display_name="第一工場".into();f.password="not-written".into();
  let out=merge_ini(raw,&f);assert!(out.contains("unknown=保留"));assert!(out.contains("[other]\nvalue=42"));assert!(!out.contains("not-written"));assert_eq!(parse_ini(&out)["display_name"],"第一工場");assert!(validate(&f).is_ok());
 }
 #[test]fn validate_and_keep_empty_secret_fields(){
  let mut f=form_from("",false);assert!(f.password.is_empty());assert!(f.sync_token.is_empty());assert!(f.coordinator_token.is_empty());f.port="0".into();assert!(validate(&f).is_err());f.port="3050".into();f.enabled=true;assert!(validate(&f).is_err());
 }
}
