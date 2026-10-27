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
        return Read { keys: plugin_keys(path), cells: Vec::new() };
    }
    let Ok(bytes) = std::fs::read(path) else { return Read { keys: (Vec::new(), true), cells: Vec::new() } };
    match editcore::plugin::keys_and_cells(&bytes) {
        Ok((k, c)) => Read {
            keys: (k, false),
            cells: c
                .into_iter()
                .map(|c| match c {
                    editcore::plugin::CellRef::Int(n) => Cell::Int(n),
                    editcore::plugin::CellRef::Ext(x, y) => Cell::Ext(x, y),
                })
                .collect(),
        },
        Err(_) => Read { keys: (Vec::new(), false), cells: Vec::new() },
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
    Some(Read { keys: (keys, v["r"].as_bool()?), cells })
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
    }
    r
}

/// The keys alone, for [`crate::records::scan_touch`].
pub fn cached_keys(cache: Option<&Cache>) -> impl Fn(&Path) -> Keys + Sync + '_ {
    move |p| cached(cache, p).keys
}

/// Fill the cache for `plugins` (their files), on `threads` threads: what the viewer runs in
/// the background after it connects. Answers how many were read (the rest were current).
pub fn warm(cache: &Cache, plugins: &[PathBuf], threads: usize) -> usize {
    let stale: Vec<PathBuf> = match cache.stale_of(plugins, TAG) {
        Some(ix) => ix.into_iter().filter_map(|i| plugins.get(i).cloned()).collect(),
        None => plugins.to_vec(),
    };
    let next = std::sync::atomic::AtomicUsize::new(0);
    std::thread::scope(|s| {
        for _ in 0..threads.max(1).min(stale.len().max(1)) {
            s.spawn(|| loop {
                let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let Some(p) = stale.get(i) else { break };
                let r = read_plugin(p);
                cache.put(p, TAG, &pack(&r), Some(&[]));
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
}
