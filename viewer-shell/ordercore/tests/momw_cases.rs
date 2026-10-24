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
fn every_yml_reads_as_the_python_line_reader_reads_it() {
    let doc: Value = serde_json::from_str(CASES).unwrap();
    let mut failed = Vec::new();
    for case in section(&doc, "yml") {
        let got: Vec<Value> =
            parse_text(case["text"].as_str().unwrap()).iter().map(entry_json).collect();
        if Value::Array(got.clone()) != case["entries"] {
            failed.push(format!("{}: got {}, want {}", case["name"], Value::Array(got), case["entries"]));
        }
    }
    assert!(failed.is_empty(), "\n{}", failed.join("\n"));
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
