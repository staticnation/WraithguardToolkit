//! The shared case files (tests/fixtures at the repository's root), through editcore's
//! session, built as the Python tests build theirs (`EditorSession(read=...)`): the record
//! cases through `find` (tests/test_record_index.py), the reference cases through
//! `winning_reference` (tests/test_ref_cases.py) and the move cases through `find_ref`
//! (tests/test_move_cases.py), and the queue, edit, place, search and use cases through the
//! Editor. The two sides read the same files and must agree.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use editcore::key::type_of;
use editcore::session::Session;
use serde_json::{json, Value};

fn cases(name: &str) -> Vec<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures").join(name);
    let v: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    v["cases"].as_array().unwrap().clone()
}

/// A session whose plugins are the given records by tag (`name -> tag -> records`).
fn session(order: &[String], records: HashMap<String, HashMap<String, Vec<Value>>>) -> Session {
    let plugins = order.iter().map(|n| (n.clone(), PathBuf::from(n))).collect();
    Session::with_reader(
        plugins,
        Box::new(move |path, tag| {
            let name = path.to_string_lossy().to_string();
            Ok(records.get(&name).and_then(|t| t.get(tag)).cloned().unwrap_or_default())
        }),
    )
}

fn header(masters: &Value) -> Value {
    let m: Vec<Value> = masters.as_array().unwrap().iter().map(|m| json!([m, 0])).collect();
    json!({"type": "Header", "masters": m})
}

#[test]
fn record_cases_resolve_as_the_python_editor_does() {
    let mut failed = Vec::new();
    for case in cases("record_cases.json") {
        let plugins = case["plugins"].as_array().unwrap();
        let names: Vec<String> = (0..plugins.len()).map(|i| format!("P{i}.esp")).collect();
        let mut records: HashMap<String, HashMap<String, Vec<Value>>> = HashMap::new();
        let mut every = std::collections::BTreeSet::new();
        for (name, recs) in names.iter().zip(plugins) {
            for r in recs.as_array().unwrap() {
                let (tag, id, deleted) = (r[0].as_str().unwrap(), r[1].as_str().unwrap(), r[2].as_bool().unwrap());
                let rec = json!({"type": type_of(tag).unwrap(), "id": id, "flags": if deleted { "DELETED" } else { "" }});
                records.entry(name.clone()).or_default().entry(tag.to_string()).or_default().push(rec);
                every.insert((tag.to_string(), id.to_lowercase()));
            }
        }
        let s = session(&names, records);
        let live: HashMap<(String, String), Vec<String>> = case["expect"]["live"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| {
                let at = l[2].as_array().unwrap().iter().map(|i| names[i.as_u64().unwrap() as usize].clone()).collect();
                ((l[0].as_str().unwrap().to_string(), l[1].as_str().unwrap().to_string()), at)
            })
            .collect();
        for (tag, rid) in every {
            let got = s.find(&tag, &rid, None).map(|f| f.versions.iter().map(|(p, _)| p.clone()).collect::<Vec<_>>());
            let want = live.get(&(tag.clone(), rid.clone())).cloned();
            if got != want {
                failed.push(format!("{}: {tag} {rid}: got {got:?}, want {want:?}", case["name"]));
            }
        }
    }
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}

#[test]
fn ref_cases_resolve_as_the_python_editor_does() {
    let mut failed = Vec::new();
    for case in cases("ref_cases.json") {
        let plugins = case["plugins"].as_array().unwrap();
        let names: Vec<String> = plugins.iter().map(|p| p["name"].as_str().unwrap().to_string()).collect();
        let mut records: HashMap<String, HashMap<String, Vec<Value>>> = HashMap::new();
        let mut cells = Vec::new();
        for p in plugins {
            let name = p["name"].as_str().unwrap().to_string();
            let refs: Vec<Value> = p["refs"]
                .as_array()
                .unwrap()
                .iter()
                .map(|r| json!({"mast_index": r[0], "refr_index": r[1], "deleted": r[2], "id": "thing"}))
                .collect();
            cells.push((name.clone(), json!({"type": "Cell", "references": refs})));
            records.entry(name).or_default().insert("TES3".into(), vec![header(&p["masters"])]);
        }
        let s = session(&names, records);
        let (origin, index) = (case["ask"][0].as_str().unwrap(), case["ask"][1].as_i64().unwrap());
        let got = s.winning_reference(&cells, origin, index);
        let want = &case["expect"];
        let ok = match (&got, want["winner"].as_str()) {
            (None, None) => true,
            (Some((p, r)), Some(w)) => p == w && r["deleted"].as_bool() == want["deleted"].as_bool(),
            _ => false,
        };
        if !ok {
            failed.push(format!("{}: got {got:?}, want {want}", case["name"]));
        }
    }
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}

#[test]
fn move_cases_stand_where_the_python_editor_says() {
    let mut failed = Vec::new();
    for case in cases("move_cases.json") {
        let plugins = case["plugins"].as_array().unwrap();
        let names: Vec<String> = plugins.iter().map(|p| p["name"].as_str().unwrap().to_string()).collect();
        let mut records: HashMap<String, HashMap<String, Vec<Value>>> = HashMap::new();
        for p in plugins {
            let refs: Vec<Value> = p["refs"]
                .as_array()
                .unwrap()
                .iter()
                .map(|r| json!({"mast_index": r[0], "refr_index": r[1], "id": "rock", "deleted": r[2], "moved_cell": r[3]}))
                .collect();
            let cell = json!({"type": "Cell", "name": "", "data": {"flags": 2, "grid": [1, 1]}, "references": refs});
            let t = records.entry(p["name"].as_str().unwrap().to_string()).or_default();
            t.insert("CELL".into(), vec![cell]);
            t.insert("TES3".into(), vec![header(&p["masters"])]);
        }
        let s = session(&names, records);
        let found = s.find_ref("(1, 1)", "A.esm", 1, None);
        let got: Vec<Value> = match found {
            Some(f) if !f.reference["deleted"].as_bool().unwrap_or(false) => {
                vec![match &f.reference["moved_cell"] {
                    Value::Null => json!([1, 1]),
                    m => m.clone(),
                }]
            }
            _ => vec![],
        };
        // An open case pins each side's answer apart: this is the Editor's.
        let want = case["expect"].get("editor").unwrap_or(&case["expect"]).clone();
        if Value::Array(got.clone()) != want {
            failed.push(format!("{}: got {got:?}, want {want}", case["name"]));
        }
        // From the cell it was moved to, it is found in the cell it left.
        if let Some(m) = got.first().filter(|m| **m != json!([1, 1])) {
            let there = format!("{},{}", m[0], m[1]);
            if s.find_ref(&there, "a.esm", 1, None).map(|f| f.cell) != Some("(1, 1)".into()) {
                failed.push(format!("{}: not found from {there}", case["name"]));
            }
        }
    }
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}

/// One case of `queue_cases.json` through editcore::queue: what the case checks after
/// its operations (the shape `tests/test_queue_cases.py`'s `run` gives).
fn run_queue(case: &Value, tmp: &Path) -> Value {
    use editcore::queue::{Choice, Fields, History, NewRecord, NewRef, PatchQueue, RefEdit, Selection};
    let mut q = PatchQueue::new();
    let mut h = History::new(&q);
    let mut snaps: HashMap<String, editcore::queue::Decisions> = HashMap::new();
    let (mut restored, mut stepped) = (Vec::new(), Vec::new());
    let s = |v: &Value| v.as_str().unwrap().to_string();
    let list = |v: &Value| v.as_array().unwrap().iter().map(|x| x.as_str().unwrap().to_string()).collect::<Vec<_>>();
    for op in case["ops"].as_array().unwrap() {
        let a = &op.as_array().unwrap()[1..];
        match op[0].as_str().unwrap() {
            "whole" => q.add_whole(Selection { plugin: s(&a[2]), record_type: s(&a[0]), key: s(&a[1]) }),
            "field" => {
                let choice = match a[3].get("value") {
                    Some(v) => Choice::Value { path: s(&a[2]), value: v.clone() },
                    None => Choice::Plugin { path: s(&a[2]), plugin: s(&a[3]["plugin"]) },
                };
                q.add_field(&s(&a[0]), &s(&a[1]), choice);
            }
            "base" => q.set_base(&s(&a[0]), &s(&a[1]), &s(&a[2])),
            "remove_record" => q.remove_record(&s(&a[0]), &s(&a[1])),
            "remove_field" => q.remove_field(&s(&a[0]), &s(&a[1]), &s(&a[2])),
            "ref" => q.add_ref_edit(RefEdit {
                cell: s(&a[0]),
                origin: s(&a[1]),
                refr_index: a[2].as_i64().unwrap(),
                changes: Fields::from_object(&a[3]),
                plugins: list(&a[4]),
            }),
            "remove_ref" => q.remove_ref_edit(&s(&a[0]), &s(&a[1]), a[2].as_i64().unwrap(), a[3].as_str()),
            "made" => q.add_new_record(NewRecord { record_type: s(&a[0]), key: s(&a[1]), record: a[2].clone(), source: s(&a[3]), topic: s(&a[4]) }),
            "remove_made" => q.remove_new_record(&s(&a[0]), &s(&a[1])),
            "new" => q.add_new_ref(NewRef {
                cell: s(&a[0]),
                uid: s(&a[1]),
                fields: Fields::from_object(&a[2]),
                base_plugin: s(&a[3]),
                plugins: list(&a[4]),
                tag: s(&a[5]),
            }),
            "remove_new" => q.remove_new_ref(&s(&a[0]), &s(&a[1])),
            "snapshot" => {
                snaps.insert(s(&a[0]), q.snapshot());
            }
            "restore" => q.restore_snapshot(&snaps[&s(&a[0])]),
            "consume" => q.consume(&snaps[&s(&a[0])]),
            "clear" => q.clear(),
            "restore_journal" => {
                let journal = tmp.join("journal.json");
                std::fs::write(&journal, a[0].to_string()).unwrap();
                restored.push(q.restore(&journal));
            }
            "step" => h.record(&q),
            "undo" => stepped.push(h.step(&mut q, false)),
            "redo" => stepped.push(h.step(&mut q, true)),
            other => panic!("unknown operation {other}"),
        }
    }
    let (undo, redo) = h.counts();
    json!({
        "journal": q.journal(),
        "len": q.len(),
        "revision": q.revision,
        "restored": restored,
        "stepped": stepped,
        "history": [undo, redo],
    })
}

#[test]
fn queue_cases_queue_as_the_python_editor_does() {
    let tmp = std::env::temp_dir().join(format!("wg-queue-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    let mut failed = Vec::new();
    for case in cases("queue_cases.json") {
        let got = run_queue(&case, &tmp);
        if got != case["expect"] {
            failed.push(format!("{}:\n  got  {got}\n  want {}", case["name"], case["expect"]));
        }
    }
    let _ = std::fs::remove_dir_all(&tmp);
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}

#[test]
fn a_journal_written_here_is_the_journal_read_here() {
    use editcore::queue::{Choice, PatchQueue, Selection};
    let tmp = std::env::temp_dir().join(format!("wg-journal-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    let journal = tmp.join("j.json");
    let mut q = PatchQueue::new();
    q.save(&journal).unwrap();
    assert!(!journal.exists(), "an empty queue writes no journal");
    q.add_whole(Selection { plugin: "A.esp".into(), record_type: "Weapon".into(), key: "w".into() });
    q.add_field("Npc", "n", Choice::Value { path: "name".into(), value: json!("x") });
    q.save(&journal).unwrap();
    let mut back = PatchQueue::new();
    assert_eq!(back.restore(&journal), 2);
    assert_eq!(back.journal(), q.journal());
    q.clear();
    q.save(&journal).unwrap();
    assert!(!journal.exists(), "emptied, the journal goes");
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn a_record_the_patch_makes_is_found_without_reading_a_plugin() {
    use editcore::queue::{NewRecord, PatchQueue};
    let reads = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let seen = reads.clone();
    let s = Session::with_reader(
        vec![("A.esm".into(), PathBuf::from("A.esm"))],
        Box::new(move |_, _| {
            seen.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(vec![json!({"type": "Weapon", "id": "iron", "flags": ""})])
        }),
    );
    let mut q = PatchQueue::new();
    q.add_new_record(NewRecord { record_type: "Weapon".into(), key: "My_Sword".into(), record: json!({"type": "Weapon", "id": "My_Sword"}), source: "A.esm".into(), topic: String::new() });
    let f = s.find_in("WEAP", "my_sword", None, Some(&q)).unwrap();
    assert!(f.new);
    assert_eq!((f.winner(), f.key.as_str()), (editcore::session::PATCH, "My_Sword"));
    assert_eq!(reads.load(std::sync::atomic::Ordering::SeqCst), 0, "no plugin read for it");
    let f = s.find_in("WEAP", "iron", None, Some(&q)).unwrap();
    assert!(!f.new);
    assert_eq!(f.winner(), "A.esm");
}

/// A packed field's bytes: base64, zstd'd or not.
fn packed(v: &Value) -> Vec<u8> {
    use base64::Engine as _;
    let raw = base64::engine::general_purpose::STANDARD.decode(v.as_str().unwrap()).unwrap();
    if raw.starts_with(&[0x28, 0xB5, 0x2F, 0xFD]) { zstd::decode_all(raw.as_slice()).unwrap() } else { raw }
}

#[test]
fn the_crate_makes_the_blank_records_the_python_editor_made() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/blank_records.json");
    let doc: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut failed = Vec::new();
    for (t, want) in doc["blanks"].as_object().unwrap() {
        let Some(mut got) = editcore::blank::blank(t) else {
            failed.push(format!("{t}: the crate has no such type"));
            continue;
        };
        let mut want = want.clone();
        for field in ["variables", "bytecode"] {
            if let (Some(a), Some(b)) = (got.get(field).cloned(), want.get(field).cloned())
                && packed(&a) == packed(&b)
            {
                got[field] = Value::Null;
                want[field] = Value::Null;
            }
        }
        if got != want {
            failed.push(format!("{t}:\n  crate  {got}\n  python {want}"));
        }
    }
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}

/// One case of `edit_cases.json` through editcore::edit (the shape
/// `tests/test_edit_cases.py`'s `run` gives).
fn run_edit(case: &Value) -> Value {
    use editcore::edit::Editor;
    use editcore::queue::PatchQueue;
    let plugins = case["plugins"].as_array().unwrap();
    let names: Vec<String> = plugins.iter().map(|p| p["name"].as_str().unwrap().to_string()).collect();
    let by: HashMap<String, Vec<Value>> = plugins.iter().map(|p| (p["name"].as_str().unwrap().to_string(), p["records"].as_array().unwrap().clone())).collect();
    let s = Session::with_reader(
        names.iter().map(|n| (n.clone(), PathBuf::from(n))).collect(),
        Box::new(move |path, tag| {
            let want = type_of(tag);
            Ok(by[&path.to_string_lossy().to_string()].iter().filter(|r| Some(r["type"].as_str().unwrap()) == want).cloned().collect())
        }),
    );
    let mut ed = Editor::new(s, PatchQueue::new(), None);
    let s = |v: &Value| v.as_str().unwrap().to_string();
    let mut results = Vec::new();
    for op in case["ops"].as_array().unwrap() {
        let a = &op.as_array().unwrap()[1..];
        let out: Result<Value, editcore::edit::EditError> = match op[0].as_str().unwrap() {
            "view" => Ok(ed.find(&s(&a[0]), &s(&a[1]), None).map(|f| ed.view(&f)).unwrap_or(Value::Null)),
            "set" => match ed.find(&s(&a[0]), &s(&a[1]), None) {
                None => Ok(json!("not found")),
                Some(f) => ed.set_field(&f, &s(&a[2]), &a[3]).map(|()| json!("ok")),
            },
            "set_many" => {
                let founds: Vec<_> = a[1].as_array().unwrap().iter().filter_map(|i| ed.find(&s(&a[0]), &s(i), None)).collect();
                Ok(ed.set_many(&founds, &s(&a[2]), &a[3]))
            }
            "revert" => match ed.find(&s(&a[0]), &s(&a[1]), None) {
                None => Ok(json!("not found")),
                Some(f) => ed.revert(&f, a[2].as_str()).map(|()| json!("ok")),
            },
            "insert" => ed.insert(&s(&a[0]), &s(&a[1])).map(|f| json!(f.key)),
            "duplicate" => match ed.find(&s(&a[0]), &s(&a[1]), None) {
                None => Ok(json!("not found")),
                Some(f) => ed.duplicate(&f, &s(&a[2])).map(|f| json!(f.key)),
            },
            "pending" => Ok(Value::Array(ed.pending())),
            "undo" => Ok(ed.undo()),
            "redo" => Ok(ed.redo()),
            other => panic!("unknown operation {other}"),
        };
        results.push(out.unwrap_or_else(|e| json!({"error": e.0})));
    }
    // A script's packed fields are left out, as the Python runner leaves them out (their
    // encoding depends on the zstd backend; `blank_records.json` checks them decoded).
    let mut journal = ed.queue.journal();
    for made in journal["made"].as_array_mut().into_iter().flatten() {
        if made["type"] == "Script" {
            for field in ["variables", "bytecode"] {
                if made["record"].get(field).is_some() {
                    made["record"][field] = json!("<packed>");
                }
            }
        }
    }
    json!({"results": results, "journal": journal})
}

/// Where two values first differ, for a failure message.
fn first_difference(a: &Value, b: &Value, at: &str) -> Option<String> {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            for k in x.keys().chain(y.keys()) {
                if let Some(d) = first_difference(x.get(k).unwrap_or(&Value::Null), y.get(k).unwrap_or(&Value::Null), &format!("{at}.{k}")) {
                    return Some(d);
                }
            }
            None
        }
        (Value::Array(x), Value::Array(y)) if x.len() == y.len() => {
            x.iter().zip(y).enumerate().find_map(|(i, (p, q))| first_difference(p, q, &format!("{at}[{i}]")))
        }
        _ => (a != b).then(|| format!("{at}: got {a}, want {b}")),
    }
}

#[test]
fn edit_cases_edit_as_the_python_editor_does() {
    let mut failed = Vec::new();
    for case in cases("edit_cases.json") {
        let got = run_edit(&case);
        assert_eq!(got["results"].as_array().map(Vec::len), case["ops"].as_array().map(Vec::len));
        if got != case["expect"] {
            let d = first_difference(&got, &case["expect"], "").unwrap_or_else(|| "a key missing on one side".into());
            failed.push(format!("{}: {d}", case["name"]));
        }
    }
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}

/// A landscape's packed grids as `sha256:` of their bytes (`test_place_cases.packed`).
fn packed_land(record: &mut Value) {
    use sha2::{Digest, Sha256};
    if record["type"] != "Landscape" {
        return;
    }
    for name in ["vertex_normals", "vertex_heights", "world_map_data", "vertex_colors", "texture_indices"] {
        if let Some(Value::String(t)) = record.get(name).and_then(|g| g.get("data")).cloned() {
            let digest: String = Sha256::digest(packed(&json!(t))).iter().map(|b| format!("{b:02x}")).collect();
            record[name]["data"] = json!(format!("sha256:{digest}"));
        }
    }
}

/// Every new reference's editor name in `v` as its label.
fn relabel(v: &mut Value, labels: &HashMap<String, String>) {
    match v {
        Value::String(s) => {
            if let Some(l) = labels.get(s.as_str()) {
                *s = l.clone();
            }
        }
        Value::Array(a) => a.iter_mut().for_each(|x| relabel(x, labels)),
        Value::Object(m) => m.values_mut().for_each(|x| relabel(x, labels)),
        _ => {}
    }
}

/// One case of `place_cases.json` through editcore::places (the shape
/// `tests/test_place_cases.py`'s `run` gives).
fn run_place(case: &Value) -> Value {
    use editcore::edit::Editor;
    use editcore::queue::PatchQueue;
    let plugins = case["plugins"].as_array().unwrap().clone();
    let names: Vec<String> = plugins.iter().map(|p| p["name"].as_str().unwrap().to_string()).collect();
    let by: HashMap<String, Value> = plugins.iter().map(|p| (p["name"].as_str().unwrap().to_string(), p.clone())).collect();
    let session = Session::with_reader(
        names.iter().map(|n| (n.clone(), PathBuf::from(n))).collect(),
        Box::new(move |path, tag| {
            let p = &by[&path.to_string_lossy().to_string()];
            if tag == "TES3" {
                return Ok(vec![header(p.get("masters").unwrap_or(&json!([])))]);
            }
            let want = type_of(tag);
            Ok(p["records"].as_array().unwrap().iter().filter(|r| Some(r["type"].as_str().unwrap()) == want).cloned().collect())
        }),
    );
    let mut ed = Editor::new(session, PatchQueue::new(), None);
    let (mut labels, mut uids): (HashMap<String, String>, HashMap<String, String>) = (HashMap::new(), HashMap::new());
    let s = |v: &Value| v.as_str().unwrap().to_string();
    let mut results = Vec::new();
    for op in case["ops"].as_array().unwrap() {
        let a = &op.as_array().unwrap()[1..];
        let found_ref = |ed: &Editor| ed.session.find_ref(&s(&a[0]), &s(&a[1]), a[2].as_i64().unwrap(), None).expect("a placed object");
        let new_ref = |ed: &Editor, label: &Value| ed.find_new(&uids[&s(label)]).expect("a new reference");
        let out: Result<Value, editcore::edit::EditError> = match op[0].as_str().unwrap() {
            "ref_view" => Ok(ed.session.find_ref(&s(&a[0]), &s(&a[1]), a[2].as_i64().unwrap(), None).map(|f| ed.ref_view(&f)).unwrap_or(Value::Null)),
            "set_ref" => {
                let f = found_ref(&ed);
                ed.set_ref_field(&f, &s(&a[3]), &a[4]).map(|()| json!("ok"))
            }
            "revert_ref" => {
                let f = found_ref(&ed);
                ed.revert_ref(&f, a[3].as_str());
                Ok(json!("ok"))
            }
            "place" => {
                let f = ed.find(&s(&a[1]), &s(&a[2]), None).unwrap();
                let rotation = (!a[4].is_null()).then(|| a[4].clone());
                ed.place(&s(&a[0]), &f, &a[3], rotation.as_ref(), None).map(|n| json!(n.uid))
            }
            "place_many" => {
                let items: Vec<_> = a[1]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|i| (ed.find(&s(&i[0]), &s(&i[1]), None).unwrap(), i[2].clone(), (!i[3].is_null()).then(|| i[3].clone()), i[4].as_f64().unwrap()))
                    .collect();
                ed.place_many(&s(&a[0]), &items).map(|v| json!(v.iter().map(|n| n.uid.clone()).collect::<Vec<_>>()))
            }
            "new_view" => Ok(ed.new_view(&new_ref(&ed, &a[0]))),
            "set_new" => {
                let n = new_ref(&ed, &a[0]);
                ed.set_new_field(&n, &s(&a[1]), &a[2]).map(|n| json!(n.cell))
            }
            "remove_new" => {
                let n = new_ref(&ed, &a[0]);
                ed.remove_new(&n);
                Ok(json!("ok"))
            }
            "new_cell" => ed.new_cell(&s(&a[0]), a[1].as_bool().unwrap()),
            "new_land" => ed.new_land(&s(&a[0]), a[1].as_f64().unwrap(), &s(&a[2]), a[3].as_bool().unwrap(), a[4].as_bool()),
            "duplicate_cell" => ed.duplicate_cell(&s(&a[0]), &s(&a[1])),
            "pathgrid_view" => Ok(ed.pathgrid_view(&s(&a[0]))),
            "set_pathgrid" => ed.set_pathgrid(&s(&a[0]), a[1].as_array().unwrap(), a[2].as_array().unwrap()),
            "revert_pathgrid" => Ok(ed.revert_pathgrid(&s(&a[0]))),
            "flat_land" => {
                let g = a[0].as_array().unwrap();
                editcore::places::flat_land((g[0].as_i64().unwrap() as i32, g[1].as_i64().unwrap() as i32), a[1].as_f64().unwrap(), a[2].as_i64().unwrap()).map(|mut r| {
                    packed_land(&mut r);
                    r
                })
            }
            "pending" => Ok(Value::Array(ed.pending())),
            "undo" => Ok(ed.undo()),
            other => panic!("unknown operation {other}"),
        };
        for n in ed.queue.new_refs() {
            if !labels.contains_key(&n.uid) {
                let l = format!("U{}", labels.len() + 1);
                labels.insert(n.uid.clone(), l.clone());
                uids.insert(l, n.uid.clone());
            }
        }
        results.push(out.unwrap_or_else(|e| json!({"error": e.0})));
    }
    let mut journal = ed.queue.journal();
    for made in journal["made"].as_array_mut().into_iter().flatten() {
        packed_land(&mut made["record"]);
    }
    let mut out = json!({"results": results, "journal": journal});
    relabel(&mut out, &labels);
    out
}

#[test]
fn place_cases_place_as_the_python_editor_does() {
    let mut failed = Vec::new();
    for case in cases("place_cases.json") {
        let got = run_place(&case);
        assert_eq!(got["results"].as_array().map(Vec::len), case["ops"].as_array().map(Vec::len));
        if got != case["expect"] {
            let d = first_difference(&got, &case["expect"], "").unwrap_or_else(|| "a key missing on one side".into());
            failed.push(format!("{}: {d}", case["name"]));
        }
    }
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}

/// One case of `search_cases.json` through editcore::search and editcore::columns (the
/// shape `tests/test_search_cases.py`'s `run` gives).
fn run_search(case: &Value) -> Value {
    use editcore::edit::Editor;
    use editcore::queue::{Choice, PatchQueue, Selection};
    let plugins = case["plugins"].as_array().unwrap();
    let names: Vec<String> = plugins.iter().map(|p| p["name"].as_str().unwrap().to_string()).collect();
    let by: HashMap<String, Vec<Value>> = plugins.iter().map(|p| (p["name"].as_str().unwrap().to_string(), p["records"].as_array().unwrap().clone())).collect();
    let session = Session::with_reader(
        names.iter().map(|n| (n.clone(), PathBuf::from(n))).collect(),
        Box::new(move |path, tag| {
            let want = type_of(tag);
            Ok(by[&path.to_string_lossy().to_string()].iter().filter(|r| Some(r["type"].as_str().unwrap()) == want).cloned().collect())
        }),
    );
    let mut ed = Editor::new(session, PatchQueue::new(), None);
    let s = |v: &Value| v.as_str().unwrap().to_string();
    let limit = |a: &[Value], i: usize, d: usize| a.get(i).and_then(Value::as_u64).map(|n| n as usize).unwrap_or(d);
    let mut last_hits: Vec<Value> = Vec::new();
    let mut results = Vec::new();
    for op in case["ops"].as_array().unwrap() {
        let a = &op.as_array().unwrap()[1..];
        let out: Result<Value, editcore::edit::EditError> = match op[0].as_str().unwrap() {
            "set" => {
                let f = ed.find(&s(&a[0]), &s(&a[1]), None).unwrap();
                ed.set_field(&f, &s(&a[2]), &a[3]).map(|()| json!("ok"))
            }
            "choose" => {
                ed.queue.add_field(type_of(&s(&a[0])).unwrap(), &s(&a[1]), Choice::Plugin { path: s(&a[2]), plugin: s(&a[3]) });
                Ok(json!("ok"))
            }
            "whole" => {
                ed.queue.add_whole(Selection { plugin: s(&a[2]), record_type: type_of(&s(&a[0])).unwrap().into(), key: s(&a[1]) });
                Ok(json!("ok"))
            }
            "insert" => ed.insert(&s(&a[0]), &s(&a[1])).map(|f| json!(f.key)),
            "search" => ed.search(&s(&a[0]), &s(&a[1]), limit(a, 2, 20000)),
            "columns" => Ok(ed.columns(&s(&a[0]))),
            "search_all" => {
                let out = ed.search_all(&s(&a[0]), a[1].as_bool().unwrap(), limit(a, 2, 2000));
                last_hits = out["hits"].as_array().unwrap().clone();
                Ok(out)
            }
            "replace_all" => {
                let hits = if a[0] == "last" { last_hits.clone() } else { a[0].as_array().unwrap().clone() };
                Ok(ed.replace_all(&hits, &s(&a[1]), &s(&a[2]), a[3].as_bool().unwrap()))
            }
            "filter_choices" => Ok(editcore::fields::filter_choices()),
            "pending" => Ok(Value::Array(ed.pending())),
            other => panic!("unknown operation {other}"),
        };
        results.push(out.unwrap_or_else(|e| json!({"error": e.0})));
    }
    json!({"results": results, "journal": ed.queue.journal()})
}

#[test]
fn search_cases_search_as_the_python_editor_does() {
    let mut failed = Vec::new();
    for case in cases("search_cases.json") {
        let got = run_search(&case);
        assert_eq!(got["results"].as_array().map(Vec::len), case["ops"].as_array().map(Vec::len));
        if got != case["expect"] {
            let d = first_difference(&got, &case["expect"], "").unwrap_or_else(|| "a key missing on one side".into());
            failed.push(format!("{}: {d}", case["name"]));
        }
        // Order counts too: hits and rows are listed as the Python lists them.
        let (g, w) = (got.to_string(), case["expect"].to_string());
        if g != w && got == case["expect"] {
            let at = g.bytes().zip(w.bytes()).position(|(x, y)| x != y).unwrap_or(0);
            let from = at.saturating_sub(120);
            failed.push(format!("{}: the same answer in another order, at\n  got  ...{}\n  want ...{}", case["name"], &g[from..(at + 80).min(g.len())], &w[from..(at + 80).min(w.len())]));
        }
    }
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}

/// The cases' script compiler (`StubCompiler` in tests/test_use_cases.py too): a script
/// compiles unless its text says `nocompile`, to a record the case can tell apart.
struct StubCompiler;

impl editcore::scripts::Compile for StubCompiler {
    fn compile(&self, text: &str) -> editcore::scripts::Compiled {
        let ok = !text.to_lowercase().contains("nocompile");
        let messages = if ok { Vec::new() } else { vec![json!({"line": 1, "level": "error", "message": "stub: does not compile"})] };
        editcore::scripts::Compiled { ok, messages }
    }
    fn compile_record(&self, key: &str, text: &str) -> Option<Value> {
        Some(json!({
            "header": {"num_shorts": 1, "num_longs": 0, "num_floats": 0, "bytecode_length": text.chars().count(), "variables_length": 2},
            "variables": "eAA=",
            "bytecode": format!("compiled:{key}"),
        }))
    }
}

/// A plugin of a case read whole: its header (from its masters), then its records.
fn read_whole(p: &Value) -> Vec<Value> {
    let mut all = vec![header(p.get("masters").unwrap_or(&json!([])))];
    all.extend(p["records"].as_array().unwrap().iter().cloned());
    all
}

/// One case of `use_cases.json` through editcore::uses, editcore::rename and
/// editcore::scripts (the shape `tests/test_use_cases.py`'s `run` gives).
fn run_use(case: &Value) -> Value {
    use editcore::edit::Editor;
    use editcore::queue::PatchQueue;
    use editcore::uses::{replace_summary, Repoint};
    let plugins = case["plugins"].as_array().unwrap().clone();
    let names: Vec<String> = plugins.iter().map(|p| p["name"].as_str().unwrap().to_string()).collect();
    let by: HashMap<String, Value> = plugins.iter().map(|p| (p["name"].as_str().unwrap().to_string(), p.clone())).collect();
    let whole = by.clone();
    let session = Session::with_reader(
        names.iter().map(|n| (n.clone(), PathBuf::from(n))).collect(),
        Box::new(move |path, tag| {
            let p = &by[&path.to_string_lossy().to_string()];
            if tag == "TES3" {
                return Ok(vec![header(p.get("masters").unwrap_or(&json!([])))]);
            }
            let want = type_of(tag);
            Ok(p["records"].as_array().unwrap().iter().filter(|r| Some(r["type"].as_str().unwrap()) == want).cloned().collect())
        }),
    )
    .with_read_all(Box::new(move |path| Ok(read_whole(&whole[&path.to_string_lossy().to_string()]))));
    let mut ed = Editor::new(session, PatchQueue::new(), None);
    let mut labels: HashMap<String, String> = HashMap::new();
    let s = |v: &Value| v.as_str().unwrap().to_string();
    let cell = |v: &Value| v.as_str().map(str::to_string);
    let mut results = Vec::new();
    for op in case["ops"].as_array().unwrap() {
        let a = &op.as_array().unwrap()[1..];
        let find = |ed: &Editor, tag: &Value, id: &Value| ed.find(&s(tag), &s(id), None).expect("a record of the case");
        let out: Result<Value, editcore::edit::EditError> = match op[0].as_str().unwrap() {
            "uses" => Ok(ed.uses(&find(&ed, &a[0], &a[1]))),
            "replace" => {
                let f = find(&ed, &a[0], &a[1]);
                ed.replace_plan(&f, &s(&a[2]), cell(&a[3]).as_deref())
                    .and_then(|(plan, _report)| ed.replace_uses(&plan, &StubCompiler).map(|n| replace_summary(&plan, n)))
            }
            "plan" => {
                let f = find(&ed, &a[0], &a[1]);
                ed.replace_plan(&f, &s(&a[2]), cell(&a[3]).as_deref()).map(|(plan, _)| {
                    json!(plan
                        .iter()
                        .map(|p| match p {
                            Repoint::Field { user, field, value } => json!({"field": [user.tag, user.key, field, value]}),
                            Repoint::Ref(r) => json!({"ref": [r.cell, r.origin, r.refr_index, r.changes.to_object()]}),
                            Repoint::New(n) => json!({"new": [n.cell, n.uid, n.fields.to_object()]}),
                        })
                        .collect::<Vec<_>>())
                })
            }
            "script_text" => {
                let f = ed.find("SCPT", &s(&a[0]), None).unwrap();
                ed.set_script_text(&f, &s(&a[1]), &StubCompiler)
            }
            "place" => {
                let f = find(&ed, &a[1], &a[2]);
                ed.place(&s(&a[0]), &f, &a[3], None, None).map(|n| json!(n.uid))
            }
            "index" => {
                let p = plugins.iter().find(|p| p["name"] == a[0]).unwrap();
                Ok(json!(editcore::uses::index_rows(&read_whole(p)).into_iter().map(|(v, t, k, ps, n)| json!([v, t, k, ps, n])).collect::<Vec<_>>()))
            }
            "rename" => {
                let (text, n) = editcore::rename::replace_id(&s(&a[0]), &s(&a[1]), &s(&a[2]));
                Ok(json!([text, n]))
            }
            "pending" => Ok(Value::Array(ed.pending())),
            "can_undo" => Ok(ed.can_undo()),
            "undo" => Ok(ed.undo()),
            other => panic!("unknown operation {other}"),
        };
        for n in ed.queue.new_refs() {
            if !labels.contains_key(&n.uid) {
                let l = format!("U{}", labels.len() + 1);
                labels.insert(n.uid.clone(), l);
            }
        }
        results.push(out.unwrap_or_else(|e| json!({"error": e.0})));
    }
    let mut out = json!({"results": results, "journal": ed.queue.journal()});
    relabel(&mut out, &labels);
    out
}

#[test]
fn use_cases_report_and_replace_as_the_python_editor_does() {
    let mut failed = Vec::new();
    for case in cases("use_cases.json") {
        let got = run_use(&case);
        assert_eq!(got["results"].as_array().map(Vec::len), case["ops"].as_array().map(Vec::len));
        if got != case["expect"] {
            let d = first_difference(&got, &case["expect"], "").unwrap_or_else(|| "a key missing on one side".into());
            failed.push(format!("{}: {d}", case["name"]));
        }
        let (g, w) = (got.to_string(), case["expect"].to_string());
        if g != w && got == case["expect"] {
            let at = g.bytes().zip(w.bytes()).position(|(x, y)| x != y).unwrap_or(0);
            let from = at.saturating_sub(120);
            failed.push(format!("{}: the same answer in another order, at\n  got  ...{}\n  want ...{}", case["name"], &g[from..(at + 80).min(g.len())], &w[from..(at + 80).min(w.len())]));
        }
    }
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}

/// The use index answers each case's Use Report as the records read do (the same uses,
/// the same versions winning): its rows from `index_rows`, queried as the Editor does.
#[test]
fn the_use_index_answers_as_reading_the_records_does() {
    use editcore::cache::Cache;
    use editcore::uses::{from_index, report_with, Use};
    for case in cases("use_cases.json") {
        let plugins = case["plugins"].as_array().unwrap();
        let dir = std::env::temp_dir().join(format!("wg-use-index-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let paths: Vec<PathBuf> = plugins
            .iter()
            .map(|p| {
                let path = dir.join(p["name"].as_str().unwrap());
                std::fs::write(&path, p["name"].as_str().unwrap()).unwrap();
                path
            })
            .collect();
        let cache = Cache::new(dir.join("c.sqlite3"), "", editcore::cache::MAX_BYTES);
        for (p, path) in plugins.iter().zip(&paths) {
            cache.put_uses(path, &editcore::uses::index_rows(&read_whole(p)));
        }
        let sorted = |mut v: Vec<Use>| {
            v.sort_by(|a, b| (a.plugin, &a.record_type, &a.key).cmp(&(b.plugin, &b.record_type, &b.key)));
            v
        };
        for op in case["ops"].as_array().unwrap().iter().filter(|o| o[0] == "uses") {
            let (tag, rid) = (op[1].as_str().unwrap(), op[2].as_str().unwrap());
            let rtype = type_of(tag).unwrap();
            let read = report_with(plugins.len(), |i| Ok(read_whole(&plugins[i])), rtype, rid);
            let sources = |i: usize| {
                let recs = read_whole(&plugins[i]);
                let of = |t: &str| recs.iter().filter(|r| r["type"] == t).cloned().collect::<Vec<_>>();
                [of("Script"), of("DialogueInfo")].concat()
            };
            let indexed = from_index(&cache, &paths, rtype, rid, sources).expect("every plugin indexed");
            assert_eq!(sorted(indexed), sorted(read), "{rid}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// One case of `dialogue_cases.json` through editcore::dialogue and editcore::dialogue_graph (the shape
/// `tests/test_dialogue_cases.py`'s `run` gives).
fn run_dialogue(case: &Value) -> Value {
    use editcore::edit::Editor;
    use editcore::queue::PatchQueue;
    let plugins = case["plugins"].as_array().unwrap().clone();
    let names: Vec<String> = plugins.iter().map(|p| p["name"].as_str().unwrap().to_string()).collect();
    let given: std::collections::HashSet<String> = plugins.iter().flat_map(|p| p["records"].as_array().unwrap().iter().map(|r| r["id"].as_str().unwrap_or("").to_string())).collect();
    let by: HashMap<String, Value> = plugins.iter().map(|p| (p["name"].as_str().unwrap().to_string(), p.clone())).collect();
    let dial = by.clone();
    let session = Session::with_reader(
        names.iter().map(|n| (n.clone(), PathBuf::from(n))).collect(),
        Box::new(move |path, tag| {
            let p = &by[&path.to_string_lossy().to_string()];
            if tag == "TES3" {
                return Ok(vec![header(p.get("masters").unwrap_or(&json!([])))]);
            }
            let want = type_of(tag);
            Ok(p["records"].as_array().unwrap().iter().filter(|r| Some(r["type"].as_str().unwrap()) == want).cloned().collect())
        }),
    )
    .with_read_dialogue(Box::new(move |path| {
        let p = &dial[&path.to_string_lossy().to_string()];
        Ok(p["records"].as_array().unwrap().iter().filter(|r| r["type"] == "Dialogue" || r["type"] == "DialogueInfo").cloned().collect())
    }));
    let mut ed = Editor::new(session, PatchQueue::new(), None);
    let (mut labels, mut ids): (HashMap<String, String>, HashMap<String, String>) = (HashMap::new(), HashMap::new());
    let s = |v: &Value| v.as_str().unwrap().to_string();
    let mut results = Vec::new();
    for op in case["ops"].as_array().unwrap() {
        let a = &op.as_array().unwrap()[1..];
        let rid = |v: &Value| ids.get(&s(v)).cloned().unwrap_or_else(|| s(v));
        let out: Result<Value, editcore::edit::EditError> = match op[0].as_str().unwrap() {
            "topics" => Ok(json!(ed.topics())),
            "topic" => ed.topic(&s(&a[0])),
            "move_response" => {
                let (info, after) = (rid(&a[1]), rid(&a[2]));
                ed.move_response(&s(&a[0]), &info, &after)
            }
            "new_response" => {
                let after = rid(&a[1]);
                ed.new_response(&s(&a[0]), &after).map(|f| ed.view(&f))
            }
            "copy_topic" => ed.copy_topic(&s(&a[0]), &s(&a[1])),
            "set" => {
                let f = ed.find(&s(&a[0]), &rid(&a[1]), None).unwrap();
                ed.set_field(&f, &s(&a[2]), &a[3]).map(|()| json!("ok"))
            }
            "flow" => ed.dialogue_index().flow(&s(&a[0])),
            "map" => ed.dialogue_index().neighbourhood(&s(&a[0]), a[1].as_i64().unwrap()),
            "flags" => ed.dialogue_index().flags(&s(&a[0]), &s(&a[1])),
            "play" => ed.dialogue_play(&s(&a[0]), &s(&a[1]), a[2].as_i64().unwrap(), &s(&a[3]), a[4].as_i64().unwrap(), a[5].as_bool().unwrap()),
            "who_can_say" => ed.who_can_say(&rid(&a[0]), 500),
            "pending" => Ok(Value::Array(ed.pending())),
            "undo" => Ok(ed.undo()),
            other => panic!("unknown operation {other}"),
        };
        for made in ed.queue.new_records() {
            if made.record_type == "DialogueInfo" && !given.contains(&made.key) && !labels.contains_key(&made.key) {
                let l = format!("R{}", labels.len() + 1);
                labels.insert(made.key.clone(), l.clone());
                ids.insert(l, made.key.clone());
            }
        }
        results.push(out.unwrap_or_else(|e| json!({"error": e.0})));
    }
    let mut out = json!({"results": results, "journal": ed.queue.journal()});
    relabel(&mut out, &labels);
    out
}

#[test]
fn dialogue_cases_read_and_change_dialogue_as_the_python_editor_does() {
    let mut failed = Vec::new();
    for case in cases("dialogue_cases.json") {
        let got = run_dialogue(&case);
        assert_eq!(got["results"].as_array().map(Vec::len), case["ops"].as_array().map(Vec::len));
        if got != case["expect"] {
            let d = first_difference(&got, &case["expect"], "").unwrap_or_else(|| "a key missing on one side".into());
            failed.push(format!("{}: {d}", case["name"]));
        }
        let (g, w) = (got.to_string(), case["expect"].to_string());
        if g != w && got == case["expect"] {
            let at = g.bytes().zip(w.bytes()).position(|(x, y)| x != y).unwrap_or(0);
            let from = at.saturating_sub(120);
            failed.push(format!("{}: the same answer in another order, at\n  got  ...{}\n  want ...{}", case["name"], &g[from..(at + 80).min(g.len())], &w[from..(at + 80).min(w.len())]));
        }
    }
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}

/// Each plugin of a case written for the compiler to read (`write_plugins` in
/// tests/test_script_cases.py): every record with an id laid into a first compiler, which
/// compiles each script (weakly) into the record the writer takes; one that does not
/// compile is left out of the file.
fn write_plugins(case: &Value, folder: &Path) {
    use editcore::mwscript::ScriptCompiler;
    const KINDS: [&str; 5] = ["Topic", "Voice", "Greeting", "Persuasion", "Journal"];
    let mut extra = Vec::new();
    for p in case["plugins"].as_array().unwrap() {
        for r in p["records"].as_array().unwrap() {
            let id = r["id"].as_str().unwrap_or("");
            let Some(tag) = editcore::key::tag_of(r["type"].as_str().unwrap()) else { continue };
            if id.is_empty() {
                continue;
            }
            let kind = r["dialogue_type"].as_str().filter(|k| !k.is_empty()).unwrap_or("Topic");
            let dial = if tag == "DIAL" { KINDS.iter().position(|k| *k == kind).unwrap_or(0) as i32 } else { -1 };
            extra.push((tag.to_string(), id.to_string(), r["script"].as_str().unwrap_or("").to_string(), dial, Vec::new(), Vec::new(), Vec::new()));
        }
    }
    let compiler = ScriptCompiler::new(&[], &extra, None);
    for p in case["plugins"].as_array().unwrap() {
        let mut head = editcore::blank::blank("Header").unwrap();
        head["masters"] = json!(p["masters"].as_array().unwrap().iter().map(|m| json!([m, 0])).collect::<Vec<_>>());
        let mut out = vec![head];
        for r in p["records"].as_array().unwrap() {
            if r["type"] == "Script" {
                if let Some(made) = compiler.compile_record(r["id"].as_str().unwrap(), r["text"].as_str().unwrap(), "weak", true, true, true) {
                    out.push(made);
                }
                continue;
            }
            out.push(r.clone());
        }
        let bytes = editcore::plugin::records_bytes(&Value::Array(out).to_string()).unwrap();
        std::fs::write(folder.join(p["name"].as_str().unwrap()), bytes).unwrap();
    }
}

/// One case of `script_cases.json` through editcore::mwscript and editcore::scripts (the
/// shape `tests/test_script_cases.py`'s `run` gives).
fn run_script(case: &Value, folder: &Path) -> Value {
    use editcore::edit::Editor;
    use editcore::queue::PatchQueue;
    write_plugins(case, folder);
    let plugins = case["plugins"].as_array().unwrap().clone();
    let by: HashMap<String, Value> = plugins.iter().map(|p| (p["name"].as_str().unwrap().to_string(), p.clone())).collect();
    let session = Session::with_reader(
        plugins.iter().map(|p| (p["name"].as_str().unwrap().to_string(), folder.join(p["name"].as_str().unwrap()))).collect(),
        Box::new(move |path, tag| {
            let p = &by[&path.file_name().unwrap().to_string_lossy().to_string()];
            if tag == "TES3" {
                return Ok(vec![header(p.get("masters").unwrap_or(&json!([])))]);
            }
            let want = type_of(tag);
            Ok(p["records"].as_array().unwrap().iter().filter(|r| Some(r["type"].as_str().unwrap()) == want).cloned().collect())
        }),
    );
    let mut ed = Editor::new(session, PatchQueue::new(), None);
    let s = |v: &Value| v.as_str().unwrap().to_string();
    let mut results = Vec::new();
    for op in case["ops"].as_array().unwrap() {
        let a = &op.as_array().unwrap()[1..];
        let find = |ed: &Editor, tag: &Value, id: &Value| ed.find(&s(tag), &s(id), None).expect("a record of the case");
        let out: Result<Value, editcore::edit::EditError> = match op[0].as_str().unwrap() {
            "script" => {
                let f = find(&ed, &a[0], &a[1]);
                ed.edit_script(&f, a[2].as_str())
            }
            "save_script" => {
                let f = find(&ed, &a[0], &a[1]);
                let compiler = ed.script_compiler();
                ed.set_script_text(&f, &s(&a[2]), &*compiler)
            }
            "result" => {
                let f = ed.find("INFO", &s(&a[0]), None).unwrap();
                Ok(ed.edit_result(&f, a[1].as_str()))
            }
            "words" => Ok(ed.script_words()),
            "refs" => Ok(ed.script_refs(&s(&a[0]), 1000)),
            "flowchart" => {
                let f = find(&ed, &a[0], &a[1]);
                Ok(ed.script_flowchart(&f, a[2].as_str()))
            }
            "roll" => ed.edit_leveled_roll(&s(&a[0]), &s(&a[1]), &a[2]),
            "lua" => editcore::scripts::lua_chart(&s(&a[0]), &s(&a[1]), &s(&a[2]), a[3].as_i64()),
            "listing" => Ok(json!(editcore::mwscript::listing::listing_for_bytecode_field(&a[0], a[1].as_str()))),
            "insert" => ed.insert(&s(&a[0]), &s(&a[1])).map(|f| json!(f.key)),
            "duplicate" => {
                let f = find(&ed, &a[0], &a[1]);
                ed.duplicate(&f, &s(&a[2])).map(|f| json!(f.key))
            }
            "pending" => Ok(Value::Array(ed.pending())),
            other => panic!("unknown operation {other}"),
        };
        results.push(out.unwrap_or_else(|e| json!({"error": e.0})));
    }
    json!({"results": results, "journal": ed.queue.journal()})
}

#[test]
fn script_cases_read_scripts_as_the_python_editor_does() {
    let mut failed = Vec::new();
    for (n, case) in cases("script_cases.json").iter().enumerate() {
        let folder = std::env::temp_dir().join(format!("wg-script-cases-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        let got = run_script(case, &folder);
        let _ = std::fs::remove_dir_all(&folder);
        assert_eq!(got["results"].as_array().map(Vec::len), case["ops"].as_array().map(Vec::len));
        if got != case["expect"] {
            let d = first_difference(&got, &case["expect"], "").unwrap_or_else(|| "a key missing on one side".into());
            failed.push(format!("{}: {d}", case["name"]));
        }
        let (g, w) = (got.to_string(), case["expect"].to_string());
        if g != w && got == case["expect"] {
            let at = g.bytes().zip(w.bytes()).position(|(x, y)| x != y).unwrap_or(0);
            let from = at.saturating_sub(120);
            failed.push(format!("{}: the same answer in another order, at\n  got  ...{}\n  want ...{}", case["name"], &g[from..(at + 80).min(g.len())], &w[from..(at + 80).min(w.len())]));
        }
    }
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}

/// Each plugin of a verify case written for the compiler (`write_plugins` in
/// tests/test_verify_cases.py): the records with an id (not land, path grids or cells,
/// whose packed fields are the crate's own), a script as the compiler makes it.
fn write_verify_plugins(case: &Value, folder: &Path) {
    use editcore::mwscript::ScriptCompiler;
    let mut extra = Vec::new();
    for p in case["plugins"].as_array().unwrap() {
        for r in p["records"].as_array().unwrap() {
            let id = r["id"].as_str().unwrap_or("");
            let Some(tag) = editcore::key::tag_of(r["type"].as_str().unwrap()) else { continue };
            if !id.is_empty() {
                extra.push((tag.to_string(), id.to_string(), r["script"].as_str().unwrap_or("").to_string(), -1, Vec::new(), Vec::new(), Vec::new()));
            }
        }
    }
    let compiler = ScriptCompiler::new(&[], &extra, None);
    for p in case["plugins"].as_array().unwrap() {
        let mut head = editcore::blank::blank("Header").unwrap();
        head["masters"] = json!(p["masters"].as_array().unwrap().iter().map(|m| json!([m, 0])).collect::<Vec<_>>());
        let mut out = vec![head];
        for r in p["records"].as_array().unwrap() {
            match r["type"].as_str().unwrap() {
                "Script" => out.extend(compiler.compile_record(r["id"].as_str().unwrap(), r["text"].as_str().unwrap(), "weak", true, true, true)),
                "Landscape" | "PathGrid" | "Cell" => {}
                _ => out.push(r.clone()),
            }
        }
        std::fs::write(folder.join(p["name"].as_str().unwrap()), editcore::plugin::records_bytes(&Value::Array(out).to_string()).unwrap()).unwrap();
    }
}

/// One case of `verify_cases.json` through editcore::verify (the shape
/// `tests/test_verify_cases.py`'s `run` gives).
fn run_verify(case: &Value, folder: &Path) -> Value {
    use editcore::edit::Editor;
    use editcore::queue::{PatchQueue, Selection};
    write_verify_plugins(case, folder);
    let plugins = case["plugins"].as_array().unwrap().clone();
    let by: HashMap<String, Value> = plugins.iter().map(|p| (p["name"].as_str().unwrap().to_string(), p.clone())).collect();
    let dial = by.clone();
    let files: std::collections::HashSet<String> = case["files"].as_array().into_iter().flatten().map(|f| f.as_str().unwrap().to_lowercase()).collect();
    let session = Session::with_reader(
        plugins.iter().map(|p| (p["name"].as_str().unwrap().to_string(), folder.join(p["name"].as_str().unwrap()))).collect(),
        Box::new(move |path, tag| {
            let p = &by[&path.file_name().unwrap().to_string_lossy().to_string()];
            if tag == "TES3" {
                return Ok(vec![header(p.get("masters").unwrap_or(&json!([])))]);
            }
            let want = type_of(tag);
            Ok(p["records"].as_array().unwrap().iter().filter(|r| Some(r["type"].as_str().unwrap()) == want).cloned().collect())
        }),
    )
    .with_read_dialogue(Box::new(move |path| {
        let p = &dial[&path.file_name().unwrap().to_string_lossy().to_string()];
        Ok(p["records"].as_array().unwrap().iter().filter(|r| r["type"] == "Dialogue" || r["type"] == "DialogueInfo").cloned().collect())
    }));
    let mut ed = Editor::new(session, PatchQueue::new(), None);
    let s = |v: &Value| v.as_str().unwrap().to_string();
    let mut results = Vec::new();
    for op in case["ops"].as_array().unwrap() {
        let a = &op.as_array().unwrap()[1..];
        let find = |ed: &Editor, tag: &Value, id: &Value| ed.find(&s(tag), &s(id), None).expect("a record of the case");
        let out: Result<Value, editcore::edit::EditError> = match op[0].as_str().unwrap() {
            "set" => {
                let f = find(&ed, &a[0], &a[1]);
                ed.set_field(&f, &s(&a[2]), &a[3]).map(|()| json!("ok"))
            }
            "whole" => {
                ed.queue.add_whole(Selection { plugin: s(&a[2]), record_type: type_of(&s(&a[0])).unwrap().to_string(), key: s(&a[1]) });
                Ok(json!("ok"))
            }
            "set_ref" => {
                let r = ed.session.find_ref(&s(&a[0]), &s(&a[1]), a[2].as_i64().unwrap(), None).unwrap();
                ed.set_ref_field(&r, &s(&a[3]), &a[4]).map(|()| json!("ok"))
            }
            "insert" => ed.insert(&s(&a[0]), &s(&a[1])).map(|f| json!(f.key)),
            "save_script" => {
                let f = find(&ed, &a[0], &a[1]);
                let compiler = ed.script_compiler();
                ed.set_script_text(&f, &s(&a[2]), &*compiler)
            }
            "verify" => {
                let exists = |p: &str| files.contains(&p.to_lowercase());
                Ok(ed.verify(if a[0] == true { Some(&exists) } else { None }))
            }
            other => panic!("unknown operation {other}"),
        };
        results.push(out.unwrap_or_else(|e| json!({"error": e.0})));
    }
    json!({"results": results, "journal": ed.queue.journal()})
}

#[test]
fn verify_cases_check_as_the_python_editor_does() {
    let mut failed = Vec::new();
    for (n, case) in cases("verify_cases.json").iter().enumerate() {
        let folder = std::env::temp_dir().join(format!("wg-verify-cases-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        let got = run_verify(case, &folder);
        let _ = std::fs::remove_dir_all(&folder);
        assert_eq!(got["results"].as_array().map(Vec::len), case["ops"].as_array().map(Vec::len));
        if got != case["expect"] {
            let d = first_difference(&got, &case["expect"], "").unwrap_or_else(|| "a key missing on one side".into());
            failed.push(format!("{}: {d}", case["name"]));
        }
        let (g, w) = (got.to_string(), case["expect"].to_string());
        if g != w && got == case["expect"] {
            let at = g.bytes().zip(w.bytes()).position(|(x, y)| x != y).unwrap_or(0);
            let from = at.saturating_sub(120);
            failed.push(format!("{}: the same answer in another order, at\n  got  ...{}\n  want ...{}", case["name"], &g[from..(at + 80).min(g.len())], &w[from..(at + 80).min(w.len())]));
        }
    }
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}

/// A build case's plan as `editcore::build` takes it (`plan` in tests/test_build_cases.py).
fn build_plan(p: &Value) -> editcore::build::Plan {
    use editcore::queue::{Choice, Fields, NewRecord, NewRef, RefEdit, Selection};
    let list = |k: &str| p.get(k).and_then(Value::as_array).cloned().unwrap_or_default();
    let s = |v: &Value| v.as_str().unwrap_or("").to_string();
    let names = |v: &Value| v.as_array().into_iter().flatten().map(s).collect::<Vec<String>>();
    editcore::build::Plan {
        selections: list("selections").iter().map(|a| Selection { plugin: s(&a[0]), record_type: s(&a[1]), key: s(&a[2]) }).collect(),
        merges: list("merges")
            .iter()
            .map(|m| editcore::build::Merge {
                record_type: s(&m["type"]),
                key: s(&m["key"]),
                base_plugin: s(&m["base"]),
                choices: m["choices"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|c| match c.get("plugin") {
                        Some(pl) => Choice::Plugin { path: s(&c["path"]), plugin: s(pl) },
                        None => Choice::Value { path: s(&c["path"]), value: c["value"].clone() },
                    })
                    .collect(),
            })
            .collect(),
        carried: list("carried"),
        ref_edits: list("ref_edits")
            .iter()
            .map(|e| RefEdit { cell: s(&e["cell"]), origin: s(&e["origin"]), refr_index: e["refr_index"].as_i64().unwrap(), changes: Fields::from_object(&e["changes"]), plugins: names(&e["plugins"]) })
            .collect(),
        new_refs: list("new_refs")
            .iter()
            .map(|n| NewRef {
                cell: s(&n["cell"]),
                uid: s(&n["uid"]),
                fields: Fields::from_object(&n["fields"]),
                base_plugin: n.get("base_plugin").map(s).unwrap_or_default(),
                plugins: names(&n["plugins"]),
                tag: String::new(),
            })
            .collect(),
        new_records: list("new_records")
            .iter()
            .map(|m| NewRecord {
                record_type: s(&m["type"]),
                key: s(&m["key"]),
                record: m["record"].clone(),
                source: m.get("source").map(s).unwrap_or_default(),
                topic: m.get("topic").map(s).unwrap_or_default(),
            })
            .collect(),
    }
}

/// A build case's patch, as tests/test_build_cases.py makes it (a dry run).
fn run_build(case: &Value) -> Value {
    let sources: editcore::build::Sources = case["sources"].as_object().unwrap().iter().map(|(k, v)| (k.clone(), v.as_array().unwrap().clone())).collect();
    let order: Vec<String> = case["order"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
    let sizes: HashMap<String, u64> = case["sizes"].as_object().unwrap().iter().map(|(k, v)| (k.clone(), v.as_u64().unwrap())).collect();
    let mut heard = Vec::new();
    let built = editcore::build::build_record_patch(&build_plan(&case["plan"]), &sources, &order, &sizes, case["output"].as_str().unwrap(), &mut |l| heard.push(l.to_string()));
    match built {
        Err(e) => json!({"error": e.0}),
        Ok((document, mut result)) => {
            result.lines.push("dry run: nothing was written".into());
            heard.push("dry run: nothing was written".into());
            assert_eq!(heard, result.lines, "{}: what was said is what was kept", case["name"]);
            let mut out = json!({"document": document});
            if !document.iter().any(|r| r["type"] == "ScriptConfigList") {
                use base64::Engine;
                let bytes = editcore::build::patch_bytes(&document).unwrap();
                out["bytes"] = json!(base64::engine::general_purpose::STANDARD.encode(bytes));
            }
            for (k, v) in json!({"records": result.records, "carried": result.carried, "masters": result.masters, "remapped": result.remapped, "lines": result.lines}).as_object().unwrap() {
                out[k] = v.clone();
            }
            out
        }
    }
}

#[test]
fn build_cases_build_as_the_python_patch_builder_does() {
    let mut failed = Vec::new();
    for case in cases("build_cases.json") {
        let got = run_build(&case);
        if got != case["expect"] {
            let d = first_difference(&got, &case["expect"], "").unwrap_or_else(|| "a key missing on one side".into());
            failed.push(format!("{}: {d}", case["name"]));
        }
        let (g, w) = (got.to_string(), case["expect"].to_string());
        if g != w && got == case["expect"] {
            let at = g.bytes().zip(w.bytes()).position(|(x, y)| x != y).unwrap_or(0);
            let from = at.saturating_sub(120);
            failed.push(format!("{}: the same answer in another order, at\n  got  ...{}\n  want ...{}", case["name"], &g[from..(at + 80).min(g.len())], &w[from..(at + 80).min(w.len())]));
        }
    }
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}

/// One buildjob case's call (`answer` in tests/test_buildjob_cases.py).
fn buildjob_answer(case: &Value, a: &Value, tmp: &Path) -> Result<Value, String> {
    use editcore::buildjob::{self, BuildState};
    use editcore::testrun;
    let state = |v: Option<&Value>| v.filter(|v| !v.is_null()).map(BuildState::from_json);
    let text = |p: PathBuf| json!(p.to_string_lossy());
    Ok(match case["kind"].as_str().unwrap() {
        "target" => text(buildjob::patch_target(&a["req"])?),
        "check" => {
            let order: Vec<String> = a["order"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
            buildjob::build_check(&buildjob::patch_target(&a["req"])?, &order)
        }
        "preview" => text(buildjob::preview_target(&a["req"])?),
        "mode" => json!(buildjob::build_mode(&a["req"], Path::new(a["target"].as_str().unwrap()), state(a.get("running")).as_ref())?),
        "next" => BuildState::next(state(a.get("running")).as_ref()).to_json(),
        "status" => buildjob::build_status(state(a.get("state")).as_ref(), &a["req"])?,
        "setups" => {
            let mut settings = a["settings"].clone();
            let changed = buildjob::update_setups(&mut settings, &a["req"])?;
            json!({"settings": settings, "changed": changed})
        }
        "setup" => testrun::clean_setup(&a["raw"])?,
        "launch" => {
            let (argv, script) = testrun::launch_args(Path::new(a["exe"].as_str().unwrap()), Path::new(a["plugin"].as_str().unwrap()), a["cell"].as_str(), a.get("setup"));
            json!({"argv": argv, "script": script})
        }
        "test" => {
            let exe = a.get("exe").and_then(Value::as_str).map(PathBuf::from);
            let (found, cell, setup) = buildjob::test_request(&a["req"], a.get("given").and_then(Value::as_str), exe, &a["settings"])?;
            json!({"exe": found.to_string_lossy(), "cell": cell, "setup": setup})
        }
        "settings" => {
            let file = tmp.join("openmw_test.json");
            if let Some(t) = a.get("file").and_then(Value::as_str) {
                std::fs::write(&file, t).unwrap();
            }
            testrun::load_settings(&file)
        }
        other => panic!("unknown kind {other}"),
    })
}

#[test]
fn buildjob_cases_decide_as_the_python_links_do() {
    let mut failed = Vec::new();
    for (n, case) in cases("buildjob_cases.json").iter().enumerate() {
        let tmp = std::env::temp_dir().join(format!("wg-buildjob-cases-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        for d in case.get("dirs").and_then(Value::as_array).into_iter().flatten() {
            std::fs::create_dir_all(tmp.join(d.as_str().unwrap())).unwrap();
        }
        for f in case.get("files").and_then(Value::as_array).into_iter().flatten() {
            let f = tmp.join(f.as_str().unwrap());
            std::fs::create_dir_all(f.parent().unwrap()).unwrap();
            std::fs::write(f, b"").unwrap();
        }
        let root = tmp.to_string_lossy().replace('\\', "/");
        let args: Value = serde_json::from_str(&case["args"].to_string().replace("{tmp}", &root)).unwrap();
        let got = buildjob_answer(case, &args, &tmp).unwrap_or_else(|e| json!({"error": e}));
        // A path's separators as `/`, so the answers read the same on Windows.
        let got: Value = serde_json::from_str(&got.to_string().replace("\\\\", "/").replace(&root, "{tmp}")).unwrap();
        let _ = std::fs::remove_dir_all(&tmp);
        let (g, w) = (got.to_string(), case["expect"].to_string());
        if g != w {
            failed.push(format!("{}: got {g}, want {w}", case["name"]));
        }
    }
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}

/// A link case's string values with the names `names` swaps.
fn swap(v: &mut Value, names: &HashMap<String, String>) {
    match v {
        Value::String(s) => {
            if let Some(n) = names.get(s.as_str()) {
                *s = n.clone();
            }
        }
        Value::Array(a) => a.iter_mut().for_each(|x| swap(x, names)),
        Value::Object(m) => m.values_mut().for_each(|x| swap(x, names)),
        _ => {}
    }
}

/// A link case through `editcore::links` (`run` in tests/test_link_cases.py).
fn run_links(case: &Value, folder: &Path) -> Value {
    use editcore::edit::Editor;
    use editcore::links::Links;
    use editcore::queue::PatchQueue;
    let plugins = case["plugins"].as_array().unwrap().clone();
    let names: Vec<String> = plugins.iter().map(|p| p["name"].as_str().unwrap().to_string()).collect();
    let by: HashMap<String, Value> = plugins.iter().map(|p| (p["name"].as_str().unwrap().to_string(), p.clone())).collect();
    let session = Session::with_reader(
        names.iter().map(|n| (n.clone(), folder.join(n))).collect(),
        Box::new(move |path, tag| {
            let p = &by[&path.file_name().unwrap().to_string_lossy().to_string()];
            if tag == "TES3" {
                return Ok(vec![header(p.get("masters").unwrap_or(&json!([])))]);
            }
            let want = type_of(tag);
            Ok(p["records"].as_array().unwrap().iter().filter(|r| Some(r["type"].as_str().unwrap()) == want).cloned().collect())
        }),
    );
    let whole: HashMap<String, Vec<Value>> = plugins.iter().map(|p| (p["name"].as_str().unwrap().to_string(), read_whole(p))).collect();
    let dialogue: HashMap<String, Vec<Value>> =
        whole.iter().map(|(k, v)| (k.clone(), v.iter().filter(|r| r["type"] == "Dialogue" || r["type"] == "DialogueInfo").cloned().collect())).collect();
    let name = |path: &Path| path.file_name().unwrap().to_string_lossy().to_string();
    let session = session
        .with_read_all(Box::new(move |path| Ok(whole[&name(path)].clone())))
        .with_read_dialogue(Box::new(move |path| Ok(dialogue[&name(path)].clone())));
    let journal = folder.join("journal.json");
    let links = Links::with_editor(Editor::new(session, PatchQueue::new(), Some(journal.clone())));
    let mut labels: HashMap<String, String> = HashMap::new(); // uid -> label
    let mut results = Vec::new();
    for op in case["ops"].as_array().unwrap() {
        let back: HashMap<String, String> = labels.iter().map(|(k, v)| (v.clone(), k.clone())).collect();
        if op[0] == "journal" {
            if op[1].is_null() {
                let _ = std::fs::remove_file(&journal);
            } else {
                let mut doc = op[1].clone();
                swap(&mut doc, &back);
                std::fs::write(&journal, doc.to_string()).unwrap();
            }
            results.push(json!("written"));
            continue;
        }
        let mut body = op[2].clone();
        swap(&mut body, &back);
        let name = op[1].as_str().unwrap();
        let out = match links.call(name, &body.to_string()) {
            Ok(text) => {
                let mut v: Value = serde_json::from_str(&text).unwrap_or(Value::String(text));
                if name == "editRevision" {
                    v.as_object_mut().unwrap().shift_remove("rev");
                }
                v
            }
            Err(e) => json!({"error": e}),
        };
        {
            let ed = links.editor.lock().unwrap();
            let count = |labels: &HashMap<String, String>, p: char| labels.values().filter(|v| v.starts_with(p)).count() + 1;
            for n in ed.queue.new_refs() {
                if !labels.contains_key(&n.uid) {
                    let l = format!("U{}", count(&labels, 'U'));
                    labels.insert(n.uid.clone(), l);
                }
            }
            for m in ed.queue.new_records() {
                if m.record_type == "DialogueInfo" && !labels.contains_key(&m.key) {
                    let l = format!("R{}", count(&labels, 'R'));
                    labels.insert(m.key.clone(), l);
                }
            }
        }
        results.push(out);
    }
    let mut out = json!({"results": results, "journal": links.editor.lock().unwrap().queue.journal()});
    swap(&mut out, &labels);
    out
}

#[test]
fn link_cases_answer_as_wraithguard_does() {
    let mut failed = Vec::new();
    for (n, case) in cases("link_cases.json").iter().enumerate() {
        let folder = std::env::temp_dir().join(format!("wg-link-cases-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        let got = run_links(case, &folder);
        let _ = std::fs::remove_dir_all(&folder);
        if got != case["expect"] {
            let d = first_difference(&got, &case["expect"], "").unwrap_or_else(|| "a key missing on one side".into());
            failed.push(format!("{}: {d}", case["name"]));
        }
        let (g, w) = (got.to_string(), case["expect"].to_string());
        if g != w && got == case["expect"] {
            let at = g.bytes().zip(w.bytes()).position(|(x, y)| x != y).unwrap_or(0);
            let from = at.saturating_sub(120);
            failed.push(format!("{}: the same answer in another order, at\n  got  ...{}\n  want ...{}", case["name"], &g[from..(at + 80).min(g.len())], &w[from..(at + 80).min(w.len())]));
        }
    }
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}
