//! The Construction Set's Use Report over a load order: every record that names an id.
//!
//! Was `wraithguard/patch/uses.py`, which read every plugin whole into Python dicts and
//! walked them. Here each plugin is mapped, and parsed only when its bytes hold the id
//! at all (any case) - most plugins of a load order do not, and are passed over at the
//! speed of a byte scan. A plugin that does is read with the tes3 crate and each record
//! walked as its serde value (the tes3conv schema, so paths are the toolkit's dotted
//! paths). Plugins are scanned on threads, with Python released.
//!
//! Which version of a using record wins is told from every plugin's records keyed the
//! cheap way - their tag and NAME (INAM for a response) from the file's framing, no
//! parse - so a use a later plugin overrides is marked, as before. A cell is not
//! last-wins (each plugin's CELL record adds references to it): its uses always count.

use std::collections::{HashMap, HashSet};
use std::io;
use std::path::PathBuf;

use pyo3::prelude::*;
use serde_json::Value;
use tes3::esp::TypeInfo;

use crate::lint::{is_py_word, records, subrecords, zstr};

/// One record that uses the id: `(plugin index, type, tag, key, paths, count, wins)`.
pub type Use = (usize, String, String, String, Vec<String>, usize, bool);

/// Whether `hay` holds `needle` (already lower case), ASCII case aside.
fn contains_ci(hay: &[u8], needle: &[u8]) -> bool {
    let Some((&first, rest)) = needle.split_first() else { return true };
    let n = needle.len();
    if hay.len() < n {
        return false;
    }
    (0..=hay.len() - n).any(|i| {
        hay[i].to_ascii_lowercase() == first && hay[i + 1..i + n].iter().zip(rest).all(|(a, b)| a.to_ascii_lowercase() == *b)
    })
}

/// How many times `word` (lower case) stands as a word in `text` (Python `re`'s `\w`
/// boundaries), case aside: a script naming the id.
fn word_count(text: &str, word: &str) -> usize {
    let hay = text.to_lowercase();
    let (h, w) = (hay.as_bytes(), word.as_bytes());
    if w.is_empty() || h.len() < w.len() {
        return 0;
    }
    let mut n = 0;
    let mut i = 0;
    while i + w.len() <= h.len() {
        if &h[i..i + w.len()] == w {
            let before = i == 0 || !is_py_word(h[i - 1]);
            let after = i + w.len() == h.len() || !is_py_word(h[i + w.len()]);
            if before && after {
                n += 1;
                i += w.len();
                continue;
            }
        }
        i += 1;
    }
    n
}

/// A record's key as `wraithguard.patch.records.record_key` gives it: its id; an
/// interior cell's name; an exterior cell's or a landscape's grid as `(x, y)`.
fn key_of(v: &Value) -> String {
    if let Some(id) = v.get("id").and_then(Value::as_str).filter(|s| !s.is_empty()) {
        return id.to_string();
    }
    let interior = v
        .get("data")
        .and_then(|d| d.get("flags"))
        .and_then(Value::as_str)
        .map(|f| f.contains("IS_INTERIOR"))
        .unwrap_or(false);
    if v.get("type").and_then(Value::as_str) == Some("Cell") && interior {
        return v.get("name").and_then(Value::as_str).unwrap_or("").to_string();
    }
    let grid = v.get("grid").or_else(|| v.get("data").and_then(|d| d.get("grid")));
    if let Some(g) = grid.and_then(Value::as_array).filter(|g| g.len() == 2) {
        let n = |x: &Value| x.as_f64().unwrap_or(0.0) as i64;
        return format!("({}, {})", n(&g[0]), n(&g[1]));
    }
    String::new()
}

/// The dotted paths in a value at which a string equal to `want` (lower case) sits.
fn walk(v: &Value, want: &str, path: &mut String, out: &mut Vec<String>) {
    match v {
        Value::String(s) => {
            if s.len() == want.len() && s.to_lowercase() == want {
                out.push(path.clone());
            }
        }
        Value::Object(m) => {
            for (k, x) in m {
                let len = path.len();
                if !path.is_empty() {
                    path.push('.');
                }
                path.push_str(k);
                walk(x, want, path, out);
                path.truncate(len);
            }
        }
        Value::Array(a) => {
            for (i, x) in a.iter().enumerate() {
                let len = path.len();
                path.push('.');
                path.push_str(&i.to_string());
                walk(x, want, path, out);
                path.truncate(len);
            }
        }
        _ => {}
    }
}

/// Every record of a plugin keyed the cheap way: `(tag, NAME or INAM, lower case)`.
fn keys(data: &[u8]) -> HashSet<([u8; 4], String)> {
    let mut out = HashSet::new();
    for (tag, body) in records(data) {
        let Ok(tag) = <[u8; 4]>::try_from(tag) else { continue };
        let want: &[u8] = if &tag == b"INFO" { b"INAM" } else { b"NAME" };
        if let Some((_, d)) = subrecords(body).find(|(t, _)| *t == want) {
            out.insert((tag, zstr(d).to_lowercase()));
        }
    }
    out
}

/// One plugin's uses of the id (without `wins`), and its keys.
fn scan(data: &[u8], plugin: usize, record_type: &str, want: &str) -> io::Result<(Vec<Use>, HashSet<([u8; 4], String)>)> {
    let index = keys(data);
    let mut uses = Vec::new();
    if !contains_ci(data, want.as_bytes()) {
        return Ok((uses, index));
    }
    let parsed = crate::esp::read(data)?;
    for obj in &parsed.objects {
        let tag = *obj.tag();
        if &tag == b"TES3" {
            continue;
        }
        let Ok(v) = serde_json::to_value(obj) else { continue };
        let kind = v.get("type").and_then(Value::as_str).unwrap_or("").to_string();
        let key = key_of(&v);
        if kind == record_type && key.to_lowercase() == want {
            continue;
        }
        let mut found = Vec::new();
        walk(&v, want, &mut String::new(), &mut found);
        let mut paths: Vec<String> = Vec::new();
        let mut count = 0;
        for p in found {
            if p.starts_with("references.") {
                count += 1;
                if !paths.iter().any(|x| x == "references") {
                    paths.push("references".into());
                }
            } else if p != "id" {
                count += 1;
                paths.push(p);
            }
        }
        if kind == "Script"
            && let Some(text) = v.get("text").and_then(Value::as_str)
        {
            let hits = word_count(text, want);
            if hits > 0 {
                count += hits;
                paths.push("text".into());
            }
        }
        if count > 0 {
            let tag_s = String::from_utf8_lossy(&tag).to_string();
            uses.push((plugin, kind, tag_s, key, paths, count, true));
        }
    }
    Ok((uses, index))
}

/// The Use Report: every record of the load order that names `id` (other than the
/// record itself, `record_type`), in load order - `(plugin index, type, tag, key, paths,
/// count, wins)`. A plugin that cannot be read is passed over.
pub fn report(paths: &[PathBuf], record_type: &str, id: &str) -> Vec<Use> {
    let want = id.trim().to_lowercase();
    let n = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(1, 16).min(paths.len().max(1));
    let mut results: Vec<Option<(Vec<Use>, HashSet<([u8; 4], String)>)>> = (0..paths.len()).map(|_| None).collect();
    std::thread::scope(|s| {
        let handles: Vec<_> = (0..n)
            .map(|t| {
                let want = &want;
                s.spawn(move || {
                    let mut got = Vec::new();
                    for (i, p) in paths.iter().enumerate().skip(t).step_by(n) {
                        let one = std::fs::File::open(p)
                            // SAFETY: the plugin is only read, and only while mapped here.
                            .and_then(|f| unsafe { memmap2::Mmap::map(&f) })
                            .and_then(|m| scan(&m, i, record_type, want));
                        if let Ok(r) = one {
                            got.push((i, r));
                        }
                    }
                    got
                })
            })
            .collect();
        for h in handles {
            if let Ok(got) = h.join() {
                for (i, r) in got {
                    results[i] = Some(r);
                }
            }
        }
    });
    let mut last: HashMap<([u8; 4], String), usize> = HashMap::new();
    for (i, r) in results.iter().enumerate() {
        if let Some((_, index)) = r {
            for k in index {
                last.insert(k.clone(), i);
            }
        }
    }
    let mut out = Vec::new();
    for r in results.into_iter().flatten() {
        for mut u in r.0 {
            let tag: [u8; 4] = u.2.as_bytes().try_into().unwrap_or(*b"    ");
            u.6 = u.1 == "Cell" || last.get(&(tag, u.3.to_lowercase())).is_none_or(|&at| at == u.0);
            out.push(u);
        }
    }
    out
}

/// `use_report(paths, record_type, id)`: the Use Report (see `report`), Python released.
#[pyfunction]
fn use_report(py: Python<'_>, paths: Vec<PathBuf>, record_type: String, id: String) -> Vec<Use> {
    py.detach(|| report(&paths, &record_type, &id))
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(use_report, m)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_and_bytes() {
        assert!(contains_ci(b"xx Gold_001 yy", b"gold_001"));
        assert!(!contains_ci(b"gold_00", b"gold_001"));
        assert_eq!(word_count("player->AddItem gold_001 5\nset x to gold_0012", "gold_001"), 1);
        assert_eq!(word_count("\"Gold_001\"", "gold_001"), 1);
    }

    #[test]
    fn keys_like_record_key() {
        let v: Value = serde_json::json!({"type": "Cell", "name": "Vault", "data": {"flags": "IS_INTERIOR", "grid": [0, 0]}});
        assert_eq!(key_of(&v), "Vault");
        let v: Value = serde_json::json!({"type": "Cell", "name": "", "data": {"flags": "", "grid": [-2, 9]}});
        assert_eq!(key_of(&v), "(-2, 9)");
        assert_eq!(key_of(&serde_json::json!({"type": "Static", "id": "rock"})), "rock");
    }
}
