//! Per-shift defect deltas from cumulative Firebird MACHINEWORK.TOTALQTYNG.
//! 07:30–19:00 day, 19:00–07:30 night (Asia/Tokyo).
//! Historical defects before the first observed sample are intentionally unknown.
use chrono::{Datelike, Timelike, FixedOffset, Utc, Duration as ChronoDuration};
use serde::{Serialize,Deserialize};
use serde_json::Value;
use std::{collections::BTreeMap,fs,path::Path};

#[derive(Default,Serialize,Deserialize)]
pub struct ShiftDefects {
    shift_key: String,
    machines: BTreeMap<i32, MachineDefects>,
}
#[derive(Default,Serialize,Deserialize)]
struct MachineDefects {
    last_total: i64,
    order_no: String,
    shift_total: i64,
}
pub fn current_shift_key() -> String {shift_key_at(Utc::now())}
fn shift_key_at(now:chrono::DateTime<Utc>) -> String {
    let jst=now.with_timezone(&FixedOffset::east_opt(9*3600).expect("JST"));
    let hour=jst.hour();
    let work_date=if hour<7 || (hour==7&&jst.minute()<30) {jst.date_naive()-ChronoDuration::days(1)}else{jst.date_naive()};
    let night=hour>=19 || hour<7 || (hour==7&&jst.minute()<30);
    format!("{:04}-{:02}-{:02}-{}",work_date.year(),work_date.month(),work_date.day(),if night{"night"}else{"day"})
}
impl ShiftDefects {
    pub fn needs_new_shift(&self)->bool {self.shift_key != current_shift_key()}
    pub fn load(path:&Path)->Self {
        fs::read(path).ok().and_then(|b|serde_json::from_slice(&b).ok()).unwrap_or_default()
    }
    // Only count deltas between observations in the current shift. A change in
    // production order or reset of cumulative counter must not yield negatives.
    pub fn apply(&mut self, data:&mut Value, path:&Path)->Result<(),String>{
        let key=current_shift_key();
        if self.shift_key!=key {
            self.shift_key=key.clone();
            self.machines.clear();
        }
        let rows=data.get_mut("machines").and_then(Value::as_array_mut)
            .ok_or("Missing machines array")?;
        if rows.is_empty(){return Ok(())}
        for m in rows.iter_mut(){
            let Some(no)=m.get("machine_no").and_then(|v|v.as_i64()).and_then(|n|i32::try_from(n).ok()) else{continue};
            let total=m.get("qty_ng").and_then(Value::as_i64).filter(|v|*v>=0);
            let Some(total)=total else{
                m["qty_ng_shift"]=Value::Null;
                m["qty_ng_shift_status"]=Value::String("unavailable".into());
                continue
            };
            let order=m.get("order_no").and_then(Value::as_str).unwrap_or("").to_owned();
            let entry=self.machines.entry(no).or_insert_with(||MachineDefects{
                last_total:total,order_no:order.clone(),shift_total:0
            });
            if entry.order_no!=order || total<entry.last_total {
                // On counter reset or order change, the current total may already
                // include production before the sample. Do not invent a delta.
                entry.order_no=order;
                entry.last_total=total;
            }else{
                entry.shift_total=entry.shift_total.saturating_add(total-entry.last_total);
                entry.last_total=total;
            }
            m["qty_ng_shift"]=Value::from(entry.shift_total);
            m["qty_ng_shift_status"]=Value::String("observed_from_first_sample".into());
            m["qty_ng_shift_key"]=Value::String(key.clone());
        }
        let bytes=serde_json::to_vec(self).map_err(|e|e.to_string())?;
        if let Some(dir)=path.parent(){fs::create_dir_all(dir).map_err(|e|e.to_string())?}
        let tmp=path.with_extension("json.tmp");
        fs::write(&tmp,bytes).map_err(|e|e.to_string())?;
        // Keep a valid copy of the previous state if a Windows rename fails.
        let bak=path.with_extension("json.bak");
        if path.exists(){
            let _=fs::remove_file(&bak);
            fs::rename(path,&bak).map_err(|e|e.to_string())?;
        }
        if let Err(e)=fs::rename(&tmp,path){
            if bak.exists(){let _=fs::rename(&bak,path);}
            return Err(e.to_string())
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn jst_shift_boundaries(){
        use chrono::TimeZone;
        let t=|h,m|Utc.with_ymd_and_hms(2026,11,14,h,m,0).single().unwrap();
        // 2026-11-14 07:29 JST is 2026-11-13 22:29 UTC.
        assert_eq!(shift_key_at(Utc.with_ymd_and_hms(2026,11,13,22,29,0).single().unwrap()),"2026-11-13-night");
        assert_eq!(shift_key_at(Utc.with_ymd_and_hms(2026,11,13,22,30,0).single().unwrap()),"2026-11-14-day");
        assert_eq!(shift_key_at(t(9,59)),"2026-11-14-day");
        assert_eq!(shift_key_at(t(10,0)),"2026-11-14-night");
        assert_eq!(shift_key_at(t(22,29)),"2026-11-14-night");
    }
    #[test] fn tracks_deltas_and_order_reset(){
        let path=std::env::temp_dir().join(format!("mics-shift-test-{}.json",std::process::id()));
        let _=fs::remove_file(&path);
        let mut t=ShiftDefects::default();
        let mut a=serde_json::json!({"machines":[{"machine_no":5,"order_no":"A","qty_ng":20}]});
        t.apply(&mut a,&path).unwrap();
        assert_eq!(a["machines"][0]["qty_ng_shift"],0);
        let mut b=serde_json::json!({"machines":[{"machine_no":5,"order_no":"A","qty_ng":24}]});
        t.apply(&mut b,&path).unwrap();
        assert_eq!(b["machines"][0]["qty_ng_shift"],4);
        let mut c=serde_json::json!({"machines":[{"machine_no":5,"order_no":"B","qty_ng":2}]});
        t.apply(&mut c,&path).unwrap();
        assert_eq!(c["machines"][0]["qty_ng_shift"],4);
        let mut d=serde_json::json!({"machines":[{"machine_no":5,"order_no":"B","qty_ng":5}]});
        t.apply(&mut d,&path).unwrap();
        assert_eq!(d["machines"][0]["qty_ng_shift"],7);
        assert_eq!(ShiftDefects::load(&path).machines.get(&5).unwrap().shift_total,7);
        let _=fs::remove_file(path);
    }
}
