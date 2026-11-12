#[cfg(windows)]
mod credentials;
use chrono::{DateTime, Utc};
use rsfbclient::prelude::*;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, env, fs, path::PathBuf, thread, time::Duration};

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
    if !c.enabled {println!("MICS standby disabled in mics.ini");return Ok(())}
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
    loop {
        match acquire(&c, &coordinator_token) {
            Ok(Some(lease)) => {
                match read(&c, &db_password) {
                    Ok(mut data) => {
                        if data["machines"].as_array().is_none_or(|a| a.is_empty()) {
                            eprintln!("MICS_READ_EMPTY: refused to overwrite current data");
                        } else {
                            let hash = format!("{:x}", Sha256::digest(serde_json::to_vec(&data).unwrap_or_default()));
                            // Recheck lease to minimize reading during PRIMARY recovery.
                            match acquire(&c, &coordinator_token) {
                                Ok(Some(new_lease)) if new_lease.epoch == lease.epoch => {
                                    if hash != previous_hash {
                                        data["sampled_at"] = json!(Utc::now().to_rfc3339());
                                        match upload(&c, &sync_token, &new_lease, data) {
                                            Ok(()) => { previous_hash = hash; println!("MICS_SYNC_OK epoch={}", new_lease.epoch); },
                                            Err(e) => eprintln!("{e}"),
                                        }
                                    }
                                }
                                _ => eprintln!("LEASE_LOST: discard snapshot"),
                            }
                        }
                    }
                    Err(e) => eprintln!("MICS_READ_FAILED {e}"),
                }
            }
            Ok(None) => { previous_hash.clear(); println!("STANDBY: coordinator denied lease"); }
            Err(e) => { previous_hash.clear(); eprintln!("{e}"); }
        }
        thread::sleep(Duration::from_secs(c.interval));
    }
}
fn main() {
    let result = Config::load().and_then(|c| {
        if env::args().any(|arg| arg == "--check-config") {
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
