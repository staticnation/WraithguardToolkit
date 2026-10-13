//! The shared openmw.cfg cases (tests/fixtures/openmw_cfg_cases.json at the repository's
//! root), which the Python side runs too: each built in a fresh folder and read.

use std::path::{Path, PathBuf};

use ordercore::cfg::{read, Tokens};
use serde_json::Value;

const CASES: &str = include_str!("../../../tests/fixtures/openmw_cfg_cases.json");

/// A path as the cases write it: relative to the case's folder, '/' separated.
fn rel(root: &Path, p: &Path) -> String {
    let r = p.strip_prefix(root).unwrap_or(p);
    r.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect::<Vec<_>>().join("/")
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array().unwrap().iter().map(|s| s.as_str().unwrap().to_string()).collect()
}

#[test]
fn every_case_reads_as_openmw_documents() {
    let cases: Value = serde_json::from_str(CASES).unwrap();
    let base = std::env::temp_dir().join(format!("ordercore-cases-{}", std::process::id()));
    let mut failed = Vec::new();
    for (n, case) in cases["cases"].as_array().unwrap().iter().enumerate() {
        let name = case["name"].as_str().unwrap();
        let root = base.join(n.to_string());
        let _ = std::fs::remove_dir_all(&root);
        for d in strings(&case["dirs"]) {
            std::fs::create_dir_all(root.join(&d)).unwrap();
        }
        for (f, text) in case["files"].as_object().unwrap() {
            let p = root.join(f);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, text.as_str().unwrap()).unwrap();
        }
        let tok = |k: &str| case["tokens"][k].as_str().map(|s| root.join(s));
        let tokens = Tokens { userdata: tok("userdata"), userconfig: tok("userconfig"), local: None, global: None };
        let got = read(&root.join(case["cfg"].as_str().unwrap()), &tokens).unwrap();
        let want = &case["expect"];
        let paths = |ps: &[PathBuf]| ps.iter().map(|p| rel(&root, p)).collect::<Vec<_>>();
        let located: Vec<Value> = got
            .locate(&strings(&want["located"].as_array().unwrap().iter().map(|l| l[0].clone()).collect::<Value>()))
            .into_iter()
            .map(|(n, p)| serde_json::json!([n, p.map(|p| rel(&root, &p))]))
            .collect();
        let checks: Vec<(&str, Value, Value)> = vec![
            ("data", serde_json::json!(paths(&got.data)), want["data"].clone()),
            ("missing_data", serde_json::json!(paths(&got.missing_data)), want["missing_data"].clone()),
            ("content", serde_json::json!(got.content), want["content"].clone()),
            ("groundcover", serde_json::json!(got.groundcover), want["groundcover"].clone()),
            ("archives", serde_json::json!(got.archives), want["archives"].clone()),
            ("plugins", serde_json::json!(got.plugins()), want["plugins"].clone()),
            ("located", Value::Array(located), want["located"].clone()),
            ("files", serde_json::json!(paths(&got.files)), want["files"].clone()),
            ("warnings", serde_json::json!(got.warnings.len()), want["warnings"].clone()),
        ];
        for (what, g, w) in checks {
            if g != w {
                failed.push(format!("{name}: {what} is {g}, want {w} (warnings: {:?})", got.warnings));
            }
        }
        if let Some(w) = want.get("user_data") {
            let g = got.user_data.as_ref().map(|p| rel(&root, p));
            if serde_json::json!(g) != *w {
                failed.push(format!("{name}: user_data is {g:?}, want {w}"));
            }
        }
    }
    let _ = std::fs::remove_dir_all(&base);
    assert!(failed.is_empty(), "\n{}", failed.join("\n"));
}
