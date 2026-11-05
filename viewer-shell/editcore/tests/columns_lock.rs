//! The Object Window's columns (`editColumns`) read every plugin's records of a type. Static,
//! 2026-11-05: that read ran under the Editor's one lock, one plugin after another, and an
//! `editRecord` asked for meanwhile (the Open click) waited for all of it. These pin what
//! replaced it: the read is made on several threads, once per plugin and tag however many
//! ask, and without the Editor's lock.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use editcore::edit::Editor;
use editcore::links::Links;
use editcore::queue::PatchQueue;
use editcore::session::Session;
use serde_json::{Value, json};

fn plugins(n: usize) -> Vec<(String, PathBuf)> {
    (0..n).map(|i| (format!("P{i}.esp"), PathBuf::from(format!("/nowhere/P{i}.esp")))).collect()
}

fn misc(id: &str) -> Value {
    json!({"type": "MiscItem", "id": id, "name": id, "data": {"weight": 1.0, "value": 5}})
}

/// A reader that counts its reads by tag and, while `hold` is set, does not return.
fn reader(reads: Arc<Mutex<Vec<(String, String)>>>, hold: Arc<AtomicBool>, started: Arc<AtomicUsize>) -> Box<editcore::session::Reader> {
    Box::new(move |path, tag| {
        started.fetch_add(1, Ordering::SeqCst);
        let who = path.file_stem().unwrap().to_string_lossy().to_string();
        reads.lock().unwrap().push((who.clone(), tag.to_string()));
        let t = Instant::now();
        while hold.load(Ordering::SeqCst) && t.elapsed() < Duration::from_secs(10) {
            std::thread::sleep(Duration::from_millis(5));
        }
        Ok(if tag == "MISC" { vec![misc(&format!("item_{who}"))] } else { Vec::new() })
    })
}

/// The first of the two this exists for: another link is answered while columns are being read.
#[test]
fn a_link_is_answered_while_a_types_columns_are_being_read() {
    let reads = Arc::new(Mutex::new(Vec::new()));
    let hold = Arc::new(AtomicBool::new(true));
    let started = Arc::new(AtomicUsize::new(0));
    let session = Session::with_reader(plugins(4), reader(reads.clone(), hold.clone(), started.clone()));
    let links = Arc::new(Links::with_editor(Editor::new(session, PatchQueue::new(), None)));

    let asker = {
        let links = links.clone();
        std::thread::spawn(move || links.call("editColumns", r#"{"tag": "MISC"}"#).unwrap())
    };
    // A failsafe: whatever happens, the reads are let go, so a failure fails and does not hang.
    {
        let hold = hold.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(3));
            hold.store(false, Ordering::SeqCst);
        });
    }
    let t = Instant::now();
    while started.load(Ordering::SeqCst) == 0 && t.elapsed() < Duration::from_secs(2) {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(started.load(Ordering::SeqCst) > 0, "the columns' read began");

    let asked = Instant::now();
    links.call("editRevision", "{}").unwrap();
    let waited = asked.elapsed();
    assert!(hold.load(Ordering::SeqCst), "the read was still out when the other link was answered");
    assert!(waited < Duration::from_millis(1000), "the other link waited {waited:?} for the columns' read");

    hold.store(false, Ordering::SeqCst);
    let out: Value = serde_json::from_str(&asker.join().unwrap()).unwrap();
    assert_eq!(out["rows"].as_object().unwrap().len(), 4, "a row from each plugin: {out}");
}

/// The plugins are read at once, each of them once, and the table is the same as a serial read's.
#[test]
fn the_plugins_are_read_together_once_each_and_the_table_is_the_same() {
    let reads = Arc::new(Mutex::new(Vec::new()));
    let hold = Arc::new(AtomicBool::new(false));
    let started = Arc::new(AtomicUsize::new(0));
    let session = Session::with_reader(plugins(12), reader(reads.clone(), hold, started));
    let links = Arc::new(Links::with_editor(Editor::new(session, PatchQueue::new(), None)));

    // Two askers for the one type at the same moment (the Object Window does that).
    let a = { let l = links.clone(); std::thread::spawn(move || l.call("editColumns", r#"{"tag": "MISC"}"#).unwrap()) };
    let b = { let l = links.clone(); std::thread::spawn(move || l.call("editColumns", r#"{"tag": "MISC"}"#).unwrap()) };
    let (a, b) = (a.join().unwrap(), b.join().unwrap());
    assert_eq!(a, b);
    let got = reads.lock().unwrap().len();
    assert_eq!(got, 12, "each plugin read once, not once per asker: {:?}", reads.lock().unwrap());
    let out: Value = serde_json::from_str(&a).unwrap();
    let keys: Vec<&String> = out["rows"].as_object().unwrap().keys().collect();
    assert_eq!(keys.len(), 12);
    // Load order: the rows are in the order of the plugins, as a serial read had them.
    let expected: Vec<String> = (0..12).map(|i| format!("item_p{i}")).collect();
    assert_eq!(keys.iter().map(|k| k.to_string()).collect::<Vec<_>>(), expected);
    // Asked again: cached, nothing read.
    links.call("editColumns", r#"{"tag": "MISC"}"#).unwrap();
    assert_eq!(reads.lock().unwrap().len(), 12);
}

/// A plugin forgotten while it is being read (a patch written over it) is not kept as it was.
#[test]
fn a_read_that_began_before_a_plugin_was_forgotten_is_not_kept() {
    let reads = Arc::new(Mutex::new(Vec::new()));
    let hold = Arc::new(AtomicBool::new(true));
    let started = Arc::new(AtomicUsize::new(0));
    let dir = std::env::temp_dir().join(format!("wg-forget-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("A.esp");
    std::fs::write(&path, b"x").unwrap();
    let session = Arc::new(Session::with_reader(vec![("A.esp".into(), path.clone())], reader(reads.clone(), hold.clone(), started.clone())));

    let reading = { let s = session.clone(); std::thread::spawn(move || s.records("A.esp", "MISC").len()) };
    let t = Instant::now();
    while started.load(Ordering::SeqCst) == 0 && t.elapsed() < Duration::from_secs(2) {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(session.forget_file(&path));
    hold.store(false, Ordering::SeqCst);
    assert_eq!(reading.join().unwrap(), 1, "the reader still got what it read");
    // Not cached: the next ask reads the file again.
    session.records("A.esp", "MISC");
    assert_eq!(reads.lock().unwrap().len(), 2, "read again after the forget");
    session.records("A.esp", "MISC");
    assert_eq!(reads.lock().unwrap().len(), 2, "and that one is kept");
    let _ = std::fs::remove_dir_all(&dir);
}
