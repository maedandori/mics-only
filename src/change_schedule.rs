//! Offline 7-day (Monday-Sunday JST 07:30 workday) mold, color, stamp & drying change schedule.
use chrono::{DateTime, Days, Duration, FixedOffset, NaiveDate, NaiveDateTime, Timelike, Utc, Datelike};
use serde_json::{json, Value};
use std::{collections::{HashMap, HashSet}, fs, path::{Path, PathBuf}};

pub const MACHINES: [i64; 14] = [5, 6, 7, 8, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19];
const DEFAULT_ALIASES_JSON: &str = include_str!("../local-vue/original/data/change-aliases.json");
const DOW_JP: [&str; 7] = ["月", "火", "水", "木", "金", "土", "日"];

#[derive(Clone, Debug)]
pub struct CompiledRule {
    alias_utf16_len: usize,
    lower_alias: String,
    ascii_word_only: bool,
    canonical: String,
    idx: usize,
}

#[derive(Clone, Debug, Default)]
pub struct AliasConfig {
    color: Vec<CompiledRule>,
    mold: Vec<CompiledRule>,
    stamp: Vec<CompiledRule>,
    drying_raw: Value,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShiftInfo {
    pub work_date: NaiveDate,
    pub work_date_key: String,
    pub shift: u8,
    pub next_day_morning: bool,
    pub time_str: String,
    pub time_label: String,
}

pub fn local_path_for_data_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("change-schedule-week.json")
}

pub fn empty_weekly_payload_bytes() -> Vec<u8> {
    let now_utc = Utc::now();
    let jst = FixedOffset::east_opt(9 * 3600).unwrap();
    let now_jst = now_utc.with_timezone(&jst).naive_local();
    let (week_start, week_end, days) = jst_week_range(now_jst);
    serde_json::to_vec(&json!({
        "ok": true,
        "source": "mics-only-offline",
        "week_start": week_start,
        "week_end": week_end,
        "days": days,
        "updated_at": null,
        "machines": []
    }))
    .unwrap_or_else(|_| b"{\"ok\":true,\"source\":\"mics-only-offline\",\"machines\":[]}".to_vec())
}

fn compile_rules(list: Option<&Vec<Value>>) -> Vec<CompiledRule> {
    let mut out = Vec::new();
    let Some(items) = list else { return out };
    for (idx, rule) in items.iter().enumerate() {
        let Some(canonical) = rule.get("canonical").and_then(Value::as_str).map(str::trim) else {
            continue;
        };
        if canonical.is_empty() {
            continue;
        }
        if let Some(aliases) = rule.get("aliases").and_then(Value::as_array) {
            for a_val in aliases {
                let Some(alias) = a_val.as_str() else { continue };
                if alias.is_empty() {
                    continue;
                }
                let ascii_word_only = alias.bytes().all(|b| b.is_ascii_alphabetic());
                out.push(CompiledRule {
                    alias_utf16_len: alias.encode_utf16().count(),
                    lower_alias: alias.to_lowercase(),
                    ascii_word_only,
                    canonical: canonical.to_string(),
                    idx,
                });
            }
        }
    }
    out.sort_by(|x, y| y.alias_utf16_len.cmp(&x.alias_utf16_len).then(x.idx.cmp(&y.idx)));
    out
}

pub fn parse_alias_config(raw: &Value) -> AliasConfig {
    let drying_raw = raw.get("drying").cloned().unwrap_or_else(|| json!([]));
    AliasConfig {
        color: compile_rules(raw.get("color").and_then(Value::as_array)),
        mold: compile_rules(raw.get("mold").and_then(Value::as_array)),
        stamp: compile_rules(raw.get("stamp").and_then(Value::as_array)),
        drying_raw,
    }
}

pub fn load_alias_config(data_dir: &Path) -> AliasConfig {
    let alias_path = data_dir.join("change-aliases.json");
    if let Ok(bytes) = fs::read(&alias_path) {
        if let Ok(val) = serde_json::from_slice::<Value>(&bytes) {
            if val.get("color").and_then(Value::as_array).is_some() {
                return parse_alias_config(&val);
            }
        }
    }
    let fallback: Value = serde_json::from_str(DEFAULT_ALIASES_JSON).unwrap_or_else(|_| json!({}));
    parse_alias_config(&fallback)
}

fn contains_ascii_word(haystack_lower: &str, needle_lower: &str) -> bool {
    if needle_lower.is_empty() {
        return false;
    }
    let hb = haystack_lower.as_bytes();
    let nb = needle_lower.as_bytes();
    if nb.len() > hb.len() {
        return false;
    }
    let mut start = 0usize;
    while let Some(pos) = haystack_lower[start..].find(needle_lower) {
        let idx = start + pos;
        let end = idx + nb.len();
        let left_ok = idx == 0 || !hb[idx - 1].is_ascii_alphanumeric();
        let right_ok = end == hb.len() || !hb[end].is_ascii_alphanumeric();
        if left_ok && right_ok {
            return true;
        }
        start = idx + 1;
        while start < hb.len() && !haystack_lower.is_char_boundary(start) {
            start += 1;
        }
    }
    false
}

pub fn match_alias_rule(text: &str, rules: &[CompiledRule]) -> String {
    let s = text.trim();
    if s.is_empty() || rules.is_empty() {
        return String::new();
    }
    let lower = s.to_lowercase();
    for rule in rules {
        let hit = if rule.ascii_word_only {
            contains_ascii_word(&lower, &rule.lower_alias)
        } else {
            lower.contains(&rule.lower_alias)
        };
        if hit {
            return rule.canonical.clone();
        }
    }
    String::new()
}

pub fn extract_color(row: &Value, cfg: &AliasConfig) -> String {
    if let Some(col) = row.get("color").and_then(Value::as_str) {
        let t = col.trim();
        if !t.is_empty() {
            return t.to_string();
        }
    }
    let prod = row.get("product_name").and_then(Value::as_str).unwrap_or("");
    match_alias_rule(prod, &cfg.color)
}

pub fn extract_stamp(row: &Value, cfg: &AliasConfig) -> String {
    let prod = row.get("product_name").and_then(Value::as_str).unwrap_or("");
    match_alias_rule(prod, &cfg.stamp)
}

pub fn extract_mold_alias(row: &Value, cfg: &AliasConfig) -> String {
    let prod = row.get("product_name").and_then(Value::as_str).unwrap_or("");
    let mold_name = row.get("mold_name").and_then(Value::as_str).unwrap_or("");
    let joined = format!("{prod} {mold_name}");
    match_alias_rule(&joined, &cfg.mold)
}

fn nfkc_ascii_char(c: char) -> char {
    match c {
        '\u{FF01}'..='\u{FF5E}' => char::from_u32(c as u32 - 0xFEE0).unwrap_or(c),
        '\u{3000}' => ' ',
        '‐' | '‑' | '‒' | '–' | '—' | '−' => '-',
        _ => c,
    }
}

fn drying_key(v: &str) -> String {
    v.chars()
        .map(nfkc_ascii_char)
        .filter(|c| !c.is_whitespace())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

fn drying_root(v: &str) -> &str {
    if let Some(pos) = v.rfind('-') {
        let suffix = &v[pos + 1..];
        if !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_alphanumeric()) {
            return &v[..pos];
        }
    }
    v
}

pub fn extract_drying_mold(row: &Value, cfg: &AliasConfig) -> String {
    let Some(rules) = cfg.drying_raw.as_array() else {
        return String::new();
    };
    let code = drying_key(row.get("mold_no").and_then(Value::as_str).unwrap_or(""));
    let name = drying_key(row.get("mold_name").and_then(Value::as_str).unwrap_or(""));
    if code.is_empty() && name.is_empty() {
        return String::new();
    }
    for group in rules {
        let Some(canonical) = group.get("canonical").and_then(Value::as_str) else {
            continue;
        };
        let mut raw_list = vec![canonical];
        if let Some(aliases) = group.get("aliases").and_then(Value::as_array) {
            for a in aliases {
                if let Some(s) = a.as_str() {
                    raw_list.push(s);
                }
            }
        }
        for raw in raw_list {
            let alias = drying_key(raw);
            if alias.is_empty() {
                continue;
            }
            if !code.is_empty() && (code == alias || drying_root(&code) == drying_root(&alias)) {
                return canonical.to_string();
            }
            if !name.is_empty() && (name == alias || (alias.encode_utf16().count() >= 3 && name.contains(&alias))) {
                return canonical.to_string();
            }
        }
    }
    String::new()
}

pub fn detect_drying_resins(row: &Value) -> Vec<String> {
    let mut raw: Vec<&str> = Vec::new();
    if let Some(arr) = row.get("material_names").and_then(Value::as_array) {
        for v in arr {
            if let Some(s) = v.as_str().map(str::trim) {
                if !s.is_empty() {
                    raw.push(s);
                }
            }
        }
    }
    if let Some(arr) = row.get("material_details").and_then(Value::as_array) {
        for item in arr {
            if let Some(s) = item.get("name").and_then(Value::as_str).map(str::trim) {
                if !s.is_empty() {
                    raw.push(s);
                }
            }
        }
    }
    let mut out = Vec::new();
    for name in raw {
        let norm_upper: String = name.chars().map(nfkc_ascii_char).flat_map(|c| c.to_uppercase()).collect();
        let norm_lower = norm_upper.to_lowercase();
        let has_abs = contains_ascii_word(&norm_lower, "abs");
        let has_ps_th21 = contains_ascii_word(&norm_lower, "ps-th21");
        let has_pc = contains_ascii_word(&norm_lower, "pc");
        let has_nylon = norm_upper.contains("ナイロン") || contains_ascii_word(&norm_lower, "nylon");
        let has_as = contains_ascii_word(&norm_lower, "as");

        for (hit, label) in [
            (has_abs, "ABS"),
            (has_ps_th21, "PS-TH21"),
            (has_pc, "PC"),
            (has_nylon, "ナイロン"),
            (has_as, "AS"),
        ] {
            if hit && !out.iter().any(|x| x == label) {
                out.push(label.to_string());
            }
        }
    }
    out
}

pub fn detect_change_types(
    prev_mold: &str,
    prev_color: &str,
    next_row: &Value,
    prev_stamp: &str,
    prev_mold_alias: &str,
    cfg: &AliasConfig,
) -> Vec<String> {
    let mold_no = next_row.get("mold_no").and_then(Value::as_str).unwrap_or("");
    if mold_no.is_empty() {
        return Vec::new();
    }
    let mut types = Vec::new();
    let next_mold_alias = extract_mold_alias(next_row, cfg);
    let mold_changed = mold_no != prev_mold
        || (!prev_mold_alias.is_empty() && !next_mold_alias.is_empty() && next_mold_alias != prev_mold_alias);
    if mold_changed {
        types.push("型替え".to_string());
    }
    let next_color = extract_color(next_row, cfg);
    if !next_color.is_empty() && !prev_color.is_empty() && next_color != prev_color {
        types.push("色替え".to_string());
    }
    let next_stamp = extract_stamp(next_row, cfg);
    if mold_changed {
        if !next_stamp.is_empty() {
            types.push("刻印確認".to_string());
        }
    } else if (!next_stamp.is_empty() || !prev_stamp.is_empty()) && next_stamp != prev_stamp {
        types.push("刻印替え".to_string());
    }
    types
}

pub fn parse_jst_datetime(raw: &str) -> Option<NaiveDateTime> {
    let s = raw.trim();
    if s.is_empty() {
        return None;
    }
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        let jst = FixedOffset::east_opt(9 * 3600)?;
        return Some(dt.with_timezone(&jst).naive_local());
    }
    for fmt in [
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%d %H:%M",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%dT%H:%M",
        "%Y/%m/%d %H:%M:%S",
        "%Y/%m/%d %H:%M",
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S%.f",
    ] {
        if let Ok(dt) = NaiveDateTime::parse_from_str(s, fmt) {
            return Some(dt);
        }
    }
    if let Ok(d) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return d.and_hms_opt(0, 0, 0);
    }
    None
}

pub fn workday_and_shift(dt: NaiveDateTime) -> ShiftInfo {
    let mins = dt.hour() * 60 + dt.minute();
    let (work_date, shift, next_day_morning) = if mins < 450 {
        (dt.date() - Duration::days(1), 2u8, true)
    } else if mins < 1140 {
        (dt.date(), 1u8, false)
    } else {
        (dt.date(), 2u8, false)
    };
    let time_str = format!("{:02}:{:02}", dt.hour(), dt.minute());
    let time_label = if next_day_morning {
        format!("翌{time_str}")
    } else {
        time_str.clone()
    };
    ShiftInfo {
        work_date,
        work_date_key: work_date.format("%Y-%m-%d").to_string(),
        shift,
        next_day_morning,
        time_str,
        time_label,
    }
}

pub fn jst_week_range(now_jst: NaiveDateTime) -> (String, String, Vec<Value>) {
    let anchor = workday_and_shift(now_jst).work_date;
    let dow_from_mon = anchor.weekday().num_days_from_monday() as u64;
    let monday = anchor - Days::new(dow_from_mon);
    let sunday = monday + Duration::days(6);
    let mut days = Vec::with_capacity(7);
    for i in 0..7i64 {
        let d = monday + Duration::days(i);
        let idx = d.weekday().num_days_from_monday() as usize;
        days.push(json!({
            "date": d.format("%Y-%m-%d").to_string(),
            "label": format!("{}/{}({})", d.month(), d.day(), DOW_JP[idx]),
            "dow": DOW_JP[idx],
            "is_sat": idx == 5,
            "is_sun": idx == 6
        }));
    }
    (
        monday.format("%Y-%m-%d").to_string(),
        sunday.format("%Y-%m-%d").to_string(),
        days,
    )
}

fn seq_sort_key(row: &Value) -> (u8, f64, String) {
    if let Some(n) = row.get("sequence").and_then(Value::as_f64) {
        return (0, n, String::new());
    }
    if let Some(s) = row.get("sequence").and_then(Value::as_str) {
        if let Ok(n) = s.trim().parse::<f64>() {
            return (0, n, String::new());
        }
    }
    let start_at = row.get("start_at").and_then(Value::as_str).unwrap_or("").to_string();
    (1, 0.0, start_at)
}

pub fn build_weekly_change_schedule(
    schedule_doc: &Value,
    alias_cfg: &AliasConfig,
    now_utc: DateTime<Utc>,
) -> Value {
    let jst = FixedOffset::east_opt(9 * 3600).unwrap();
    let now_jst = now_utc.with_timezone(&jst).naive_local();
    let (week_start, week_end, days) = jst_week_range(now_jst);

    let mut by_machine: HashMap<i64, Vec<Value>> = HashMap::new();
    if let Some(rows) = schedule_doc.get("rows").and_then(Value::as_array) {
        for row in rows {
            let Some(mno) = row.get("machine_no").and_then(Value::as_i64) else {
                continue;
            };
            by_machine.entry(mno).or_default().push(row.clone());
        }
    }

    let mut machines_out = Vec::with_capacity(MACHINES.len());
    let mut total_events = 0usize;

    for mno in MACHINES {
        let mut rows = by_machine.remove(&mno).unwrap_or_default();
        rows.sort_by(|a, b| {
            let ka = seq_sort_key(a);
            let kb = seq_sort_key(b);
            ka.0.cmp(&kb.0)
                .then_with(|| ka.1.partial_cmp(&kb.1).unwrap_or(std::cmp::Ordering::Equal))
                .then_with(|| ka.2.cmp(&kb.2))
                .then_with(|| {
                    a.get("start_at")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .cmp(b.get("start_at").and_then(Value::as_str).unwrap_or(""))
                })
        });

        let mut prev_mold: Option<String> = None;
        let mut prev_mold_name = String::new();
        let mut prev_color = String::new();
        let mut prev_stamp = String::new();
        let mut prev_mold_alias = String::new();
        let mut events = Vec::new();

        for row in &rows {
            let mold_no = row.get("mold_no").and_then(Value::as_str).unwrap_or("");
            if mold_no.is_empty() {
                continue;
            }
            let cur_color = extract_color(row, alias_cfg);
            let cur_stamp = extract_stamp(row, alias_cfg);
            let cur_mold_alias = extract_mold_alias(row, alias_cfg);

            let types = if let Some(ref pm) = prev_mold {
                detect_change_types(pm, &prev_color, row, &prev_stamp, &prev_mold_alias, alias_cfg)
            } else {
                Vec::new()
            };

            let p_mold = prev_mold.clone().unwrap_or_default();
            let p_mold_name = prev_mold_name.clone();
            let p_color = prev_color.clone();
            let p_stamp = prev_stamp.clone();

            prev_mold = Some(mold_no.to_string());
            prev_mold_name = row.get("mold_name").and_then(Value::as_str).unwrap_or("").to_string();
            prev_color = cur_color.clone();
            prev_stamp = cur_stamp.clone();
            prev_mold_alias = cur_mold_alias;

            if types.is_empty() {
                continue;
            }

            let Some(start_raw) = row.get("start_at").and_then(Value::as_str) else {
                continue;
            };
            let Some(start_dt) = parse_jst_datetime(start_raw) else {
                continue;
            };
            let shift_info = workday_and_shift(start_dt);
            if shift_info.work_date_key < week_start || shift_info.work_date_key > week_end {
                continue;
            }

            let is_mold_or_color = types.iter().any(|t| t == "型替え" || t == "色替え");
            let drying_mold_label = extract_drying_mold(row, alias_cfg);
            let drying = is_mold_or_color && !drying_mold_label.is_empty();
            let drying_resins = detect_drying_resins(row);
            let mut display_types = types.clone();
            if drying && !display_types.iter().any(|t| t == "乾燥") {
                display_types.push("乾燥".to_string());
            }

            let mut mat_names: Vec<String> = Vec::new();
            let mut seen_mat: HashSet<String> = HashSet::new();
            if let Some(arr) = row.get("material_names").and_then(Value::as_array) {
                for m in arr {
                    if let Some(s) = m.as_str().map(str::trim) {
                        if !s.is_empty() && seen_mat.insert(s.to_string()) {
                            mat_names.push(s.to_string());
                        }
                    }
                }
            }
            if mat_names.is_empty() {
                if let Some(arr) = row.get("material_details").and_then(Value::as_array) {
                    for d in arr {
                        if let Some(s) = d.get("name").and_then(Value::as_str).map(str::trim) {
                            if !s.is_empty() && seen_mat.insert(s.to_string()) {
                                mat_names.push(s.to_string());
                            }
                        }
                    }
                }
            }

            events.push(json!({
                "machine_no": mno,
                "order_no": row.get("order_no").and_then(Value::as_str).unwrap_or(""),
                "sequence": row.get("sequence").cloned().unwrap_or(Value::Null),
                "start_at": start_raw,
                "end_at": row.get("end_at").cloned().unwrap_or(Value::Null),
                "work_date": shift_info.work_date_key,
                "shift": shift_info.shift,
                "next_day_morning": shift_info.next_day_morning,
                "time_label": shift_info.time_label,
                "types": types,
                "display_types": display_types,
                "prev_mold": p_mold,
                "prev_mold_name": p_mold_name,
                "mold_no": mold_no,
                "mold_name": row.get("mold_name").and_then(Value::as_str).unwrap_or(""),
                "prev_color": p_color,
                "cur_color": cur_color,
                "prev_stamp": p_stamp,
                "cur_stamp": cur_stamp,
                "product_name": row.get("product_name").and_then(Value::as_str).unwrap_or(""),
                "drying": drying,
                "drying_mold_label": drying_mold_label,
                "drying_resins": drying_resins,
                "material_names": mat_names
            }));
        }

        total_events += events.len();
        machines_out.push(json!({
            "machine_no": mno,
            "week_start": week_start,
            "week_end": week_end,
            "event_count": events.len(),
            "events": events
        }));
    }

    json!({
        "ok": true,
        "source": "mics-only-offline",
        "week_start": week_start,
        "week_end": week_end,
        "days": days,
        "event_count": total_events,
        "updated_at": now_utc.to_rfc3339(),
        "machines": machines_out
    })
}

fn payload_content_matches(existing: &Value, next: &Value) -> bool {
    existing.get("week_start") == next.get("week_start")
        && existing.get("week_end") == next.get("week_end")
        && existing.get("days") == next.get("days")
        && existing.get("machines") == next.get("machines")
}

/// Builds and atomically writes `change-schedule-week.json` in `data_dir` from `schedule-local.json` and `change-aliases.json`.
pub fn sync_for_data_dir(data_dir: &Path) -> Result<PathBuf, String> {
    let sched_path = data_dir.join("schedule-local.json");
    let sched_bytes = fs::read(&sched_path)
        .map_err(|e| format!("CHANGE_SCHEDULE_READ_FAILED {}: {e}", sched_path.display()))?;
    let sched_doc: Value = serde_json::from_slice(&sched_bytes)
        .map_err(|e| format!("CHANGE_SCHEDULE_PARSE_FAILED {}: {e}", sched_path.display()))?;
    let alias_cfg = load_alias_config(data_dir);
    let payload = build_weekly_change_schedule(&sched_doc, &alias_cfg, Utc::now());

    let out_path = local_path_for_data_dir(data_dir);
    if let Ok(prev_bytes) = fs::read(&out_path) {
        if let Ok(prev_val) = serde_json::from_slice::<Value>(&prev_bytes) {
            if payload_content_matches(&prev_val, &payload) {
                return Ok(out_path);
            }
        }
    }

    fs::create_dir_all(data_dir).map_err(|e| e.to_string())?;
    let bytes = serde_json::to_vec_pretty(&payload).map_err(|e| e.to_string())?;
    let tmp = out_path.with_extension("json.tmp");
    let bak = out_path.with_extension("json.bak");
    fs::write(&tmp, &bytes).map_err(|e| e.to_string())?;
    if out_path.exists() {
        let _ = fs::remove_file(&bak);
        fs::rename(&out_path, &bak).map_err(|e| e.to_string())?;
    }
    if let Err(e) = fs::rename(&tmp, &out_path) {
        if bak.exists() {
            let _ = fs::rename(&bak, &out_path);
        }
        return Err(e.to_string());
    }
    Ok(out_path)
}

pub fn sync_default() -> Result<PathBuf, String> {
    let sched_path = super::schedule::local_path();
    let dir = sched_path.parent().ok_or("Missing data directory")?;
    sync_for_data_dir(dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn sample_aliases() -> AliasConfig {
        let raw = json!({
            "color": [
                {"canonical": "ｲｵﾝW", "aliases": ["ｲｵﾝW"]},
                {"canonical": "W", "aliases": ["(W)"]},
                {"canonical": "ｸﾞﾚｰ", "aliases": ["ｸﾞﾚｰ"]},
                {"canonical": "ﾌﾞﾗｯｸ", "aliases": ["ﾌﾞﾗｯｸ", "Bk"]}
            ],
            "mold": [
                {"canonical": "ｲﾝﾅｰ小", "aliases": ["ｲﾝﾅｰ小"]},
                {"canonical": "ｲﾝﾅｰset", "aliases": ["ｲﾝﾅｰset"]}
            ],
            "stamp": [
                {"canonical": "刻印カインズ", "aliases": ["刻印カインズ", "刻印cainz"]},
                {"canonical": "刻印ライクイット", "aliases": ["刻印Likeit"]}
            ],
            "drying": [
                {"canonical": "M-100", "aliases": ["TESTMOLD"]}
            ]
        });
        parse_alias_config(&raw)
    }

    #[test]
    fn jst_workday_boundary_and_monday_sunday_week_range() {
        // Tuesday 2026-11-24 06:15 JST -> Monday 2026-11-23 shift 2 (next_day_morning=true)
        let dt = NaiveDate::from_ymd_opt(2026, 11, 24).unwrap().and_hms_opt(6, 15, 0).unwrap();
        let si = workday_and_shift(dt);
        assert_eq!(si.work_date_key, "2026-11-23");
        assert_eq!(si.shift, 2);
        assert!(si.next_day_morning);
        assert_eq!(si.time_label, "翌06:15");

        // Sunday night -> Sunday 2026-11-29 22:00 JST belongs to week 2026-11-23..2026-11-29
        let sun_dt = NaiveDate::from_ymd_opt(2026, 11, 29).unwrap().and_hms_opt(22, 0, 0).unwrap();
        let (ws, we, days) = jst_week_range(sun_dt);
        assert_eq!(ws, "2026-11-23");
        assert_eq!(we, "2026-11-29");
        assert_eq!(days.len(), 7);
        assert_eq!(days[0]["date"], "2026-11-23");
        assert_eq!(days[6]["date"], "2026-11-29");
        assert_eq!(days[6]["is_sun"], true);
    }

    #[test]
    fn ascii_word_boundary_prevents_false_substring_match() {
        let cfg = sample_aliases();
        assert_eq!(match_alias_rule("ABC_Bk_01", &cfg.color), "ﾌﾞﾗｯｸ");
        assert_eq!(match_alias_rule("BKA-100", &cfg.color), "");
    }

    #[test]
    fn build_weekly_schedule_captures_mon_to_sun_changes_and_syncs_disk() {
        let cfg = sample_aliases();
        // Now = Wed 2026-11-25 10:00 JST -> 01:00 UTC
        let now_utc = Utc.with_ymd_and_hms(2026, 11, 25, 1, 0, 0).unwrap();
        let sched = json!({
            "rows": [
                {
                    "machine_no": 12, "sequence": 1, "order_no": "O1",
                    "mold_no": "M-100", "mold_name": "TESTMOLD",
                    "product_name": "ケースｲｵﾝW", "color": "",
                    "start_at": "2026-11-22 10:00:00", "end_at": "2026-11-23 09:00:00",
                    "material_names": ["PP"]
                },
                {
                    "machine_no": 12, "sequence": 2, "order_no": "O2",
                    "mold_no": "M-100", "mold_name": "TESTMOLD",
                    "product_name": "ケースﾌﾞﾗｯｸ 刻印cainz", "color": "",
                    "start_at": "2026-11-25 14:00:00", "end_at": "2026-11-26 14:00:00",
                    "material_names": ["ＡＢＳ樹脂"]
                },
                {
                    "machine_no": 12, "sequence": 3, "order_no": "O3",
                    "mold_no": "M-200", "mold_name": "NEWMOLD",
                    "product_name": "蓋ｸﾞﾚｰ 刻印Likeit", "color": "",
                    "start_at": "2026-11-29 20:00:00", "end_at": "2026-11-30 06:00:00",
                    "material_names": ["ナイロン6"]
                },
                {
                    "machine_no": 12, "sequence": 4, "order_no": "O4",
                    "mold_no": "M-300", "mold_name": "NEXTWEEK",
                    "product_name": "蓋(W)", "color": "",
                    "start_at": "2026-11-30 09:00:00", "end_at": "2026-12-01 09:00:00"
                }
            ]
        });

        let built = build_weekly_change_schedule(&sched, &cfg, now_utc);
        assert_eq!(built["week_start"], "2026-11-23");
        assert_eq!(built["week_end"], "2026-11-29");
        assert_eq!(built["machines"].as_array().unwrap().len(), 14);
        let m12 = built["machines"].as_array().unwrap().iter().find(|m| m["machine_no"] == 12).unwrap();
        let events = m12["events"].as_array().unwrap();
        assert_eq!(events.len(), 2);
        // First event in week: same mold M-100, color change + stamp install + drying
        assert_eq!(events[0]["work_date"], "2026-11-25");
        assert_eq!(events[0]["shift"], 1);
        assert_eq!(events[0]["types"], json!(["色替え", "刻印替え"]));
        assert_eq!(events[0]["display_types"], json!(["色替え", "刻印替え", "乾燥"]));
        assert_eq!(events[0]["drying_resins"], json!(["ABS"]));
        // Second event: Sunday night 2026-11-29 20:00 must be included in Mon..Sun offline file!
        assert_eq!(events[1]["work_date"], "2026-11-29");
        assert_eq!(events[1]["shift"], 2);
        assert_eq!(events[1]["types"], json!(["型替え", "色替え", "刻印確認"]));
        assert_eq!(events[1]["drying_resins"], json!(["ナイロン"]));

        // Disk sync roundtrip
        let dir = std::env::temp_dir().join(format!("mics-chg-week-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("schedule-local.json"), serde_json::to_vec(&sched).unwrap()).unwrap();
        let written = sync_for_data_dir(&dir).unwrap();
        assert!(written.is_file());
        let saved: Value = serde_json::from_slice(&fs::read(&written).unwrap()).unwrap();
        assert_eq!(saved["source"], "mics-only-offline");
        assert_eq!(saved["machines"].as_array().unwrap().len(), 14);
        let _ = fs::remove_dir_all(&dir);
    }
}
