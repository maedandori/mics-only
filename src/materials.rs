use crate::Config;
use rsfbclient::prelude::*;
use std::{collections::BTreeMap,env,fs,path::PathBuf};
use chrono::Utc;
use sha2::Digest;

fn csv_field(s:&str)->String { format!("\"{}\"",s.replace('"',"\"\"")) }
fn output_path()->Result<PathBuf,String>{
 let exe=env::current_exe().map_err(|e|e.to_string())?;
 let dir=exe.parent().ok_or("Missing EXE directory")?.join("data");
 fs::create_dir_all(&dir).map_err(|e|e.to_string())?;
 Ok(dir.join("materials.csv"))
}
// Read-only Firebird connection. Never alter MICS database.
// Reconcile once per day; rewrite local CSV only when data changed.
pub fn sync(c:&Config,password:&str)->Result<usize,String>{
 let mut b=rsfbclient::builder_pure_rust();
 b.host(&c.host).port(c.port).db_name(&c.database).user(&c.user).pass(password);
 b.with_transaction(|tr|tr.read_only().no_wait());
 let mut db=b.connect().map_err(|e|e.to_string())?;
 let sql="SELECT TRIM(CAST(P.ENTRYNO AS VARCHAR(24))), COALESCE(TRIM(CAST(P.PARTSNO AS VARCHAR(80))),''), COALESCE(TRIM(CAST(R1.PARTSNAME AS VARCHAR(255))),''), COALESCE(TRIM(CAST(R2.PARTSNAME AS VARCHAR(255))),''), COALESCE(TRIM(CAST(R3.PARTSNAME AS VARCHAR(255))),''), COALESCE(TRIM(CAST(R4.PARTSNAME AS VARCHAR(255))),''), COALESCE(TRIM(CAST(R5.PARTSNAME AS VARCHAR(255))),''), COALESCE(TRIM(CAST(R6.PARTSNAME AS VARCHAR(255))),'') FROM PARTS P LEFT JOIN RESIN R1 ON P.MATERIAL1=R1.ENTRYNO AND R1.PARTSNAME NOT CONTAINING '廃番' LEFT JOIN RESIN R2 ON P.MATERIAL2=R2.ENTRYNO AND R2.PARTSNAME NOT CONTAINING '廃番' LEFT JOIN RESIN R3 ON P.MATERIAL3=R3.ENTRYNO AND R3.PARTSNAME NOT CONTAINING '廃番' LEFT JOIN RESIN R4 ON P.MATERIAL4=R4.ENTRYNO AND R4.PARTSNAME NOT CONTAINING '廃番' LEFT JOIN RESIN R5 ON P.MATERIAL5=R5.ENTRYNO AND R5.PARTSNAME NOT CONTAINING '廃番' LEFT JOIN RESIN R6 ON P.MATERIAL6=R6.ENTRYNO AND R6.PARTSNAME NOT CONTAINING '廃番' WHERE COALESCE(P.DELETEFLG,0)=0 ORDER BY P.ENTRYNO";
 type Row=(String,String,String,String,String,String,String,String);
 let rows:Vec<Row>=db.query(sql,()).map_err(|e|format!("MATERIAL_QUERY_FAILED {e}"))?;
 if rows.is_empty(){return Err("MATERIAL_EMPTY: preserving existing CSV".into())}
 let mut out=String::from("\u{FEFF}ENTRYNO,PARTSNO,MAT1,MAT2,MAT3,MAT4,MAT5,MAT6\n");
 let mut count=0;
 for (id,code,a,b,c,d,e,f) in rows {
   if code.is_empty(){continue}
   let values=[a,b,c,d,e,f];
   if values.iter().all(|v|v.is_empty()){continue}
   let mut fields=vec![id,code];fields.extend(values);
   out.push_str(&fields.iter().map(|s|csv_field(s)).collect::<Vec<_>>().join(","));
   out.push('\n');count+=1;
 }
 if count==0{return Err("MATERIAL_NO_MATCH: preserving existing CSV".into())}
 let path=output_path()?;
 if fs::read_to_string(&path).ok().as_deref()==Some(out.as_str()){return Ok(count)}
 let tmp=path.with_extension("csv.tmp");
 fs::write(&tmp,out.as_bytes()).map_err(|e|e.to_string())?;
 // Windows rename cannot replace an existing file: use backup + restore on failure.
 let backup=path.with_extension("csv.bak");
 if backup.exists(){fs::remove_file(&backup).map_err(|e|e.to_string())?;}
 if path.exists(){fs::rename(&path,&backup).map_err(|e|e.to_string())?;}
 if let Err(e)=fs::rename(&tmp,&path){
   if backup.exists(){let _=fs::rename(&backup,&path);}
   return Err(format!("MATERIAL_SAVE_FAILED {e}"));
 }
 let _=fs::remove_file(&backup);
 println!("MATERIAL_CSV_UPDATED count={count} at={} ",Utc::now().to_rfc3339());
 Ok(count)
}

fn parse_csv_line(line:&str)->Vec<String>{
 let mut fields=Vec::new();let mut field=String::new();let mut chars=line.chars().peekable();let mut quoted=false;
 while let Some(ch)=chars.next(){
  match ch{
   '"' if quoted&&chars.peek()==Some(&'"')=>{field.push('"');chars.next();},
   '"'=>quoted=!quoted,
   ',' if !quoted=>{fields.push(std::mem::take(&mut field));},
   _=>field.push(ch)
  }
 }
 fields.push(field);fields
}
#[derive(serde::Serialize,serde::Deserialize,Clone,PartialEq,Eq)]
struct MaterialEntry{
 parts_no:String,
 entry_no:String,
 names:Vec<String>,
}
#[derive(serde::Serialize,serde::Deserialize)]
struct MaterialUploadState{
 digest:String,
 rows:Vec<MaterialEntry>,
}
fn save_upload_state(path:&PathBuf,state:&MaterialUploadState)->Result<(),String>{
 let bytes=serde_json::to_vec(state).map_err(|e|e.to_string())?;
 let tmp=path.with_extension("json.tmp");
 let backup=path.with_extension("json.bak");
 fs::write(&tmp,bytes).map_err(|e|e.to_string())?;
 if backup.exists(){fs::remove_file(&backup).map_err(|e|e.to_string())?}
 if path.exists(){fs::rename(path,&backup).map_err(|e|e.to_string())?}
 if let Err(e)=fs::rename(&tmp,path){
  if backup.exists(){let _=fs::rename(&backup,path);}
  return Err(format!("MATERIAL_STATE_SAVE_FAILED {e}"))
 }
 let _=fs::remove_file(&backup);
 Ok(())
}

// Keep the complete PARTS -> RESIN mapping on disk. Turso only receives changed
// products (plus removed product keys) after a previously acknowledged upload.
pub fn upload(c:&Config,token:&str)->Result<usize,String>{
 let csv_path=output_path()?;
 let content=fs::read_to_string(&csv_path).map_err(|e|e.to_string())?;
 let mut unique=BTreeMap::<String,MaterialEntry>::new();
 for line in content.lines().skip(1){
  let fields=parse_csv_line(line.trim_end_matches('\r'));
  if fields.len()!=8||fields[1].is_empty(){continue}
  let names=fields[2..].iter()
   .filter(|name|!name.is_empty()&&!name.contains("廃番"))
   .cloned().collect::<Vec<_>>();
  if names.is_empty(){continue}
  let row=MaterialEntry{
   entry_no:fields[0].clone(),parts_no:fields[1].clone(),names
  };
  unique.insert(row.parts_no.clone(),row);
 }
 if unique.is_empty(){return Err("MATERIAL_UPLOAD_EMPTY: preserving acknowledged snapshot".into())}
 let digest=format!("{:x}",sha2::Sha256::digest(content.as_bytes()));
 let ack=csv_path.with_file_name("materials-last-ack.sha256");
 let state_path=csv_path.with_file_name("materials-upload-state.json");
 let current=MaterialUploadState{digest:digest.clone(),rows:unique.values().cloned().collect()};
 let previous:Option<MaterialUploadState>=fs::read(&state_path).ok()
  .and_then(|bytes|serde_json::from_slice(&bytes).ok());

 // Existing deployments only have a digest. If it matches, their entire
 // snapshot was already acknowledged; seed the per-product baseline locally.
 if previous.is_none()&&fs::read_to_string(&ack).ok().as_deref()==Some(digest.as_str()){
  save_upload_state(&state_path,&current)?;
  return Ok(0)
 }
 if previous.as_ref().is_some_and(|old|old.digest==digest){return Ok(0)}

 let is_delta=previous.is_some();
 let mut changed:Vec<MaterialEntry>=Vec::new();
 let mut removed:Vec<String>=Vec::new();
 if let Some(old)=previous.as_ref(){
  let old_by_code:BTreeMap<String,&MaterialEntry>=old.rows.iter()
   .map(|row|(row.parts_no.clone(),row)).collect();
  for row in &current.rows{
   if old_by_code.get(&row.parts_no).is_none_or(|prior|*prior!=row){
    changed.push(row.clone());
   }
  }
  for code in old_by_code.keys(){
   if !unique.contains_key(code){removed.push(code.clone())}
  }
 }else{
  changed=current.rows.clone();
 }
 if changed.is_empty()&&removed.is_empty(){
  save_upload_state(&state_path,&current)?;
  fs::write(&ack,&digest).map_err(|e|e.to_string())?;
  return Ok(0)
 }

 let endpoint=if c.coordinator_url.ends_with("/api/mics-failover"){
  format!("{}/api/integrations/mics/materials",
   c.coordinator_url.trim_end_matches("/api/mics-failover"))
 }else{
  format!("{}/materials",c.coordinator_url.trim_end_matches('/'))
   .replace("/api/mics-failover/materials","/api/integrations/mics/materials")
 };
 let mut operations:Vec<(Option<MaterialEntry>,Option<String>)>=changed.iter().cloned()
  .map(|row|(Some(row),None)).collect();
 operations.extend(removed.iter().cloned().map(|code|(None,Some(code))));
 let total=(operations.len()+199)/200;
 for (index,chunk) in operations.chunks(200).enumerate(){
  let rows=chunk.iter().filter_map(|(row,_)|row.as_ref()).collect::<Vec<_>>();
  let deleted=chunk.iter().filter_map(|(_,key)|key.as_ref()).collect::<Vec<_>>();
  let result:serde_json::Value=super::client().post(&endpoint)
   .set("Authorization",&format!("Bearer {token}"))
   .send_json(serde_json::json!({
    "mode":if is_delta{"delta"}else{"full"},
    "rows":rows,"removed":deleted,
    "snapshot_id":digest,"final":index+1==total
   }))
   .map_err(|e|format!("MATERIAL_DELTA_UPLOAD_FAILED {e}"))?
   .into_json().map_err(|e|e.to_string())?;
  if result["ok"]!=true{return Err("MATERIAL_DELTA_NOT_ACKNOWLEDGED".into())}
 }
 // Update the local baseline only after every batch was acknowledged.
 // A failed batch retries from the previous baseline next time.
 save_upload_state(&state_path,&current)?;
 fs::write(&ack,digest).map_err(|e|e.to_string())?;
 println!("MATERIAL_DELTA_SYNC changed={} removed={} total={}",
  changed.len(),removed.len(),current.rows.len());
 Ok(changed.len()+removed.len())
}
