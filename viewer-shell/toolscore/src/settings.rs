//! Wraithguard's settings file (`wraithguard_toolkit_settings.json`, beside the app), read
//! for the viewer's pages (the Load order, Tools and Conflicts pages' fields) (port step 4.5, slice 2: settings and data migration, DROP-TK.md).
//!
//! The file is the Tk app's: a flat JSON object it rewrites on close. Since the load order moved
//! to the viewer (4.2, slice 2k) the Tk app no longer has the sort's fields, but a file written
//! before that still holds them (`rules`, `customizations`, `list_name` and the rest), and the
//! Tk app now keeps keys it does not know when it saves. So the person's setup carries over:
//! the Load order page opens on the rule files, TOML, list and options they last sorted with,
//! instead of empty fields.
//!
//! Read leniently, as the Python reads it: a value of the wrong type is passed over, never an
//! error, and a missing or unreadable file is no settings at all.
//!
//! Once Tk is gone the viewer owns the file: its Setup page and the pages' own fields write
//! back through [`write_fields`], in the Python's keys, keeping every key it does not know (so
//! a Tk app still in use, or an older file, loses nothing).

use std::path::Path;

use serde_json::{Map, Value};

/// The file's name, in the app's folder.
pub const FILE: &str = "wraithguard_toolkit_settings.json";

/// The settings, or an empty object for a missing, unreadable or malformed file.
pub fn read(path: &Path) -> Map<String, Value> {
    std::fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .and_then(|v| match v {
            Value::Object(m) => Some(m),
            _ => None,
        })
        .unwrap_or_default()
}

fn text(s: &Map<String, Value>, k: &str) -> Option<String> {
    s.get(k).and_then(Value::as_str).map(str::trim).filter(|v| !v.is_empty()).map(str::to_string)
}

fn flag(s: &Map<String, Value>, k: &str) -> Option<bool> {
    s.get(k).and_then(Value::as_bool)
}

/// The Load order page's fields from the settings (`59_wg_loadorder.js`'s names), only those
/// the file sets: `cfg`, `customizations`, `subsetFile`, `rules` and `groundcover` (one a line),
/// `pluginOrderYml`, `listName`, `dataPathsCache`, `emitToml`, and the flags `sortDataPaths`,
/// `subsetFromCfg`, `noPredicateWarnings`, `writeCfg`, `dryRun`, `noBackup`.
///
/// As the Tk window ran the sort: "write directly back to customizations.toml"
/// (`write_toml_inplace`) writes the TOML to the customizations file itself; the grass
/// plugins were one field, comma-separated.
pub fn loadorder_fields(s: &Map<String, Value>) -> Map<String, Value> {
    let mut out = Map::new();
    let mut put = |k: &str, v: Option<String>| {
        if let Some(v) = v {
            out.insert(k.to_string(), Value::String(v));
        }
    };
    put("cfg", text(s, "cfg"));
    put("customizations", text(s, "customizations"));
    put("subsetFile", text(s, "subset_file"));
    put("pluginOrderYml", text(s, "plugin_order_yml"));
    put("listName", text(s, "list_name"));
    put("dataPathsCache", text(s, "data_paths_cache"));
    let emit = if flag(s, "write_toml_inplace") == Some(true) { text(s, "customizations") } else { text(s, "emit_toml") };
    put("emitToml", emit);
    if let Some(rules) = s.get("rules").and_then(Value::as_array) {
        let lines: Vec<&str> = rules.iter().filter_map(Value::as_str).map(str::trim).filter(|r| !r.is_empty()).collect();
        put("rules", (!lines.is_empty()).then(|| lines.join("\n")));
    }
    if let Some(g) = text(s, "groundcover") {
        let names: Vec<&str> = g.split(',').map(str::trim).filter(|n| !n.is_empty()).collect();
        put("groundcover", (!names.is_empty()).then(|| names.join("\n")));
    }
    for (page, key) in [
        ("sortDataPaths", "sort_data_paths"),
        ("subsetFromCfg", "subset_from_cfg"),
        ("noPredicateWarnings", "no_predicate_warnings"),
        ("writeCfg", "write_cfg"),
        ("dryRun", "dry_run"),
        ("noBackup", "no_backup"),
    ] {
        if let Some(b) = flag(s, key) {
            out.insert(page.to_string(), Value::Bool(b));
        }
    }
    out
}

/// The Tools page's remembered fields from the settings: `tes3cmd` (the executable),
/// `mergeLandsFolder` (where Merge Lands writes, `merged_lands_out`).
pub fn tools_fields(s: &Map<String, Value>) -> Map<String, Value> {
    let mut out = Map::new();
    for (page, key) in [("tes3cmd", "tes3cmd"), ("mergeLandsFolder", "merged_lands_out")] {
        if let Some(v) = text(s, key) {
            out.insert(page.to_string(), Value::String(v));
        }
    }
    out
}

/// The Conflicts page's: `exclude`, the plugins the record scan leaves out (as typed:
/// patterns split by commas or lines).
pub fn conflicts_fields(s: &Map<String, Value>) -> Map<String, Value> {
    let mut out = Map::new();
    if let Some(v) = text(s, "exclude") {
        out.insert("exclude".into(), Value::String(v));
    }
    out
}

/// The viewer's theme code for Wraithguard's theme (`log_theme`, a preset's name), as
/// `tools/gen_viewer_themes.py` names the 23 presets: `wg-` and the name in lower case, each
/// run of other characters one `-`. `None` when the file names none.
pub fn theme_code(s: &Map<String, Value>) -> Option<String> {
    let name = text(s, "log_theme")?;
    let mut code = String::new();
    for c in name.to_lowercase().chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            code.push(c);
        } else if !code.ends_with('-') {
            code.push('-');
        }
    }
    let code = code.trim_matches('-');
    (!code.is_empty()).then(|| format!("wg-{code}"))
}

/// Every page's fields: `{loadorder, tools, conflicts, theme}` (`theme` the viewer's code for
/// Wraithguard's theme, or null), the viewer's to wear when its own profile names none once
/// Wraithguard no longer hands it the palette (`--theme`).
pub fn pages(s: &Map<String, Value>) -> Value {
    serde_json::json!({
        "loadorder": loadorder_fields(s),
        "tools": tools_fields(s),
        "conflicts": conflicts_fields(s),
        "theme": theme_code(s),
    })
}

/// Every field the pages read, one flat set of names (the Setup page shows them all): the
/// Load order page's, the Tools page's, the Conflicts page's, and `openmwInstall` (the OpenMW
/// folder whose Lua API the Lua checks read), `userRules` (the rule maker's file) and `scanDir`.
pub fn all_fields(s: &Map<String, Value>) -> Map<String, Value> {
    let mut out = loadorder_fields(s);
    out.extend(tools_fields(s));
    out.extend(conflicts_fields(s));
    for (page, key) in [("openmwInstall", "openmw_install"), ("userRules", "user_rules"), ("scanDir", "scan_dir")] {
        if let Some(v) = text(s, key) {
            out.insert(page.to_string(), Value::String(v));
        }
    }
    out
}

/// A page field's key in the file, and how its value is kept there.
enum Kept {
    Text(&'static str),
    Flag(&'static str),
    /// One a line on the page, a list in the file.
    Lines(&'static str),
    /// One a line on the page, comma-separated in the file (the Tk window's grass field).
    Commas(&'static str),
}

fn kept(field: &str) -> Option<Kept> {
    Some(match field {
        "cfg" => Kept::Text("cfg"),
        "customizations" => Kept::Text("customizations"),
        "subsetFile" => Kept::Text("subset_file"),
        "pluginOrderYml" => Kept::Text("plugin_order_yml"),
        "listName" => Kept::Text("list_name"),
        "dataPathsCache" => Kept::Text("data_paths_cache"),
        "emitToml" => Kept::Text("emit_toml"),
        "tes3cmd" => Kept::Text("tes3cmd"),
        "mergeLandsFolder" => Kept::Text("merged_lands_out"),
        "exclude" => Kept::Text("exclude"),
        "openmwInstall" => Kept::Text("openmw_install"),
        "userRules" => Kept::Text("user_rules"),
        "scanDir" => Kept::Text("scan_dir"),
        "rules" => Kept::Lines("rules"),
        "groundcover" => Kept::Commas("groundcover"),
        "sortDataPaths" => Kept::Flag("sort_data_paths"),
        "subsetFromCfg" => Kept::Flag("subset_from_cfg"),
        "noPredicateWarnings" => Kept::Flag("no_predicate_warnings"),
        "writeCfg" => Kept::Flag("write_cfg"),
        "dryRun" => Kept::Flag("dry_run"),
        "noBackup" => Kept::Flag("no_backup"),
        _ => return None,
    })
}

/// `fields` (page names, as [`all_fields`] gives them) set in the settings `s`, in the
/// file's keys. A field this does not know is passed over; an empty text clears its key's
/// value to `""` (as the Tk window wrote an empty field). Setting `emitToml` turns off the Tk
/// window's "write back to customizations.toml", which the field now says outright.
pub fn set_fields(s: &mut Map<String, Value>, fields: &Map<String, Value>) {
    for (field, v) in fields {
        let Some(k) = kept(field) else { continue };
        let lines = |v: &Value| -> Vec<String> {
            v.as_str().unwrap_or_default().lines().map(str::trim).filter(|l| !l.is_empty()).map(str::to_string).collect()
        };
        match k {
            Kept::Text(key) => {
                if let Some(t) = v.as_str() {
                    s.insert(key.into(), Value::String(t.trim().to_string()));
                    if key == "emit_toml" {
                        s.insert("write_toml_inplace".into(), Value::Bool(false));
                    }
                }
            }
            Kept::Flag(key) => {
                if let Some(b) = v.as_bool() {
                    s.insert(key.into(), Value::Bool(b));
                }
            }
            Kept::Lines(key) => {
                s.insert(key.into(), Value::Array(lines(v).into_iter().map(Value::String).collect()));
            }
            Kept::Commas(key) => {
                s.insert(key.into(), Value::String(lines(v).join(", ")));
            }
        }
    }
}

/// [`set_fields`] on the file at `path`, written whole (a temporary file renamed over it, so a
/// failure part way leaves the old one), two-space indented as the Python writes it.
///
/// # Errors
/// The write's.
pub fn write_fields(path: &Path, fields: &Map<String, Value>) -> std::io::Result<Map<String, Value>> {
    let mut s = read(path);
    set_fields(&mut s, fields);
    let text = serde_json::to_string_pretty(&Value::Object(s.clone())).map_err(std::io::Error::other)?;
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)?;
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn obj(v: Value) -> Map<String, Value> {
        v.as_object().unwrap().clone()
    }

    #[test]
    fn an_old_files_sort_fields_carry_over() {
        // what the Tk window saved before the load order moved (4.2 slice 2k)
        let s = obj(json!({
            "cfg": "C:/OpenMW/openmw.cfg", "customizations": "C:/m/custom.toml", "subset_file": "",
            "emit_toml": "C:/m/out.toml", "list_name": "total-overhaul", "plugin_order_yml": "  ",
            "data_paths_cache": "C:/m/cache.txt", "groundcover": "Grass A.esp, ,Grass B.esp",
            "rules": ["C:/r/mlox_base.txt", "", "C:/r/mlox_user.txt", 3], "write_toml_inplace": false,
            "dry_run": true, "write_cfg": false, "sort_data_paths": true, "subset_from_cfg": "yes",
            "no_backup": false, "no_predicate_warnings": false, "log_theme": "Dark (default)",
        }));
        let f = loadorder_fields(&s);
        assert_eq!(
            Value::Object(f),
            json!({
                "cfg": "C:/OpenMW/openmw.cfg", "customizations": "C:/m/custom.toml", "listName": "total-overhaul",
                "dataPathsCache": "C:/m/cache.txt", "emitToml": "C:/m/out.toml",
                "rules": "C:/r/mlox_base.txt\nC:/r/mlox_user.txt", "groundcover": "Grass A.esp\nGrass B.esp",
                "sortDataPaths": true, "noPredicateWarnings": false, "writeCfg": false, "dryRun": true, "noBackup": false,
            })
        );
    }

    #[test]
    fn the_tools_and_conflicts_fields() {
        let s = obj(json!({"tes3cmd": "C:/t/tes3cmd.exe", "merged_lands_out": "", "exclude": "s3lightfixes*, *grass*"}));
        assert_eq!(
            pages(&s),
            json!({"loadorder": {}, "tools": {"tes3cmd": "C:/t/tes3cmd.exe"}, "conflicts": {"exclude": "s3lightfixes*, *grass*"}, "theme": null})
        );
    }

    #[test]
    fn the_theme_by_the_generators_code() {
        // python: [slug(n) for n in ("Dark (default)", "Solarized Light", "Nord")]
        let code = |n: &str| theme_code(&obj(json!({"log_theme": n})));
        assert_eq!(code("Dark (default)").as_deref(), Some("wg-dark-default"));
        assert_eq!(code("Solarized Light").as_deref(), Some("wg-solarized-light"));
        assert_eq!(code("  ").as_deref(), None);
    }

    #[test]
    fn the_pages_fields_go_back_in_the_files_keys_and_keep_the_rest() {
        let dir = std::env::temp_dir().join(format!("wg_settings_w_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(FILE);
        std::fs::write(&path, r#"{"log_theme": "Nord", "rules": ["old.txt"], "write_toml_inplace": true}"#).unwrap();
        let fields = obj(json!({
            "cfg": " C:/OpenMW/openmw.cfg ", "rules": "a.txt\n\n b.txt ", "groundcover": "G1.esp\nG2.esp",
            "dryRun": true, "emitToml": "out.toml", "mergeLandsFolder": "C:/out", "nonsense": 1,
        }));
        let s = write_fields(&path, &fields).unwrap();
        assert_eq!(s, read(&path));
        assert_eq!(s["log_theme"], "Nord");
        assert_eq!(s["cfg"], "C:/OpenMW/openmw.cfg");
        assert_eq!(s["rules"], json!(["a.txt", "b.txt"]));
        assert_eq!(s["groundcover"], "G1.esp, G2.esp");
        assert_eq!((s["dry_run"].clone(), s["write_toml_inplace"].clone()), (json!(true), json!(false)));
        assert_eq!(s["merged_lands_out"], "C:/out");
        assert!(!s.contains_key("nonsense"));
        // and they read back as the page wrote them
        let back = all_fields(&s);
        assert_eq!(back["rules"], "a.txt\nb.txt");
        assert_eq!(back["groundcover"], "G1.esp\nG2.esp");
        assert_eq!(back["emitToml"], "out.toml");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn in_place_writes_back_to_the_customizations() {
        let s = obj(json!({"customizations": "c.toml", "emit_toml": "other.toml", "write_toml_inplace": true}));
        assert_eq!(loadorder_fields(&s)["emitToml"], "c.toml");
    }

    #[test]
    fn a_missing_or_broken_file_is_no_settings() {
        let dir = std::env::temp_dir().join(format!("wg_settings_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(read(&dir.join("absent.json")).is_empty());
        std::fs::write(dir.join("bad.json"), "[1, 2").unwrap();
        assert!(read(&dir.join("bad.json")).is_empty());
        std::fs::write(dir.join("list.json"), "[1, 2]").unwrap();
        assert!(read(&dir.join("list.json")).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
