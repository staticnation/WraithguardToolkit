//! The shared rule cases (tests/fixtures/rule_cases.json at the repository's root), which
//! the Python side runs too.

use ordercore::rules::parse_text;
use serde_json::{json, Value};

const CASES: &str = include_str!("../../../tests/fixtures/rule_cases.json");

#[test]
fn every_rule_text_reads_as_the_python_parser_reads_it() {
    let cases: Value = serde_json::from_str(CASES).unwrap();
    let mut failed = Vec::new();
    for case in cases["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let got = parse_text(case["text"].as_str().unwrap());
        let blocks: Vec<Value> =
            got.blocks.iter().map(|b| json!([b.kind.as_str(), b.names])).collect();
        if Value::Array(blocks.clone()) != case["blocks"] {
            failed.push(format!("{name}: blocks are {}, want {}", Value::Array(blocks), case["blocks"]));
        }
        if json!(got.skipped) != case["skipped"] {
            failed.push(format!("{name}: skipped is {}, want {}", got.skipped, case["skipped"]));
        }
    }
    assert!(failed.is_empty(), "\n{}", failed.join("\n"));
}
