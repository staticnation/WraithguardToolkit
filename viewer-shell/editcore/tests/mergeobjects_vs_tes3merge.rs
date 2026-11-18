//! The merge against TES3Merge's own output on the same load order (port check, Static
//! 2026-11-18): run TES3Merge on plugins, then
//!
//!   TES3MERGE_REF="Merged Objects.esp" TES3MERGE_PLUGINS="a.esm|b.esp|..." \
//!       cargo test --release --test mergeobjects_vs_tes3merge -- --ignored --nocapture
//!
//! Each record either side has is compared as values: the ones only one side has, and for the
//! rest the first field they differ in. `TES3MERGE_OURS` writes our plugin there too.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde_json::Value;

fn diff_path(a: &Value, b: &Value, at: &str) -> Option<String> {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            for k in x.keys().chain(y.keys()) {
                let (u, v) = (x.get(k).unwrap_or(&Value::Null), y.get(k).unwrap_or(&Value::Null));
                if let Some(p) = diff_path(u, v, &format!("{at}.{k}")) {
                    return Some(p);
                }
            }
            None
        }
        _ if a == b => None,
        _ => Some(format!("{at}: TES3Merge {} | ours {}", short(a), short(b))),
    }
}

fn short(v: &Value) -> String {
    let s = v.to_string();
    if s.len() > 160 { format!("{}...", &s[..160]) } else { s }
}

fn keyed(bytes: &[u8]) -> BTreeMap<String, Value> {
    editcore::plugin::objects(bytes, None, true)
        .unwrap()
        .iter()
        .map(|o| serde_json::to_value(o).unwrap())
        .filter(|v| v["type"] != "Header")
        .map(|v| (format!("{} {}", v["type"].as_str().unwrap_or(""), editcore::mergeobjects::record_id(&v).unwrap_or_default().to_lowercase()), v))
        .collect()
}

#[test]
#[ignore = "needs TES3Merge's output and its plugins: TES3MERGE_REF, TES3MERGE_PLUGINS"]
fn the_same_records_as_tes3merge() {
    let reference = std::fs::read(std::env::var("TES3MERGE_REF").unwrap()).unwrap();
    let plugins: Vec<(String, PathBuf)> = std::env::var("TES3MERGE_PLUGINS")
        .unwrap()
        .split('|')
        .map(|p| (PathBuf::from(p).file_name().unwrap().to_string_lossy().into_owned(), PathBuf::from(p)))
        .collect();
    let opts = editcore::mergeobjects::Options { inclusive: std::env::var("INCLUSIVE").is_ok_and(|v| v == "1"), ..Default::default() };
    let t = std::time::Instant::now();
    let out = editcore::mergeobjects::merge(&plugins, &opts);
    println!("merged {} records in {:.2} s ({:?}); unreadable {:?}; failed {:?}", out.records.len(), t.elapsed().as_secs_f64(), out.by_type, out.unreadable, out.failed);
    let sizes = plugins.iter().map(|(n, p)| (n.clone(), std::fs::metadata(p).unwrap().len())).collect();
    let ours = editcore::mergeobjects::plugin_bytes(&out, &sizes, "check").unwrap();
    if let Ok(p) = std::env::var("TES3MERGE_OURS") {
        std::fs::write(p, &ours).unwrap();
    }
    let (r, o) = (keyed(&reference), keyed(&ours));
    let only_ref: Vec<&String> = r.keys().filter(|k| !o.contains_key(*k)).collect();
    let only_ours: Vec<&String> = o.keys().filter(|k| !r.contains_key(*k)).collect();
    let mut differ = Vec::new();
    for (k, v) in &r {
        if let Some(w) = o.get(k)
            && let Some(p) = diff_path(v, w, "")
        {
            differ.push(format!("{k}{p}"));
        }
    }
    println!("TES3Merge {} records, ours {}: {} only TES3Merge's, {} only ours, {} differ", r.len(), o.len(), only_ref.len(), only_ours.len(), differ.len());
    for k in only_ref.iter().take(30) {
        println!("  only TES3Merge's: {k}");
    }
    for k in only_ours.iter().take(30) {
        println!("  only ours: {k}");
    }
    for d in differ.iter().take(40) {
        println!("  differ: {d}");
    }
}
