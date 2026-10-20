//! The shared reference cases (tests/fixtures/ref_cases.json at the repository's root),
//! which the Python side runs too.

use ordercore::refs::{origin, winner, Version};
use serde_json::{json, Value};

const CASES: &str = include_str!("../../../tests/fixtures/ref_cases.json");

#[test]
fn every_case_resolves_as_the_game_does() {
    let cases: Value = serde_json::from_str(CASES).unwrap();
    let mut failed = Vec::new();
    for case in cases["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let plugins = case["plugins"].as_array().unwrap();
        let (want_origin, want_index) = (case["ask"][0].as_str().unwrap().to_ascii_lowercase(), case["ask"][1].as_i64().unwrap());
        // Every record for the asked reference, in load order.
        let mut versions = Vec::new();
        for (i, p) in plugins.iter().enumerate() {
            let pname = p["name"].as_str().unwrap();
            let masters: Vec<String> = p["masters"].as_array().unwrap().iter().map(|m| m.as_str().unwrap().to_string()).collect();
            for r in p["refs"].as_array().unwrap() {
                let at = origin(pname, &masters, r[0].as_i64().unwrap());
                if at.is_some_and(|o| o.eq_ignore_ascii_case(&want_origin)) && r[1].as_i64() == Some(want_index) {
                    versions.push(Version { plugin: i, deleted: r[2].as_bool().unwrap() });
                }
            }
        }
        let name_of = |v: &Version| plugins[v.plugin]["name"].clone();
        let got = match winner(&versions) {
            None => json!({"winner": null}),
            Some(w) => json!({"winner": name_of(&versions[w.at]), "deleted": w.deleted,
                              "ignored": w.ignored.iter().map(|&i| name_of(&versions[i])).collect::<Vec<_>>()}),
        };
        if got != case["expect"] {
            failed.push(format!("{name}: got {got}, want {}", case["expect"]));
        }
    }
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}
