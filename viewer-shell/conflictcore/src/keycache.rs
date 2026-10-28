//! A plugin's conflict keys and cells, kept in an SQLite cache between scans (the Editor's
//! record cache, `editcore::cache`, as a store of its own tag): a plugin is read once, and
//! read again only when its size or modification time changes. The record scan, the plugin
//! view and the cell map then take an unchanged load order's keys from the cache instead of
//! reading 600 plugins again, and the viewer fills the cache in the background after it
//! connects ([`warm`]), so the first scan is a lookup too.
//!
//! The cache is only ever a shortcut: any failure of it is a miss, and the plugin is read.

use std::path::{Path, PathBuf};

use editcore::cache::Cache;
use serde_json::{json, Value};

use crate::records::{plugin_keys, Keys};

/// The tag a plugin's keys and cells are stored under in the cache.
pub const TAG: &str = "~WGCK";

/// Bumped when what is stored changes shape (the cache's `parser` stamp: a file made by
/// another is emptied).
pub const PARSER: &str = "wraithguard-scan-1";

/// What a plugin gives the scans: its conflict keys and the cells it touches.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Read {
    pub keys: Keys,
    /// `(interior name)` or `(x, y)`, in the order met.
    pub cells: Vec<Cell>,
    /// Where each key's record lies in the file, when the file was read whole (a read from
    /// the cache, or a file that is not a TES3 plugin, has none): kept beside the keys, in
    /// the cache's `spans`, not in their entry.
    pub spans: Option<Vec<editcore::plugin::Span>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Cell {
    Int(String),
    Ext(i64, i64),
}

/// A plugin read whole, as [`plugin_keys`] reads it, with its cells too.
pub fn read_plugin(path: &Path) -> Read {
    let low = path.to_string_lossy().to_lowercase();
    if low.ends_with(".omwscripts") {
        return Read { keys: plugin_keys(path), cells: Vec::new(), spans: None };
    }
    let Ok(bytes) = std::fs::read(path) else { return Read { keys: (Vec::new(), true), cells: Vec::new(), spans: None } };
    match editcore::plugin::keys_cells_spans(&bytes) {
        Ok((k, c, spans)) => Read {
            spans: Some(spans),
            keys: (k, false),
            cells: c
                .into_iter()
                .map(|c| match c {
                    editcore::plugin::CellRef::Int(n) => Cell::Int(n),
                    editcore::plugin::CellRef::Ext(x, y) => Cell::Ext(x, y),
                })
                .collect(),
        },
        Err(_) => Read { keys: (Vec::new(), false), cells: Vec::new(), spans: None },
    }
}

fn pack(r: &Read) -> Vec<u8> {
    let keys: Vec<Value> = r.keys.0.iter().map(|(t, i, d)| json!([t, i, d])).collect();
    let cells: Vec<Value> = r
        .cells
        .iter()
        .map(|c| match c {
            Cell::Int(n) => json!(n),
            Cell::Ext(x, y) => json!([x, y]),
        })
        .collect();
    json!({"k": keys, "r": r.keys.1, "c": cells}).to_string().into_bytes()
}

fn unpack(b: &[u8]) -> Option<Read> {
    let v: Value = serde_json::from_slice(b).ok()?;
    let keys = v["k"]
        .as_array()?
        .iter()
        .map(|k| Some((k[0].as_str()?.to_string(), k[1].as_str()?.to_string(), k[2].as_bool()?)))
        .collect::<Option<Vec<_>>>()?;
    let cells = v["c"]
        .as_array()?
        .iter()
        .map(|c| match c {
            Value::String(n) => Some(Cell::Int(n.clone())),
            Value::Array(a) => Some(Cell::Ext(a.first()?.as_i64()?, a.get(1)?.as_i64()?)),
            _ => None,
        })
        .collect::<Option<Vec<_>>>()?;
    Some(Read { keys: (keys, v["r"].as_bool()?), cells, spans: None })
}

/// A plugin's keys and cells: from the cache when it holds them for the file as it is, else
/// read and kept.
pub fn cached(cache: Option<&Cache>, path: &Path) -> Read {
    if let Some(r) = cache.and_then(|c| c.get(path, TAG)).and_then(|b| unpack(&b)) {
        return r;
    }
    let r = read_plugin(path);
    if let Some(c) = cache {
        // an empty key list stamps the entry (`key_stamps`), which [`warm`] asks about
        c.put(path, TAG, &pack(&r), Some(&[]));
        if let Some(sp) = &r.spans {
            c.put_spans(path, sp);
        }
    }
    r
}

/// One record of a plugin, by its key (`rtype`, `id`), in its tes3conv-schema form: the
/// cache says where it lies in the file and only those bytes are read and decoded - a few
/// hundred, not the plugin through. A plugin whose spans are not kept yet is read once
/// (as the scan reads it) and they are kept, so the next record of it is a seek. `None`:
/// the plugin has no such record, or cannot be read.
pub fn record_at(cache: Option<&Cache>, path: &Path, rtype: &str, id: &str) -> Option<Value> {
    use std::io::{Read as _, Seek, SeekFrom};
    if path.to_string_lossy().to_lowercase().ends_with(".omwscripts") {
        return None;
    }
    let from_file = |off: u64, len: u32| -> Option<Value> {
        let mut f = std::fs::File::open(path).ok()?;
        f.seek(SeekFrom::Start(off)).ok()?;
        let mut buf = vec![0u8; len as usize];
        f.read_exact(&mut buf).ok()?;
        editcore::plugin::record_value(&buf).ok()
    };
    if let Some(c) = cache.filter(|c| c.spans_current(path)) {
        // indexed as the file is: its record is where the index says, or it has none
        return c.span(path, rtype, id).and_then(|(off, len)| from_file(off, len));
    }
    let bytes = std::fs::read(path).ok()?;
    match editcore::plugin::keys_cells_spans(&bytes) {
        Ok((_, _, spans)) => {
            let want = crate::records::fold_id(id);
            let at = spans.iter().find(|(t, i, _, _)| t == rtype && crate::records::fold_id(i) == want).map(|(_, _, off, len)| (*off as usize, *len as usize));
            if let Some(c) = cache {
                c.put_spans(path, &spans);
            }
            at.and_then(|(off, len)| bytes.get(off..off + len)).and_then(|b| editcore::plugin::record_value(b).ok())
        }
        // a file the key reader cannot take whole: the reader of the wanted type alone
        Err(_) => {
            let key = (rtype.to_string(), id.to_string());
            let wanted: std::collections::HashSet<_> = std::iter::once(key.clone()).collect();
            crate::fields::records_of(&bytes, &wanted).ok().and_then(|mut m| m.remove(&key))
        }
    }
}

/// The keys alone, for [`crate::records::scan_touch`].
pub fn cached_keys(cache: Option<&Cache>) -> impl Fn(&Path) -> Keys + Sync + '_ {
    move |p| cached(cache, p).keys
}

/// Fill the cache for `plugins` (their files), on `threads` threads: what the viewer runs in
/// the background after it connects. Answers how many were read (the rest were current).
pub fn warm(cache: &Cache, plugins: &[PathBuf], threads: usize) -> usize {
    // Plugins whose keys are not kept as they are, and TES3 plugins whose record spans are
    // not (a cache made before the spans existed has every key and no span: read once more).
    let mut want: std::collections::BTreeSet<usize> = match cache.stale_of(plugins, TAG) {
        Some(ix) => ix.into_iter().collect(),
        None => (0..plugins.len()).collect(),
    };
    let tes3: Vec<usize> =
        (0..plugins.len()).filter(|&i| !plugins[i].to_string_lossy().to_lowercase().ends_with(".omwscripts")).collect();
    let tes3_paths: Vec<PathBuf> = tes3.iter().map(|&i| plugins[i].clone()).collect();
    match cache.stale_spans(&tes3_paths) {
        Some(ix) => want.extend(ix.into_iter().map(|j| tes3[j])),
        None => want.extend(tes3.iter().copied()),
    }
    let stale: Vec<PathBuf> = want.into_iter().filter_map(|i| plugins.get(i).cloned()).collect();
    let next = std::sync::atomic::AtomicUsize::new(0);
    std::thread::scope(|s| {
        for _ in 0..threads.max(1).min(stale.len().max(1)) {
            s.spawn(|| loop {
                let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let Some(p) = stale.get(i) else { break };
                let r = read_plugin(p);
                cache.put(p, TAG, &pack(&r), Some(&[]));
                if let Some(sp) = &r.spans {
                    cache.put_spans(p, sp);
                }
            });
        }
    });
    stale.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_is_kept_reads_back() {
        let r = Read {
            keys: (vec![("Static".into(), "rock".into(), false), ("Cell".into(), "(1, 1)".into(), true)], false),
            cells: vec![Cell::Int("Vault".into()), Cell::Ext(-2, 3)],
            spans: None,
        };
        assert_eq!(unpack(&pack(&r)), Some(r));
        assert_eq!(unpack(b"not json"), None);
    }

    #[test]
    fn a_plugin_is_read_once_until_it_changes() {
        let dir = std::env::temp_dir().join(format!("wg_keycache_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let plugin = dir.join("A.omwscripts");
        std::fs::write(&plugin, "PLAYER: scripts/a.lua\n").unwrap();
        let cache = Cache::new(dir.join("scan.sqlite3"), PARSER, 1 << 20);
        assert_eq!(warm(&cache, std::slice::from_ref(&plugin), 2), 1);
        assert_eq!(warm(&cache, std::slice::from_ref(&plugin), 2), 0);
        let got = cached(Some(&cache), &plugin);
        assert_eq!(got.keys.0, vec![("LuaScript".to_string(), "scripts/a.lua".to_string(), false)]);
        cache.close();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_record_is_read_by_its_span() {
        let path = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../ui/tests/fixture/Data Files/Lamp.esm"));
        let bytes = std::fs::read(&path).unwrap();
        let (keys, _) = editcore::plugin::keys_and_cells(&bytes).unwrap();
        assert!(!keys.is_empty());
        let dir = std::env::temp_dir().join(format!("wg_spans_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let cache = Cache::new(dir.join("scan.sqlite3"), PARSER, 1 << 20);
        let want = |t: &String, i: &String| {
            let k = (t.clone(), i.clone());
            let wanted: std::collections::HashSet<_> = std::iter::once(k.clone()).collect();
            crate::fields::records_of(&bytes, &wanted).unwrap().remove(&k)
        };
        // not indexed: the plugin is read once and indexed; then each record is a seek
        for pass in 0..2 {
            for (t, i, _) in keys.iter().take(25) {
                let got = record_at(Some(&cache), &path, t, i);
                assert!(got.is_some(), "pass {pass}: {t} {i}");
                assert_eq!(got, want(t, i), "pass {pass}: {t} {i}");
                // an id is the same id in any case
                assert_eq!(record_at(Some(&cache), &path, t, &i.to_ascii_uppercase()), got, "pass {pass}: {t} {i} upper");
            }
            assert!(cache.spans_current(&path));
        }
        // no cache at all reads the same record
        let (t, i, _) = &keys[0];
        assert_eq!(record_at(None, &path, t, i), want(t, i));
        // indexed, and not there
        assert_eq!(record_at(Some(&cache), &path, "Static", "no such record ~"), None);
        cache.close();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
