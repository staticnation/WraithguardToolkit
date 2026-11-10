//! Search all over the plugins' own bytes (the program's way, `Links::search_all_link`:
//! `search::scan_plugin`, a record parsed only when its bytes hold the text) gives what the
//! scan of every record gives (`Editor::search_all`, the tests' sessions' way, which holds
//! every record in memory): the same hits, in the same order, with the same count - the
//! patch's edits, the records it makes, a deletion and an override included - with the use
//! index and without it.

use editcore::cache::Cache;
use editcore::links::Links;
use serde_json::{json, Value};
use std::path::PathBuf;

fn rec(t: &str, id: &str, fields: Value) -> Value {
    let mut r = editcore::blank::blank(t).unwrap();
    r["id"] = json!(id);
    for (k, v) in fields.as_object().unwrap() {
        r[k] = v.clone();
    }
    r
}

fn plugin(masters: &[&str], records: Vec<Value>) -> Vec<u8> {
    let mut head = editcore::blank::blank("Header").unwrap();
    head["masters"] = json!(masters.iter().map(|m| json!([m, 0])).collect::<Vec<_>>());
    let mut all = vec![head];
    all.extend(records);
    editcore::plugin::records_bytes(&Value::Array(all).to_string()).unwrap()
}

fn hits(v: &Value) -> (Value, Value) {
    (v["hits"].clone(), v["count"].clone())
}

#[test]
fn the_plugins_bytes_answer_as_the_scan() {
    let dir = std::env::temp_dir().join(format!("wg-search-index-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let long = "A long tale of the Iron Sword of Balmora, ".repeat(8);
    let files: Vec<(&str, Vec<u8>)> = vec![
        (
            "Base.esm",
            plugin(&[], vec![
                rec("Book", "bk_tale", json!({"name": "The Tale", "text": long})),
                rec("MiscItem", "misc_cup", json!({"name": "Iron Cup", "mesh": "m\\cup.nif"})),
                rec("Npc", "guard", json!({"name": "Guard of Iron"})),
                rec("MiscItem", "misc_plate", json!({"name": "Plate"})),
            ]),
        ),
        (
            "Mod.esp",
            plugin(&["Base.esm"], vec![
                // An override that loses the word: the index finds it in Base, the winner has it not.
                rec("MiscItem", "misc_cup", json!({"name": "Clay Cup", "mesh": "m\\cup.nif"})),
                rec("Npc", "smith", json!({"name": "Smith", "script": "iron_script"})),
                // A deletion stands as the winner, as in the scan.
                rec("Npc", "guard", json!({"name": "Guard of Iron", "flags": "DELETED"})),
            ]),
        ),
    ];
    let mut plugins: Vec<(String, PathBuf)> = Vec::new();
    for (name, bytes) in &files {
        let p = dir.join(name);
        std::fs::write(&p, bytes).unwrap();
        plugins.push((name.to_string(), p));
    }
    let paths: Vec<PathBuf> = plugins.iter().map(|p| p.1.clone()).collect();
    let db = dir.join("use.sqlite3");
    let links = Links::open(plugins, None).with_use_index(db.clone());
    let cache = Cache::new(db, "editor-use-index", editcore::cache::MAX_BYTES);
    {
        // The patch types a word into one record and makes another.
        let mut ed = links.editor.lock().unwrap();
        let plate = ed.find("MISC", "misc_plate", None).unwrap();
        ed.set_field(&plate, "name", &json!("Iron Plate")).unwrap();
        let made = ed.insert("MISC", "misc_new").unwrap();
        ed.set_field(&made, "name", &json!("New Iron Thing")).unwrap();
    }
    let ask = |text: &str, whole: bool| -> Value {
        serde_json::from_str(&links.call("editSearchAll", &json!({"text": text, "whole": whole}).to_string()).unwrap()).unwrap()
    };
    let queries = [("iron", false), ("IRON", false), ("iron", true), ("sword of balmora", false), ("cup.nif", false), ("Clay", false), ("ir", false), ("nothing at all", false)];
    // Without the use index (who has a record last is asked of the files' framing), then with it.
    for indexed in [false, true] {
        if indexed {
            editcore::uses::index_missing(&cache, &paths, 1);
            assert!(cache.missing_uses(&paths).is_empty());
        }
        for (text, whole) in queries {
            let streamed = ask(text, whole);
            assert!(streamed.get("took").is_some(), "{text}: the plugins' bytes were searched");
            let scanned = links.editor.lock().unwrap().search_all(text, whole, 2000);
            assert_eq!(hits(&streamed), hits(&scanned), "{text} (whole: {whole}, use index: {indexed})");
        }
    }
    let iron = ask("iron", false);
    let found: Vec<(&str, &str)> = iron["hits"].as_array().unwrap().iter().map(|h| (h["id"].as_str().unwrap(), h["path"].as_str().unwrap())).collect();
    assert!(found.contains(&("misc_plate", "name")) && found.contains(&("misc_new", "name")), "the patch's text: {found:?}");
    assert!(!found.iter().any(|(id, _)| *id == "misc_cup"), "the override's text, not the master's: {found:?}");
    // A session the search alone has used holds no records: none was read into it.
    let fresh = Links::open(paths.iter().map(|p| (p.file_name().unwrap().to_string_lossy().into_owned(), p.clone())).collect(), None);
    let got: Value = serde_json::from_str(&fresh.call("editSearchAll", &json!({"text": "tale"}).to_string()).unwrap()).unwrap();
    assert_eq!(got["count"], json!(3), "{got}");
    assert_eq!(fresh.editor.lock().unwrap().session.has_read("BOOK", "bk_tale"), None, "the books were never read into the session");
    let _ = std::fs::remove_dir_all(&dir);
}

/// On real plugins (`WG_SEARCH_PLUGINS=<file>;<file>...`, `WG_SEARCH_TEXT`): how long Search all
/// takes over them and the most memory the process held (Linux), to see it stays flat.
#[test]
#[ignore]
fn search_given() {
    let files = std::env::var("WG_SEARCH_PLUGINS").expect("WG_SEARCH_PLUGINS");
    let plugins: Vec<(String, PathBuf)> = files.split(';').filter(|f| !f.is_empty()).map(|f| (PathBuf::from(f).file_name().unwrap().to_string_lossy().into_owned(), PathBuf::from(f))).collect();
    let links = Links::open(plugins, None);
    for text in std::env::var("WG_SEARCH_TEXT").unwrap_or_else(|_| "the;Caius;netch".into()).split(';') {
        let t = std::time::Instant::now();
        let got: Value = serde_json::from_str(&links.call("editSearchAll", &json!({"text": text}).to_string()).unwrap()).unwrap();
        let peak = std::fs::read_to_string("/proc/self/status").ok().and_then(|s| s.lines().find(|l| l.starts_with("VmHWM")).map(str::to_string)).unwrap_or_default();
        println!("{text:?}: {} field(s) in {} record(s), {:.2} s; {peak}", got["count"], got["took"]["records"], t.elapsed().as_secs_f32());
    }
}
