//! Timing probe for Merged Lands' read of the plugins: what it did before (the scan of every
//! byte for the two tags, then a second read) against the one read, over real plugin files.
//! Also a check on real data: the two must give the same records for every plugin.
//! Not run with the normal tests. From `viewer-shell/landcore`:
//!
//!   $env:WG_DATA="E:\OpenMW\Morrowind\Data Files;E:\OpenMW\Mods\total-overhaul\..."   (folders, ';' on Windows)
//!   cargo test --release --test probe_land_read -- --ignored --nocapture
//!
//! Every `.esp`, `.esm` and `.omwaddon` directly in each folder is read. Run it twice: the
//! first time may be the disk's, the second the files in the system's cache (what a second
//! Merge Lands in a session sees).

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use landcore::read::{has_landscape, read_landscape_records, read_terrain_records, Record};

fn plugins() -> Vec<PathBuf> {
    let data = std::env::var("WG_DATA").expect("WG_DATA: the plugin folders, separated as PATH is");
    let mut found = Vec::new();
    for folder in std::env::split_paths(&data) {
        let Ok(entries) = std::fs::read_dir(&folder) else { continue };
        for e in entries.flatten() {
            let p = e.path();
            let ext = p.extension().map(|x| x.to_string_lossy().to_lowercase()).unwrap_or_default();
            if p.is_file() && matches!(ext.as_str(), "esp" | "esm" | "omwaddon") {
                found.push(p);
            }
        }
    }
    found.sort();
    found
}

fn before(path: &Path) -> Result<Vec<Record>, String> {
    if !has_landscape(path) {
        return Ok(Vec::new());
    }
    read_landscape_records(path)
}

#[test]
#[ignore]
fn probe_land_read() {
    let files = plugins();
    assert!(!files.is_empty(), "no plugins in WG_DATA");
    let bytes: u64 = files.iter().filter_map(|p| std::fs::metadata(p).ok()).map(|m| m.len()).sum();
    println!("{} plugins, {:.0} MB", files.len(), bytes as f64 / 1_048_576.0);

    let (mut scan, mut scan_yes) = (Duration::ZERO, 0usize);
    for p in &files {
        let t = Instant::now();
        let yes = has_landscape(p);
        scan += t.elapsed();
        scan_yes += usize::from(yes);
    }
    println!("has_landscape over every plugin (read + scan of every byte): {scan:?} ({scan_yes} mention terrain)");

    let t = Instant::now();
    let old: Vec<Result<Vec<Record>, String>> = files.iter().map(|p| before(p)).collect();
    let old_took = t.elapsed();

    let t = Instant::now();
    let new: Vec<Result<Vec<Record>, String>> = files.iter().map(|p| read_terrain_records(p)).collect();
    let new_took = t.elapsed();

    let records: usize = new.iter().filter_map(|r| r.as_ref().ok()).map(Vec::len).sum();
    let different: Vec<&PathBuf> = files.iter().zip(old.iter().zip(&new)).filter(|(_, (a, b))| a != b).map(|(p, _)| p).collect();
    println!("before (scan, then read): {old_took:?}");
    println!("now (one read):           {new_took:?}   ({records} terrain records)");
    println!("saved {:?}", old_took.saturating_sub(new_took));
    for p in &different {
        println!("DIFFERENT: {}", p.display());
    }
    assert!(different.is_empty(), "the one read and the scan-then-read disagree");
}
