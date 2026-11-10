//! A plugin read and written back with nothing changed: the ground the Editor's "edit a plugin
//! in place" stands on (`TODO.md`, Next, 1). Every record is read into its JSON form
//! (`plugin::records_json`, as a build carries an existing file's records forward) and written
//! again (`plugin::records_bytes`); what comes back is compared record by record.
//!
//! They do not come back the same: over Morrowind, Tribunal and Bloodmoon thousands of
//! records differ (padding the game left in an `AIDT` or a `PGRP`, a script's text without its
//! terminator, an empty `SCVR` or a `NAM9` dropped). None of it changes what the record
//! says, but a file rewritten so is a file changed everywhere - which is why `inplace` copies
//! the bytes of every record a build does not decide. With `WG_ROUNDTRIP` set to plugin files
//! (`;` between them), each record that differs is listed
//! (`cargo test --test roundtrip -- --ignored --nocapture`).

use std::collections::BTreeMap;
use std::path::Path;

/// A plugin's records as `(tag, bytes)` in file order.
fn records(bytes: &[u8]) -> Vec<(String, &[u8])> {
    let mut out = Vec::new();
    let mut at = 0;
    while at + 16 <= bytes.len() {
        let size = u32::from_le_bytes(bytes[at + 4..at + 8].try_into().unwrap()) as usize;
        let end = (at + 16 + size).min(bytes.len());
        out.push((String::from_utf8_lossy(&bytes[at..at + 4]).into_owned(), &bytes[at..end]));
        at = end;
    }
    out
}

/// A record's id for a report: its first NAME (or INAM) subrecord's text.
fn name_of(rec: &[u8]) -> String {
    let mut at = 16;
    while at + 8 <= rec.len() {
        let size = u32::from_le_bytes(rec[at + 4..at + 8].try_into().unwrap()) as usize;
        let end = (at + 8 + size).min(rec.len());
        if &rec[at..at + 4] == b"NAME" || &rec[at..at + 4] == b"INAM" {
            return String::from_utf8_lossy(&rec[at + 8..end]).trim_end_matches('\0').to_string();
        }
        at = end;
    }
    String::new()
}

/// The subrecords of a record as `(tag, bytes)`.
fn subs(rec: &[u8]) -> Vec<(String, &[u8])> {
    let mut out = Vec::new();
    let mut at = 16;
    while at + 8 <= rec.len() {
        let size = u32::from_le_bytes(rec[at + 4..at + 8].try_into().unwrap()) as usize;
        let end = (at + 8 + size).min(rec.len());
        out.push((String::from_utf8_lossy(&rec[at..at + 4]).into_owned(), &rec[at + 8..end]));
        at = end;
    }
    out
}

/// What differs, by record type: `{tag: (count, first few as "id: what")}`; empty when the
/// file comes back the same bytes.
fn differences(path: &Path) -> Result<BTreeMap<String, (usize, Vec<String>)>, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let json = editcore::plugin::records_json(&bytes, None).map_err(|e| format!("read: {e}"))?;
    let back = editcore::plugin::records_bytes(&json).map_err(|e| format!("write: {e}"))?;
    let mut out: BTreeMap<String, (usize, Vec<String>)> = BTreeMap::new();
    if back == bytes {
        return Ok(out);
    }
    let (a, b) = (records(&bytes), records(&back));
    if a.len() != b.len() {
        out.insert("(file)".into(), (1, vec![format!("{} records, {} written back", a.len(), b.len())]));
    }
    for ((tag, was), (_, now)) in a.iter().zip(&b) {
        if was == now {
            continue;
        }
        let (sa, sb) = (subs(was), subs(now));
        let what = if was[8..16] != now[8..16] {
            "the record's header".to_string()
        } else if sa.len() != sb.len() {
            let order = |s: &[(String, &[u8])]| s.iter().map(|(t, _)| t.as_str()).collect::<Vec<_>>().join(" ");
            format!("subrecords [{}] -> [{}]", order(&sa), order(&sb))
        } else {
            match sa.iter().zip(&sb).find(|(x, y)| x != y) {
                Some(((t, x), (u, y))) if t == u => format!("{t}: {} -> {} bytes{}", x.len(), y.len(), if x.len() == y.len() { format!(", first at {}", x.iter().zip(y.iter()).position(|(p, q)| p != q).unwrap_or(0)) } else { String::new() }),
                Some(((t, _), (u, _))) => format!("subrecord order: {t} -> {u}"),
                None => "sizes".to_string(),
            }
        };
        let e = out.entry(tag.clone()).or_default();
        e.0 += 1;
        if e.1.len() < 4 {
            e.1.push(format!("{}: {what}", name_of(was)));
        }
    }
    Ok(out)
}

#[test]
#[ignore]
fn roundtrip_of_given() {
    let given = std::env::var("WG_ROUNDTRIP").expect("WG_ROUNDTRIP=<plugin>[;<plugin>...]");
    for f in given.split(';').filter(|f| !f.is_empty()) {
        match differences(Path::new(f)) {
            Err(e) => println!("{f}: {e}"),
            Ok(d) if d.is_empty() => println!("{f}: the same bytes"),
            Ok(d) => {
                println!("{f}: {} record(s) differ", d.values().map(|v| v.0).sum::<usize>());
                for (tag, (n, first)) in d {
                    println!("  {tag}: {n}");
                    for line in first {
                        println!("    {line}");
                    }
                }
            }
        }
    }
}
