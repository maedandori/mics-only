#![cfg_attr(windows, windows_subsystem = "windows")]
#[cfg(windows)]
mod credentials;
#[cfg(windows)]
mod updater;
mod local_web;
mod schedule;
mod change_schedule;
mod materials;
mod mirror;
mod cloud_socket;
use chrono::{DateTime, Utc};
use rsfbclient::prelude::*;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, env, fs, net::{TcpStream,ToSocketAddrs}, thread, time::Duration};

// Local schedule refresh is independent of cloud writes and lease heartbeats.
const ACTIVE_SCHEDULE_SECONDS: u64 = 20;
const STANDBY_MIRROR_SECONDS: u64 = 100;
const PRESENCE_SECONDS: u64 = 60;

fn remaining_poll_seconds(last: Option<std::time::Instant>, interval: u64) -> u64 {
    last.map(|at| interval.saturating_sub(at.elapsed().as_secs()).max(1)).unwrap_or(1)
}

#[derive(Debug)]
struct Config {
    enabled: bool, agent_id: String, host: String, port: u16,
    database: String, user: String, coordinator_url: String, interval: u64, display_name: String,
}
impl Config {
    fn load() -> Result<Self, String> {
        let path = env::current_exe().map_err(|e| e.to_string())?
            .parent().ok_or("Missing executable directory")?.join("mics.ini");
        let raw = fs::read_to_string(path).map_err(|e| format!("Cannot read mics.ini: {e}"))?;
        let mut m = BTreeMap::new();
        let mut section = false;
        for line in raw.lines() {
            let line = line.trim();
            if line.starts_with('[') { section = line.eq_ignore_ascii_case("[mics]"); continue; }
            if !section || line.is_empty() || line.starts_with('#') || line.starts_with(';') {continue}
            if let Some((key, val)) = line.split_once('=') {
                m.insert(key.trim().to_lowercase(), val.trim().to_string());
            }
        }
        let field = |key: &str| m.get(key).cloned().unwrap_or_default();
        let config = Self {
            enabled: matches!(field("enabled").to_lowercase().as_str(), "1" | "true"),
            agent_id: field("agent_id"),
            host: field("host"),
            port: field("port").parse().map_err(|_| "Invalid Firebird port")?,
            database: field("database"),
            user: field("user"),
            coordinator_url: field("coordinator_url"),
            interval: field("interval_seconds").parse().map_err(|_| "Invalid interval")?,
            display_name: field("display_name"),
        };
        config.validate()?;
        Ok(config)
    }
    fn validate(&self) -> Result<(), String> {
        if !self.enabled {return Ok(())}
        if !(15..=3600).contains(&self.interval) || self.port == 0 {return Err("Invalid interval/port".into())}
        if self.agent_id.len() < 3 || self.agent_id.len() > 100
            || !self.agent_id.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b)) {
            return Err("agent_id must be 3..100 ASCII alphanumeric/-/_".into())
        }
        if [&self.host, &self.database, &self.user].iter().any(|s| s.is_empty()) {
            return Err("Missing Firebird connection setting".into())
        }
        let u = url::Url::parse(&self.coordinator_url).map_err(|e| e.to_string())?;
        if u.scheme() != "https" || u.host_str().is_none()
           || !u.username().is_empty() || u.password().is_some() || u.fragment().is_some()
           || u.query().is_some() {
            return Err("Coordinator URL must be HTTPS without credentials/query/fragment".into())
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Deserialize)]
struct Lease {
    granted: bool,
    #[serde(default)] epoch: u64,
    #[serde(default)] lease_token: String,
    #[serde(default)] expires_at: String,
    #[serde(default)] retry_after_ms: u64,
}
impl Lease {
    fn valid(&self, agent_id: &str) -> bool {
        if !self.granted || self.epoch == 0 || self.lease_token.len() < 16 || agent_id.is_empty() {return false}
        DateTime::parse_from_rfc3339(&self.expires_at)
            .map(|expiry| expiry.with_timezone(&Utc).signed_duration_since(Utc::now()).num_seconds() > 10)
            .unwrap_or(false)
    }
}
#[cfg(not(windows))]
fn secret(name: &str) -> Result<String, String> {
    let val = env::var(name).map_err(|_| format!("Missing environment credential {name}"))?;
    if val.is_empty() || val.contains(['\n', '\r']) {return Err(format!("Invalid {name}"))}
    Ok(val)
}
fn client() -> ureq::Agent {
    ureq::AgentBuilder::new().timeout(Duration::from_secs(15)).build()
}
fn presence(c:&Config,coordinator_token:&str,alias_ack:Option<i64>)->Result<Value,String>{
 if coordinator_token.is_empty()||c.coordinator_url.is_empty(){return Err("Presence credentials missing".into())}
 let url=url::Url::parse(&c.coordinator_url).map_err(|e|e.to_string())?;
 if url.scheme()!="https"||url.host_str().is_none(){return Err("Invalid coordinator URL".into())}
 let payload=json!({"agent_id":c.agent_id,"display_name":if c.display_name.is_empty(){&c.agent_id}else{&c.display_name},"role":"backup","standby_enabled":c.enabled,"version":env!("CARGO_PKG_VERSION"),"alias_sync_ack":alias_ack,
   "update_failed_version":env::current_exe().ok().and_then(|p|p.parent().map(|dir|dir.join("data").join("update-blocked-version.txt"))).and_then(|p|fs::read_to_string(p).ok()).map(|v|v.trim().trim_start_matches('v').to_owned())});
 if let Some(result)=cloud_socket::call("presence",&payload,None){return result}
 let result=client().post(&format!("{}/presence",c.coordinator_url.trim_end_matches('/')))
  .set("Authorization",&format!("Bearer {coordinator_token}"))
  .send_json(payload)
  .map_err(|e|match e {
      ureq::Error::Status(status, response) => {
          let detail=response.into_string().unwrap_or_default();
          format!("PRESENCE_HTTP_{status} {}",detail.chars().take(250).collect::<String>())
      },
      other => format!("PRESENCE_FAILED {other}")
  })?;
 if result.status()!=200{return Err(format!("PRESENCE_HTTP_{}",result.status()))}
 result.into_json().map_err(|e|format!("Bad presence reply: {e}"))
}
thread_local!{
    static LEASE_CACHE: std::cell::RefCell<(Option<Lease>,Option<std::time::Instant>,Option<std::time::Instant>)> =
        std::cell::RefCell::new((None,None,None));
}
// A cached lease remains a server grant, not a local election. Refresh only
// when due or when the remaining time is too short to begin another operation.
fn acquire(c: &Config, coordinator_token: &str) -> Result<Option<Lease>, String> {
    LEASE_CACHE.with(|cell|{
        let mut state=cell.borrow_mut();
        let now=std::time::Instant::now();
        if let (Some(lease),Some(at))=(&state.0,state.1){
            let remaining=DateTime::parse_from_rfc3339(&lease.expires_at).ok()
                .map(|expiry|expiry.with_timezone(&Utc).signed_duration_since(Utc::now()).num_seconds()).unwrap_or(0);
            if at.elapsed()<Duration::from_secs(30)&&remaining>15&&lease.valid(&c.agent_id){return Ok(Some(lease.clone()))}
        }
        if state.0.is_none()&&state.2.is_some_and(|at|at>now){return Ok(None)}
        // Never continue using a previous token after a failed renewal.
        *state=(None,None,None);
        let payload=json!({"agent_id":c.agent_id,"role":"backup","ttl_seconds":60});
        let lease:Lease=match cloud_socket::call("lease",&payload,None){
            Some(result)=>serde_json::from_value(result?).map_err(|_|"Invalid socket lease response".to_string())?,
            None=>{
                // The old HTTP server accepts only TTL=45; keep mixed versions safe.
                let response=client().post(&format!("{}/lease",c.coordinator_url.trim_end_matches('/')))
                    .set("Authorization",&format!("Bearer {coordinator_token}"))
                    .send_json(json!({"agent_id":c.agent_id,"role":"backup","ttl_seconds":45}))
                    .map_err(|e|format!("LEASE_REQUEST_FAILED {e}"))?;
                response.into_json().map_err(|_|"Invalid HTTP lease response".to_string())?
            }
        };
        if !lease.granted{
            let delay=if lease.retry_after_ms==0{20000}else{lease.retry_after_ms.clamp(5000,60000)};
            state.2=Some(std::time::Instant::now()+Duration::from_millis(delay));
            return Ok(None)
        }
        if !lease.valid(&c.agent_id){return Err("Invalid/expired coordinator lease; refusing Firebird access".into())}
        state.0=Some(lease.clone());state.1=Some(std::time::Instant::now());
        Ok(Some(lease))
    })
}
fn lease_wait_seconds()->u64{
    LEASE_CACHE.with(|cell|{
        let state=cell.borrow();
        if let Some(at)=state.1{return 30u64.saturating_sub(at.elapsed().as_secs()).max(1)}
        state.2.map(|at|at.saturating_duration_since(std::time::Instant::now()).as_secs().max(1)).unwrap_or(20)
    })
}
fn read(c: &Config, password: &str) -> Result<Value, String> {
    let mut b = rsfbclient::builder_pure_rust();
    b.host(&c.host).port(c.port).db_name(&c.database).user(&c.user).pass(password);
    b.with_transaction(|tr| tr.read_only().no_wait());
    let mut db = b.connect().map_err(|e| e.to_string())?;
    // Cache schema column presence once per process so 20s polls do not repeat 3 system-catalog queries.
    static SCHEMA_FLAGS: std::sync::OnceLock<(bool, bool, bool)> = std::sync::OnceLock::new();
    let (has_mend, has_pick, has_today) = match SCHEMA_FLAGS.get().copied() {
        Some(flags) => flags,
        None => {
            let mend_rows: Vec<(i64,)> = db.query(
                "SELECT COUNT(*) FROM RDB$RELATION_FIELDS WHERE RDB$RELATION_NAME='SCHEDULE' AND TRIM(RDB$FIELD_NAME)='MEND'", ()
            ).map_err(|e| e.to_string())?;
            let pick_rows: Vec<(i64,)> = db.query(
                "SELECT COUNT(*) FROM RDB$RELATION_FIELDS WHERE RDB$RELATION_NAME='MACHINEWORK' AND TRIM(RDB$FIELD_NAME)='REALPICKQTY'", ()
            ).map_err(|e| e.to_string())?;
            let today_rows: Vec<(i64,)> = db.query(
                "SELECT COUNT(*) FROM RDB$RELATION_FIELDS WHERE RDB$RELATION_NAME='MACHINEWORK' AND TRIM(RDB$FIELD_NAME)='TODAYQTYNG'", ()
            ).map_err(|e| e.to_string())?;
            let flags = (
                mend_rows.first().is_some_and(|r| r.0 > 0),
                pick_rows.first().is_some_and(|r| r.0 > 0),
                today_rows.first().is_some_and(|r| r.0 > 0),
            );
            let _ = SCHEMA_FLAGS.set(flags);
            flags
        }
    };
    let mend = if has_mend {
        "COALESCE(TRIM(CAST(S.MEND AS VARCHAR(40))), '')"
    } else { "CAST('' AS VARCHAR(40))" };
    let pick = if has_pick {
        "COALESCE(TRIM(CAST(W.REALPICKQTY AS VARCHAR(24))), '')"
    } else { "CAST('' AS VARCHAR(24))" };

    let sql = format!(
        "SELECT TRIM(CAST(W.MACHINENO AS VARCHAR(24))),
         COALESCE(TRIM(CAST(W.MORDERNO AS VARCHAR(40))), ''),
         COALESCE(TRIM(CAST(W.STATUS AS VARCHAR(16))), ''),
         COALESCE(TRIM(CAST(S.PARTSNAME AS VARCHAR(255))), ''),
         COALESCE(TRIM(CAST(T.TOOLNO AS VARCHAR(80))), ''),
         COALESCE(TRIM(CAST(T.TOOLNAME AS VARCHAR(255))), ''),
         COALESCE(TRIM(CAST(W.TOTALQTYOK AS VARCHAR(24))), ''),
         COALESCE(TRIM(CAST(W.TOTALQTYNG AS VARCHAR(24))), ''),
         COALESCE(TRIM(CAST(W.STAFFNAME AS VARCHAR(100))), ''),
         COALESCE(TRIM(CAST(W.QTYPLAN AS VARCHAR(24))), ''),
         COALESCE(TRIM(CAST(W.REALCYCLETIME AS VARCHAR(24))), ''),
         {mend}, {pick}
         FROM MACHINEWORK W
         LEFT JOIN SCHEDULE S ON S.MORDERNO=W.MORDERNO AND S.MACHINENO=W.MACHINENO AND COALESCE(S.DELETEFLG,0)=0
         LEFT JOIN TOOL T ON T.ENTRYNO=S.TOOLENTRYNO
         WHERE W.MACHINENO IN (5,6,7,8,10,11,12,13,14,15,16,17,18,19)
         ORDER BY W.MACHINENO"
    );
    type Row = (String,String,String,String,String,String,String,String,String,String,String,String,String);
    let rows: Vec<Row> = db.query(&sql, ()).map_err(|e| e.to_string())?;
    // A separate query avoids extending the 13-column Firebird row tuple.
    // MICS maintains TODAYQTYNG itself, so no local shift subtraction is needed.
    let mut today_by_machine: BTreeMap<i32,i64> = BTreeMap::new();
    if has_today {
        let today_values: Vec<(String,String)> = db.query(
            "SELECT TRIM(CAST(MACHINENO AS VARCHAR(24))), COALESCE(TRIM(CAST(TODAYQTYNG AS VARCHAR(24))), '') FROM MACHINEWORK WHERE MACHINENO IN (5,6,7,8,10,11,12,13,14,15,16,17,18,19)", ()
        ).map_err(|e|e.to_string())?;
        for (no,ng) in today_values {
            if let (Ok(n),Ok(count))=(no.parse::<i32>(),ng.parse::<i64>()) {
                if count>=0 {today_by_machine.entry(n).and_modify(|v|*v=(*v).max(count)).or_insert(count);}
            }
        }
    }
    let qty = |s: &str| s.trim().parse::<i64>().unwrap_or(0).max(0);
    let mut result: BTreeMap<i32, Value> = BTreeMap::new();
    for (machine, order, status, product, tool_no, mold, good, bad, staff, plan, cycle, scheduled_end, pick) in rows {
        let Ok(no) = machine.parse::<i32>() else {continue};
        if let Some(existing) = result.get_mut(&no) {
            if existing["order_no"] != order || existing["mold_name"] != mold {
                existing["status_code"] = json!("AMBIGUOUS");
                existing["mold_name"] = json!("");
                existing["staff_name"] = json!("");
            }
            continue;
        }
        let p = qty(&plan); let ok = qty(&good);
        result.insert(no, json!({
            "machine_no": no, "order_no":order, "status_code":status,
            "product_name":product, "mold_no":tool_no, "mold_name":mold,
            "qty_plan":p, "qty_ok":ok, "qty_ng":qty(&bad),
             "qty_ng_shift":today_by_machine.get(&no).copied(),
             "qty_ng_shift_status":if today_by_machine.contains_key(&no){"mics_today"}else{"unavailable"},
            "qty_remaining":(p-ok).max(0),
            "cycle_seconds":cycle.parse::<f64>().ok(),
            "pick_qty":pick.trim().parse::<f64>().ok().filter(|n| n.is_finite() && *n > 0.0),
            "staff_name":staff.trim(),
            "scheduled_end_at":if scheduled_end.trim().is_empty(){None}else{Some(scheduled_end.trim())}
        }));
    }
    Ok(json!({"source":"mics-data","machines":result.into_values().collect::<Vec<_>>() }))
}
#[derive(Clone, Debug)]
struct CloudMachineBaseline {
    fingerprint: String,
    status_code: String,
    qty_plan: i64,
    qty_ok: i64,
    qty_ng: i64,
    qty_remaining: i64,
    cycle_seconds: Option<f64>,
    pick_qty: Option<f64>,
    sampled_at_ms: i64,
}
impl CloudMachineBaseline {
    fn from_machine(m: &Value, sampled_at_ms: i64) -> Self {
        Self {
            fingerprint: critical_machine_fingerprint(m),
            status_code: m["status_code"].as_str().unwrap_or_default().to_string(),
            qty_plan: m["qty_plan"].as_i64().unwrap_or(0).max(0),
            qty_ok: m["qty_ok"].as_i64().unwrap_or(0).max(0),
            qty_ng: m["qty_ng"].as_i64().unwrap_or(0).max(0),
            qty_remaining: m["qty_remaining"].as_i64().unwrap_or(0).max(0),
            cycle_seconds: m["cycle_seconds"].as_f64().filter(|c| c.is_finite() && *c > 0.0),
            pick_qty: m["pick_qty"].as_f64().filter(|p| p.is_finite() && *p > 0.0),
            sampled_at_ms,
        }
    }
    fn predict_qty_ok(&self, now_ms: i64) -> i64 {
        if !matches!(self.status_code.as_str(), "200" | "210") {
            return self.qty_ok;
        }
        let (Some(cycle), Some(pick)) = (self.cycle_seconds, self.pick_qty) else {
            return self.qty_ok;
        };
        let elapsed_ms = now_ms - self.sampled_at_ms;
        if elapsed_ms <= 0 || elapsed_ms > 86_400_000 {
            return self.qty_ok;
        }
        let extra_shots = ((elapsed_ms as f64) / (cycle * 1000.0)).floor();
        if extra_shots <= 0.0 {
            return self.qty_ok;
        }
        let added = (extra_shots * pick).round() as i64;
        if self.qty_plan > 0 {
            (self.qty_ok + added).min(self.qty_plan).max(self.qty_ok)
        } else {
            self.qty_ok + added.max(0)
        }
    }
    fn needs_sync(&self, m: &Value, now_ms: i64) -> bool {
        if critical_machine_fingerprint(m) != self.fingerprint {
            return true;
        }
        let real_ok = m["qty_ok"].as_i64().unwrap_or(0).max(0);
        let real_ng = m["qty_ng"].as_i64().unwrap_or(0).max(0);
        let real_rem = m["qty_remaining"].as_i64().unwrap_or(0).max(0);
        if real_ng != self.qty_ng {
            return true;
        }
        if real_rem == 0 && self.qty_remaining > 0 {
            return true;
        }
        let predicted_ok = self.predict_qty_ok(now_ms);
        let status = m["status_code"].as_str().unwrap_or_default();
        let tolerance = if matches!(status, "200" | "210") {
            let pick = m["pick_qty"].as_f64().unwrap_or(1.0).ceil() as i64;
            pick.max(1) * 2
        } else {
            1
        };
        (real_ok - predicted_ok).abs() >= tolerance
    }
}
fn critical_machine_fingerprint(m: &Value) -> String {
    let has_cycle = m["cycle_seconds"].as_f64().is_some_and(|c| c > 0.0);
    serde_json::to_string(&json!([
        m["machine_no"],
        m["status_code"],
        m["order_no"],
        m["product_name"],
        m["mold_no"],
        m["mold_name"],
        m["staff_name"],
        m["qty_plan"],
        m["pick_qty"],
        m["scheduled_end_at"],
        has_cycle,
    ])).unwrap_or_default()
}
fn changed_cloud_machines(machines: &[Value], previous: &BTreeMap<i32, CloudMachineBaseline>, now_ms: i64) -> Vec<(i32, CloudMachineBaseline, Value)> {
    let mut out = Vec::new();
    for m in machines {
        let Some(no) = m["machine_no"].as_i64().map(|n| n as i32) else { continue };
        let should_sync = match previous.get(&no) {
            Some(base) => base.needs_sync(m, now_ms),
            None => true,
        };
        if should_sync {
            out.push((no, CloudMachineBaseline::from_machine(m, now_ms), m.clone()));
        }
    }
    out
}
fn upload(c: &Config, sync_token: &str, lease: &Lease, snapshot: Value) -> Result<(), String> {
    if !lease.valid(&c.agent_id){return Err("Lease expired before upload".into())}
    if let Some(result)=cloud_socket::call("sync",&snapshot,Some(lease)){
        return if result?["ok"]==true{Ok(())}else{Err("Socket sync not acknowledged".into())}
    }
    let response = client().post(&format!("{}/sync", c.coordinator_url.trim_end_matches('/')))
        .set("Authorization", &format!("Bearer {sync_token}"))
        .set("X-Mics-Agent-Id", &c.agent_id)
        .set("X-Mics-Lease-Token", &lease.lease_token)
        .set("X-Mics-Lease-Epoch", &lease.epoch.to_string())
        .send_json(snapshot).map_err(|e| format!("SYNC_FAILED {e}"))?;
    let body: Value = response.into_json().map_err(|e| format!("Bad sync JSON: {e}"))?;
    if body["ok"] != true {return Err("Sync not acknowledged by coordinator".into())}
    Ok(())
}
fn remote_alias_sync_request(response:&Value,local:&local_web::LocalState){
 let Some(id)=response["alias_sync_command_id"].as_i64().filter(|id|*id>0) else{return};
 if local.alias_sync_ack.lock().ok().and_then(|ack|*ack)==Some(id){return}
 match local_web::sync_aliases_now(local){
  Ok(())=>{
   if let Ok(mut ack)=local.alias_sync_ack.lock(){*ack=Some(id);}
   println!("ALIAS_D1_SYNC_OK command_id={id}");
  },
  Err(e)=>eprintln!("ALIAS_D1_SYNC_FAILED command_id={id} {e}")
 }
}
#[cfg(windows)]
fn remote_update_request(response:&Value,local:&local_web::LocalState){
    use std::sync::atomic::{AtomicBool,Ordering};
    static CHECKING: AtomicBool=AtomicBool::new(false);
    if response["update_requested"]!=true{return}
    let mode=local.mode.lock().ok().map(|s|s.clone()).unwrap_or_default();
    if mode!="MIRROR_ONLY"&&mode!="STANDBY"{return}
    if CHECKING.swap(true,Ordering::SeqCst){return}
    let state=local.clone();
    thread::spawn(move ||{
        if let Ok(exe)=env::current_exe(){
            let safe=state.mode.lock().ok().is_some_and(|mode|mode.as_str()=="MIRROR_ONLY"||mode.as_str()=="STANDBY");
            if safe {
                if let Err(e)=updater::install_if_available(&exe){
                    eprintln!("ADMIN_UPDATE_FAILED {e}");
                }
            }
        }
        CHECKING.store(false,Ordering::SeqCst);
    });
}
fn run(c: Config) -> Result<(), String> {
    // Always serve the local dashboard, even when standby is disabled or lease denied.
    let local=local_web::LocalState::new(local_web::default_snapshot_path());
    let active_poll_seconds=c.interval.min(ACTIVE_SCHEDULE_SECONDS);
    local.set_poll_interval(active_poll_seconds);
    // Alias settings use shared D1; local machine/schedule API remains entirely local.
    // The coordinator secret is read from DPAPI and never exposed to browser JS.
    #[cfg(windows)]
    let alias_token=credentials::load().ok().map(|v|v.coordinator_token).unwrap_or_default();
    #[cfg(not(windows))]
    let alias_token=env::var("MICS_COORDINATOR_TOKEN").unwrap_or_default();
    local.set_alias_sync(&c.coordinator_url,&alias_token);
    if schedule::local_path().is_file(){
        if let Err(e)=change_schedule::sync_default(){
            eprintln!("LOCAL_CHANGE_SCHEDULE_INIT_FAILED {e}");
        }
    }
    local_web::start(local.clone())?;
    // Fetch alias from D1 in the agent, not from the browser. Works in standby,
    // active and mirror-only modes and never blocks the web UI.
    local_web::start_alias_sync(local.clone());
    // Check GitHub every six hours; postpone while ACTIVE (Firebird lease holder).
    // Update applies only to this standby agent; the independent print agent is untouched.
    #[cfg(windows)]
    {
        let state=local.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_secs(300));
            loop {
                let safe=state.mode.lock().map(|m|m.as_str()!="ACTIVE").unwrap_or(false);
                if safe {
                    if let Ok(exe)=env::current_exe(){
                        match updater::install_if_available(&exe){
                            Ok(result) if result.starts_with("UPDATE_STAGED ")=>{
                                println!("{result}");
                            },
                            Ok(_)=>{},
                            Err(e)=>eprintln!("AUTO_UPDATE_CHECK_FAILED {e}")
                        }
                    }
                }
                thread::sleep(Duration::from_secs(21600));
            }
        });
    }
    if !c.enabled {
        // Safe MIRROR_ONLY mode: local dashboard can show the primary's
        // authenticated Cloudflare cache while NEVER opening Firebird or
        // asking to become ACTIVE. The standby toggle remains OFF.
        local.set_mode("MIRROR_ONLY");
        local.record_sync("MIRROR_ONLY");
        #[cfg(windows)]
        let mirror_secret=credentials::load().ok().map(|v|v.mics_sync_token).unwrap_or_default();
        #[cfg(not(windows))]
        let mirror_secret=env::var("MICS_SYNC_TOKEN").unwrap_or_default();
        #[cfg(windows)]
        let presence_secret=credentials::load().ok().map(|v|v.coordinator_token).unwrap_or_default();
        #[cfg(not(windows))]
        let presence_secret=env::var("MICS_COORDINATOR_TOKEN").unwrap_or_default();

        if !mirror_secret.is_empty()&&!presence_secret.is_empty(){cloud_socket::configure(&c,&presence_secret,&mirror_secret);}
        let valid_url=url::Url::parse(&c.coordinator_url).ok()
            .is_some_and(|u|u.scheme()=="https"&&u.host_str().is_some()
                &&u.username()==""&&u.password().is_none());
        let mut last_mirror:Option<std::time::Instant>=None;
        let mut last_presence:Option<std::time::Instant>=None;
        local.set_poll_interval(STANDBY_MIRROR_SECONDS);
        loop {
            if valid_url&&!presence_secret.is_empty()&&last_presence.map(|t|t.elapsed()>=Duration::from_secs(PRESENCE_SECONDS)).unwrap_or(true){
                last_presence=Some(std::time::Instant::now());
                if let Ok(response)=presence(&c,&presence_secret,local.alias_sync_ack.lock().ok().and_then(|ack|*ack)){
                    remote_alias_sync_request(&response,&local);
                    #[cfg(windows)]
                    remote_update_request(&response,&local);
                }
            }
            if last_mirror.map(|t|t.elapsed()>=Duration::from_secs(STANDBY_MIRROR_SECONDS)).unwrap_or(true){
                last_mirror=Some(std::time::Instant::now());
                local.schedule_next_poll(None);
                if valid_url&&!mirror_secret.is_empty(){
                    if let Err(e)=mirror::refresh(&c,&mirror_secret,&local){
                        eprintln!("MIRROR_ONLY_CACHE_ERROR {e}");
                    }
                }
            }
            let mirror_wait=remaining_poll_seconds(last_mirror,STANDBY_MIRROR_SECONDS);
            local.schedule_next_poll(if valid_url&&!mirror_secret.is_empty(){Some(mirror_wait)}else{None});
            let presence_wait=if valid_url&&!presence_secret.is_empty(){remaining_poll_seconds(last_presence,PRESENCE_SECONDS)}else{STANDBY_MIRROR_SECONDS};
            thread::sleep(Duration::from_secs(mirror_wait.min(presence_wait)))
        }
    }
    // Read secrets once; never print secrets or raw employee/snapshot data.
    // On Windows, use a per-user DPAPI vault distinct from mics-print.
    // Never send secrets via command-line arguments or store them in mics.ini.
    #[cfg(windows)]
    let (db_password, sync_token, coordinator_token) = {
        let v = credentials::load()?;
        (v.db_password, v.mics_sync_token, v.coordinator_token)
    };
    #[cfg(not(windows))]
    let (db_password, sync_token, coordinator_token) = (
        secret("MICS_DB_PASSWORD")?,
        secret("MICS_SYNC_TOKEN")?,
        secret("MICS_COORDINATOR_TOKEN")?,
    );
    cloud_socket::configure(&c,&coordinator_token,&sync_token);
    let mut previous_hash = String::new();
    let mut previous_cloud_state: BTreeMap<i32, CloudMachineBaseline> = BTreeMap::new();
    let mut last_mirror:Option<std::time::Instant>=None;
    let mut schedule_polled_at:Option<std::time::Instant>=None;
    let mut materials_polled_at:Option<std::time::Instant>=None;
    let mut materials_sync_failed=false;
    let mut last_presence:Option<std::time::Instant>=None;
    let mut last_data_poll:Option<std::time::Instant>=None;
    loop {
        local.schedule_next_poll(None); // Current agent cycle is in progress.
        if last_presence.map(|t|t.elapsed()>=Duration::from_secs(60)).unwrap_or(true){
            if let Ok(response)=presence(&c,&coordinator_token,local.alias_sync_ack.lock().ok().and_then(|ack|*ack)){
                remote_alias_sync_request(&response,&local);
                #[cfg(windows)]
                remote_update_request(&response,&local);
            }
            last_presence=Some(std::time::Instant::now());
        }
        let mut next_wait=active_poll_seconds;
        let mut next_data_wait=active_poll_seconds;
        match acquire(&c, &coordinator_token) {
            Ok(Some(lease)) => {
                local.set_mode("ACTIVE");
                local.set_poll_interval(active_poll_seconds);
                last_mirror=None;
                // A lease-only wake-up must not skip a due production OR schedule poll.
                if last_data_poll.is_some_and(|at|at.elapsed()<Duration::from_secs(active_poll_seconds))
                    && schedule_polled_at.is_some_and(|at|at.elapsed()<Duration::from_secs(ACTIVE_SCHEDULE_SECONDS)){
                    let data_wait=remaining_poll_seconds(last_data_poll,active_poll_seconds)
                        .min(remaining_poll_seconds(schedule_polled_at,ACTIVE_SCHEDULE_SECONDS));
                    local.schedule_next_poll(Some(data_wait));
                    thread::sleep(Duration::from_secs(data_wait.min(lease_wait_seconds())));
                    continue
                }
                last_data_poll=Some(std::time::Instant::now());
                let material_refresh_seconds=if materials_sync_failed {300}else{86400};
                if materials_polled_at.map(|t|t.elapsed()>=Duration::from_secs(material_refresh_seconds)).unwrap_or(true){
                    materials_polled_at=Some(std::time::Instant::now());
                    match materials::sync(&c,&db_password){
                        Ok(n)=>{
                            materials_sync_failed=false;
                            println!("MATERIAL_LOCAL_ONLY_OK mapped_products={n}; no Turso upload");
                        },
                        Err(e)=>{
                            materials_sync_failed=true;
                            eprintln!("MATERIAL_LOCAL_SYNC_FAILED {e}");
                        }
                    }
                }
                if schedule_polled_at.map(|t|t.elapsed()>=Duration::from_secs(ACTIVE_SCHEDULE_SECONDS)).unwrap_or(true){
                    // A valid coordinator lease is required before opening Firebird
                    // even for the local-only schedule cache.
                    if let Ok(Some(grant))=acquire(&c,&coordinator_token){
                        if grant.epoch==lease.epoch{
                            match schedule::collect(&c,&db_password){
                                Ok(cache)=>{
                                    schedule_polled_at=Some(std::time::Instant::now());
                                    println!("LOCAL_SCHEDULE_CACHE_OK");
                                    // Fenced upload; retain cache and retry on later reads.
                                    match acquire(&c,&coordinator_token) {
                                        Ok(Some(upload_lease)) if upload_lease.epoch==lease.epoch=>{
                                            match schedule::upload_if_changed(&c,&sync_token,&upload_lease,&cache){
                                                Ok(true)=>println!("MICS_SCHEDULE_SYNC_OK"),
                                                Ok(false)=>(),
                                                Err(e)=>eprintln!("MICS_SCHEDULE_SYNC_FAILED {e}")
                                            }
                                        },
                                        _=>eprintln!("MICS_SCHEDULE_LEASE_LOST")
                                    }
                                },
                                Err(e)=>{schedule_polled_at=Some(std::time::Instant::now());eprintln!("LOCAL_SCHEDULE_FAILED {e}");}
                            }
                        }
                    }
                }
                // Refresh a near-expiry grant before opening another Firebird connection.
                let read_grant=acquire(&c,&coordinator_token);
                if !matches!(read_grant,Ok(Some(ref grant)) if grant.epoch==lease.epoch){
                    local.set_mode("STANDBY");local.record_sync("LEASE_LOST");
                    thread::sleep(Duration::from_secs(lease_wait_seconds()));continue
                }
                match read(&c, &db_password) {
                    Ok(mut data) => {
                        if data["machines"].as_array().is_none_or(|a| a.is_empty()) {
                            local.record_sync("READ_EMPTY");
                            eprintln!("MICS_READ_EMPTY: refused to overwrite current data");
                        } else {
                            let hash = format!("{:x}", Sha256::digest(serde_json::to_vec(&data).unwrap_or_default()));
                            let now = Utc::now();
                            let now_rfc3339 = now.to_rfc3339();
                            let now_ms = now.timestamp_millis();
                            if hash != previous_hash {
                                data["sampled_at"] = json!(now_rfc3339.clone());
                                if let Err(e)=local.update(data.clone()){eprintln!("LOCAL_SNAPSHOT_FAILED {e}")}
                                previous_hash = hash;
                            }
                            let machines_slice = data["machines"].as_array().cloned().unwrap_or_default();
                            let changed = changed_cloud_machines(&machines_slice, &previous_cloud_state, now_ms);
                            if !changed.is_empty() {
                                // Only upload to Cloud/Turso when status/mold/order changes or real qty drifts from Web auto-increment.
                                match acquire(&c, &coordinator_token) {
                                    Ok(Some(new_lease)) if new_lease.epoch == lease.epoch => {
                                        let cloud_payload = json!({
                                            "source": "mics-data",
                                            "sampled_at": now_rfc3339,
                                            "machines": changed.iter().map(|(_, _, m)| m.clone()).collect::<Vec<_>>()
                                        });
                                        match upload(&c, &sync_token, &new_lease, cloud_payload) {
                                            Ok(()) => {
                                                for (no, base, _) in changed {
                                                    previous_cloud_state.insert(no, base);
                                                }
                                                local.record_sync("SYNCED");
                                                println!("MICS_SYNC_OK epoch={} synced_machines={}", new_lease.epoch, previous_cloud_state.len());
                                            },
                                            Err(e) => {local.record_sync("RETRY_PENDING");eprintln!("{e}");},
                                        }
                                    }
                                    _ => {local.record_sync("LEASE_LOST");eprintln!("LEASE_LOST: discard snapshot");},
                                }
                            }
                        }
                    }
                    Err(e) => {local.record_sync("READ_FAILED");eprintln!("MICS_READ_FAILED {e}");},
                }
                // UI reports the next local data update, not a lease renewal.
                next_data_wait=remaining_poll_seconds(last_data_poll,active_poll_seconds)
                    .min(remaining_poll_seconds(schedule_polled_at,ACTIVE_SCHEDULE_SECONDS));
                next_wait=next_data_wait.min(lease_wait_seconds());
            }
            Ok(None) => {
                local.set_mode("STANDBY");
                local.set_poll_interval(STANDBY_MIRROR_SECONDS);
                previous_hash.clear();
                previous_cloud_state.clear();
                schedule_polled_at=None;last_data_poll=None;
                if last_mirror.map(|t|t.elapsed()>=Duration::from_secs(STANDBY_MIRROR_SECONDS)).unwrap_or(true){
                    last_mirror=Some(std::time::Instant::now());
                    if let Err(e)=mirror::refresh(&c,&sync_token,&local){
                        eprintln!("STANDBY_LOCAL_MIRROR_FAILED {e}");
                    }
                }
                next_data_wait=remaining_poll_seconds(last_mirror,STANDBY_MIRROR_SECONDS);
                // Keep the lease election cadence independent of mirror downloads.
                next_wait=lease_wait_seconds().min(next_data_wait);
            }
            Err(e) => { local.set_mode("OFFLINE"); previous_hash.clear(); previous_cloud_state.clear(); schedule_polled_at=None; last_data_poll=None; last_mirror=None; eprintln!("{e}"); }
        }
        local.schedule_next_poll(Some(next_data_wait));
        thread::sleep(Duration::from_secs(next_wait));
    }
}
fn test_connection(c:&Config)->Result<(),String>{
    // Safe connectivity test: TCP reachability only, never authenticates to
    // Firebird and never runs SQL or acquires a failover lease.
    if c.host.is_empty()||c.port==0{return Err("Firebird host/port not configured".into())}
    let addr=format!("{}:{}",c.host,c.port);
    let addresses=addr.to_socket_addrs().map_err(|e|format!("Firebird DNS error: {e}"))?;
    let mut ok=false;
    for address in addresses {
        if TcpStream::connect_timeout(&address,Duration::from_secs(3)).is_ok(){ok=true;break}
    }
    if !ok{return Err("Firebird TCP connectivity FAILED (no database login attempted)".into())}
    println!("FIREBIRD_TCP_OK (no SQL executed)");
    if c.coordinator_url.is_empty(){
        return Err("Coordinator URL missing: cannot register this PC in D1".into())
    }
    let health=format!("{}/health",c.coordinator_url.trim_end_matches('/'));
    let response=client().get(&health).call().map_err(|e|format!("COORDINATOR_HTTP_FAILED {e}"))?;
    let result:Value=response.into_json().map_err(|e|format!("COORDINATOR_BAD_RESPONSE {e}"))?;
    if result["service"]!="mics-failover"||result["ok"]!=true{return Err("Coordinator health response is invalid".into())}
    println!("COORDINATOR_HTTP_OK; enabled={}",result["enabled"]);
    // Identify this exact installation in Cloudflare D1 via the authenticated
    // Coordinator /presence endpoint. No Firebird SQL or lease is requested.
    if c.agent_id.len()<3 || c.agent_id.len()>100
        || !c.agent_id.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b)) {
        return Err("Invalid Agent ID: set a unique ID on each PC".into())
    }
    #[cfg(windows)]
    let coordinator_token=credentials::load()
        .map_err(|e|format!("D1 registration requires saved DPAPI Coordinator Token: {e}"))?
        .coordinator_token;
    #[cfg(not(windows))]
    let coordinator_token=secret("MICS_COORDINATOR_TOKEN")?;
    if coordinator_token.is_empty(){return Err("Coordinator Token is missing; D1 registration not attempted".into())}
    presence(c,&coordinator_token,None).map_err(|e|format!("D1_PRESENCE_FAILED: {e}"))?;
    println!("D1_PRESENCE_OK agent_id={} display_name={}",
        c.agent_id,if c.display_name.is_empty(){&c.agent_id}else{&c.display_name});
    println!("NOTE: Each PC MUST have its own unique agent_id in mics.ini.");
    Ok(())
}

fn sql_read_only(sql: &str) -> Result<String,String> {
    let sql=sql.trim().trim_end_matches(';').trim();
    if sql.is_empty() || sql.contains(';') {return Err("Only one SELECT statement is allowed".into())}
    let upper=sql.to_ascii_uppercase();
    if !(upper.starts_with("SELECT ") || upper.starts_with("SELECT\n") || upper.starts_with("WITH ")) {
        return Err("Read-only SELECT/WITH queries only".into())
    }
    for word in [" INSERT "," UPDATE "," DELETE "," DROP "," ALTER "," CREATE "," EXECUTE "," MERGE "," GRANT "," REVOKE "," INTO "," FOR UPDATE"," UNION "] {
        if format!(" {upper} ").contains(word) {return Err("Unsafe SQL keyword rejected".into())}
    }
    if sql.contains("--") || sql.contains("/*") {return Err("SQL comments are not supported".into())}
    Ok(sql.to_string())
}
fn inspect_database(c:&Config, password:&str, sql:Option<&str>) -> Result<String,String> {
    let mut b=rsfbclient::builder_pure_rust();
    b.host(&c.host).port(c.port).db_name(&c.database).user(&c.user).pass(password);
    b.with_transaction(|tr| tr.read_only().no_wait());
    let mut db=b.connect().map_err(|e|e.to_string())?;
    let export_dir=env::current_exe().map_err(|e|e.to_string())?
        .parent().ok_or("Missing EXE directory")?.join("database-export");
    fs::create_dir_all(&export_dir).map_err(|e|e.to_string())?;
    let mut report=String::new();
    let tables:Vec<String>=if let Some(statement)=sql {
        // Wrap user SELECT in a row limit; only return text representation.
        let checked=sql_read_only(statement)?;
        let limited=format!("SELECT FIRST 10 * FROM ({checked}) Q");
        let cols:Vec<(String,)> = db.query(
            "SELECT TRIM(RF.RDB$FIELD_NAME) FROM RDB$RELATION_FIELDS RF WHERE RF.RDB$RELATION_NAME = 'SCHEDULE' ORDER BY RF.RDB$FIELD_POSITION", ()
        ).unwrap_or_default();
        let _=cols;
        // Arbitrary projections are not safely decodable without runtime column typing.
        // Use Firebird's SQL engine to validate the statement and report query errors.
        let rows:Vec<(String,)> = db.query(&format!("SELECT FIRST 10 CAST(Q.ENTRYNO AS VARCHAR(255)) FROM ({checked}) Q"), ())
            .map_err(|e|format!("Query needs an ENTRYNO column for preview: {e}"))?;
        report=rows.into_iter().map(|r|r.0).collect::<Vec<_>>().join("\n");
        let _=limited;
        Vec::new()
    } else {
        let rows:Vec<(String,)>=db.query("SELECT TRIM(RDB$RELATION_NAME) FROM RDB$RELATIONS WHERE COALESCE(RDB$SYSTEM_FLAG,0)=0 AND RDB$VIEW_BLR IS NULL ORDER BY RDB$RELATION_NAME",())
            .map_err(|e|e.to_string())?;
        rows.into_iter().map(|r|r.0).collect()
    };
    let mut exported=0;
    let format_value = |val: rsfbclient::SqlType| -> String {
        match val {
            rsfbclient::SqlType::Text(t) => t,
            rsfbclient::SqlType::Integer(i) => i.to_string(),
            rsfbclient::SqlType::Int128(i) => i.to_string(),
            rsfbclient::SqlType::Floating(f) => f.to_string(),
            rsfbclient::SqlType::Timestamp(ts) => ts.to_string(),
            rsfbclient::SqlType::Boolean(b) => b.to_string(),
            rsfbclient::SqlType::Binary(bytes) => String::from_utf8(bytes)
                .unwrap_or_else(|e| format!("<BLOB: {} bytes>", e.into_bytes().len())),
            rsfbclient::SqlType::Null => String::new(),
        }
    };
    let csv_escape=|s:&str|format!("\"{}\"",s.replace('"',"\"\""));
    for table in tables {
        if !table.bytes().all(|x|x.is_ascii_alphanumeric()||x==b'_') {continue}
        // Query all columns together in one statement so rows stay aligned and
        // non-string/BLOB/long columns are decoded without VARCHAR(255) truncation.
        let query=format!("SELECT FIRST 10 * FROM \"{table}\"");
        let rows:Vec<rsfbclient::Row>=match db.query(&query,()) {
            Ok(r)=>r,
            Err(e)=>{report.push_str(&format!("{table}：取得できませんでした。\n"));continue}
        };
        if rows.is_empty(){continue}
        let names:Vec<String>=rows[0].cols.iter().map(|c|c.name.to_string()).collect();
        // Prepend UTF-8 BOM so Excel on Windows opens Japanese/Vietnamese CSVs with correct encoding.
        let mut out=String::from("\u{FEFF}");
        out.push_str(&names.iter().map(|s|csv_escape(s)).collect::<Vec<_>>().join(","));
        out.push('\n');
        for row in rows {
            let line=row.cols.into_iter().map(|c|csv_escape(&format_value(c.value))).collect::<Vec<_>>().join(",");
            out.push_str(&line);
            out.push('\n');
        }
        fs::write(export_dir.join(format!("{table}.csv")),out).map_err(|e|e.to_string())?;
        exported+=1;
    }
    if sql.is_none(){report.push_str(&format!("{exported} 件のテーブルを保存しました。保存先：{}",export_dir.display()));}
    Ok(report)
}

fn main() {
    let result = Config::load().and_then(|c| {
        if env::args().any(|arg|arg=="--sql-query") {
            let file=env::args().skip_while(|x|x!="--sql-query").nth(1).ok_or("SQL file missing")?;
            let sql=fs::read_to_string(file).map_err(|e|e.to_string())?;
            #[cfg(windows)] let password=credentials::load().map_err(|e|e.to_string())?.db_password;
            #[cfg(not(windows))] let password=secret("MICS_DB_PASSWORD")?;
            println!("{}",inspect_database(&c,&password,Some(&sql))?);Ok(())
        } else if env::args().any(|arg|arg=="--dump-tables") {
            #[cfg(windows)] let password=credentials::load().map_err(|e|e.to_string())?.db_password;
            #[cfg(not(windows))] let password=secret("MICS_DB_PASSWORD")?;
            println!("{}",inspect_database(&c,&password,None)?);Ok(())
        } else if env::args().any(|arg| arg == "--test-connection") {
            test_connection(&c)
        } else if env::args().any(|arg| arg == "--check-config") {
            println!("Configuration syntax OK; credentials/network not checked");
            Ok(())
        } else {run(c)}
    });
    if let Err(e) = result {eprintln!("{e}");std::process::exit(1)}
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn denied_lease_is_not_valid() {
        let l = Lease {granted:false, epoch:1, lease_token:"a".repeat(32), expires_at:"2099-01-01T00:00:00Z".into(), retry_after_ms:0};
        assert!(!l.valid("backup"));
    }
    #[test] fn empty_lease_is_not_valid() {
        let l = Lease {granted:true, epoch:1, lease_token:"".into(), expires_at:"2099-01-01T00:00:00Z".into(), retry_after_ms:0};
        assert!(!l.valid("backup"));
    }
    #[test] fn critical_change_detection_filters_shot_counter_noise() {
        let m1 = json!({
            "machine_no": 8, "order_no": "ORD-1", "status_code": "200",
            "product_name": "PROD-A", "mold_no": "M-01", "mold_name": "MOLD-A",
            "qty_plan": 1000, "qty_ok": 120, "qty_ng": 1, "qty_remaining": 880,
            "cycle_seconds": 20.0, "pick_qty": 4.0, "staff_name": "SATO", "scheduled_end_at": null
        });
        let t0 = 1_700_000_000_000i64;
        let mut state = BTreeMap::new();
        let initial = changed_cloud_machines(std::slice::from_ref(&m1), &state, t0);
        assert_eq!(initial.len(), 1);
        for (no, base, _) in initial { state.insert(no, base); }

        // After 20s (1 cycle of 20s * 4 pick_qty = +4 pieces), real qty_ok=124 matches Web prediction => 0 cloud writes.
        let mut m1_on_pace = m1.clone();
        m1_on_pace["qty_ok"] = json!(124);
        m1_on_pace["qty_remaining"] = json!(876);
        m1_on_pace["cycle_seconds"] = json!(20.1);
        assert!(changed_cloud_machines(&[m1_on_pace], &state, t0 + 20_000).is_empty());

        // If real qty_ok drifts from Web prediction by >= 2 shots (>= 8 pieces) at t0 + 60s, sync real qty immediately!
        let mut m1_drifted = m1.clone();
        m1_drifted["qty_ok"] = json!(120); // Machine stalled without changing status yet (Web predicted 132)
        m1_drifted["qty_remaining"] = json!(880);
        assert_eq!(changed_cloud_machines(&[m1_drifted], &state, t0 + 60_000).len(), 1);

        // Status, mold, order/product, or NG change MUST trigger an immediate cloud write.
        let mut m1_status_change = m1.clone();
        m1_status_change["status_code"] = json!("100");
        assert_eq!(changed_cloud_machines(&[m1_status_change], &state, t0 + 5_000).len(), 1);

        let mut m1_mold_change = m1.clone();
        m1_mold_change["mold_no"] = json!("M-02");
        assert_eq!(changed_cloud_machines(&[m1_mold_change], &state, t0 + 5_000).len(), 1);

        let mut m1_order_change = m1.clone();
        m1_order_change["order_no"] = json!("ORD-2");
        assert_eq!(changed_cloud_machines(&[m1_order_change], &state, t0 + 5_000).len(), 1);
    }
}

#[cfg(test)]
mod polling_interval_tests {
    use super::*;
    use std::time::Instant;
    #[test]
    fn active_and_standby_refresh_targets() {
        assert_eq!(ACTIVE_SCHEDULE_SECONDS,20);
        assert_eq!(STANDBY_MIRROR_SECONDS,100);
        assert_eq!(remaining_poll_seconds(None,20),1);
        assert_eq!(remaining_poll_seconds(Some(Instant::now()-Duration::from_secs(10)),20),10);
        assert_eq!(remaining_poll_seconds(Some(Instant::now()-Duration::from_secs(80)),100),20);
        assert_eq!(remaining_poll_seconds(Some(Instant::now()-Duration::from_secs(101)),100),1);
    }
}
