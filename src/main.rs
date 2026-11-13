#![cfg_attr(windows, windows_subsystem = "windows")]
#[cfg(windows)]
mod credentials;
mod local_web;
mod schedule;
mod mirror;
use chrono::{DateTime, Utc};
use rsfbclient::prelude::*;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, env, fs, net::{TcpStream,ToSocketAddrs}, thread, time::Duration};

#[derive(Debug)]
struct Config {
    enabled: bool, agent_id: String, host: String, port: u16,
    database: String, user: String, coordinator_url: String, interval: u64,
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
#[derive(Debug, Deserialize)]
struct Lease {
    granted: bool,
    #[serde(default)] epoch: u64,
    #[serde(default)] lease_token: String,
    #[serde(default)] expires_at: String,
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
fn acquire(c: &Config, coordinator_token: &str) -> Result<Option<Lease>, String> {
    let response = client().post(&format!("{}/lease", c.coordinator_url.trim_end_matches('/')))
        .set("Authorization", &format!("Bearer {coordinator_token}"))
        .send_json(json!({"agent_id":c.agent_id,"role":"backup","ttl_seconds":45}))
        .map_err(|e| format!("LEASE_REQUEST_FAILED {e}"))?;
    let lease: Lease = response.into_json().map_err(|e| format!("Invalid lease response: {e}"))?;
    if !lease.granted {return Ok(None)}
    if !lease.valid(&c.agent_id) {return Err("Invalid/expired coordinator lease; refusing Firebird access".into())}
    Ok(Some(lease))
}
fn read(c: &Config, password: &str) -> Result<Value, String> {
    let mut b = rsfbclient::builder_pure_rust();
    b.host(&c.host).port(c.port).db_name(&c.database).user(&c.user).pass(password);
    b.with_transaction(|tr| tr.read_only().no_wait());
    let mut db = b.connect().map_err(|e| e.to_string())?;
    // Match the mics-print mapping and preserve compatibility with older MICS schemas.
    let mend_rows: Vec<(i64,)> = db.query(
        "SELECT COUNT(*) FROM RDB$RELATION_FIELDS WHERE RDB$RELATION_NAME='SCHEDULE' AND TRIM(RDB$FIELD_NAME)='MEND'", ()
    ).map_err(|e| e.to_string())?;
    let mend = if mend_rows.first().is_some_and(|r| r.0 > 0) {
        "COALESCE(TRIM(CAST(S.MEND AS VARCHAR(40))), '')"
    } else { "CAST('' AS VARCHAR(40))" };
    let pick_rows: Vec<(i64,)> = db.query(
        "SELECT COUNT(*) FROM RDB$RELATION_FIELDS WHERE RDB$RELATION_NAME='MACHINEWORK' AND TRIM(RDB$FIELD_NAME)='REALPICKQTY'", ()
    ).map_err(|e| e.to_string())?;
    let pick = if pick_rows.first().is_some_and(|r| r.0 > 0) {
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
            "qty_remaining":(p-ok).max(0),
            "cycle_seconds":cycle.parse::<f64>().ok(),
            "pick_qty":pick.trim().parse::<f64>().ok().filter(|n| n.is_finite() && *n > 0.0),
            "staff_name":staff.trim(),
            "scheduled_end_at":if scheduled_end.trim().is_empty(){None}else{Some(scheduled_end.trim())}
        }));
    }
    Ok(json!({"source":"mics-data","machines":result.into_values().collect::<Vec<_>>() }))
}
fn upload(c: &Config, sync_token: &str, lease: &Lease, snapshot: Value) -> Result<(), String> {
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
fn run(c: Config) -> Result<(), String> {
    // Always serve the local dashboard, even when standby is disabled or lease denied.
    let local=local_web::LocalState::new(local_web::default_snapshot_path());
    local_web::start(local.clone())?;
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
        let valid_url=url::Url::parse(&c.coordinator_url).ok()
            .is_some_and(|u|u.scheme()=="https"&&u.host_str().is_some()
                &&u.username()==""&&u.password().is_none());
        loop {
            if valid_url&&!mirror_secret.is_empty(){
                if let Err(e)=mirror::refresh(&c,&mirror_secret,&local){
                    eprintln!("MIRROR_ONLY_CACHE_ERROR {e}");
                }
            }
            thread::sleep(Duration::from_secs(300))
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
    let mut previous_hash = String::new();
    let mut last_mirror:Option<std::time::Instant>=None;
    let mut schedule_polled_at:Option<std::time::Instant>=None;
    loop {
        match acquire(&c, &coordinator_token) {
            Ok(Some(lease)) => {
                local.set_mode("ACTIVE");
                if schedule_polled_at.map(|t|t.elapsed()>=Duration::from_secs(60)).unwrap_or(true){
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
                match read(&c, &db_password) {
                    Ok(mut data) => {
                        if data["machines"].as_array().is_none_or(|a| a.is_empty()) {
                            local.record_sync("READ_EMPTY");
                            eprintln!("MICS_READ_EMPTY: refused to overwrite current data");
                        } else {
                            let hash = format!("{:x}", Sha256::digest(serde_json::to_vec(&data).unwrap_or_default()));
                            // Recheck lease to minimize reading during PRIMARY recovery.
                            match acquire(&c, &coordinator_token) {
                                Ok(Some(new_lease)) if new_lease.epoch == lease.epoch => {
                                    if hash != previous_hash {
                                        data["sampled_at"] = json!(Utc::now().to_rfc3339());
                                        if let Err(e)=local.update(data.clone()){eprintln!("LOCAL_SNAPSHOT_FAILED {e}")}
                                        match upload(&c, &sync_token, &new_lease, data) {
                                            Ok(()) => { previous_hash = hash; local.record_sync("SYNCED"); println!("MICS_SYNC_OK epoch={}", new_lease.epoch); },
                                            Err(e) => {local.record_sync("RETRY_PENDING");eprintln!("{e}");},
                                        }
                                    }
                                }
                                _ => {local.record_sync("LEASE_LOST");eprintln!("LEASE_LOST: discard snapshot");},
                            }
                        }
                    }
                    Err(e) => {local.record_sync("READ_FAILED");eprintln!("MICS_READ_FAILED {e}");},
                }
            }
            Ok(None) => {
                local.set_mode("STANDBY");
                previous_hash.clear();
                if last_mirror.map(|t|t.elapsed()>Duration::from_secs(300)).unwrap_or(true){
                    last_mirror=Some(std::time::Instant::now());
                    if let Err(e)=mirror::refresh(&c,&sync_token,&local){
                        eprintln!("STANDBY_LOCAL_MIRROR_FAILED {e}");
                    }
                }
            }
            Err(e) => { local.set_mode("OFFLINE"); previous_hash.clear(); eprintln!("{e}"); }
        }
        thread::sleep(Duration::from_secs(c.interval));
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
        println!("COORDINATOR_NOT_CONFIGURED");return Ok(())
    }
    let health=format!("{}/health",c.coordinator_url.trim_end_matches('/'));
    let response=client().get(&health).call().map_err(|e|format!("COORDINATOR_HTTP_FAILED {e}"))?;
    let result:Value=response.into_json().map_err(|e|format!("COORDINATOR_BAD_RESPONSE {e}"))?;
    if result["service"]!="mics-failover"||result["ok"]!=true{return Err("Coordinator health response is invalid".into())}
    println!("COORDINATOR_HTTP_OK; enabled={}",result["enabled"]);
    Ok(())
}
fn main() {
    let result = Config::load().and_then(|c| {
        if env::args().any(|arg| arg == "--test-connection") {
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
        let l = Lease {granted:false, epoch:1, lease_token:"a".repeat(32), expires_at:"2099-01-01T00:00:00Z".into()};
        assert!(!l.valid("backup"));
    }
    #[test] fn empty_lease_is_not_valid() {
        let l = Lease {granted:true, epoch:1, lease_token:"".into(), expires_at:"2099-01-01T00:00:00Z".into()};
        assert!(!l.valid("backup"));
    }
}
