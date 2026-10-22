//! The shared pattern and version cases (tests/fixtures/pattern_cases.json at the
//! repository's root), which the Python side runs too.

use ordercore::patterns::{expand, format_version, has_meta, Unsupported};
use serde_json::{json, Value};

const CASES: &str = include_str!("../../../tests/fixtures/pattern_cases.json");

#[test]
fn every_pattern_stands_for_the_names_the_python_says() {
    let cases: Value = serde_json::from_str(CASES).unwrap();
    let pool: Vec<String> =
        cases["pool"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
    let mut failed = Vec::new();
    for case in cases["patterns"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let pattern = case["pattern"].as_str().unwrap();
        if json!(has_meta(pattern)) != case["has_meta"] {
            failed.push(format!("{name}: has_meta is {}", has_meta(pattern)));
        }
        let got = expand(pattern, &pool);
        if case["supported"].as_bool().unwrap() {
            match got {
                Ok(names) if json!(names) == case["expand"] => {}
                other => failed.push(format!("{name}: {pattern:?} gave {other:?}, want {}", case["expand"])),
            }
        } else if got != Err(Unsupported) {
            failed.push(format!("{name}: {pattern:?} should be declined, gave {got:?}"));
        }
    }
    assert!(failed.is_empty(), "\n{}", failed.join("\n"));
}

#[test]
fn every_version_is_made_comparable_as_the_python_does() {
    let cases: Value = serde_json::from_str(CASES).unwrap();
    let mut failed = Vec::new();
    for case in cases["versions"].as_array().unwrap() {
        let input = case["in"].as_str().unwrap();
        let got = format_version(input);
        if case["supported"].as_bool().unwrap() {
            if got.as_deref().map(|s| json!(s)) != Ok(case["out"].clone()) {
                failed.push(format!("{input:?} gave {got:?}, want {}", case["out"]));
            }
        } else if got != Err(Unsupported) {
            failed.push(format!("{input:?} should be declined, gave {got:?}"));
        }
    }
    assert!(failed.is_empty(), "\n{}", failed.join("\n"));
}
