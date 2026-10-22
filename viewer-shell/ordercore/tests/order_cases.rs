//! The shared sort-order cases (tests/fixtures/order_cases.json at the repository's root),
//! which the Python side runs too.

use std::collections::BTreeMap;

use ordercore::order::{build_and_sort, Unsupported};
use ordercore::rules::{parse_text, Kind};
use serde_json::{json, Map, Value};

const CASES: &str = include_str!("../../../tests/fixtures/order_cases.json");

fn strings(v: &Value) -> Vec<String> {
    v.as_array().unwrap().iter().map(|s| s.as_str().unwrap().to_string()).collect()
}

#[test]
fn every_sort_places_the_customs_as_the_python_does() {
    let cases: Value = serde_json::from_str(CASES).unwrap();
    let mut failed = Vec::new();
    for case in cases["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let base = strings(&case["base"]);
        let subset = strings(&case["subset"]);
        // A rule file's index is its priority: its [Order] blocks are chains, and the hint
        // names of every file make two flat lists, in file order.
        let mut blocks: Vec<(Vec<String>, i64)> = Vec::new();
        let (mut nearstart, mut nearend) = (Vec::new(), Vec::new());
        for rule in case["rules"].as_array().unwrap() {
            let priority = rule["priority"].as_i64().unwrap();
            for block in parse_text(rule["text"].as_str().unwrap()).blocks {
                match block.kind {
                    Kind::Order => blocks.push((block.names, priority)),
                    Kind::NearStart => nearstart.extend(block.names),
                    Kind::NearEnd => nearend.extend(block.names),
                }
            }
        }
        let masters: BTreeMap<String, Vec<String>> =
            case["masters"].as_object().unwrap().iter().map(|(k, v)| (k.clone(), strings(v))).collect();
        let got = build_and_sort(&base, &subset, &blocks, &masters, &nearstart, &nearend);
        if !case["supported"].as_bool().unwrap() {
            if got != Err(Unsupported) {
                failed.push(format!("{name}: should be declined, gave {got:?}"));
            }
            continue;
        }
        let Ok(got) = got else {
            failed.push(format!("{name}: declined"));
            continue;
        };
        if json!(got.order) != case["order"] {
            failed.push(format!("{name}: order is {:?}, want {}", got.order, case["order"]));
        }
        let anchors: Map<String, Value> =
            got.anchors.iter().map(|(k, (how, anchor))| (k.clone(), json!([how.as_str(), anchor]))).collect();
        if Value::Object(anchors.clone()) != case["anchors"] {
            failed.push(format!("{name}: anchors are {}, want {}", Value::Object(anchors), case["anchors"]));
        }
    }
    assert!(failed.is_empty(), "\n{}", failed.join("\n"));
}
