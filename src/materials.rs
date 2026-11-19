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
 let sql="SELECT TRIM(CAST(P.ENTRYNO AS VARCHAR(24))), COALESCE(TRIM(CAST(P.PARTSNO AS VARCHAR(80))),''), COALESCE(TRIM(CAST(R1.PARTSNAME AS VARCHAR(255))),''), COALESCE(TRIM(CAST(R2.PARTSNAME AS VARCHAR(255))),''), COALESCE(TRIM(CAST(R3.PARTSNAME AS VARCHAR(255))),''), COALESCE(TRIM(CAST(R4.PARTSNAME AS VARCHAR(255))),''), COALESCE(TRIM(CAST(R5.PARTSNAME AS VARCHAR(255))),''), COALESCE(TRIM(CAST(R6.PARTSNAME AS VARCHAR(255))),''), COALESCE(TRIM(CAST(P.MIXRATIO1 AS VARCHAR(40))), '') || '|' || COALESCE(TRIM(CAST(P.MIXRATIO2 AS VARCHAR(40))), '') || '|' || COALESCE(TRIM(CAST(P.MIXRATIO3 AS VARCHAR(40))), '') || '|' || COALESCE(TRIM(CAST(P.MIXRATIO4 AS VARCHAR(40))), '') || '|' || COALESCE(TRIM(CAST(P.MIXRATIO5 AS VARCHAR(40))), '') || '|' || COALESCE(TRIM(CAST(P.MIXRATIO6 AS VARCHAR(40))), ''), COALESCE(TRIM(CAST(P.WEIGHT AS VARCHAR(40))), '') FROM PARTS P LEFT JOIN RESIN R1 ON P.MATERIAL1=R1.ENTRYNO AND R1.PARTSNAME NOT CONTAINING '廃番' LEFT JOIN RESIN R2 ON P.MATERIAL2=R2.ENTRYNO AND R2.PARTSNAME NOT CONTAINING '廃番' LEFT JOIN RESIN R3 ON P.MATERIAL3=R3.ENTRYNO AND R3.PARTSNAME NOT CONTAINING '廃番' LEFT JOIN RESIN R4 ON P.MATERIAL4=R4.ENTRYNO AND R4.PARTSNAME NOT CONTAINING '廃番' LEFT JOIN RESIN R5 ON P.MATERIAL5=R5.ENTRYNO AND R5.PARTSNAME NOT CONTAINING '廃番' LEFT JOIN RESIN R6 ON P.MATERIAL6=R6.ENTRYNO AND R6.PARTSNAME NOT CONTAINING '廃番' WHERE COALESCE(P.DELETEFLG,0)=0 ORDER BY P.ENTRYNO";
 type Row=(String,String,String,String,String,String,String,String,String,String);
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
 let mut out=String::from("\u{FEFF}ENTRYNO,PARTSNO,MAT1,MAT2,MAT3,MAT4,MAT5,MAT6,MIXRATIO1,MIXRATIO2,MIXRATIO3,MIXRATIO4,MIXRATIO5,MIXRATIO6,WEIGHT\n");
 let mut count=0;
 for (id,code,a,b,c,d,e,f,ratios,weight) in rows {
   if code.is_empty(){continue}
   let values=[a,b,c,d,e,f];
   if values.iter().all(|v|v.is_empty()){continue}
   let mut fields=vec![id,code];fields.extend(values);fields.extend(ratios.split('|').map(str::to_owned));fields.push(weight);
   if fields.len()!=15{return Err("MATERIAL_BAD_RATIO_COLUMNS".into())}
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

// Match by exact PARTSNO. Ambiguous records are excluded to avoid wrong recipes.
#[derive(Clone,Debug,PartialEq)]
pub struct MaterialDetail {
 pub name: String,
 pub percentage: Option<f64>,
}
#[derive(Clone,Debug,PartialEq)]
pub struct MaterialProfile {
 pub details: Vec<MaterialDetail>,
 // MICS PARTS.WEIGHT is displayed raw because its unit is not verified.
 pub part_weight_raw: Option<String>,
}
fn valid_ratio(raw:&str)->Option<f64>{
 let v=raw.trim().parse::<f64>().ok()?;
 if v.is_finite()&&v>=0.0{Some(v)}else{None}
}
fn profile_from_csv_fields(f:&[String])->Option<MaterialProfile>{
 if f.len()!=8&&f.len()!=15{return None}
 if f[1].trim().is_empty(){return None}
 let has_ratios=f.len()==15;
 let ratios:Vec<Option<f64>>=(0..6).map(|i|{
  if has_ratios{valid_ratio(&f[8+i])}else{None}
 }).collect();
 // Never invent percentages if an active ingredient is missing its ratio.
 let complete=has_ratios&&(0..6).all(|i|{
  f[2+i].trim().is_empty()||f[2+i].contains("廃番")||ratios[i].is_some()
 });
 let sum:f64=ratios.iter().flatten().sum();
 let mut details=Vec::new();
 for i in 0..6{
  let name=f[2+i].trim();
  if name.is_empty()||name.contains("廃番"){continue}
  let percentage=if complete&&sum>0.0{ratios[i].map(|r|r*100.0/sum)}else{None};
  details.push(MaterialDetail{name:name.to_string(),percentage});
 }
 if details.is_empty(){return None}
 let part_weight_raw=if f.len()==15{
  Some(f[14].trim().to_string()).filter(|w|!w.is_empty())
 }else{None};
 Some(MaterialProfile{details,part_weight_raw})
}
pub fn schedule_material_profiles()->BTreeMap<String,MaterialProfile>{
 let content=match output_path().ok().and_then(|path|fs::read_to_string(path).ok()){
  Some(content)=>content,
  None=>return BTreeMap::new()
 };
 let mut map=BTreeMap::<String,MaterialProfile>::new();
 let mut ambiguous=BTreeSet::<String>::new();
 for line in content.lines().skip(1){
  let fields=parse_csv_line(line.trim_end_matches('\r'));
  let Some(profile)=profile_from_csv_fields(&fields)else{continue};
  let code=fields[1].clone();
  if let Some(previous)=map.get(&code){
   if previous!=&profile{ambiguous.insert(code);}
  }else{map.insert(code,profile);}
 }
 for code in ambiguous{map.remove(&code);}
 map
}
#[cfg(test)]
mod tests{
 use super::{parse_csv_line,profile_from_csv_fields};
 #[test]
 fn quoted_csv_material_names(){
  assert_eq!(parse_csv_line("\"22\",\"ABC\",\"PP, \"\"Grade A\"\"\""),
   vec!["22","ABC","PP, \"Grade A\""]);
 }
 #[test]
 fn mixratio_9800_200_becomes_98_and_2(){
  let mut f=vec!["3998".to_string(),"2013005B0002".to_string(),
   "P608".to_string(),"Pigment".to_string()];
  f.extend(vec!["".to_string();4]);
  f.extend(["9800","200","0","0","0","0","117.00"].iter().map(|x|x.to_string()));
  let p=profile_from_csv_fields(&f).unwrap();
  assert_eq!(p.details.len(),2);
  assert!((p.details[0].percentage.unwrap()-98.0).abs()<1e-8);
  assert!((p.details[1].percentage.unwrap()-2.0).abs()<1e-8);
  assert_eq!(p.part_weight_raw.as_deref(),Some("117.00"));
 }
 #[test]
 fn legacy_csv_never_fabricates_ratio(){
  let mut f=vec!["1".to_string(),"P".to_string(),"Resin".to_string()];
  f.extend(vec!["".to_string();5]);
  assert_eq!(profile_from_csv_fields(&f).unwrap().details[0].percentage,None);
 }
}
