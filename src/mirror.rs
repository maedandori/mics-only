//! Read-only Cloudflare mirror; never connects Firebird while standby.
use serde_json::{json,Value};
use std::{fs,time::Duration};
pub fn refresh(c:&super::Config,token:&str,local:&super::local_web::LocalState)->Result<(),String>{
    if token.is_empty(){return Err("Mirror credentials unavailable".into())}
    let url=format!("{}/cache",c.coordinator_url.trim_end_matches('/'));
    let response=super::client().get(&url)
        .set("Authorization",&format!("Bearer {token}"))
        .call().map_err(|e|format!("MIRROR_FETCH_FAILED {e}"))?;
    let body:Value=response.into_json().map_err(|e|e.to_string())?;
    if body["ok"]!=true{return Err("Mirror was not acknowledged".into())}
    let machines=body["machines"].as_array().ok_or("Mirror machines missing")?;
    if machines.len()!=14{return Err("Mirror must contain 14 machines; keeping previous cache".into())}
    let last_sample=machines.iter().filter_map(|m|m["sampled_at"].as_str())
        .max().map(str::to_owned);
    local.update(json!({"source":"mics-primary-cache","sampled_at":last_sample,"machines":machines}))?;
    let batches=body["schedule"].as_array().ok_or("Mirror schedule missing")?;
    let mut rows=Vec::new();
    let mut from=String::new();
    let mut to=String::new();
    for item in batches{
        if let Some(v)=item["window_start"].as_str(){
            if from.is_empty()||v<from.as_str(){from=v.into()}
        }
        if let Some(v)=item["window_end"].as_str(){
            if v>to.as_str(){to=v.into()}
        }
        if let Some(group)=item["rows"].as_array(){rows.extend(group.iter().cloned())}
    }
    if rows.is_empty(){return Ok(())}
    if rows.len()>10000{return Err("Mirror schedule too large".into())}
    let payload=json!({"from":from,"to":to,"rows":rows});
    let path=super::schedule::local_path();
    if let Some(parent)=path.parent(){fs::create_dir_all(parent).map_err(|e|e.to_string())?}
    let bytes=serde_json::to_vec_pretty(&payload).map_err(|e|e.to_string())?;
    if fs::read(&path).ok().as_deref()==Some(bytes.as_slice()){return Ok(())}
    let tmp=path.with_extension("json.mirror-tmp");
    fs::write(&tmp,bytes).map_err(|e|e.to_string())?;
    let backup=path.with_extension("json.mirror-bak");
    if path.exists(){
        let _=fs::remove_file(&backup);
        fs::rename(&path,&backup).map_err(|e|e.to_string())?;
    }
    if let Err(e)=fs::rename(&tmp,&path){
        if backup.exists(){let _=fs::rename(&backup,&path)}
        return Err(e.to_string())
    }
    Ok(())
}
