//! Local-only hourly counters. Append compact events; no secrets, payloads,
//! DB writes or telemetry network requests. Each process has its own append log.
use std::{collections::BTreeMap,fs,io::Write,path::PathBuf,sync::{Mutex,OnceLock},time::Duration};
use serde_json::{json,Value};
use chrono::{Utc,FixedOffset,Datelike,Timelike};
type Counts=BTreeMap<String,u64>;
fn dir()->Option<PathBuf>{std::env::current_exe().ok()?.parent().map(|p|p.join("data").join("traffic"))}
fn hour()->i64{Utc::now().timestamp().div_euclid(3600)}
fn add(c:&mut Counts,key:&str,n:u64){let v=c.entry(key.to_owned()).or_default();*v=v.saturating_add(n)}
static LOCK:OnceLock<Mutex<i64>>=OnceLock::new();
pub(crate) fn record(op:&str,key:&str,n:u64){
 if n==0{return}
 let Some(dir)=dir()else{return};
 let Ok(mut last)=LOCK.get_or_init(||Mutex::new(i64::MIN)).lock()else{return};
 let now=hour();
 if *last!=now{
  if fs::create_dir_all(&dir).is_err(){return}
  if let Ok(entries)=fs::read_dir(&dir){for e in entries.flatten(){
   let name=e.file_name();let name=name.to_string_lossy();
   if let Some(h)=name.split('-').next().and_then(|s|s.parse::<i64>().ok()){
    if h<now-24*30{let _=fs::remove_file(e.path());}
   }
  }}
  *last=now;
 }
 let path=dir.join(format!("{}-{}.log",now,std::process::id()));
 if let Ok(mut file)=fs::OpenOptions::new().create(true).append(true).open(path){
  let _=file.write_all(format!("{op} {key} {n}\n").as_bytes());
 }
}
pub(crate) fn receipt(op:&str,v:&Value){
 let Some(attempts)=v["attempted"].as_u64()else{record(op,"turso_receipt_missing",1);return};
 record(op,"turso_receipts",1);
 record(op,"turso_requests",attempts);
 for (field,key) in [("responded","turso_responses"),("http_ok","turso_http_ok"),("pipeline_ok","turso_pipeline_ok")]{
  record(op,key,v[field].as_u64().unwrap_or(0).min(attempts));
 }
}
pub(crate) fn has_turso(op:&str)->bool{matches!(op,"sync"|"schedule"|"cache")}
fn header_receipt(op:&str,r:&ureq::Response){
 if !has_turso(op){return}
 let mut v=json!({});
 for (field,header) in [("attempted","x-mics-turso-requests"),("responded","x-mics-turso-responses"),("http_ok","x-mics-turso-http-ok"),("pipeline_ok","x-mics-turso-pipeline-ok")]{
  if let Some(n)=r.header(header).and_then(|s|s.parse::<u64>().ok()){v[field]=json!(n);}
 }
 receipt(op,&v);
}
pub(crate) fn http(op:&str,worker:bool,request:impl FnOnce()->Result<ureq::Response,ureq::Error>)->Result<ureq::Response,ureq::Error>{
 let key=if worker{"worker_http"}else{"github_http"};
 record(op,key,1);
 let result=request();
 match &result{
  Ok(r)=>{record(op,if worker{"worker_http_response"}else{"github_http_response"},1);if worker{header_receipt(op,r);}},
  Err(ureq::Error::Status(_,r))=>{record(op,if worker{"worker_http_response"}else{"github_http_response"},1);record(op,"http_error",1);if worker{header_receipt(op,r);}},
  Err(_)=>{record(op,"http_error",1);if worker&&has_turso(op){record(op,"turso_receipt_missing",1);}}
 }
 result
}
pub(crate) fn firebird(op:&str,elapsed:Duration,ok:bool){
 record(op,"firebird_reads",1);record(op,"firebird_ms",elapsed.as_millis().min(u64::MAX as u128) as u64);
 if !ok{record(op,"firebird_error",1)}
}
fn parse_line(line:&str,ops:&mut BTreeMap<String,Counts>){
 let mut it=line.split_whitespace();let (Some(op),Some(key),Some(n))=(it.next(),it.next(),it.next().and_then(|s|s.parse::<u64>().ok()))else{return};
 if it.next().is_some()||op.len()>40||key.len()>40{return}
 add(ops.entry(op.to_owned()).or_default(),key,n);
}
pub(crate) fn snapshot(days:u64)->Value{
 let days=days.clamp(1,30);let now=hour();
 let jst=FixedOffset::east_opt(9*3600).expect("JST");
 let date=Utc::now().with_timezone(&jst);
 let first=now-date.hour() as i64-(days as i64-1)*24;
 let mut by_hour:BTreeMap<i64,BTreeMap<String,Counts>>=BTreeMap::new();
 if let Some(dir)=dir(){if let Ok(entries)=fs::read_dir(dir){for e in entries.flatten(){
  let name=e.file_name();let name=name.to_string_lossy();
  if !name.ends_with(".log"){continue}
  let Some(h)=name.split('-').next().and_then(|s|s.parse::<i64>().ok())else{continue};
  if h<first||h>now{continue}
  if let Ok(raw)=fs::read_to_string(e.path()){let ops=by_hour.entry(h).or_default();for line in raw.lines(){parse_line(line,ops);}}
 }}}
 let mut totals=Counts::new();let mut operations:BTreeMap<String,Counts>=BTreeMap::new();
 let mut hours=Vec::new();
 for h in first..=now{
  let ops=by_hour.remove(&h).unwrap_or_default();let mut count=Counts::new();
  for (op,c) in &ops{for (key,n) in c{add(&mut totals,key,*n);add(&mut count,key,*n);add(operations.entry(op.clone()).or_default(),key,*n);}}
  let label=chrono::DateTime::from_timestamp(h*3600,0).map(|d|d.with_timezone(&jst).format("%Y-%m-%d %H:00").to_string()).unwrap_or_default();
  hours.push(json!({"hour":label,"counts":count}));
 }
 json!({"timezone":"JST","days":days,"totals":totals,"hours":hours,"operations":operations})
}
#[cfg(test)]
mod tests{use super::*;#[test]fn aggregates_without_double_counting(){let mut o=BTreeMap::new();parse_line("sync worker_http 1",&mut o);parse_line("sync turso_requests 4",&mut o);parse_line("sync turso_requests 2",&mut o);parse_line("sync invalid xx",&mut o);assert_eq!(o["sync"]["worker_http"],1);assert_eq!(o["sync"]["turso_requests"],6);assert!(!o["sync"].contains_key("invalid"));}}
