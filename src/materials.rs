use crate::Config;
use rsfbclient::prelude::*;
use std::{collections::{BTreeMap,BTreeSet},env,fs,path::PathBuf};
use chrono::Utc;
use serde_json::{json,Value};

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
 // Independent full RESIN catalog, for offline use only. No Turso/R2 upload.
 // Keep deleted and obsolete entries locally; presentation filters them.
 let resin_sql="SELECT TRIM(CAST(ENTRYNO AS VARCHAR(24))), COALESCE(TRIM(CAST(PARTSNO AS VARCHAR(80))),''), COALESCE(TRIM(CAST(PARTSNAME AS VARCHAR(255))),''), COALESCE(TRIM(CAST(DELETEFLG AS VARCHAR(8))),'0') FROM RESIN ORDER BY ENTRYNO";
 let resin_rows:Vec<(String,String,String,String)>=db.query(resin_sql,())
  .map_err(|e|format!("RESIN_QUERY_FAILED {e}"))?;
 if resin_rows.is_empty(){return Err("RESIN_EMPTY: preserving existing catalog".into())}
 let resin_catalog=json!({"rows":resin_rows.into_iter().map(|(entry_no,parts_no,name,deleted)|{
  json!({"entry_no":entry_no,"parts_no":parts_no,"name":name,
   "deleted":deleted!="0","obsolete":name.contains("廃番")})
 }).collect::<Vec<Value>>()});
 let resin_path=output_path()?.with_file_name("resin.json");
 write_if_changed(&resin_path,&serde_json::to_vec(&resin_catalog).map_err(|e|e.to_string())?)?;
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
fn write_if_changed(path:&PathBuf,bytes:&[u8])->Result<(),String>{
 if fs::read(path).ok().as_deref()==Some(bytes){return Ok(())}
 let tmp=path.with_extension("tmp");
 let backup=path.with_extension("bak");
 fs::write(&tmp,bytes).map_err(|e|e.to_string())?;
 if backup.exists(){fs::remove_file(&backup).map_err(|e|e.to_string())?}
 if path.exists(){fs::rename(path,&backup).map_err(|e|e.to_string())?}
 if let Err(e)=fs::rename(&tmp,path){
  if backup.exists(){let _=fs::rename(&backup,path);}
  return Err(format!("MATERIAL_CATALOG_SAVE_FAILED {e}"))
 }
 let _=fs::remove_file(&backup);
 Ok(())
}

// PARTSNO is verified in both SCHEDULE and PARTS; use exact values only.
// Ambiguous duplicate PARTSNO entries are excluded rather than risking wrong materials.
pub fn schedule_material_names()->BTreeMap<String,Vec<String>>{
 let content=match fs::read_to_string(output_path().unwrap_or_default()){
  Ok(content)=>content,
  Err(_)=>return BTreeMap::new()
 };
 let mut map=BTreeMap::<String,Vec<String>>::new();
 let mut ambiguous=BTreeSet::<String>::new();
 for line in content.lines().skip(1){
  let f=parse_csv_line(line.trim_end_matches('\r'));
  if f.len()!=8||f[1].is_empty(){continue}
  let names=f[2..].iter().filter(|name|!name.trim().is_empty()&&!name.contains("廃番"))
   .cloned().collect::<Vec<_>>();
  if names.is_empty(){continue}
  let code=f[1].clone();
  if let Some(previous)=map.get(&code){
   if previous!=&names{ambiguous.insert(code);}
  }else{map.insert(code,names);}
 }
 for code in ambiguous{map.remove(&code);}
 map
}

#[cfg(test)]
mod tests{
 use super::parse_csv_line;
 #[test]
 fn material_csv_escapes_quoted_names(){
  assert_eq!(parse_csv_line("\"22\",\"ABC\",\"PP, \"\"Grade A\"\"\""),
   vec!["22","ABC","PP, \"Grade A\""]);
 }
}
