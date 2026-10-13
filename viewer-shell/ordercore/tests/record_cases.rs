//! The shared record cases (tests/fixtures/record_cases.json at the repository's root),
//! which the Python side runs too.

use ordercore::records::{Entry, Index, Tag};
use serde_json::{json, Value};

const CASES: &str = include_str!("../../../tests/fixtures/record_cases.json");

fn tag(s: &str) -> Tag {
    s.as_bytes().try_into().unwrap()
}

#[test]
fn every_case_resolves_as_openmw_does() {
    let cases: Value = serde_json::from_str(CASES).unwrap();
    let mut failed = Vec::new();
    for case in cases["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let plugins: Vec<Vec<Entry>> = case["plugins"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| {
                p.as_array()
                    .unwrap()
                    .iter()
                    .map(|r| Entry {
                        tag: tag(r[0].as_str().unwrap()),
                        id: r[1].as_str().unwrap().to_string(),
                        deleted: r[2].as_bool().unwrap(),
                    })
                    .collect()
            })
            .collect();
        let ix = Index::build(plugins);
        let mut live: Vec<Value> = ix
            .iter()
            .map(|(t, id, v)| json!([String::from_utf8_lossy(t), id, v.plugins]))
            .collect();
        live.sort_by_key(|v| v.to_string());
        if Value::Array(live.clone()) != case["expect"]["live"] {
            failed.push(format!("{name}: live is {}", Value::Array(live)));
        }
        for (id, want) in case["expect"]["placed"].as_object().unwrap() {
            let got = ix.placed_type(id).map(|t| String::from_utf8_lossy(&t).into_owned());
            if json!(got) != *want {
                failed.push(format!("{name}: placed {id} is {got:?}, want {want}"));
            }
        }
    }
    assert!(failed.is_empty(), "\n{}", failed.join("\n"));
}
