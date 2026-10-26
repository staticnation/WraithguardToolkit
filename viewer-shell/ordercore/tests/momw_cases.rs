//! The shared MOMW cases (tests/fixtures/momw_cases.json at the repository's root), which
//! the Python side runs too.

use std::collections::HashSet;

use ordercore::momw::{
    base_order_matches_yml, curated_covers, curated_for_list, needs_cleaning_set, parse_text,
    post_sort_findings, Entry,
};
use serde_json::{json, Value};

const CASES: &str = include_str!("../../../tests/fixtures/momw_cases.json");

fn strings(v: &Value) -> Vec<String> {
    v.as_array().unwrap().iter().map(|s| s.as_str().unwrap().to_string()).collect()
}

fn set(v: &Value) -> HashSet<String> {
    strings(v).into_iter().collect()
}

fn entries(v: &Value) -> Vec<Entry> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|e| Entry {
            file_name: e["file_name"].as_str().unwrap().to_string(),
            for_mod: e["for_mod"].as_str().map(str::to_string),
            on_lists: strings(&e["on_lists"]),
            needs_cleaning: e["needs_cleaning"].as_bool().unwrap(),
        })
        .collect()
}

fn sorted(s: HashSet<String>) -> Vec<String> {
    let mut v: Vec<String> = s.into_iter().collect();
    v.sort();
    v
}

fn entry_json(e: &Entry) -> Value {
    json!({
        "file_name": e.file_name,
        "for_mod": e.for_mod,
        "on_lists": e.on_lists,
        "needs_cleaning": e.needs_cleaning,
    })
}

fn section<'a>(doc: &'a Value, name: &str) -> &'a Vec<Value> {
    doc[name].as_array().unwrap()
}

#[test]
fn every_yml_is_read_or_refused_as_the_case_says() {
    let doc: Value = serde_json::from_str(CASES).unwrap();
    let mut failed = Vec::new();
    for case in section(&doc, "yml") {
        let got = parse_text(case["text"].as_str().unwrap());
        let refused = case["refused"].as_bool().unwrap_or(false);
        match got {
            Err(e) if !refused => failed.push(format!("{}: refused ({e}), want {}", case["name"], case["entries"])),
            Ok(entries) if refused => {
                let got = Value::Array(entries.iter().map(entry_json).collect());
                failed.push(format!("{}: read {got}, want it refused", case["name"]));
            }
            Ok(entries) => {
                let got = Value::Array(entries.iter().map(entry_json).collect());
                if got != case["entries"] {
                    failed.push(format!("{}: got {got}, want {}", case["name"], case["entries"]));
                }
            }
            Err(_) => {}
        }
    }
    assert!(failed.is_empty(), "\n{}", failed.join("\n"));
}

#[test]
fn momws_own_file_is_read() {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/plugin-order.yml")).unwrap();
    let entries = parse_text(&text).unwrap();
    assert!(entries.len() > 100);
    assert!(entries.iter().all(|e| !e.file_name.is_empty()));
}

#[test]
fn every_list_picks_its_plugins() {
    let doc: Value = serde_json::from_str(CASES).unwrap();
    let mut failed = Vec::new();
    for case in section(&doc, "lists") {
        let (got_set, got_order) =
            curated_for_list(&entries(&case["entries"]), case["list"].as_str().unwrap());
        if json!(sorted(got_set)) != case["set"] || json!(got_order) != case["order"] {
            failed.push(format!("{}: wrong", case["name"]));
        }
    }
    for case in section(&doc, "cleaning") {
        let got = sorted(needs_cleaning_set(&entries(&case["entries"])));
        if json!(got) != case["set"] {
            failed.push(format!("{}: cleaning is {got:?}", case["name"]));
        }
    }
    assert!(failed.is_empty(), "\n{}", failed.join("\n"));
}

#[test]
fn drift_and_cover_and_findings_are_what_the_python_says() {
    let doc: Value = serde_json::from_str(CASES).unwrap();
    let mut failed = Vec::new();
    for case in section(&doc, "drift") {
        let got = base_order_matches_yml(&strings(&case["base"]), &strings(&case["curated"]));
        if json!(got) != case["warnings"] {
            failed.push(format!("{}: drift is {got:?}", case["name"]));
        }
    }
    for case in section(&doc, "covers") {
        let got = curated_covers(
            case["plugin"].as_str().unwrap(),
            &set(&case["curated"]),
            &set(&case["needs_cleaning"]),
        );
        if json!(got) != case["expect"] {
            failed.push(format!("{}: covers is {got}", case["name"]));
        }
    }
    for case in section(&doc, "findings") {
        let final_order = case["final_order"].as_array().map(|_| strings(&case["final_order"]));
        let got = post_sort_findings(
            final_order.as_deref(),
            &strings(&case["base"]),
            &entries(&case["entries"]),
            &set(&case["curated"]),
            &strings(&case["curated_order"]),
            &set(&case["declared"]),
            case["list"].as_str(),
        );
        if json!(got) != case["warnings"] {
            failed.push(format!("{}: findings are {got:?}, want {}", case["name"], case["warnings"]));
        }
    }
    assert!(failed.is_empty(), "\n{}", failed.join("\n"));
}
