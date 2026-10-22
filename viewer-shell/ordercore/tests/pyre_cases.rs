//! Python's `re`, as `ordercore::pyre` must answer it (tests/fixtures/pyre_cases.json at the
//! repository's root): the answer of every case is Python's, or the pattern is declined.

use ordercore::pyre::{Flags, Match, PatError, Regex};
use serde_json::Value;

const CASES: &str = include_str!("../../../tests/fixtures/pyre_cases.json");

/// The spans of a match, as the case file writes them.
fn spans(m: &Option<Match>, groups: usize) -> Value {
    match m {
        None => Value::Null,
        Some(m) => Value::Array(
            (0..=groups)
                .map(|g| match if g == 0 { Some((m.start(), m.end())) } else { m.group(g) } {
                    Some((a, b)) => serde_json::json!([a, b]),
                    None => Value::Null,
                })
                .collect(),
        ),
    }
}

#[test]
fn every_pattern_is_answered_as_python_answers_it_or_declined() {
    let doc: Value = serde_json::from_str(CASES).unwrap();
    let (mut checked, mut declined, mut failed) = (0usize, 0usize, Vec::new());
    for case in doc["cases"].as_array().unwrap() {
        let pattern = case["pattern"].as_str().unwrap();
        let text = case["text"].as_str().unwrap();
        let flags = Flags {
            ignore_case: case["ignore_case"].as_bool().unwrap(),
            multiline: case["multiline"].as_bool().unwrap(),
        };
        let real = case["real"].as_bool().unwrap();
        let label = format!("{pattern:?} {flags:?} on {text:?}");
        let re = match Regex::new(pattern, flags) {
            Err(PatError::Unsupported) => {
                if real {
                    failed.push(format!("{label}: a real expression was declined"));
                }
                declined += 1;
                continue;
            }
            Err(PatError::Invalid) => {
                if !case["error"].as_bool().unwrap() {
                    failed.push(format!("{label}: Python compiles it, this says it is invalid"));
                }
                checked += 1;
                continue;
            }
            Ok(re) => re,
        };
        if case["error"].as_bool().unwrap() {
            failed.push(format!("{label}: Python rejects it, this compiled it"));
            continue;
        }
        let chars: Vec<char> = text.chars().collect();
        let groups = case["search"]
            .as_array()
            .map_or_else(|| case["match"].as_array().map_or(0, |a| a.len() - 1), |a| a.len() - 1);
        let (found, matched) = (re.search(&chars, 0), re.match_at(&chars, 0));
        let (Ok(found), Ok(matched)) = (found, matched) else {
            if real {
                failed.push(format!("{label}: a real expression was declined on a text"));
            }
            declined += 1;
            continue;
        };
        checked += 1;
        if spans(&found, groups) != case["search"] {
            failed.push(format!("{label}: search is {}, want {}", spans(&found, groups), case["search"]));
        }
        if spans(&matched, groups) != case["match"] {
            failed.push(format!("{label}: match is {}, want {}", spans(&matched, groups), case["match"]));
        }
    }
    eprintln!("pyre: {checked} answered, {declined} declined, {} wrong", failed.len());
    assert!(failed.is_empty(), "\n{}", failed[..failed.len().min(40)].join("\n"));
}
