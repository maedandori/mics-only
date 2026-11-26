//! GitHub Release updater shared by Settings and background standby agent.
//! Never contacts Firebird, changes configuration, or touches data snapshots.
#![cfg(windows)]
use std::{fs, io::{Read,Write}, path::Path, process::Command, time::Duration};
use std::os::windows::{process::CommandExt,fs::OpenOptionsExt};
use sha2::{Digest,Sha256};
const CREATE_NO_WINDOW:u32=0x08000000;
use std::sync::atomic::{AtomicBool,Ordering};
static UPDATER_BUSY:AtomicBool=AtomicBool::new(false);
struct Unlock;
impl Drop for Unlock{fn drop(&mut self){UPDATER_BUSY.store(false,Ordering::SeqCst)}}

fn parse_version(s:&str)->Option<(u64,u64,u64)>{
 let v=s.strip_prefix('v')?;
 let parts:Vec<_>=v.split('.').collect();
 if parts.len()!=3{return None}
 Some((parts[0].parse().ok()?,parts[1].parse().ok()?,parts[2].parse().ok()?))
}
const LEGACY_ASSET:&str="MICS-Only-Portable-Windows.zip";
const MIGRATION_ASSET:&str="MICS-Only-Launcher-Windows.zip";
fn select_asset(info:&serde_json::Value,manual:bool)->Result<(&serde_json::Value,bool),String>{
 let assets=info["assets"].as_array().ok_or("Missing release assets")?;
 if let Some(asset)=assets.iter().find(|a|a["name"]==LEGACY_ASSET){return Ok((asset,false))}
 if let Some(asset)=assets.iter().find(|a|a["name"]==MIGRATION_ASSET){
  if manual{return Ok((asset,true))}
  return Err("Launcher migration must be started from Settings on the PC".into())
 }
 Err("Compatible ZIP asset missing".into())
}
fn release(manual:bool)->Result<Option<(String,String,u64,Option<String>,bool)>,String>{
 let info:serde_json::Value=super::traffic::http("update_check",false,||ureq::AgentBuilder::new().timeout(Duration::from_secs(15))
  .build().get("https://api.github.com/repos/maedandori/mics-only/releases/latest")
  .set("User-Agent","mics-only-updater")
  .call()).map_err(|e|format!("Release check: {e}"))?
  .into_json().map_err(|e|e.to_string())?;
 let tag=info["tag_name"].as_str().ok_or("Missing release tag")?;
 let latest=parse_version(tag).ok_or("Unsupported release version")?;
 let installed=parse_version(concat!("v",env!("CARGO_PKG_VERSION"))).ok_or("Invalid installed version")?;
 if latest<=installed{return Ok(None)}
 let (asset,migration)=select_asset(&info,manual)?;
 let asset_name=if migration{MIGRATION_ASSET}else{LEGACY_ASSET};
 let url=asset["browser_download_url"].as_str().ok_or("Missing asset URL")?;
 if url!=format!("https://github.com/maedandori/mics-only/releases/download/{tag}/{asset_name}"){return Err("Untrusted release URL".into())}
 let size=asset["size"].as_u64().ok_or("Unknown ZIP size")?;
 if size==0||size>120_000_000{return Err("Unexpected ZIP size".into())}
 let digest=asset["digest"].as_str().and_then(|d|d.strip_prefix("sha256:")).map(str::to_owned);
 if migration&&digest.is_none(){return Err("Launcher migration requires SHA256".into())}
 if let Some(d)=&digest{
  if d.len()!=64||!d.bytes().all(|c|c.is_ascii_hexdigit()){return Err("Invalid digest".into())}
 }
 Ok(Some((tag.to_owned(),url.to_owned(),size,digest,migration)))
}
pub fn install_if_available(exe:&Path)->Result<String,String>{
 install_with_progress(exe,|_,_|{})
}
pub fn install_with_progress(exe:&Path,mut progress:impl FnMut(u32,&str))->Result<String,String>{
 progress(2,"最新版を確認中");
 if UPDATER_BUSY.swap(true,Ordering::SeqCst){return Err("Update already running".into())}
 let _unlock=Unlock;
 let folder=exe.parent().ok_or("Cannot find installation directory")?;
 let is_background_agent=std::env::current_exe().ok()
  .and_then(|p|p.file_name().and_then(|n|n.to_str()).map(|s|s.eq_ignore_ascii_case("mics-only.exe")))
  .unwrap_or(false);
 let data=folder.join("data");
 fs::create_dir_all(&data).map_err(|e|e.to_string())?;
 let _download_lock=fs::OpenOptions::new().create(true).read(true).write(true).share_mode(0)
  .open(data.join("update-download.lock")).map_err(|_|"Another update is preparing")?;
 let _probe=fs::OpenOptions::new().create(true).read(true).write(true).share_mode(0)
  .open(data.join("update-apply.lock")).map_err(|_|"Another update is applying")?;
 drop(_probe);
 let Some((tag,url,size,digest,migration))=release(!is_background_agent)? else{
  progress(100,"最新版を使用しています");
  return Ok(format!("最新版を使用しています (v{}).",env!("CARGO_PKG_VERSION")))
 };
 progress(10,"更新版を確認しました。ダウンロード準備中");
 let blocked=folder.join("data").join("update-blocked-version.txt");
 if is_background_agent {
  if fs::read_to_string(&blocked).ok().is_some_and(|v|v.trim()==tag){
   return Err(format!("Release {tag} previously failed; waiting for a newer release"))
  }
 } else {
  let _=fs::remove_file(&blocked);
 }
 let dir=data.join("update-staging").join(format!("{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|e|e.to_string())?.as_millis()));
 fs::create_dir_all(&dir).map_err(|e|e.to_string())?;
 let zip=dir.join(if migration{MIGRATION_ASSET}else{LEGACY_ASSET});
 let response=super::traffic::http("update_download",false,||ureq::AgentBuilder::new().timeout(Duration::from_secs(180))
  .build().get(&url).set("User-Agent","mics-only-updater")
  .call()).map_err(|e|format!("Download error: {e}"))?;
 let mut dst=fs::File::create(&zip).map_err(|e|e.to_string())?;
 let mut reader=response.into_reader().take(120_000_001);
 let mut hasher=Sha256::new();
 let mut total=0u64;
 let mut last_percent=0u32;
 let mut buffer=[0u8;65536];
 loop{
  let n=reader.read(&mut buffer).map_err(|e|e.to_string())?;
  if n==0{break}
  dst.write_all(&buffer[..n]).map_err(|e|e.to_string())?;
  hasher.update(&buffer[..n]);total+=n as u64;
  let percent=10+((total.saturating_mul(75)/size).min(75)) as u32;
  if percent>last_percent { last_percent=percent; progress(percent,"更新ファイルを取得中"); }
 }
 dst.flush().map_err(|e|e.to_string())?;
 dst.sync_all().map_err(|e|e.to_string())?;
 drop(dst);
 progress(87,"ファイルのサイズと SHA256 を検証中");
 if total!=size{return Err("Release ZIP size mismatch".into())}
 if let Some(expected)=digest{
  if !format!("{:x}",hasher.finalize()).eq_ignore_ascii_case(&expected){
   return Err("Release ZIP SHA256 mismatch".into())
  }
 }
 progress(94,"検証完了。更新の適用準備中");
 let script=dir.join("apply-update.ps1");
 fs::write(&script,include_str!("../scripts/apply-update.ps1")).map_err(|e|e.to_string())?;
 let parent_id=if is_background_agent {0} else {std::process::id()};
 let mut cmd=Command::new("powershell.exe");
 cmd.args(["-NoProfile","-NonInteractive","-ExecutionPolicy","Bypass","-File"])
  .arg(&script).arg("-Archive").arg(&zip).arg("-Target").arg(folder)
  .arg("-ParentId").arg(parent_id.to_string())
  .arg("-Version").arg(&tag);
 if migration{cmd.arg("-Migration");}
 cmd.creation_flags(CREATE_NO_WINDOW).spawn()
  .map_err(|e|format!("Cannot start update helper: {e}"))?;
 progress(100,"準備完了。更新のため設定画面を閉じます");
 Ok(format!("UPDATE_STAGED {tag}; closing parent to apply and verify, recover last working EXE on failed startup; see data/update.log"))
}

#[cfg(test)]
mod tests{
 use super::*;
 #[test] fn migration_requires_settings(){
  let old=serde_json::json!({"assets":[{"name":LEGACY_ASSET}]});
  let next=serde_json::json!({"assets":[{"name":MIGRATION_ASSET},{"name":"MICS-Core-Windows.zip"}]});
  assert!(!select_asset(&old,false).unwrap().1);
  assert!(select_asset(&next,false).is_err());
  assert!(select_asset(&next,true).unwrap().1);
 }
 #[test] fn version_order(){
  assert!(parse_version("v0.2.4")>parse_version("v0.2.3"));
  assert_eq!(parse_version("latest"),None);
 }
}
