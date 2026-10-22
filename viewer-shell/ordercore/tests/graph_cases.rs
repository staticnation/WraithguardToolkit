//! The shared sort-graph cases (tests/fixtures/graph_cases.json at the repository's root),
//! which the Python side runs too.

use std::collections::BTreeMap;

use ordercore::graph::{
    build_edges, is_master_file, unique_conflicts, would_create_cycle, Adjacency, Unsupported,
};
use ordercore::rules::{parse_text, Kind};
use serde_json::{json, Value};

const CASES: &str = include_str!("../../../tests/fixtures/graph_cases.json");

fn strings(v: &Value) -> Vec<String> {
    v.as_array().unwrap().iter().map(|s| s.as_str().unwrap().to_string()).collect()
}

fn pairs(list: &[(String, String)]) -> Value {
    Value::Array(list.iter().map(|(a, b)| json!([a, b])).collect())
}

#[test]
fn every_master_is_told_as_the_python_tells_it() {
    let cases: Value = serde_json::from_str(CASES).unwrap();
    let mut failed = Vec::new();
    for case in cases["masters"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        if is_master_file(name) != case["is_master"].as_bool().unwrap() {
            failed.push(format!("{name:?}: is_master_file is {}", is_master_file(name)));
        }
    }
    assert!(failed.is_empty(), "\n{}", failed.join("\n"));
}

#[test]
fn every_edge_closes_a_cycle_when_the_python_says_so() {
    let cases: Value = serde_json::from_str(CASES).unwrap();
    let mut failed = Vec::new();
    for case in cases["cycles"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let mut adj = Adjacency::new();
        for (node, targets) in case["adjacency"].as_object().unwrap() {
            adj.insert(node.clone(), strings(targets).into_iter().collect());
        }
        let got = would_create_cycle(&adj, case["start"].as_str().unwrap(), case["target"].as_str().unwrap());
        if got != case["closes_cycle"].as_bool().unwrap() {
            failed.push(format!("{name}: would_create_cycle is {got}"));
        }
    }
    assert!(failed.is_empty(), "\n{}", failed.join("\n"));
}

#[test]
fn every_graph_is_built_as_the_python_builds_it() {
    let cases: Value = serde_json::from_str(CASES).unwrap();
    let mut failed = Vec::new();
    for case in cases["graphs"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let base = strings(&case["base"]);
        let subset = strings(&case["subset"]);
        // A rule file's index is its priority, and its [Order] blocks are chains.
        let mut blocks: Vec<(Vec<String>, i64)> = Vec::new();
        for rule in case["rules"].as_array().unwrap() {
            let priority = rule["priority"].as_i64().unwrap();
            for block in parse_text(rule["text"].as_str().unwrap()).blocks {
                if block.kind == Kind::Order {
                    blocks.push((block.names, priority));
                }
            }
        }
        let masters: BTreeMap<String, Vec<String>> =
            case["masters"].as_object().unwrap().iter().map(|(k, v)| (k.clone(), strings(v))).collect();
        let got = build_edges(&base, &subset, &blocks, &masters);
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
        if json!(got.subset) != case["subset_out"] {
            failed.push(format!("{name}: subset is {:?}, want {}", got.subset, case["subset_out"]));
        }
        let edges = Value::Array(got.edges().iter().map(|(a, b)| json!([a, b])).collect());
        if edges != case["edges"] {
            failed.push(format!("{name}: edges are {edges}, want {}", case["edges"]));
        }
        let indegree =
            Value::Array(got.indegree.iter().filter(|(_, k)| **k > 0).map(|(n, k)| json!([n, k])).collect());
        if indegree != case["indegree"] {
            failed.push(format!("{name}: indegree is {indegree}, want {}", case["indegree"]));
        }
        if pairs(&got.conflicts) != case["conflicts"] {
            failed.push(format!(
                "{name}: conflicts are {}, want {}",
                pairs(&got.conflicts),
                case["conflicts"]
            ));
        }
        let unique = pairs(&unique_conflicts(&got.conflicts));
        if unique != case["unique_conflicts"] {
            failed.push(format!("{name}: unique conflicts are {unique}, want {}", case["unique_conflicts"]));
        }
    }
    assert!(failed.is_empty(), "\n{}", failed.join("\n"));
}
