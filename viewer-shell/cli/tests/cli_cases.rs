//! The command line's shared cases (tests/fixtures/cli_cases.json at the repository's root,
//! recorded from `wraithguard_toolkit.py` by tools/gen_cli_cases.py): each setup made on disk,
//! the `wraithguard` binary run on its command line, and what it printed on stdout and stderr,
//! how it exited and the files it left compared with the Python's.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, UNIX_EPOCH};

use base64::Engine as _;
use serde_json::Value;

const CASES: &str = include_str!("../../../tests/fixtures/cli_cases.json");

fn s(v: &Value) -> String {
    v.as_str().unwrap().to_string()
}

/// What the Python records: the setup's folder as `<root>`, every backslash a slash.
fn unrooted(text: &str, root: &str) -> String {
    let doubled = root.replace('\\', "\\\\");
    let slashed = root.replace('\\', "/");
    text.replace(&doubled, "<root>")
        .replace(root, "<root>")
        .replace("\\\\", "/")
        .replace('\\', "/")
        .replace(&slashed, "<root>")
}

/// A header-only plugin with these masters (`gen_plan_cases.plugin_bytes`).
fn plugin_bytes(masters: &[(String, u64)], data: bool, pad: usize) -> Vec<u8> {
    let mut body = b"HEDR".to_vec();
    body.extend(300u32.to_le_bytes());
    body.extend(1.3f32.to_le_bytes());
    body.extend([0u8; 4 + 32 + 256 + 4]);
    for (name, size) in masters {
        let mut raw = name.as_bytes().to_vec();
        raw.push(0);
        body.extend(b"MAST");
        body.extend(u32::try_from(raw.len()).unwrap().to_le_bytes());
        body.extend(&raw);
        if data {
            body.extend(b"DATA");
            body.extend(8u32.to_le_bytes());
            body.extend(size.to_le_bytes());
        }
    }
    let mut out = b"TES3".to_vec();
    out.extend(u32::try_from(body.len()).unwrap().to_le_bytes());
    out.extend([0u8; 8]);
    out.extend(body);
    out.extend(vec![0u8; pad]);
    out
}

fn make_setup(root: &Path, files: &Value) {
    let r = root.to_string_lossy().into_owned();
    for (rel, spec) in files.as_object().unwrap() {
        let path = root.join(rel);
        if spec.is_null() {
            fs::create_dir_all(&path).unwrap();
            continue;
        }
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        if let Some(text) = spec.as_str() {
            // a Windows folder goes into a TOML basic string escaped (see plan_cases.rs)
            let here = if rel.ends_with(".toml") { r.replace('\\', "\\\\") } else { r.clone() };
            fs::write(&path, text.replace("<root>", &here)).unwrap();
            continue;
        }
        if let Some(b64) = spec.get("b64").and_then(Value::as_str) {
            fs::write(&path, base64::engine::general_purpose::STANDARD.decode(b64).unwrap()).unwrap();
            continue;
        }
        let masters: Vec<(String, u64)> =
            spec["masters"].as_array().unwrap().iter().map(|m| (s(&m[0]), m[1].as_u64().unwrap())).collect();
        let data = spec.get("data").and_then(Value::as_bool).unwrap_or(true);
        let pad = spec.get("pad").and_then(Value::as_u64).unwrap_or(0) as usize;
        fs::write(&path, plugin_bytes(&masters, data, pad)).unwrap();
        if let Some(t) = spec.get("mtime").and_then(Value::as_u64) {
            let f = fs::File::options().write(true).open(&path).unwrap();
            f.set_modified(UNIX_EPOCH + Duration::from_secs(t)).unwrap();
        }
    }
}

fn root_for(name: &str) -> PathBuf {
    let slug: String = name.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).take(40).collect();
    let d = std::env::temp_dir().join(format!("wg_cli_{}_{slug}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    let c = fs::canonicalize(&d).unwrap();
    let text = c.to_string_lossy().into_owned();
    PathBuf::from(text.strip_prefix(r"\\?\").unwrap_or(&text).to_string())
}

fn snapshot(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(dir: &Path, root: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        for e in fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, root, out);
            } else {
                let rel = p.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
                out.insert(rel, fs::read(&p).unwrap());
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, root, &mut out);
    out
}

fn first_difference(name: &str, what: &str, got: &[String], want: &[String]) {
    if got != want {
        let at = got.iter().zip(want).position(|(a, b)| a != b).unwrap_or(got.len().min(want.len()));
        panic!("{name}: {what} line {at}\n got: {:?}\nwant: {:?}", got.get(at), want.get(at));
    }
}

#[test]
fn the_command_line_answers_as_the_python_did() {
    let doc: Value = serde_json::from_str(CASES).unwrap();
    for case in doc["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let dir = root_for(name);
        let root = dir.to_string_lossy().into_owned();
        make_setup(&dir, &case["files"]);
        let before = snapshot(&dir);
        let argv: Vec<String> = case["argv"].as_array().unwrap().iter().map(|a| s(a).replace("<root>", &root)).collect();
        let out = Command::new(env!("CARGO_BIN_EXE_wraithguard"))
            .args(&argv)
            .env("WRAITHGUARD_CLI_NOW", "2026-10-22T12:00:00")
            .output()
            .unwrap();
        let lines = |b: &[u8]| -> Vec<String> {
            String::from_utf8_lossy(b).replace("\r\n", "\n").split('\n').map(|l| unrooted(l, &root)).collect()
        };
        let want = |k: &str| -> Vec<String> { case[k].as_array().unwrap().iter().map(s).collect() };
        first_difference(name, "stdout", &lines(&out.stdout), &want("stdout"));
        first_difference(name, "stderr", &lines(&out.stderr), &want("stderr"));
        assert_eq!(i64::from(out.status.code().unwrap_or(-1)), case["exit"].as_i64().unwrap(), "{name}: exit");
        let after = snapshot(&dir);
        let written: BTreeSet<&String> = after.iter().filter(|(k, v)| before.get(*k) != Some(*v)).map(|(k, _)| k).collect();
        let expect = case["written"].as_object().unwrap();
        assert_eq!(written, expect.keys().collect::<BTreeSet<_>>(), "{name}: files written");
        for (rel, text) in expect {
            let got = unrooted(&String::from_utf8_lossy(&after[rel]).replace("\r\n", "\n"), &root);
            let want: Vec<String> = s(text).split('\n').map(str::to_string).collect();
            first_difference(name, rel, &got.split('\n').map(str::to_string).collect::<Vec<_>>(), &want);
        }
        let _ = fs::remove_dir_all(&dir);
    }
}
