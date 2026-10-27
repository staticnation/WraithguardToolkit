//! The shared plan cases (tests/fixtures/plan_cases.json at the repository's root), which the
//! Python side runs too: a whole sort over a setup made on disk - the plan, every line printed
//! and every warning logged, or the message the run stops with.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

use ordercore::export::DataInsert;
use ordercore::export::LineEnding;
use ordercore::plan::{compute_plan, export, Counts, ExportArgs, PlanArgs, PlanError};
use serde_json::{json, Value};

const CASES: &str = include_str!("../../../tests/fixtures/plan_cases.json");

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

fn plugin_bytes(masters: &[(String, u64)]) -> Vec<u8> {
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
        body.extend(b"DATA");
        body.extend(8u32.to_le_bytes());
        body.extend(size.to_le_bytes());
    }
    let mut out = b"TES3".to_vec();
    out.extend(u32::try_from(body.len()).unwrap().to_le_bytes());
    out.extend([0u8; 8]);
    out.extend(body);
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
            // A TOML basic string reads a backslash as an escape (`C:\Users` is `\U`), so a
            // Windows folder goes into a TOML file escaped: it reads back as the same path the
            // cfg's `data=` lines hold, which the anchors are matched against as text.
            let here = if rel.ends_with(".toml") { r.replace('\\', "\\\\") } else { r.clone() };
            fs::write(&path, text.replace("<root>", &here)).unwrap();
            continue;
        }
        let masters: Vec<(String, u64)> = spec["masters"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| (s(&m[0]), m[1].as_u64().unwrap()))
            .collect();
        fs::write(&path, plugin_bytes(&masters)).unwrap();
        if let Some(t) = spec.get("mtime").and_then(Value::as_u64) {
            let f = fs::File::options().write(true).open(&path).unwrap();
            f.set_modified(UNIX_EPOCH + Duration::from_secs(t)).unwrap();
        }
    }
}

/// The export's flags off the command line.
fn export_args_of(argv: &[String], root: &str) -> ExportArgs {
    let argv: Vec<String> = argv.iter().map(|a| a.replace("<root>", root)).collect();
    let after = |flag: &str| argv.iter().position(|a| a == flag).map(|i| argv[i + 1].clone());
    let has = |flag: &str| argv.iter().any(|a| a == flag);
    ExportArgs {
        write_cfg: has("--write-cfg"),
        cfg: after("--cfg").unwrap(),
        emit_toml: after("--emit-toml"),
        dry_run: has("--dry-run"),
        no_backup: has("--no-backup"),
        sort_data_paths: has("--sort-data-paths"),
        list_name: after("--list-name"),
        stamp: "20261022-120000".into(),
        toml_line_ending: Some(LineEnding::Lf),
    }
}

fn opt_strs(v: Option<&Value>, root: &str) -> Option<Vec<String>> {
    v.and_then(Value::as_array).map(|a| a.iter().map(|x| s(x).replace("<root>", root)).collect())
}

/// The command line as `build_arg_parser` reads the flags these cases use.
fn args_of(argv: &[String], extra: &Value, root: &str) -> PlanArgs {
    let argv: Vec<String> = argv.iter().map(|a| a.replace("<root>", root)).collect();
    let mut a = PlanArgs::default();
    let mut i = 0;
    let values = |i: &mut usize| -> Vec<String> {
        let mut out = Vec::new();
        while *i + 1 < argv.len() && !argv[*i + 1].starts_with('-') {
            *i += 1;
            out.push(argv[*i].clone());
        }
        out
    };
    while i < argv.len() {
        let flag = argv[i].clone();
        match flag.as_str() {
            "--sort-data-paths" => a.sort_data_paths = true,
            "--subset-from-cfg" => a.subset_from_cfg = true,
            "--no-predicate-warnings" => a.no_predicate_warnings = true,
            "--write-cfg" | "--dry-run" | "--no-backup" => {}
            "-v" | "--verbose" => a.verbose = true,
            "--rules" => a.rules = values(&mut i),
            "--subset" => a.subset = values(&mut i),
            "--groundcover" => a.groundcover = values(&mut i),
            _ => {
                let v = values(&mut i).pop();
                match flag.as_str() {
                    "--cfg" => a.cfg = v.unwrap(),
                    "--customizations" => a.customizations = v,
                    "--subset-file" => a.subset_file = v,
                    "--scan-dir" => a.scan_dir = v,
                    "--plugin-order-yml" => a.plugin_order_yml = v,
                    "--list-name" => a.list_name = v,
                    "--data-paths-cache" => a.data_paths_cache = v,
                    "--emit-toml" => a.emit_toml = v.is_some(),
                    other => panic!("a flag the test does not read: {other}"),
                }
            }
        }
        i += 1;
    }
    if let Some(lines) = extra.get("subset_lines").and_then(Value::as_array) {
        a.subset_lines = Some(lines.iter().map(|l| s(l).replace("<root>", root)).collect());
    }
    a.allow_empty_sort = extra.get("allow_empty_sort").and_then(Value::as_bool).unwrap_or(false);
    a
}

fn inserts(v: &[DataInsert], root: &str) -> Value {
    v.iter()
        .map(|d| {
            let u = |o: &Option<String>| o.as_deref().map(|x| unrooted(x, root));
            json!({"value": unrooted(&d.value, root), "after": u(&d.after), "before": u(&d.before)})
        })
        .collect()
}

fn root_for(name: &str) -> PathBuf {
    let slug: String = name.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).take(40).collect();
    let d = std::env::temp_dir().join(format!("ordercore_plan_{}_{slug}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    let c = fs::canonicalize(&d).unwrap();
    let text = c.to_string_lossy().into_owned();
    PathBuf::from(text.strip_prefix(r"\\?\").unwrap_or(&text).to_string())
}

#[test]
fn a_whole_sort_gives_the_recorded_plan_and_report() {
    let doc: Value = serde_json::from_str(CASES).unwrap();
    for case in doc["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let dir = root_for(name);
        let root = dir.to_string_lossy().into_owned();
        make_setup(&dir, &case["files"]);
        let argv: Vec<String> = case["argv"].as_array().unwrap().iter().map(s).collect();
        let args = args_of(&argv, &case["extra"], &root);
        let un = |lines: Vec<String>| -> Vec<String> { lines.iter().map(|l| unrooted(l, &root)).collect() };
        let printed = |v: &Value| -> Vec<String> { v.as_array().unwrap().iter().map(s).collect() };
        match compute_plan(&args) {
            Err(PlanError::Refused { message, report }) => {
                assert_eq!(Some(message.as_str()), case["refused"].as_str(), "{name}");
                assert_eq!(un(report.text()), printed(&case["printed"]), "{name}");
            }
            Err(PlanError::Unsupported) => panic!("{name}: declined"),
            Ok(got) => {
                assert!(case.get("refused").is_none(), "{name}: ran, where the Python stops");
                let text = un(got.report.text());
                let want = printed(&case["printed"]);
                if text != want {
                    let at = text.iter().zip(&want).position(|(a, b)| a != b).unwrap_or(text.len().min(want.len()));
                    panic!("{name}: line {at}\n got: {:?}\nwant: {:?}", text.get(at), want.get(at));
                }
                assert_eq!(un(got.report.logged()), printed(&case["logged"]), "{name}");
                let p = &got.plan;
                let u = |v: &[String]| -> Value { un(v.to_vec()).into() };
                let mut dest: Vec<String> = p.replace_dest_names.iter().cloned().collect();
                dest.sort();
                let anchors: BTreeMap<String, Value> =
                    p.custom_anchors.iter().map(|(k, (how, a))| (unrooted(k, &root), json!([how, a]))).collect();
                let values: BTreeMap<String, String> =
                    p.original_content_values.iter().map(|(k, v)| (unrooted(k, &root), unrooted(v, &root))).collect();
                let data_result = p.data_result.as_ref().map(|r| {
                    r.iter()
                        .map(|(l, n, v)| json!([unrooted(l, &root), n, v.as_deref().map(|x| unrooted(x, &root))]))
                        .collect::<Vec<_>>()
                });
                let got_plan = json!({
                    "subset": u(&p.subset),
                    "lines": u(&p.lines),
                    "content_positions": p.content_positions,
                    "data_positions": p.data_positions,
                    "data_order": u(&p.data_order),
                    "final_order": if got.sorted { u(&p.final_order) } else { Value::Null },
                    "data_result": data_result,
                    "predicate_warnings": u(&got.predicate_warnings),
                    "yml_warnings": u(&got.yml_warnings),
                    "master_warnings": u(&got.master_warnings),
                    "master_problem_plugins": u(&got.master_problem_plugins),
                    "custom_anchors": anchors,
                    "original_content_values": values,
                    "replace_dest_names": dest,
                    "raw_toml_data_inserts": inserts(&p.raw_toml_data_inserts, &root),
                    "data_inserts": inserts(&p.data_inserts, &root),
                    "orphan_data_paths": u(&p.orphan_data_paths),
                    "base_order_names": u(&p.base_order_names),
                    "curated_set": u(&p.curated_set),
                    "new_groundcover": u(&p.new_groundcover),
                    "scan_dirs": u(&got.scan_dirs),
                    "custom_data_dirs": u(&got.custom_data_dirs),
                });
                for (key, want) in case["plan"].as_object().unwrap() {
                    assert_eq!(&got_plan[key], want, "{name}: {key}");
                }
                if let Some(want) = case.get("exported") {
                    let spec = &case["extra"]["export"];
                    let before: Vec<String> =
                        fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
                    let counts =
                        Counts { rule_warnings: got.predicate_warnings.len(), yml_warnings: got.yml_warnings.len() };
                    let final_order = opt_strs(spec.get("final_order"), &root);
                    let data_order = opt_strs(spec.get("data_order"), &root);
                    let disabled_plugins = opt_strs(spec.get("disabled_plugins"), &root).unwrap_or_default();
                    let disabled_data = opt_strs(spec.get("disabled_data"), &root).unwrap_or_default();
                    let done = export(
                        &export_args_of(&argv, &root),
                        &got.plan,
                        counts,
                        final_order.as_deref(),
                        data_order.as_deref(),
                        &disabled_plugins,
                        &disabled_data,
                    )
                    .unwrap_or_else(|e| panic!("{name}: the export failed: {e:?}"));
                    let text = un(done.report.text());
                    let want_text = printed(&want["printed"]);
                    if text != want_text {
                        let at = text.iter().zip(&want_text).position(|(a, b)| a != b).unwrap_or(text.len().min(want_text.len()));
                        panic!("{name}: export line {at}\n got: {:?}\nwant: {:?}", text.get(at), want_text.get(at));
                    }
                    assert_eq!(json!(done.written.wrote_cfg), want["wrote_cfg"], "{name}");
                    assert_eq!(json!(done.written.wrote_toml), want["wrote_toml"], "{name}");
                    let mut files = BTreeMap::new();
                    for e in fs::read_dir(&dir).unwrap().flatten() {
                        let n = e.file_name().to_string_lossy().into_owned();
                        if e.path().is_file() && (!before.contains(&n) || n == "openmw.cfg" || n == "out.toml") {
                            files.insert(n, Value::from(unrooted(&fs::read_to_string(e.path()).unwrap(), &root)));
                        }
                    }
                    assert_eq!(Value::from(serde_json::Map::from_iter(files)), want["files"], "{name}: the files");
                }
                if let Some(want) = case.get("scanned") {
                    let text = fs::read_to_string(dir.join("scanned.txt")).unwrap();
                    assert_eq!(unrooted(&text, &root), s(want), "{name}: the subset file written");
                }
            }
        }
        let _ = fs::remove_dir_all(&dir);
    }
}
