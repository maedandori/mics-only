//! GitHub Release updater shared by Settings and background standby agent.
//! Never contacts Firebird, changes configuration, or touches data snapshots.
#![cfg(windows)]
use std::{fs, io::{Read,Write}, path::Path, process::Command, time::Duration};
use std::os::windows::process::CommandExt;
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
fn release()->Result<Option<(String,String,u64,Option<String>)>,String>{
 let info:serde_json::Value=ureq::AgentBuilder::new().timeout(Duration::from_secs(15))
  .build().get("https://api.github.com/repos/maedandori/mics-only/releases/latest")
  .set("User-Agent","mics-only-updater")
  .call().map_err(|e|format!("Release check: {e}"))?
  .into_json().map_err(|e|e.to_string())?;
 let tag=info["tag_name"].as_str().ok_or("Missing release tag")?;
 let latest=parse_version(tag).ok_or("Unsupported release version")?;
 let installed=parse_version(concat!("v",env!("CARGO_PKG_VERSION"))).ok_or("Invalid installed version")?;
 if latest<=installed{return Ok(None)}
 let asset=info["assets"].as_array().and_then(|a|a.iter().find(|v|
  v["name"]=="MICS-Only-Portable-Windows.zip")).ok_or("Missing ZIP asset")?;
 let url=asset["browser_download_url"].as_str().ok_or("Missing asset URL")?;
 if !url.starts_with("https://github.com/maedandori/mics-only/releases/download/")
   || !url.ends_with("/MICS-Only-Portable-Windows.zip"){return Err("Untrusted release URL".into())}
 let size=asset["size"].as_u64().ok_or("Unknown ZIP size")?;
 if size==0||size>120_000_000{return Err("Unexpected ZIP size".into())}
 let digest=asset["digest"].as_str().and_then(|d|d.strip_prefix("sha256:")).map(str::to_owned);
 if let Some(d)=&digest{
  if d.len()!=64||!d.bytes().all(|c|c.is_ascii_hexdigit()){return Err("Invalid digest".into())}
 }
 Ok(Some((tag.to_owned(),url.to_owned(),size,digest)))
}
pub fn install_if_available(exe:&Path)->Result<String,String>{
 if UPDATER_BUSY.swap(true,Ordering::SeqCst){return Err("Update already running".into())}
 let _unlock=Unlock;
 let Some((tag,url,size,digest))=release()? else{
  return Ok(format!("Already up to date (v{}).",env!("CARGO_PKG_VERSION")))
 };
 let folder=exe.parent().ok_or("Cannot find installation directory")?;
 let blocked=folder.join("data").join("update-blocked-version.txt");
 if fs::read_to_string(blocked).ok().is_some_and(|v|v.trim()==tag){
  return Err(format!("Release {tag} previously failed; waiting for a newer release"))
 }
 let dir=folder.join("data").join("update-staging");
 fs::create_dir_all(&dir).map_err(|e|e.to_string())?;
 let zip=dir.join("MICS-Only-Portable-Windows.zip");
 let response=ureq::AgentBuilder::new().timeout(Duration::from_secs(180))
  .build().get(&url).set("User-Agent","mics-only-updater")
  .call().map_err(|e|format!("Download error: {e}"))?;
 let mut dst=fs::File::create(&zip).map_err(|e|e.to_string())?;
 let mut reader=response.into_reader().take(120_000_001);
 let mut hasher=Sha256::new();
 let mut total=0u64;
 let mut buffer=[0u8;65536];
 loop{
  let n=reader.read(&mut buffer).map_err(|e|e.to_string())?;
  if n==0{break}
  dst.write_all(&buffer[..n]).map_err(|e|e.to_string())?;
  hasher.update(&buffer[..n]);total+=n as u64;
 }
 dst.flush().map_err(|e|e.to_string())?;
 if total!=size{return Err("Release ZIP size mismatch".into())}
 if let Some(expected)=digest{
  if !format!("{:x}",hasher.finalize()).eq_ignore_ascii_case(&expected){
   return Err("Release ZIP SHA256 mismatch".into())
  }
 }
 let script=dir.join("apply-update.ps1");
 fs::write(&script,include_str!("../scripts/apply-update.ps1")).map_err(|e|e.to_string())?;
 Command::new("powershell.exe")
  .args(["-NoProfile","-NonInteractive","-ExecutionPolicy","Bypass","-File"])
  .arg(&script).arg("-Archive").arg(&zip).arg("-Target").arg(folder)
  .arg("-ParentId").arg(if exe.file_name().and_then(|n|n.to_str())==Some("mics-only.exe"){0}else{std::process::id()}.to_string())
  .arg("-Version").arg(&tag)
  .creation_flags(CREATE_NO_WINDOW).spawn()
  .map_err(|e|format!("Cannot start update helper: {e}"))?;
 Ok(format!("UPDATE_STAGED {tag}; closing parent to apply and verify, rollback on failure"))
}

#[cfg(test)]
mod tests{
 use super::*;
 #[test] fn version_order(){
  assert!(parse_version("v0.2.4")>parse_version("v0.2.3"));
  assert_eq!(parse_version("latest"),None);
 }
}
