//! The shared case files (tests/fixtures at the repository's root), through editcore's
//! session, built as the Python tests build theirs (`EditorSession(read=...)`): the record
//! cases through `find` (tests/test_record_index.py), the reference cases through
//! `winning_reference` (tests/test_ref_cases.py) and the move cases through `find_ref`
//! (tests/test_move_cases.py). The two sides read the same files and must agree.

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
