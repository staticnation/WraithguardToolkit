//! The shared predicate cases (tests/fixtures/predicate_cases.json at the repository's root),
//! which the Python side runs too: expressions, versions, and rule text against a load order
//! with plugin files.

use std::collections::BTreeMap;

use ordercore::patterns::Unsupported;
use ordercore::predicates::{check_predicates, describe, format_version, parse, tokenize, Node, Plugins};
use serde_json::{json, Value};

const CASES: &str = include_str!("../../../tests/fixtures/predicate_cases.json");

fn strings(v: &Value) -> Vec<String> {
    v.as_array().unwrap().iter().map(|s| s.as_str().unwrap().to_string()).collect()
}

fn tree(node: &Node) -> Value {
    match node {
        Node::Leaf(s) => json!(s),
        Node::Group(items) => Value::Array(items.iter().map(tree).collect()),
    }
}

/// The plugin files a case says the Python found and read.
struct Disk {
    usable: bool,
    files: BTreeMap<String, (u64, String)>,
}

impl Plugins for Disk {
    fn usable(&self) -> bool {
        self.usable
    }
    fn exists(&self, name: &str) -> bool {
        self.files.contains_key(&name.to_lowercase())
    }
    fn size(&self, name: &str) -> Option<u64> {
        self.files.get(&name.to_lowercase()).map(|f| f.0)
    }
    fn description(&self, name: &str) -> String {
        self.files.get(&name.to_lowercase()).map(|f| f.1.clone()).unwrap_or_default()
    }
}

#[test]
fn every_expression_reads_as_the_python_reads_it() {
    let doc: Value = serde_json::from_str(CASES).unwrap();
    let mut failed = Vec::new();
    for case in doc["expressions"].as_array().unwrap() {
        let text = case["text"].as_str().unwrap();
        let Ok(tokens) = tokenize(text) else {
            failed.push(format!("{text:?}: declined"));
            continue;
        };
        if json!(tokens) != case["tokens"] {
            failed.push(format!("{text:?}: tokens are {tokens:?}, want {}", case["tokens"]));
            continue;
        }
        let ast = parse(&tokens).unwrap();
        let got = Value::Array(ast.iter().map(tree).collect());
        if got != case["ast"] {
            failed.push(format!("{text:?}: tree is {got}, want {}", case["ast"]));
        }
        let described: Vec<String> = ast.iter().map(describe).collect();
        if json!(described) != case["described"] {
            failed.push(format!("{text:?}: described as {described:?}, want {}", case["described"]));
        }
    }
    assert!(failed.is_empty(), "\n{}", failed.join("\n"));
}

#[test]
fn every_version_is_formatted_as_the_python_formats_it() {
    let doc: Value = serde_json::from_str(CASES).unwrap();
    let mut failed = Vec::new();
    for case in doc["versions"].as_array().unwrap() {
        let version = case["version"].as_str().unwrap();
        match format_version(version) {
            Ok(got) if json!(got) == case["formatted"] => {}
            Ok(got) => failed.push(format!("{version:?}: {got:?}, want {}", case["formatted"])),
            // a string whose number Python reads from text this does not
            Err(Unsupported) if version.chars().any(|c| !c.is_ascii() || c == '+' || c == ' ') => {}
            Err(Unsupported) => failed.push(format!("{version:?}: declined")),
        }
    }
    assert!(failed.is_empty(), "\n{}", failed.join("\n"));
}

#[test]
fn every_rule_text_warns_as_the_python_warns() {
    let doc: Value = serde_json::from_str(CASES).unwrap();
    let (mut answered, mut declined, mut failed) = (0usize, 0usize, Vec::new());
    for case in doc["checks"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let files: BTreeMap<String, (u64, String)> = case["files"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| {
                (k.clone(), (v["size"].as_u64().unwrap(), v["description"].as_str().unwrap().to_string()))
            })
            .collect();
        let disk = Disk { usable: case["usable"].as_bool().unwrap(), files };
        let origins: BTreeMap<String, String> = case["origins"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_string()))
            .collect();
        let got =
            check_predicates(case["rules"].as_str().unwrap(), &strings(&case["order"]), &origins, &disk);
        match got {
            Ok(warnings) => {
                answered += 1;
                if json!(warnings) != case["warnings"] {
                    failed.push(format!("{name}: warnings are {warnings:?}, want {}", case["warnings"]));
                }
            }
            Err(Unsupported) => {
                declined += 1;
                if case["must_support"].as_bool().unwrap() {
                    failed.push(format!("{name}: declined"));
                }
            }
        }
    }
    eprintln!("predicates: {answered} answered, {declined} declined, {} wrong", failed.len());
    assert!(failed.is_empty(), "\n{}", failed[..failed.len().min(30)].join("\n"));
}
