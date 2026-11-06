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

/// The stall itself: a slow link takes the Editor and a record is opened at the same moment
/// (the Open button names the row's plugin). For each of `editColumns` (the Misc tab's
/// columns) and `editUses` (the Use Report) it prints how long the slow link takes and how
/// long `editRecord` waits for it.
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

    for slow in ["editColumns", "editUses"] {
        let links = Arc::new(Links::open(plugins.clone(), None));
        let slow_body = if slow == "editColumns" {
            format!(r#"{{"tag": "{tag}"}}"#)
        } else {
            format!(r#"{{"tag": "{tag}", "id": "{id}", "plugins": ["{named}"]}}"#)
        };
        let held = {
            let links = links.clone();
            std::thread::spawn(move || {
                let t = Instant::now();
                let r = links.call(slow, &slow_body);
                (t.elapsed(), r.map(|s| s.len()))
            })
        };
        // Let the slow link take the lock first, as it does in the viewer.
        std::thread::sleep(std::time::Duration::from_millis(50));
        let body = format!(r#"{{"tag": "{tag}", "id": "{id}", "plugins": ["{named}"]}}"#);
        let t = Instant::now();
        let r = links.call("editRecord", &body);
        let waited = t.elapsed();
        let (slow_took, slow_len) = held.join().unwrap();
        println!("{slow:<12} took {slow_took:?} ({slow_len:?} bytes);  editRecord waited {waited:?} (ok={})", r.is_ok());
        let t = Instant::now();
        let _ = links.call("editRecord", &body);
        println!("{:<12} editRecord again, alone: {:?}", "", t.elapsed());
    }
}

/// The cell-name index against the cells themselves: for some cells, the plugins that have
/// them as `Session::cell_holders` says (the files' framing alone) and as the parsed CELL
/// records say (every plugin's cells read), and how long each takes. They must be equal.
/// WG_CELLS: the cell keys to check, `;`-separated (an interior's name, an exterior's
/// `(x, y)`), lower case.
///   cargo test --release --test probe_find probe_cell_index -- --ignored --nocapture
#[test]
#[ignore]
fn probe_cell_index() {
    let plugins = load_order();
    let keys = std::env::var("WG_CELLS")
        .unwrap_or_else(|_| "(-2, -9);(0, 0);(-3, -2);(41, 16);balmora, guild of mages;seyda neen, census and excise office;no such cell".into());
    let session = Session::new(plugins);

    let t = Instant::now();
    session.warm_cell_keys();
    println!("cell names of {} plugin(s) read in {:?}", session.order().len(), t.elapsed());

    let fast: Vec<(String, Option<Vec<String>>, std::time::Duration)> = keys
        .split(';')
        .map(|k| {
            let k = k.trim().to_lowercase();
            let t = Instant::now();
            let h = session.cell_holders(&k);
            (k, h, t.elapsed())
        })
        .collect();

    let t = Instant::now();
    session.warm(&["CELL"]);
    println!("every plugin's cells parsed in {:?}", t.elapsed());

    let mut bad = 0;
    for (k, h, took) in &fast {
        let slow: Vec<String> = session.order().iter().filter(|n| session.records(n, "CELL").contains_key(k)).cloned().collect();
        let same = h.as_ref() == Some(&slow);
        bad += usize::from(!same);
        println!("{:<44} index {:?} -> {} plugin(s); cells {} plugin(s); {}", k, took, h.as_ref().map_or(0, Vec::len), slow.len(), if same { "same" } else { "DIFFERENT" });
        if !same {
            println!("   index: {h:?}\n   cells: {slow:?}");
        }
    }
    assert_eq!(bad, 0, "the index and the cells disagree");
}

/// The use-report index against the scan: indexes the load order's plugins into a database
/// in the temp folder (the first time that is every plugin read whole: minutes), then the
/// Use Report for a record from the scan and from the index, how long each takes, and
/// whether they agree.
///   cargo test --release --test probe_find probe_uses_index -- --ignored --nocapture
#[test]
#[ignore]
fn probe_uses_index() {
    use editcore::cache::{Cache, MAX_BYTES};
    let plugins = load_order();
    let tag = std::env::var("WG_TAG").unwrap_or_else(|_| "MISC".into());
    let id = std::env::var("WG_ID").unwrap_or_else(|_| "AB_Misc_CeramicTeapot01".into());
    let rtype = editcore::key::type_of(&tag).expect("a record tag");
    let paths: Vec<PathBuf> = plugins.iter().map(|(_, p)| p.clone()).collect();
    let dir = std::env::temp_dir().join("wg-probe-use-index");
    std::fs::create_dir_all(&dir).unwrap();
    let cache = Cache::new(dir.join("use.sqlite3"), "editor-use-index", MAX_BYTES);

    let missing = cache.missing_uses(&paths).len();
    let t = Instant::now();
    editcore::uses::index_missing(&cache, &paths, 4);
    println!("indexed {missing} plugin(s) in {:?}; {} still missing", t.elapsed(), cache.missing_uses(&paths).len());

    let t = Instant::now();
    let scan = editcore::uses::report(&paths, rtype, &id);
    println!("scan : {:?}  {} use(s)", t.elapsed(), scan.len());
    let t = Instant::now();
    let index = editcore::uses::from_index(&cache, &paths, rtype, &id, |_| Vec::new());
    println!("index: {:?}  {}", t.elapsed(), index.as_ref().map_or("cannot answer".to_string(), |i| format!("{} use(s)", i.len())));

    let norm = |u: &[editcore::uses::Use]| {
        let mut v: Vec<(usize, String, String, usize, bool)> = u.iter().map(|u| (u.plugin, u.record_type.clone(), u.key.to_lowercase(), u.count, u.wins)).collect();
        v.sort();
        v
    };
    match index {
        Some(i) if norm(&i) == norm(&scan) => println!("the same {} use(s)", i.len()),
        Some(i) => {
            let (a, b) = (norm(&scan), norm(&i));
            println!("DIFFERENT: only in the scan: {:?}", a.iter().filter(|x| !b.contains(x)).take(8).collect::<Vec<_>>());
            println!("           only in the index: {:?}", b.iter().filter(|x| !a.contains(x)).take(8).collect::<Vec<_>>());
            panic!("the index and the scan disagree");
        }
        None => panic!("the index cannot answer after indexing every plugin"),
    }
}
