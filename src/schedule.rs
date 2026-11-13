//! Read-only Firebird schedule cache shared by local dashboard.
use rsfbclient::prelude::*;
use serde_json::{json,Value};
use std::{collections::BTreeSet,fs,path::PathBuf};
use chrono::{Datelike,FixedOffset,Utc};
use sha2::{Digest,Sha256};
const MACHINES:[i32;14]=[5,6,7,8,10,11,12,13,14,15,16,17,18,19];
fn window()->(String,String,String){
 let jst=FixedOffset::east_opt(9*3600).expect("JST");
 let now=Utc::now().with_timezone(&jst);
 // Align to Monday 00:00 JST so the full current and following calendar
 // weeks are available, even when the current day is Friday or Sunday.
 let today=now.date_naive();
 let monday=today-chrono::Duration::days(today.weekday().num_days_from_monday() as i64);
 let start=monday.and_hms_opt(0,0,0).expect("week start");
 let fmt=|d:chrono::NaiveDateTime|d.format("%Y-%m-%d %H:%M:%S").to_string();
 (fmt(start),fmt(start+chrono::Duration::days(14)),fmt(start+chrono::Duration::days(30)))
}
fn normalize_time(s:&str)->Result<String,String>{
 let s=s.trim().replace('T'," ");
 if s.len()<19{return Err("Invalid schedule date".into())}
 let date=s.get(..19).ok_or("Invalid schedule date")?;
 chrono::NaiveDateTime::parse_from_str(date,"%Y-%m-%d %H:%M:%S").map_err(|e|format!("Invalid schedule date: {e}"))?;
 Ok(date.to_string())
}

fn read(host:&str,port:u16,db_name:&str,user:&str,password:&str,first:&str,end:&str)->Result<Vec<Value>,String>{
 let mut b=rsfbclient::builder_pure_rust();
 b.host(host).port(port).db_name(db_name).user(user).pass(password);
 b.with_transaction(|t|t.read_only().no_wait());
 let mut db=b.connect().map_err(|e|e.to_string())?;
 let fields:Vec<(String,)>=db.query(
  "SELECT TRIM(RDB$FIELD_NAME) FROM RDB$RELATION_FIELDS WHERE RDB$RELATION_NAME='SCHEDULE'",()
 ).map_err(|e|e.to_string())?;
 let known:BTreeSet<String>=fields.into_iter().map(|(f,)|f.to_ascii_uppercase()).collect();
 for field in ["MACHINENO","MACHINESEQ","MORDERNO","PARTSNAME","TOOLENTRYNO","MSTART","MEND","DELETEFLG"]{
  if !known.contains(field){return Err(format!("SCHEDULE missing {field}; keeping previous D1 snapshot"))}
 }
 let color=if known.contains("COLORNAME"){"S.COLORNAME"}else if known.contains("COLOR"){"S.COLOR"}else{"NULL"};
 let sql=format!(
  "SELECT TRIM(CAST(S.MACHINENO AS VARCHAR(24))), \
  COALESCE(TRIM(CAST(S.MACHINESEQ AS VARCHAR(30))),''), \
  COALESCE(TRIM(CAST(S.MORDERNO AS VARCHAR(60))),''), \
  COALESCE(TRIM(CAST(S.PARTSNAME AS VARCHAR(255))),''), \
  COALESCE(TRIM(CAST(S.TOOLENTRYNO AS VARCHAR(60))),''), \
  COALESCE(TRIM(CAST(T.TOOLNO AS VARCHAR(100))),''), \
  COALESCE(TRIM(CAST(T.TOOLNAME AS VARCHAR(255))),''), \
  COALESCE(TRIM(CAST({color} AS VARCHAR(100))),''), \
  COALESCE(TRIM(CAST(S.MSTART AS VARCHAR(40))),''), \
  COALESCE(TRIM(CAST(S.MEND AS VARCHAR(40))), '') \
  FROM SCHEDULE S LEFT JOIN TOOL T ON T.ENTRYNO=S.TOOLENTRYNO \
  WHERE COALESCE(S.DELETEFLG,0)=0 AND S.MACHINENO IN (5,6,7,8,10,11,12,13,14,15,16,17,18,19) \
  AND S.MSTART < TIMESTAMP '{end}' AND (S.MEND >= TIMESTAMP '{first}' OR S.MSTART >= TIMESTAMP '{first}') \
  ORDER BY S.MACHINENO,S.MSTART,S.MACHINESEQ,S.MORDERNO");
 let rows:Vec<(String,String,String,String,String,String,String,String,String,String)>=db.query(&sql,()).map_err(|e|e.to_string())?;
 if rows.len()>20000{return Err("Schedule exceeds local cache limit".into())}
 let mut out=Vec::new();
 for (machine,sequence,order,product,entry,mold_no,mold_name,color,start,end) in rows{
  let number=machine.parse::<i32>().map_err(|_|"Invalid machine number")?;
  if !MACHINES.contains(&number){return Err("Unexpected machine number".into())}
  out.push(json!({"machine_no":number,"sequence":sequence,"order_no":order,
   "product_name":product,"tool_entry_no":entry,"mold_no":mold_no,
   "mold_name":mold_name,"color":color,"start_at":normalize_time(&start)?,
   "end_at":if end.is_empty(){Value::Null}else{json!(normalize_time(&end)?)} }));
 }
 Ok(out)
}

pub fn local_path()->PathBuf {
    super::local_web::default_snapshot_path().with_file_name("schedule-local.json")
}
pub fn collect(c:&super::Config,password:&str)->Result<Value,String>{
    let (from,_to,local_end)=window();
    let rows=read(&c.host,c.port,&c.database,&c.user,password,&from,&local_end)?;
    if rows.is_empty(){return Err("Schedule returned no rows; keeping previous local cache".into())}
    let cache=json!({"from":from,"to":local_end,"rows":rows});
    let path=local_path();
    if let Some(dir)=path.parent(){fs::create_dir_all(dir).map_err(|e|e.to_string())?}
    // Never replace a usable cache with an empty or invalid response.
    let bytes=serde_json::to_vec_pretty(&cache).map_err(|e|e.to_string())?;
    if fs::read(&path).ok().as_deref()==Some(bytes.as_slice()){return Ok(cache)}
    let tmp=path.with_extension("json.tmp");
    fs::write(&tmp,&bytes).map_err(|e|e.to_string())?;
    let bak=path.with_extension("json.bak");
    if path.exists(){
        let _=fs::remove_file(&bak);
        fs::rename(&path,&bak).map_err(|e|e.to_string())?;
    }
    if let Err(e)=fs::rename(&tmp,&path){
        if bak.exists(){let _=fs::rename(&bak,&path);}
        return Err(e.to_string())
    }
    Ok(cache)
}

pub fn upload_if_changed(c:&super::Config,token:&str,grant:&super::Lease,cache:&Value)->Result<bool,String>{
    let from=cache["from"].as_str().ok_or("Missing schedule from")?;
    let to=chrono::NaiveDateTime::parse_from_str(from,"%Y-%m-%d %H:%M:%S")
        .map_err(|e|e.to_string())?+chrono::Duration::days(14);
    let to=to.format("%Y-%m-%d %H:%M:%S").to_string();
    let rows=cache["rows"].as_array().ok_or("Missing schedule rows")?
        .iter().filter(|r|{
            let start=r["start_at"].as_str().unwrap_or("");
            let end=r["end_at"].as_str().unwrap_or(start);
            start<to.as_str()&&end>=from
        }).cloned().collect::<Vec<Value>>();
    if rows.is_empty()||rows.len()>3000{return Err("Invalid 14-day backup schedule snapshot".into())}
    let payload=json!({"source":"mics-print-schedule","window_start":from,"window_end":to,
        "machines":MACHINES,"rows":rows});
    let bytes=serde_json::to_vec(&payload).map_err(|e|e.to_string())?;
    let digest=format!("{:x}",Sha256::digest(&bytes));
    let ack=local_path().with_file_name("schedule-last-ack.sha256");
    if fs::read_to_string(&ack).ok().as_deref()==Some(digest.as_str()){return Ok(false)}
    let endpoint=format!("{}/schedule",c.coordinator_url.trim_end_matches('/'));
    let response=super::client().post(&endpoint)
        .set("Authorization",&format!("Bearer {token}"))
        .set("X-Mics-Agent-Id",&c.agent_id)
        .set("X-Mics-Lease-Token",&grant.lease_token)
        .set("X-Mics-Lease-Epoch",&grant.epoch.to_string())
        .send_json(payload).map_err(|e|format!("SCHEDULE_UPLOAD_FAILED {e}"))?;
    let result:Value=response.into_json().map_err(|e|e.to_string())?;
    if result["ok"]!=true{return Err("Schedule sync not acknowledged".into())}
    fs::write(ack,digest).map_err(|e|e.to_string())?;
    Ok(true)
}
