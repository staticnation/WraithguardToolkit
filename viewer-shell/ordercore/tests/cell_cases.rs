//! The shared cell cases (tests/fixtures/cell_cases.json at the repository's root), which
//! the Python side runs too.

use ordercore::cells::CellKey;
use serde_json::{json, Value};

const CASES: &str = include_str!("../../../tests/fixtures/cell_cases.json");

#[test]
fn every_spelling_reads_as_the_cases_say() {
    let cases: Value = serde_json::from_str(CASES).unwrap();
    let mut failed = Vec::new();
    for case in cases["cases"].as_array().unwrap() {
        let spelling = case["in"].as_str().unwrap();
        let got = match CellKey::parse(spelling) {
            None => json!({"fold": null}),
            Some(k) => json!({"fold": k.fold(), "spec": k.spec(), "pool": k.pool_key(),
                              "grid": k.grid().map(|(x, y)| json!([x, y]))}),
        };
        let want: Value = match case["fold"] {
            Value::Null => json!({"fold": null}),
            _ => json!({"fold": case["fold"], "spec": case["spec"], "pool": case["pool"], "grid": case["grid"]}),
        };
        if got != want {
            failed.push(format!("{spelling:?}: got {got}, want {want}"));
        }
    }
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}
