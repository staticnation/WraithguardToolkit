//! The shared rule-maker cases (tests/fixtures/authoring_cases.json at the repository's root),
//! which the Python side runs too: how a rule renders and validates, the proposals derived from
//! what the tool observed, and the user's rule file over a temp folder.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use ordercore::authoring::{
    append_authored_rule, append_user_rule, format_ref, order_rule_frozen_conflicts, render_rule, validate,
    AppendError, Expr, Rule,
};
use ordercore::derive::{
    needs_citation, order_candidates_from_conflicts, order_from_masters, patch_candidates, propose_all,
    requires_from_masters, ConflictRecord, Proposal,
};
use ordercore::export::LineEnding;
use ordercore::patterns::Unsupported;
use ordercore::pyre::{Flags, Regex};
use serde_json::{json, Value};

const CASES: &str = include_str!("../../../tests/fixtures/authoring_cases.json");

fn doc() -> Value {
    serde_json::from_str(CASES).unwrap()
}

fn s(v: &Value) -> String {
    v.as_str().unwrap().to_string()
}

fn strs(v: &Value) -> Vec<String> {
    v.as_array().unwrap().iter().map(s).collect()
}

fn expr(v: &Value) -> Expr {
    match v["t"].as_str().unwrap() {
        "plugin" => Expr::Plugin(s(&v["name"])),
        "desc" => Expr::Desc {
            pattern: s(&v["pattern"]),
            plugin: s(&v["plugin"]),
            negated: v["negated"].as_bool().unwrap(),
        },
        "size" => Expr::Size {
            size: v["size"].as_i64().unwrap(),
            plugin: s(&v["plugin"]),
            negated: v["negated"].as_bool().unwrap(),
        },
        "ver" => {
            Expr::Ver { operator: s(&v["operator"]), version: s(&v["version"]), plugin: s(&v["plugin"]) }
        }
        _ => Expr::Group {
            operator: s(&v["operator"]),
            parts: v["parts"].as_array().unwrap().iter().map(expr).collect(),
        },
    }
}

fn expr_json(e: &Expr) -> Value {
    match e {
        Expr::Plugin(n) => json!({"t": "plugin", "name": n}),
        Expr::Desc { pattern, plugin, negated } => {
            json!({"t": "desc", "pattern": pattern, "plugin": plugin, "negated": negated})
        }
        Expr::Size { size, plugin, negated } => {
            json!({"t": "size", "size": size, "plugin": plugin, "negated": negated})
        }
        Expr::Ver { operator, version, plugin } => {
            json!({"t": "ver", "operator": operator, "version": version, "plugin": plugin})
        }
        Expr::Group { operator, parts } => {
            json!({"t": "group", "operator": operator, "parts": parts.iter().map(expr_json).collect::<Vec<_>>()})
        }
    }
}

fn rule(v: &Value) -> Rule {
    Rule {
        kind: s(&v["kind"]),
        plugins: strs(&v["plugins"]),
        expressions: v["expressions"].as_array().unwrap().iter().map(expr).collect(),
        message: s(&v["message"]),
        ref_: s(&v["ref"]),
        priority: v["priority"].as_i64().unwrap(),
        section: s(&v["section"]),
        comment: s(&v["comment"]),
        force_block: v["force_block"].as_bool().unwrap(),
    }
}

fn rule_json(r: &Rule) -> Value {
    json!({
        "kind": r.kind, "plugins": r.plugins, "expressions": r.expressions.iter().map(expr_json).collect::<Vec<_>>(),
        "message": r.message, "ref": r.ref_, "priority": r.priority, "section": r.section,
        "comment": r.comment, "force_block": r.force_block,
    })
}

fn proposal_json(p: &Proposal) -> Value {
    json!({
        "rule": rule_json(&p.rule), "confidence": p.confidence.as_str(), "reason": p.reason,
        "evidence": p.evidence, "alternatives": p.alternatives.iter().map(rule_json).collect::<Vec<_>>(),
    })
}

fn exprs_strings(e: &Expr, out: &mut Vec<String>) {
    match e {
        Expr::Plugin(n) => out.push(n.clone()),
        Expr::Desc { pattern, plugin, .. } => out.extend([pattern.clone(), plugin.clone()]),
        Expr::Size { plugin, .. } => out.push(plugin.clone()),
        Expr::Ver { operator, version, plugin } => {
            out.extend([operator.clone(), version.clone(), plugin.clone()])
        }
        Expr::Group { operator, parts } => {
            out.push(operator.clone());
            parts.iter().for_each(|p| exprs_strings(p, out));
        }
    }
}

/// Whether Rust may decline a rule: a `[DESC]` regex it does not read, or text it cannot `repr`.
fn declinable(r: &Rule) -> bool {
    let mut strings = vec![r.kind.clone(), r.ref_.clone(), r.message.clone()];
    strings.extend(r.plugins.iter().cloned());
    r.expressions.iter().for_each(|e| exprs_strings(e, &mut strings));
    let regex_unread = r.expressions.iter().any(|e| {
        e.walk().iter().any(
            |n| matches!(n, Expr::Desc { pattern, .. } if Regex::new(pattern, Flags::default()).is_err()),
        )
    });
    regex_unread || strings.iter().any(|t| !t.is_ascii())
}

fn temp_dir() -> PathBuf {
    static N: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "ordercore-authoring-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn every_citation_is_formatted_as_python_formats_it() {
    for c in doc()["refs"].as_array().unwrap() {
        assert_eq!(json!(format_ref(c["ref"].as_str().unwrap()).unwrap()), c["formatted"], "{}", c["ref"]);
    }
}

#[test]
fn every_rule_renders_and_validates_as_python_does() {
    let (mut answered, mut declined, mut failed) = (0usize, 0usize, Vec::new());
    for c in doc()["rules"].as_array().unwrap() {
        let name = s(&c["name"]);
        let r = rule(&c["rule"]);
        let rendered = render_rule(&r).unwrap();
        if json!(rendered) != c["rendered"] {
            failed.push(format!("{name}: rendered {rendered:?}, want {}", c["rendered"]));
        }
        match validate(&r) {
            Ok(problems) => {
                answered += 1;
                let got: Vec<Value> =
                    problems.iter().map(|p| json!([p.severity.as_str(), p.message, p.remedy])).collect();
                if Value::Array(got.clone()) != c["problems"] {
                    failed.push(format!("{name}: problems {}, want {}", Value::Array(got), c["problems"]));
                }
            }
            Err(Unsupported) if declinable(&r) => declined += 1,
            Err(Unsupported) => failed.push(format!("{name}: declined")),
        }
    }
    eprintln!("rules: {answered} validated, {declined} declined, {} wrong", failed.len());
    assert!(failed.is_empty(), "\n{}", failed[..failed.len().min(30)].join("\n"));
}

fn conflict(v: &Value) -> ConflictRecord {
    let plugins = v.get("plugins").and_then(Value::as_array).map(|a| {
        a.iter()
            .map(|e| match e {
                Value::String(t) => t.clone(),
                other => other.to_string(),
            })
            .collect()
    });
    let rectype = match v.get("type") {
        Some(Value::String(t)) if !t.is_empty() => t.clone(),
        Some(Value::Number(n)) if n.as_i64() != Some(0) => n.to_string(),
        _ => "?".to_string(),
    };
    ConflictRecord { plugins, rectype }
}

#[test]
fn every_proposal_is_derived_as_python_derives_it() {
    let mut failed = Vec::new();
    for c in doc()["proposals"].as_array().unwrap() {
        let masters: Vec<(String, Vec<String>)> =
            c["masters"].as_array().unwrap().iter().map(|p| (s(&p[0]), strs(&p[1]))).collect();
        let active = strs(&c["active"]);
        let conflicts: Vec<ConflictRecord> =
            c["conflicts"].as_array().unwrap().iter().map(conflict).collect();
        let minimum = c["minimum"].as_i64().unwrap();
        let list = |ps: Vec<Proposal>| Value::Array(ps.iter().map(proposal_json).collect());
        let checks = [
            ("order", list(order_from_masters(&masters, &active))),
            ("requires", list(requires_from_masters(&masters, &active))),
            ("patch", list(patch_candidates(&masters, &active))),
            ("from_conflicts", list(order_candidates_from_conflicts(&conflicts, minimum))),
        ];
        for (key, got) in checks {
            if got != c[key] {
                failed.push(format!("{} {key}: {got}, want {}", c["name"], c[key]));
            }
        }
        let all = propose_all(&masters, &active, &conflicts, minimum);
        if list(all.clone()) != c["all"] {
            failed.push(format!("{} all", c["name"]));
        }
        let needs: Vec<bool> = all.iter().map(needs_citation).collect();
        if json!(needs) != c["needs_citation"] {
            failed.push(format!("{} needs_citation", c["name"]));
        }
    }
    assert!(failed.is_empty(), "\n{}", failed[..failed.len().min(20)].join("\n"));
}

#[test]
fn every_rule_lands_in_the_users_file_as_python_puts_it() {
    let mut failed = Vec::new();
    for c in doc()["files"].as_array().unwrap() {
        let name = s(&c["name"]);
        let dir = temp_dir();
        let path = dir.join("my_rules.txt");
        if let Some(text) = c["existing"].as_str() {
            fs::write(&path, text).unwrap();
        }
        let got = if c.get("rule").is_some() {
            append_authored_rule(&path, &rule(&c["rule"]), LineEnding::Lf)
        } else {
            let u = &c["user"];
            append_user_rule(
                &path,
                u["keyword"].as_str().unwrap(),
                &strs(&u["names"]),
                u["comment"].as_str(),
                LineEnding::Lf,
            )
        };
        match got {
            Ok(text) => {
                if c.get("text").and_then(Value::as_str) != Some(text.as_str()) {
                    failed.push(format!("{name}: wrote {text:?}, want {c}"));
                }
            }
            Err(AppendError::Refused(m)) => {
                if c.get("refused").and_then(Value::as_str) != Some(m.as_str()) {
                    failed.push(format!("{name}: refused with {m:?}, want {}", c["refused"]));
                }
            }
            Err(e) => failed.push(format!("{name}: {e:?}")),
        }
        let after = fs::read(&path).ok().map(|b| String::from_utf8(b).unwrap());
        if json!(after) != c["after"] {
            failed.push(format!("{name}: file is {after:?}, want {}", c["after"]));
        }
        let _ = fs::remove_dir_all(&dir);
    }
    assert!(failed.is_empty(), "\n{}", failed.join("\n"));
}

#[test]
fn frozen_conflicts_are_the_pairs_python_names() {
    for c in doc()["frozen"].as_array().unwrap() {
        let got =
            order_rule_frozen_conflicts(&strs(&c["names"]), &strs(&c["final_order"]), &strs(&c["curated"]));
        assert_eq!(
            json!(got.iter().map(|(a, b)| json!([a, b])).collect::<Vec<_>>()),
            c["pairs"],
            "{}",
            c["names"]
        );
    }
}
