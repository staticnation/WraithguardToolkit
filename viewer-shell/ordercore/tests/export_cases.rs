//! The shared export cases (tests/fixtures/export_cases.json at the repository's root), which the
//! Python side runs too: the `data=` line helpers, `insert_data_paths`, the customizations TOML,
//! the cfg splice, and `write_plan` end to end over a temp folder.

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use ordercore::export::*;
use serde_json::{json, Value};

const CASES: &str = include_str!("../../../tests/fixtures/export_cases.json");

fn doc() -> Value {
    serde_json::from_str(CASES).unwrap()
}

fn s(v: &Value) -> String {
    v.as_str().unwrap().to_string()
}

fn opt(v: &Value) -> Option<String> {
    v.as_str().map(str::to_string)
}

fn strs(v: &Value) -> Vec<String> {
    v.as_array().unwrap().iter().map(s).collect()
}

fn set(v: &Value) -> HashSet<String> {
    strs(v).into_iter().collect()
}

fn inserts(v: &Value) -> Vec<DataInsert> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|i| DataInsert { value: s(&i["value"]), after: opt(&i["after"]), before: opt(&i["before"]) })
        .collect()
}

fn data_lines(v: &Value) -> Option<Vec<DataLine>> {
    v.as_array().map(|rows| rows.iter().map(|r| (s(&r[0]), r[1].as_bool().unwrap(), opt(&r[2]))).collect())
}

fn blocks(v: &Value) -> Vec<CustomBlock> {
    let list = |b: &Value, key: &str| b.get(key).map(strs).unwrap_or_default();
    v.as_array()
        .unwrap()
        .iter()
        .map(|b| CustomBlock {
            list_name: b.get("listName").and_then(opt),
            remove_data: list(b, "removeData"),
            remove_content: list(b, "removeContent"),
            remove_fallback: list(b, "removeFallback"),
            remove_groundcover: list(b, "removeGroundcover"),
            replace: b
                .get("replace")
                .and_then(Value::as_array)
                .map(|rs| {
                    rs.iter()
                        .map(|r| Replace {
                            source: r.get("source").and_then(opt),
                            dest: r.get("dest").and_then(opt),
                        })
                        .collect()
                })
                .unwrap_or_default(),
            append: b
                .get("append")
                .and_then(Value::as_array)
                .map(|rs| {
                    rs.iter()
                        .map(|r| Append {
                            append: r.get("append").and_then(opt),
                            append_block: r.get("appendBlock").and_then(opt),
                        })
                        .collect()
                })
                .unwrap_or_default(),
        })
        .collect()
}

fn anchors(v: &Value) -> BTreeMap<String, (String, Option<String>)> {
    v.as_object().unwrap().iter().map(|(k, a)| (k.clone(), (s(&a[0]), opt(&a[1])))).collect()
}

fn plan(v: &Value) -> Plan {
    Plan {
        lines: strs(&v["lines"]),
        content_positions: v["content_positions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n.as_u64().unwrap() as usize)
            .collect(),
        data_positions: v["data_positions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n.as_u64().unwrap() as usize)
            .collect(),
        final_order: strs(&v["final_order"]),
        data_result: data_lines(&v["data_result"]),
        new_groundcover: strs(&v["new_groundcover"]),
        subset: strs(&v["subset"]),
        curated_set: strs(&v["curated_set"]),
        base_order_names: strs(&v["base_order_names"]),
        data_order: strs(&v["data_order"]),
        data_inserts: inserts(&v["data_inserts"]),
        raw_toml_data_inserts: inserts(&v["raw_toml_data_inserts"]),
        orphan_data_paths: strs(&v["orphan_data_paths"]),
        original_toml_blocks: v["original_toml_data"].get("Customizations").map(blocks).unwrap_or_default(),
        original_content_values: v["original_content_values"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, x)| (k.clone(), s(x)))
            .collect(),
        replace_dest_names: set(&v["replace_dest_names"]),
        custom_anchors: anchors(&v["custom_anchors"]),
    }
}

fn temp_dir() -> PathBuf {
    static N: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "ordercore-export-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn every_line_helper_answers_as_python_does() {
    let doc = doc();
    let mut failed = Vec::new();
    fn check(failed: &mut Vec<String>, what: &str, input: &str, got: String, want: &Value) {
        if json!(got) != *want {
            failed.push(format!("{what}({input:?}) = {got:?}, want {want}"));
        }
    }
    for c in doc["lines"]["strings"].as_array().unwrap() {
        let x = s(&c["s"]);
        check(&mut failed, "escape", &x, escape_cfg_value(&x), &c["escape"]);
        check(&mut failed, "unescape", &x, unescape_cfg_value(&x), &c["unescape"]);
        check(&mut failed, "format bare", &x, format_data_line(&x, false), &c["format_bare"]);
        check(&mut failed, "format quoted", &x, format_data_line(&x, true), &c["format_quoted"]);
        check(&mut failed, "normalize", &x, normalize_data_path(&x), &c["normalize"]);
        check(&mut failed, "toml_value", &x, toml_value(&x), &c["toml"]);
        let extract = extract_data_path_value(&x).unwrap();
        if json!(extract) != c["extract"] {
            failed.push(format!("extract({x:?}) = {extract:?}, want {}", c["extract"]));
        }
        if json!(is_base_data_path(&x)) != c["base"] {
            failed.push(format!("is_base_data_path({x:?})"));
        }
        if json!(cfg_line_value(&x)) != c["cfg_value"] {
            failed.push(format!("cfg_line_value({x:?}) = {:?}, want {}", cfg_line_value(&x), c["cfg_value"]));
        }
    }
    for c in doc["lines"]["quoting"].as_array().unwrap() {
        let lines = strs(&c["lines"]);
        if json!(detect_data_quoting(&lines).unwrap()) != c["quoted"] {
            failed.push(format!("detect_data_quoting({lines:?})"));
        }
    }
    for c in doc["lines"]["anchors"].as_array().unwrap() {
        let got = find_anchor_index(&strs(&c["lines"]), c["anchor"].as_str().unwrap());
        if json!(got) != c["index"] {
            failed.push(format!("find_anchor_index({}) = {got:?}, want {}", c["anchor"], c["index"]));
        }
    }
    for c in doc["lines"]["removes"].as_array().unwrap() {
        let got = configurator_remove_matches(c["value"].as_str().unwrap(), c["line"].as_str().unwrap());
        if json!(got) != c["matches"] {
            failed.push(format!("remove_matches({}, {}) = {got}", c["value"], c["line"]));
        }
    }
    for c in doc["lines"]["covers"].as_array().unwrap() {
        let got = curated_covers(c["name"].as_str().unwrap(), &set(&c["curated"]), &set(&c["cleaning"]));
        if json!(got) != c["covers"] {
            failed.push(format!("curated_covers({})", c["name"]));
        }
    }
    for c in doc["lines"]["orphans"].as_array().unwrap() {
        let (plugins, data) = orphan_cfg_entries(
            &strs(&c["content"]),
            &strs(&c["data"]),
            &set(&c["curated"]),
            &set(&c["cleaning"]),
            &set(&c["declared_plugins"]),
            &set(&c["declared_data"]),
        )
        .unwrap();
        if json!([plugins, data]) != c["orphans"] {
            failed
                .push(format!("orphans of {} are {plugins:?} {data:?}, want {}", c["content"], c["orphans"]));
        }
    }
    assert!(failed.is_empty(), "\n{}", failed[..failed.len().min(40)].join("\n"));
}

#[test]
fn every_insert_lands_where_python_puts_it() {
    let mut failed = Vec::new();
    for c in doc()["inserts"].as_array().unwrap() {
        let (got, notes) = insert_data_paths(&strs(&c["lines"]), &inserts(&c["inserts"])).unwrap();
        let rows = Value::Array(got.iter().map(|(l, n, v)| json!([l, n, v])).collect());
        if rows != c["result"] || json!(notes) != c["notes"] {
            failed.push(format!("{}: {rows} {notes:?}, want {} {}", c["name"], c["result"], c["notes"]));
        }
    }
    assert!(failed.is_empty(), "\n{}", failed.join("\n"));
}

#[test]
fn every_customizations_toml_reads_as_python_writes_it() {
    let mut failed = Vec::new();
    for c in doc()["tomls"].as_array().unwrap() {
        let original = blocks(&c["original_blocks"]);
        let order = strs(&c["final_content_order"]);
        let subset = strs(&c["subset"]);
        let values: BTreeMap<String, String> = c["original_content_values"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), s(v)))
            .collect();
        let result = data_lines(&c["data_result"]);
        let raw = c["raw_data_inserts"].as_array().map(|_| inserts(&c["raw_data_inserts"]));
        let replace = set(&c["replace_dest_names"]);
        let users = strs(&c["user_data_values"]);
        let remove_content = strs(&c["remove_content"]);
        let remove_data = strs(&c["remove_data"]);
        let custom = anchors(&c["custom_anchors"]);
        let grass = strs(&c["new_groundcover"]);
        let list_name = opt(&c["list_name"]);
        let got = generate_customizations_toml(&TomlInput {
            original_blocks: &original,
            final_content_order: &order,
            subset: &subset,
            original_content_values: &values,
            data_result: result.as_deref(),
            raw_data_inserts: raw.as_deref(),
            replace_dest_names: &replace,
            user_data_values: &users,
            list_name: list_name.as_deref(),
            remove_content: &remove_content,
            remove_data: &remove_data,
            custom_anchors: &custom,
            new_groundcover: &grass,
        })
        .unwrap();
        if json!(got.text) != c["text"] || json!(got.warnings) != c["warnings"] {
            failed.push(format!("{}:\n{}\nwant\n{}", c["name"], got.text, c["text"]));
        }
    }
    assert!(failed.is_empty(), "\n{}", failed[..failed.len().min(4)].join("\n"));
}

#[test]
fn every_cfg_is_read_and_spliced_as_python_does() {
    let doc = doc();
    let mut failed = Vec::new();
    for c in doc["cfgs"].as_array().unwrap() {
        let text = decode_cfg(c["text"].as_str().unwrap().as_bytes()).unwrap();
        let read = read_cfg_text(&text).unwrap();
        let order: Vec<Value> = read.content_order.iter().map(|(v, l)| json!([v, l])).collect();
        let ok = json!(read.lines) == c["lines"]
            && json!(read.content_positions) == c["content_positions"]
            && Value::Array(order) == c["content_order"]
            && json!(read.data_positions) == c["data_positions"]
            && json!(read.data_order) == c["data_order"]
            && json!(read_groundcover_names(&read.lines).unwrap()) == c["groundcover"];
        if !ok {
            failed.push(format!("{}: read as {read:?}", c["name"]));
        }
    }
    for c in doc["renders"].as_array().unwrap() {
        let p = plan(&c["plan"]);
        let (f, d) = (
            c["final_order"].as_array().map(|_| strs(&c["final_order"])),
            c["data_order"].as_array().map(|_| strs(&c["data_order"])),
        );
        let got = render_cfg_lines(&p, f.as_deref(), d.as_deref()).unwrap();
        if json!(got) != c["lines"] {
            failed.push(format!("{}: {got:?}, want {}", c["name"], c["lines"]));
        }
    }
    assert!(failed.is_empty(), "\n{}", failed.join("\n"));
}

#[test]
fn removals_are_as_python_decides() {
    let doc = doc();
    for c in doc["removals"]["plugins"].as_array().unwrap() {
        let got = plugins_needing_removal(&strs(&c["disabled"]), &strs(&c["curated"]), &strs(&c["base"]));
        assert_eq!(json!(got), c["result"]);
    }
    for c in doc["removals"]["data"].as_array().unwrap() {
        let got =
            data_paths_needing_removal(&strs(&c["disabled"]), &set(&c["ours"]), &set(&c["base"])).unwrap();
        assert_eq!(json!(got), c["result"]);
    }
}

#[test]
fn every_export_leaves_the_files_python_leaves() {
    let mut failed = Vec::new();
    for c in doc()["writes"].as_array().unwrap() {
        let name = s(&c["name"]);
        let dir = temp_dir();
        if let Some(text) = c["cfg"].as_str() {
            fs::write(dir.join("openmw.cfg"), text).unwrap();
        }
        if let Some(text) = c["toml"].as_str() {
            fs::write(dir.join("customizations.toml"), text).unwrap();
        }
        let o = &c["opts"];
        let cfg_path = dir.join("openmw.cfg");
        let toml_path = dir.join("customizations.toml");
        let list_name = opt(&o["list_name"]);
        let targets = Targets {
            write_cfg: o["write_cfg"].as_bool().unwrap().then_some(cfg_path.as_path()),
            emit_toml: o["emit_toml"].as_bool().unwrap().then_some(toml_path.as_path()),
            dry_run: o["dry_run"].as_bool().unwrap(),
            no_backup: o["no_backup"].as_bool().unwrap(),
            sort_data_paths: o["sort_data_paths"].as_bool().unwrap(),
            list_name: list_name.as_deref(),
            stamp: "20261022-120000",
            toml_line_ending: LineEnding::Lf,
        };
        let opt_vec = |key: &str| o.get(key).and_then(Value::as_array).map(|_| strs(&o[key]));
        let (final_order, data_order) = (opt_vec("final_order"), opt_vec("data_order"));
        let (dp, dd) =
            (opt_vec("disabled_plugins").unwrap_or_default(), opt_vec("disabled_data").unwrap_or_default());
        match write_plan(&targets, &plan(&c["plan"]), final_order.as_deref(), data_order.as_deref(), &dp, &dd)
        {
            Ok(w) => {
                let mut after = serde_json::Map::new();
                let mut names: Vec<_> = fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name()).collect();
                names.sort();
                for n in names {
                    let text = String::from_utf8(fs::read(dir.join(&n)).unwrap()).unwrap();
                    after.insert(n.to_string_lossy().into_owned(), json!(text));
                }
                if json!(w.wrote_cfg) != c["wrote_cfg"] || json!(w.wrote_toml) != c["wrote_toml"] {
                    failed.push(format!("{name}: wrote cfg {} toml {}", w.wrote_cfg, w.wrote_toml));
                }
                if Value::Object(after.clone()) != c["after"] {
                    let want = c["after"].as_object().unwrap();
                    let diff: Vec<&String> =
                        want.keys().chain(after.keys()).filter(|k| want.get(*k) != after.get(*k)).collect();
                    failed.push(format!("{name}: files differ: {diff:?}"));
                }
            }
            Err(e) => failed.push(format!("{name}: {e:?}")),
        }
        let _ = fs::remove_dir_all(&dir);
    }
    assert!(failed.is_empty(), "\n{}", failed.join("\n"));
}
