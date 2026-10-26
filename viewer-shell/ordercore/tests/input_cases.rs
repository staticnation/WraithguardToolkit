//! The shared input cases (tests/fixtures/input_cases.json at the repository's root), which the
//! Python side runs too: the subset's lines and TOML, the customizations TOML, grass, the
//! folders, a list's data paths, the cfg's orphans, and a mods folder made on disk for the scan,
//! the `data=` anchors and the master check.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use ordercore::export::DataInsert;
use ordercore::inputs::*;
use serde_json::{json, Value};

const CASES: &str = include_str!("../../../tests/fixtures/input_cases.json");

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

fn inserts_json(inserts: &[DataInsert]) -> Value {
    inserts.iter().map(|i| json!({"value": i.value, "after": i.after, "before": i.before})).collect()
}

#[test]
fn subset_lines_and_their_grass_read_as_recorded() {
    for case in doc()["lines"].as_array().unwrap() {
        let lines = strs(&case["lines"]);
        let got = subset_from_lines(&lines).unwrap();
        assert_eq!(got.plugins, strs(&case["plugins"]), "{}", case["name"]);
        assert_eq!(got.data, strs(&case["data"]), "{}", case["name"]);
        assert_eq!(got.skipped, strs(&case["skipped"]), "{}", case["name"]);
        let (grass, not_grass) = groundcover_declarations(&lines).unwrap();
        assert_eq!(grass, strs(&case["groundcover"]), "{}", case["name"]);
        assert_eq!(not_grass, strs(&case["not_groundcover"]), "{}", case["name"]);
    }
}

#[test]
fn the_subset_files_toml_reads_as_recorded() {
    for case in doc()["subset_tomls"].as_array().unwrap() {
        let got = subset_from_toml(case["text"].as_str().unwrap());
        if case.get("declined").is_some() || case.get("error").is_some() {
            assert!(got.is_err(), "{}: read, where the Python is to answer", case["name"]);
            continue;
        }
        let got = got.unwrap();
        assert_eq!(got.entries.plugins, strs(&case["plugins"]), "{}", case["name"]);
        assert_eq!(got.entries.data, strs(&case["data"]), "{}", case["name"]);
        assert_eq!(got.entries.skipped, strs(&case["skipped"]), "{}", case["name"]);
        assert_eq!(got.groundcover, strs(&case["groundcover"]), "{}", case["name"]);
    }
}

#[test]
fn a_customizations_toml_reads_as_recorded() {
    for case in doc()["customizations"].as_array().unwrap() {
        let got = customizations(case["text"].as_str().unwrap());
        if case.get("declined").is_some() || case.get("error").is_some() {
            assert!(got.is_err(), "{}: read, where the Python is to answer", case["name"]);
            continue;
        }
        let got = got.unwrap_or_else(|_| panic!("{}: declined", case["name"]));
        assert_eq!(got.subset, strs(&case["subset"]), "{}", case["name"]);
        assert_eq!(inserts_json(&got.data), case["data"], "{}", case["name"]);
        let mut dest = got.replace_dest.clone();
        dest.sort();
        assert_eq!(dest, strs(&case["replace_dest"]), "{}", case["name"]);
        let want: BTreeMap<String, String> =
            case["list_names"].as_object().unwrap().iter().map(|(k, v)| (k.clone(), s(v))).collect();
        assert_eq!(got.list_names, want, "{}", case["name"]);
    }
}

#[test]
fn grass_is_held_back_and_folders_gathered_as_recorded() {
    let d = doc();
    for case in d["holdbacks"].as_array().unwrap() {
        let (kept, held) = hold_back_groundcover(&strs(&case["subset"]), &strs(&case["groundcover"]));
        assert_eq!(kept, strs(&case["kept"]));
        assert_eq!(held, strs(&case["held"]));
    }
    for case in d["scan_dirs"].as_array().unwrap() {
        let (order, raw, ins) = (strs(&case["data_order"]), strs(&case["raw"]), strs(&case["inserts"]));
        assert_eq!(all_scan_dirs(&order, &raw, &ins).unwrap(), strs(&case["all"]));
        assert_eq!(pending_custom_dirs(&raw, &ins), strs(&case["pending"]));
    }
}

#[test]
fn a_lists_data_paths_are_matched_as_recorded() {
    for case in doc()["data_paths"].as_array().unwrap() {
        let tails = parse_data_paths_cache(case["text"].as_str().unwrap());
        assert_eq!(tails, strs(&case["tails"]), "{}", case["name"]);
        let cfg = strs(&case["cfg"]);
        let mut managed: Vec<String> = managed_cfg_data_path_norms(&tails, &cfg).into_iter().collect();
        managed.sort();
        assert_eq!(managed, strs(&case["managed"]), "{}", case["name"]);
        assert_eq!(reconcile_data_paths(&tails, &cfg, false), strs(&case["reconcile"]), "{}", case["name"]);
        assert_eq!(reconcile_data_paths(&tails, &cfg, true), strs(&case["reconcile_listed"]), "{}", case["name"]);
    }
}

#[test]
fn the_cfgs_orphans_are_pulled_as_recorded() {
    for case in doc()["orphans"].as_array().unwrap() {
        let (subset, base, data, declared_data) =
            (strs(&case["subset"]), strs(&case["base_order"]), strs(&case["data_order"]), strs(&case["declared_data"]));
        let tails = case["tails"].as_array().map(|_| strs(&case["tails"]));
        let q = OrphanQuery {
            subset: &subset,
            base_order_names: &base,
            data_order: &data,
            curated_lower: set(&case["curated"]),
            needs_cleaning_lower: set(&case["needs_cleaning"]),
            declared_lower: set(&case["declared"]),
            groundcover_lower: set(&case["groundcover"]),
            declared_data: &declared_data,
            tails: tails.as_deref(),
        };
        let (plugins, orphan_data) = cfg_orphans(&q).unwrap();
        assert_eq!(plugins, strs(&case["new_plugins"]), "{}", case["name"]);
        assert_eq!(orphan_data.unwrap_or_default(), strs(&case["orphan_data"]), "{}", case["name"]);
    }
}

// ---------------------------------------------------------------- trees on disk

fn plugin_bytes(masters: &[(String, u64)], data: bool, pad: usize) -> Vec<u8> {
    let mut body = b"HEDR".to_vec();
    body.extend(300u32.to_le_bytes());
    body.extend(1.3f32.to_le_bytes());
    body.extend([0u8; 4 + 32 + 256 + 4]);
    for (name, size) in masters {
        let raw: Vec<u8> = name.chars().map(|c| u8::try_from(u32::from(c)).unwrap()).chain([0]).collect();
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

fn build_tree(root: &Path, files: &Value) {
    for (rel, spec) in files.as_object().unwrap() {
        let path = root.join(rel);
        if spec.is_null() {
            fs::create_dir_all(&path).unwrap();
            continue;
        }
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        if let Some(text) = spec.as_str() {
            fs::write(&path, text).unwrap();
        } else {
            let masters: Vec<(String, u64)> = spec
                .get("masters")
                .and_then(Value::as_array)
                .map(|m| m.iter().map(|p| (s(&p[0]), p[1].as_u64().unwrap())).collect())
                .unwrap_or_default();
            let data = spec.get("data").and_then(Value::as_bool).unwrap_or(true);
            let pad = spec.get("pad").and_then(Value::as_u64).unwrap_or(0);
            fs::write(&path, plugin_bytes(&masters, data, usize::try_from(pad).unwrap())).unwrap();
        }
    }
}

fn rooted(v: &str, root: &str) -> String {
    v.replace("<root>", root)
}

fn unrooted(v: &str, root: &str) -> String {
    v.replace(root, "<root>").replace('\\', "/")
}

fn tree_root(name: &str) -> PathBuf {
    let slug: String = name.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    let d = std::env::temp_dir().join(format!("ordercore_inputs_{}_{slug}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    // the Python resolves the temp folder (a macOS /var is /private/var), so the paths it records
    // start where the folder really is
    fs::canonicalize(&d).unwrap()
}

#[test]
fn a_mods_folder_is_scanned_anchored_and_master_checked_as_recorded() {
    for case in doc()["trees"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let dir = tree_root(name);
        let root = dir.to_string_lossy().into_owned();
        // Windows' canonical form is a verbatim path (\\?\C:\...), which no cfg holds
        let root = root.strip_prefix(r"\\?\").unwrap_or(&root).to_string();
        build_tree(Path::new(&root), &case["files"]);

        let scan = scan_mod_directories(&rooted(case["scan"].as_str().unwrap(), &root));
        let mut groups: Vec<Vec<String>> = Vec::new();
        let mut current = Vec::new();
        for line in &scan.lines {
            if line.is_empty() {
                groups.push(std::mem::take(&mut current));
            } else {
                current.push(unrooted(line, &root));
            }
        }
        groups.sort();
        let want: Vec<Vec<String>> = case["scan_groups"].as_array().unwrap().iter().map(strs).collect();
        assert_eq!(groups, want, "{name}");
        assert_eq!(json!([scan.folders, scan.plugins]), case["scan_counts"], "{name}");

        let mut inserts: Vec<DataInsert> = case["inserts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| DataInsert {
                value: rooted(&s(&i["value"]), &root),
                after: opt(&i["after"]).map(|a| rooted(&a, &root)),
                before: opt(&i["before"]).map(|b| rooted(&b, &root)),
            })
            .collect();
        let data_order: Vec<String> = strs(&case["data_order"]).iter().map(|l| rooted(l, &root)).collect();
        let notes = infer_data_path_anchors(&mut inserts, &data_order, &strs(&case["final_order"]), Some(Path::new(&root)))
            .unwrap();
        let back: Vec<DataInsert> = inserts
            .iter()
            .map(|i| DataInsert {
                value: unrooted(&i.value, &root),
                after: i.after.as_deref().map(|a| unrooted(a, &root)),
                before: i.before.as_deref().map(|b| unrooted(b, &root)),
            })
            .collect();
        assert_eq!(inserts_json(&back), case["anchored"], "{name}");
        let worded: Vec<String> = notes
            .iter()
            .map(|n| {
                unrooted(
                    &format!("Inferred anchor for '{}': {} '{}' (via plugin {})", n.value, n.mode, n.anchor, n.via),
                    &root,
                )
            })
            .collect();
        assert_eq!(worded, strs(&case["anchor_notes"]), "{name}");

        let dirs: Vec<String> = strs(&case["dirs"]).iter().map(|d| rooted(d, &root)).collect();
        let origins: HashMap<String, String> =
            case["origins"].as_object().unwrap().iter().map(|(k, v)| (k.clone(), s(v))).collect();
        let got = check_missing_masters(&strs(&case["active"]), &PluginIndex::build(&dirs), &origins);
        let m = &case["masters"];
        assert_eq!(got.missing, strs(&m["missing"]), "{name}");
        assert_eq!(got.order_problems, strs(&m["order_problems"]), "{name}");
        assert_eq!(got.size_notes, strs(&m["size_notes"]), "{name}");
        assert_eq!(json!(got.checked), m["checked"], "{name}");
        assert_eq!(got.problem_plugins, strs(&m["problem_plugins"]), "{name}");
        let _ = fs::remove_dir_all(&dir);
    }
}
