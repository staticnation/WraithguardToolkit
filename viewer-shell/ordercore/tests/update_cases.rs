//! The shared download cases (tests/fixtures/update_cases.json at the repository's root), which
//! the Python side runs too: the URL guard, the size cap, and the update flows over a scripted
//! network and a temp folder.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, UNIX_EPOCH};

use ordercore::updates::{
    check_url, data_path_tails_from_api, fetch_list_data_paths, fetch_url_bytes, parse_cache_paths,
    relative_tail, rule_file_ages, update_plugin_order_yml, update_rule_files, Env, FetchFail, Transport,
    UpdateError, ALLOWED_URL_SCHEMES, API_CFG_GENERATOR_URL, DEFAULT_BASE_DIR, DEFAULT_MOMW_HOST,
    MANAGED_RULE_FILES, MAX_DOWNLOAD_BYTES, PLUGIN_ORDER_URLS, RULES_URL_TEMPLATE,
};
use serde_json::{json, Value};

const CASES: &str = include_str!("../../../tests/fixtures/update_cases.json");

/// A network that answers from a script, and remembers what it was asked.
struct Script {
    replies: BTreeMap<String, Value>,
    asked: RefCell<Vec<String>>,
}

impl Transport for Script {
    fn get(&self, url: &str, _timeout: u64, max: usize) -> Result<Vec<u8>, String> {
        self.asked.borrow_mut().push(url.to_string());
        match self.replies.get(url) {
            None => Err(format!("<urlopen error no route to {url}>")),
            Some(r) if r.get("error").is_some() => Err(r["error"].as_str().unwrap().to_string()),
            Some(r) => {
                let body = r["body"].as_str().unwrap().as_bytes();
                Ok(body[..body.len().min(max)].to_vec())
            }
        }
    }
}

fn script(replies: &Value) -> Script {
    let replies = replies.as_object().unwrap().iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    Script { replies, asked: RefCell::new(Vec::new()) }
}

fn temp_dir() -> PathBuf {
    static N: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "ordercore-updates-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn declinable(s: &str) -> bool {
    s.contains(['[', ']']) || !s.is_ascii()
}

#[test]
fn the_constants_are_the_pythons() {
    let doc: Value = serde_json::from_str(CASES).unwrap();
    let c = &doc["constants"];
    assert_eq!(c["max_download_bytes"], json!(MAX_DOWNLOAD_BYTES));
    assert_eq!(c["managed"], json!(MANAGED_RULE_FILES));
    assert_eq!(c["rules_template"], json!(RULES_URL_TEMPLATE));
    assert_eq!(c["plugin_order_urls"], json!(PLUGIN_ORDER_URLS));
    assert_eq!(c["cfg_generator_url"], json!(API_CFG_GENERATOR_URL));
    assert_eq!(c["default_host"], json!(DEFAULT_MOMW_HOST));
    assert_eq!(ALLOWED_URL_SCHEMES, ["http", "https"]);
}

#[test]
fn every_url_is_judged_as_python_judges_it() {
    let doc: Value = serde_json::from_str(CASES).unwrap();
    let (mut judged, mut declined, mut failed) = (0usize, 0usize, Vec::new());
    for case in doc["urls"].as_array().unwrap() {
        let url = case["url"].as_str().unwrap();
        match (check_url(url), case["refused"].as_str()) {
            (Ok(()), None) => judged += 1,
            (Err(FetchFail::Value(m)), Some(want)) if m == want => judged += 1,
            (Err(FetchFail::Unsupported), _) if declinable(url) => declined += 1,
            (got, want) => failed.push(format!("{url:?}: {got:?}, want {want:?}")),
        }
    }
    eprintln!("urls: {judged} judged, {declined} declined");
    assert!(failed.is_empty(), "\n{}", failed.join("\n"));
}

#[test]
fn every_download_is_cut_as_python_cuts_it() {
    let doc: Value = serde_json::from_str(CASES).unwrap();
    let mut failed = Vec::new();
    for case in doc["fetches"].as_array().unwrap() {
        let url = case["url"].as_str().unwrap();
        let t = script(&json!({ url: case["reply"].clone() }));
        let got = fetch_url_bytes(&t, url, 1, case["max_bytes"].as_u64().unwrap() as usize);
        let ok = case.get("ok").and_then(Value::as_str);
        let value = case.get("value_error").and_then(Value::as_str);
        let os = case.get("os_error").and_then(Value::as_str);
        let matches = match &got {
            Ok(body) => ok.is_some_and(|w| w.as_bytes() == body.as_slice()),
            Err(FetchFail::Value(m)) => value == Some(m.as_str()),
            Err(FetchFail::Os(m)) => os == Some(m.as_str()),
            Err(FetchFail::Unsupported) => false,
        };
        if !matches {
            failed.push(format!("{url}: {got:?}, want {case}"));
        }
    }
    assert!(failed.is_empty(), "\n{}", failed.join("\n"));
}

#[test]
fn tails_and_caches_read_as_python_reads_them() {
    let doc: Value = serde_json::from_str(CASES).unwrap();
    let mut failed = Vec::new();
    for case in doc["tails"]["relative"].as_array().unwrap() {
        let value = case["value"].as_str().unwrap();
        let base = case["base"].as_str().unwrap_or(DEFAULT_BASE_DIR);
        match relative_tail(value, base) {
            Ok(got) if json!(got) == case["tail"] => {}
            Err(_) if !value.is_ascii() || !base.is_ascii() => {}
            other => {
                failed.push(format!("relative_tail({value:?}, {base:?}) = {other:?}, want {}", case["tail"]))
            }
        }
    }
    for case in doc["tails"]["api"].as_array().unwrap() {
        let payload = case["payload"].as_str().unwrap();
        let got = data_path_tails_from_api(payload.as_bytes());
        let bad = match (&got, case.get("tails"), case.get("value_error").and_then(Value::as_str)) {
            (Ok(Ok(tails)), Some(want), _) => json!(tails) != *want,
            (Ok(Err(text)), None, Some(want)) if case["parser_detail"].as_bool().unwrap() => {
                !(text.starts_with("not a cfg-generator response (")
                    && text.ends_with(')')
                    && want.starts_with("not a cfg-generator response ("))
            }
            (Ok(Err(text)), None, Some(want)) => text != want,
            (Err(_), _, _) => !payload.is_ascii(),
            _ => true,
        };
        if bad {
            failed.push(format!("{payload:?}: {got:?}, want {case}"));
        }
    }
    for case in doc["tails"]["cache"].as_array().unwrap() {
        let text = case["text"].as_str().unwrap();
        if json!(parse_cache_paths(text.as_bytes())) != case["paths"] {
            failed.push(format!(
                "parse_cache_paths({text:?}) = {:?}, want {}",
                parse_cache_paths(text.as_bytes()),
                case["paths"]
            ));
        }
    }
    assert!(failed.is_empty(), "\n{}", failed.join("\n"));
}

#[test]
fn every_update_does_what_the_python_does() {
    let doc: Value = serde_json::from_str(CASES).unwrap();
    let (mut ran, mut declined, mut failed) = (0usize, 0usize, Vec::new());
    for case in doc["flows"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let dir = temp_dir();
        for (file, text) in case["files"].as_object().unwrap() {
            fs::write(dir.join(file), text.as_str().unwrap()).unwrap();
        }
        let t = script(&case["script"]);
        let stamp = case["stamp"].as_str().unwrap();
        let env_value = |key: &str| case["env"].get(key).and_then(Value::as_str);
        let env = Env {
            stamp,
            plugin_order_url: env_value("MLOX_PLUGIN_ORDER_URL"),
            rules_url_template: env_value("MLOX_RULES_URL_TEMPLATE"),
            momw_api_host: env_value("MOMW_API_HOST"),
        };
        let table: BTreeMap<String, Result<usize, String>> = case
            .get("parse")
            .and_then(Value::as_array)
            .map(|rows| {
                rows.iter()
                    .map(|r| {
                        let text = r["text"].as_str().unwrap().to_string();
                        let res = match r.get("entries") {
                            Some(n) => Ok(n.as_u64().unwrap() as usize),
                            None => Err(r["error"].as_str().unwrap().to_string()),
                        };
                        (text, res)
                    })
                    .collect()
            })
            .unwrap_or_default();
        let parse = |data: &[u8]| {
            table.get(String::from_utf8_lossy(data).as_ref()).cloned().expect("a recorded text")
        };
        let result = match case["kind"].as_str().unwrap() {
            "yml" => {
                let urls: Option<Vec<String>> = case
                    .get("urls")
                    .map(|u| u.as_array().unwrap().iter().map(|s| s.as_str().unwrap().to_string()).collect());
                update_plugin_order_yml(
                    &dir.join(case["file"].as_str().unwrap()),
                    urls.as_deref(),
                    45,
                    &env,
                    &t,
                    &parse,
                )
            }
            "paths" => fetch_list_data_paths(
                &dir.join(case["file"].as_str().unwrap()),
                case["list"].as_str().unwrap(),
                case.get("host").and_then(Value::as_str),
                45,
                &env,
                &t,
            ),
            _ => {
                let paths: Vec<PathBuf> =
                    case["paths"].as_array().unwrap().iter().map(|p| dir.join(p.as_str().unwrap())).collect();
                update_rule_files(&paths, case.get("template").and_then(Value::as_str), 30, &env, &t)
            }
        };
        match result {
            Ok(report) => {
                ran += 1;
                let mut after = serde_json::Map::new();
                let mut names: Vec<_> = fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name()).collect();
                names.sort();
                for n in names {
                    let text = String::from_utf8_lossy(&fs::read(dir.join(&n)).unwrap()).into_owned();
                    after.insert(n.to_string_lossy().into_owned(), json!(text));
                }
                // a JSON parser's own words about its syntax error are masked on both sides
                let mask = |lines: &[String]| -> Vec<String> {
                    lines
                        .iter()
                        .map(|l| match (l.find("not a cfg-generator response ("), l.rfind(") (is '")) {
                            (Some(a), Some(b)) if case["parser_detail"].as_bool() == Some(true) && a < b => {
                                format!("{}...{}", &l[..a + 30], &l[b..])
                            }
                            _ => l.clone(),
                        })
                        .collect()
                };
                let want: Vec<String> = case["report"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|l| l.as_str().unwrap().to_string())
                    .collect();
                if mask(&report) != mask(&want) {
                    failed.push(format!("{name}: report is {report:?}, want {}", case["report"]));
                }
                if Value::Object(after) != case["after"] {
                    failed.push(format!("{name}: the files left are not Python's"));
                }
                if json!(t.asked.borrow().clone()) != case["requested"] {
                    failed.push(format!(
                        "{name}: asked for {:?}, want {}",
                        t.asked.borrow(),
                        case["requested"]
                    ));
                }
            }
            Err(UpdateError::Unsupported) if name.contains("is not read") => declined += 1,
            Err(e) => failed.push(format!("{name}: {e:?}")),
        }
        let _ = fs::remove_dir_all(&dir);
    }
    eprintln!("updates: {ran} ran, {declined} declined, {} wrong", failed.len());
    assert!(failed.is_empty(), "\n{}", failed.join("\n"));
}

#[test]
fn rule_files_are_as_old_as_python_says() {
    let doc: Value = serde_json::from_str(CASES).unwrap();
    for case in doc["ages"].as_array().unwrap() {
        let now = case["now"].as_f64().unwrap();
        let dir = temp_dir();
        let mut paths = Vec::new();
        for (name, days) in case["files"].as_object().unwrap() {
            let path = dir.join(name);
            if let Some(days) = days.as_f64() {
                let f = fs::File::create(&path).unwrap();
                let at = now - days * 86400.0;
                let t = if at >= 0.0 { UNIX_EPOCH + Duration::from_secs_f64(at) } else { UNIX_EPOCH };
                f.set_modified(t).unwrap();
            }
            paths.push(path);
        }
        let got: Vec<Value> = rule_file_ages(&paths, now).into_iter().map(|(n, a)| json!([n, a])).collect();
        assert_eq!(Value::Array(got), case["ages"], "{}", case["name"]);
        let _ = fs::remove_dir_all(&dir);
    }
}
