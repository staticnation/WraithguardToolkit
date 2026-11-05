//! Timing probe for the record dialog's cold read (`Session::find`): how long the first
//! lookup of a record type takes over a big load order, and how long the next ones take.
//! Not run with the normal tests (`#[ignore]`). From `viewer-shell/editcore`:
//!
//!   $env:WG_CFG="C:\path\to\openmw.cfg"      (its data= lines are the folders; best for a big order)
//!   $env:WG_TAG="MISC"                         (optional)
//!   $env:WG_ID="AB_Misc_CeramicTeapot01"       (optional)
//!   $env:WG_CELL="Seyda Neen"                  (optional)
//!   cargo test --release --test probe_find -- --ignored --nocapture
//!
//! WG_CFG is an openmw.cfg: its `data=` lines are the folders and its `content=` lines the
//! load order, each plugin found in the folders (the last that has it wins). WG_DATA, a list
//! of folders in the platform's path separator, is for a short list (an environment variable
//! is too small for a very big order); with no `content=` lines, every plugin in the folders
//! joins the order. WG_TAG and WG_ID are optional.

use std::path::PathBuf;
use std::time::Instant;

use editcore::links::Links;
use editcore::session::Session;

fn load_order() -> Vec<(String, PathBuf)> {
    let mut folders: Vec<PathBuf> = Vec::new();
    let mut content: Vec<String> = Vec::new();
    if let Ok(cfg) = std::env::var("WG_CFG") {
        let text = std::fs::read_to_string(&cfg).expect("cannot read WG_CFG");
        for line in text.lines() {
            let line = line.trim();
            let (key, rest) = if let Some(r) = line.strip_prefix("data") { ("data", r) } else if let Some(r) = line.strip_prefix("content") { ("content", r) } else { continue };
            let Some(rest) = rest.trim_start().strip_prefix('=') else { continue };
            let rest = rest.trim().trim_matches('"');
            if rest.is_empty() {
                continue;
            }
            if key == "data" {
                folders.push(PathBuf::from(rest));
            } else if !rest.to_ascii_lowercase().ends_with(".omwscripts") {
                content.push(rest.to_string());
            }
        }
    }
    if let Ok(dirs) = std::env::var("WG_DATA") {
        folders.extend(std::env::split_paths(&dirs));
    }
    assert!(!folders.is_empty(), "set WG_CFG to an openmw.cfg (or WG_DATA to data folders)");
    println!("{} data folder(s)", folders.len());

    // The plugins: with WG_CFG, the cfg's content= lines in order, each found in the data
    // folders (the last folder that has it wins, as in OpenMW); with only WG_DATA, every
    // plugin in the folders.
    let mut by_name: std::collections::HashMap<String, (String, PathBuf)> = std::collections::HashMap::new();
    let mut scanned: Vec<(String, PathBuf)> = Vec::new();
    for dir in &folders {
        let Ok(rd) = std::fs::read_dir(dir) else { continue };
        let mut found: Vec<(String, PathBuf)> = rd
            .flatten()
            .filter_map(|e| {
                let p = e.path();
                let ext = p.extension()?.to_str()?.to_ascii_lowercase();
                matches!(ext.as_str(), "esm" | "esp" | "omwaddon" | "omwgame" | "omwscripts").then(|| (p.file_name().unwrap().to_string_lossy().into_owned(), p))
            })
            .collect();
        found.sort_by_key(|a| a.0.to_lowercase());
        for (n, p) in found {
            by_name.insert(n.to_lowercase(), (n.clone(), p.clone()));
            scanned.push((n, p));
        }
    }
    let plugins: Vec<(String, PathBuf)> = if content.is_empty() {
        scanned
    } else {
        let missing: Vec<&String> = content.iter().filter(|n| !by_name.contains_key(&n.to_lowercase())).collect();
        println!("{} content= line(s), {} not found in the data folders", content.len(), missing.len());
        content.iter().filter_map(|n| by_name.get(&n.to_lowercase()).cloned()).collect()
    };
    plugins
}

#[test]
#[ignore]
fn probe_find() {
    let plugins = load_order();
    let tag = std::env::var("WG_TAG").unwrap_or_else(|_| "MISC".into());
    let id = std::env::var("WG_ID").unwrap_or_else(|_| "AB_Misc_CeramicTeapot01".into());
    println!("{} plugin(s) in the order", plugins.len());

    let session = Session::new(plugins);
    let t = Instant::now();
    let first = session.find(&tag, &id, None);
    println!("first  find({tag}, {id}): {:?}  found={}", t.elapsed(), first.is_some());

    let t = Instant::now();
    let again = session.find(&tag, &id, None);
    println!("again  find({tag}, {id}): {:?}  found={}", t.elapsed(), again.is_some());

    // A second record of the same tag: shows whether the cost is per tag (cached) or per lookup.
    let t = Instant::now();
    let other = session.find(&tag, "no_such_record_for_timing", None);
    println!("other  find({tag}, <missing>): {:?}  found={}", t.elapsed(), other.is_some());

    // A cell: its records are read from every plugin too, and there are far more of them
    // than a MISC's (the pool's list once held the Editor for minutes on this).
    let cell = std::env::var("WG_CELL").unwrap_or_else(|_| "Seyda Neen".into());
    let t = Instant::now();
    let c1 = session.find("CELL", &cell, None);
    println!("first  find(CELL, {cell}): {:?}  found={}", t.elapsed(), c1.is_some());
    let t = Instant::now();
    let c2 = session.find("CELL", &cell, None);
    println!("again  find(CELL, {cell}): {:?}  found={}", t.elapsed(), c2.is_some());
}

/// The stall itself: the Misc tab asks `editColumns` (every plugin's records of the type,
/// copied, with the Editor locked) and a record is opened at the same moment. Prints how
/// long `editColumns` takes and how long `editRecord` waits for it, with and without the
/// plugins the Object Window's row names (its Open button sends them).
///   cargo test --release --test probe_find probe_lock -- --ignored --nocapture
#[test]
#[ignore]
fn probe_lock() {
    use std::sync::Arc;
    let plugins = load_order();
    let tag = std::env::var("WG_TAG").unwrap_or_else(|_| "MISC".into());
    let id = std::env::var("WG_ID").unwrap_or_else(|_| "AB_Misc_CeramicTeapot01".into());
    let named = std::env::var("WG_PLUGIN").unwrap_or_else(|_| "OAAB_Data.esm".into());
    println!("{} plugin(s); record {tag} {id}; its row names {named}", plugins.len());

    for with_plugins in [true, false] {
        let links = Arc::new(Links::open(plugins.clone(), None));
        let cols = {
            let links = links.clone();
            let body = format!(r#"{{"tag": "{tag}"}}"#);
            std::thread::spawn(move || {
                let t = Instant::now();
                let r = links.call("editColumns", &body);
                (t.elapsed(), r.map(|s| s.len()))
            })
        };
        // Let editColumns take the lock first, as a click on a tab does before the Open.
        std::thread::sleep(std::time::Duration::from_millis(50));
        let body = if with_plugins {
            format!(r#"{{"tag": "{tag}", "id": "{id}", "plugins": ["{named}"]}}"#)
        } else {
            format!(r#"{{"tag": "{tag}", "id": "{id}"}}"#)
        };
        let t = Instant::now();
        let r = links.call("editRecord", &body);
        let waited = t.elapsed();
        let (cols_took, cols_len) = cols.join().unwrap();
        println!(
            "editRecord {:<26} waited {:?} (ok={})   editColumns took {:?} ({:?} bytes)",
            if with_plugins { "naming the row's plugin:" } else { "of every plugin:" },
            waited,
            r.is_ok(),
            cols_took,
            cols_len
        );
        // The same record again, nothing else running: the cost of the lookup itself.
        let t = Instant::now();
        let _ = links.call("editRecord", &body);
        println!("   again, alone: {:?}", t.elapsed());
    }
}
