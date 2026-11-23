//! "Check my Lua mods" and the Lua load-order lint (Static 2026-11-23: the Tools page's
//! Lua checker; "do those").
//!
//! The checker runs each Lua mod of the load order on its own (the program, `lua_check`, one
//! worker a mod) and sorts what its scripts said into what the person can act on
//! ([`classify`], [`Report`]):
//!
//! - **dependency**: a module (`require`) or an interface (`I.X`) the mod needs and the load
//!   order does not have; or the mod's own check that something is installed;
//! - **plugin**: a global variable or record its plugin defines, and the plugin is not loaded;
//! - **mod**: the mod's own fault - its Lua does not parse, or a script ran away;
//! - **runtime**: any other error (the mod's, or a part of OpenMW the Preview does not do);
//! - **gaps**: what the Preview does not run yet (`X is not run in the preview`), ranked by how
//!   many mods reach for it.
//!
//! [`lint`] reads the scripts without running them: interfaces used (`I.X`, `interfaces.X`)
//! that no script provides (`interfaceName = 'X'`), settings groups registered in two files,
//! and a content file the data folders hold more than once (the last one wins; the others are
//! another copy of the mod, maybe another version).

use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde_json::{json, Value};

/// What an error is (see the module's note): `(class, what it is about)`.
pub fn classify(text: &str, line: Option<&str>) -> (&'static str, String) {
    let t = text;
    let after = |pat: &str| -> Option<String> {
        let i = t.find(pat)? + pat.len();
        let rest = &t[i..];
        Some(rest.split(|c: char| c.is_whitespace() || c == '\'' || c == ')' || c == '"').next().unwrap_or("").to_string())
    };
    if let Some(m) = after("module ").filter(|_| t.contains(" not found")) {
        return ("dependency", format!("module {m}"));
    }
    if let Some(g) = after("no global variable ") {
        return ("plugin", format!("global {g}"));
    }
    // An interface the mod reads off `I` / `interfaces` and nothing provides.
    if t.contains("attempt to index a nil value (field '") {
        if let Some(f) = after("(field '") {
            let used = line.is_some_and(|l| l.contains(&format!("I.{f}")) || l.contains(&format!("interfaces.{f}")) || l.contains(&format!("I['{f}']")));
            if used {
                return ("dependency", format!("interface {f}"));
            }
        }
    }
    let low = t.to_ascii_lowercase();
    if low.contains("is not installed") || low.contains("requires ") || low.contains("missingdep") || low.contains("not found!") || low.contains("missing assets") {
        return ("dependency", "the mod's own check: ".to_string() + t.rsplit(": ").next().unwrap_or(t));
    }
    for s in ["unexpected symbol", "unfinished string", "' expected", "malformed number", "only Lua source is loaded"] {
        if t.contains(s) {
            return ("mod", "its Lua does not parse".into());
        }
    }
    if t.contains("ran too long") || t.contains("used too much memory") {
        return ("mod", "a script ran away (too long or too much memory)".into());
    }
    ("runtime", String::new())
}

/// Whether a `X is not run in the preview` names a part of the scripting API (a gap) rather
/// than a module of another mod (an MWSE `require` the load order does not have).
pub fn is_api(name: &str) -> bool {
    const API: [&str; 22] = [
        "openmw.", "openmw_aux", "I.", "tes3", "mwse", "mge", "lfs", "ni.", "json.", "timer.", "event.", "require(\"", "os.", "io.",
        "util.", "core.", "types.", "ui.", "the mod config menu", "a message box", "debug.", "toml.",
    ];
    API.iter().any(|p| name.starts_with(p)) || name == "lfs" || name == "ni"
}

/// One mod's run.
#[derive(Default)]
pub struct ModRun {
    /// Its content file (`.omwscripts`, `.omwaddon`) or MWSE folder.
    pub name: String,
    pub kind: String,
    pub scripts: Vec<String>,
    /// What its scripts said: `(script, kind, text)` - the run's log, `error` and `skip` lines.
    pub log: Vec<(String, String, String)>,
    /// Why it did not run at all (the worker failed), if so.
    pub failed: Option<String>,
}

/// The checker's answer: every mod's sorted findings, and the gaps ranked. `line(path, n)`
/// gives a script's line (for telling an interface apart from any other field).
pub fn report(runs: &[ModRun], line: &dyn Fn(&str, usize) -> Option<String>) -> Value {
    let mut gaps: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut mods = Vec::new();
    for r in runs {
        let mut found: Vec<Value> = Vec::new();
        let mut seen = BTreeSet::new();
        for (path, kind, text) in &r.log {
            if kind == "skip" {
                if let Some(name) = text.strip_suffix(" is not run in the preview").or_else(|| text.split(" is not run in the preview").next().filter(|_| text.contains(" is not run in the preview"))) {
                    if is_api(name) {
                        gaps.entry(name.to_string()).or_default().insert(r.name.clone());
                    } else if seen.insert(format!("dep|{name}")) {
                        found.push(json!({"class": "dependency", "about": format!("module {name}"), "script": path, "text": text}));
                    }
                }
                continue;
            }
            if kind != "error" {
                continue;
            }
            // `path:line: message` - the line, for an interface.
            let (at, msg) = match text.split_once(": ") {
                Some((a, m)) if a.rsplit(':').next().is_some_and(|n| n.parse::<usize>().is_ok()) => (Some(a), m),
                _ => (None, text.as_str()),
            };
            let src = at.and_then(|a| {
                let (p, n) = a.rsplit_once(':')?;
                line(p.trim_start_matches('@'), n.parse().ok()?)
            });
            let (class, about) = classify(msg, src.as_deref());
            if seen.insert(format!("{class}|{about}|{msg}")) {
                found.push(json!({"class": class, "about": about, "script": path, "text": text}));
            }
        }
        let worst = ["mod", "dependency", "plugin", "runtime"].iter().find(|c| found.iter().any(|f| f["class"] == **c)).copied();
        mods.push(json!({
            "name": r.name, "kind": r.kind, "scripts": r.scripts, "failed": r.failed,
            "status": if r.failed.is_some() { "failed" } else { worst.unwrap_or("ok") },
            "found": found,
        }));
    }
    let mut ranked: Vec<(String, BTreeSet<String>)> = gaps.into_iter().collect();
    ranked.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then(a.0.cmp(&b.0)));
    let gaps: Vec<Value> = ranked.into_iter().map(|(name, m)| json!({"name": name, "mods": m.len(), "examples": m.into_iter().take(5).collect::<Vec<_>>()})).collect();
    json!({"mods": mods, "gaps": gaps})
}

/// The load-order lint (see the module's note), over the scripts' texts (`sources`: path ->
/// text, every script and module the run would read, OpenMW's own included) and the content
/// files each in more than one data folder (`copies`: name -> folders).
pub fn lint(sources: &HashMap<String, String>, copies: &[(String, Vec<String>)]) -> Value {
    let mut provided: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut used: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut groups: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (path, text) in sources {
        let (prov, uses) = interfaces_of(text);
        for name in prov {
            provided.entry(name).or_default().push(path.clone());
        }
        for name in uses {
            used.entry(name).or_default().insert(path.clone());
        }
        // registerGroup { key = 'X', ... }: the key, within the call's first lines.
        let mut from = 0;
        while let Some(i) = text[from..].find("registerGroup") {
            let at = from + i + 13;
            from = at;
            let window: String = text[at..].chars().take(600).collect();
            if let Some(k) = window.find("key") {
                let r = window[k + 3..].trim_start();
                if let Some(r) = r.strip_prefix('=') {
                    let r = r.trim_start();
                    if let Some(q) = r.chars().next().filter(|c| *c == '\'' || *c == '"') {
                        let name: String = r[1..].chars().take_while(|c| *c != q).collect();
                        if !name.is_empty() {
                            groups.entry(name).or_default().insert(path.clone());
                        }
                    }
                }
            }
        }
    }
    let mut out = Vec::new();
    for (name, by) in &used {
        if !provided.contains_key(name) {
            out.push(json!({"kind": "interface", "name": name, "text": format!("I.{name} is used and no script in the load order provides it"), "files": by}));
        }
    }
    for (name, by) in &groups {
        if by.len() > 1 {
            out.push(json!({"kind": "settings", "name": name, "text": format!("the settings group {name} is registered in {} files: OpenMW refuses the second", by.len()), "files": by}));
        }
    }
    for (name, dirs) in copies {
        if dirs.len() > 1 {
            out.push(json!({"kind": "copies", "name": name, "text": format!("{name} is in {} data folders: the last one is used, the others are another copy of the mod", dirs.len()), "files": dirs}));
        }
    }
    Value::Array(out)
}

/// The interfaces a file provides (`interfaceName = 'X'`) and uses (`I.X`, `interfaces.X`).
fn interfaces_of(text: &str) -> (Vec<String>, BTreeSet<String>) {
    let ident = |s: &str| -> String { s.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect() };
    let mut provides = Vec::new();
    let mut rest = text;
    while let Some(i) = rest.find("interfaceName") {
        rest = &rest[i + 13..];
        let r = rest.trim_start().trim_start_matches('=').trim_start();
        if let Some(q) = r.chars().next().filter(|c| *c == '\'' || *c == '"') {
            let name: String = r[1..].chars().take_while(|c| *c != q).collect();
            if !name.is_empty() {
                provides.push(name);
            }
        }
    }
    let mut uses = BTreeSet::new();
    if text.contains("openmw.interfaces") {
        for pat in ["I.", "interfaces."] {
            let mut from = 0;
            while let Some(i) = text[from..].find(pat) {
                let at = from + i;
                from = at + pat.len();
                if text[..at].chars().last().is_some_and(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.') {
                    continue;
                }
                let name = ident(&text[from..]);
                if name.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
                    uses.insert(name);
                }
            }
        }
    }
    (provides, uses)
}

/// What each registered script needs to be on with it (the Scripts panel's "on with what it
/// needs", Static 2026-11-23): for each script, the interfaces its mod's files use and the
/// registered scripts that provide them (`@builtins` for OpenMW's own). A mod's files are its
/// `scripts/<mod>/` folder: an interface its modules use is its registered scripts' too.
/// `sources`: path -> text, OpenMW's own included; `registered`: the load order's scripts;
/// `builtins`: OpenMW's own registered scripts.
pub fn deps(sources: &HashMap<String, String>, registered: &[String], builtins: &[String]) -> Value {
    let folder = |p: &str| -> String {
        let p = p.replace('\\', "/").to_ascii_lowercase();
        let mut it = p.split('/');
        match (it.next(), it.next(), it.next()) {
            (Some("scripts"), Some(top), Some(_)) => format!("scripts/{top}/"),
            _ => p.clone(),
        }
    };
    let low = |p: &str| p.replace('\\', "/").to_ascii_lowercase();
    let mut providers: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut used_by_folder: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (path, text) in sources {
        let (prov, uses) = interfaces_of(text);
        let p = low(path);
        for name in prov {
            let who = if builtins.iter().any(|b| low(b) == p) {
                "@builtins".to_string()
            } else if let Some(r) = registered.iter().find(|r| low(r) == p) {
                r.clone()
            } else {
                continue;
            };
            let v = providers.entry(name).or_default();
            if !v.contains(&who) {
                v.push(who);
            }
        }
        used_by_folder.entry(folder(&p)).or_default().extend(uses);
    }
    let mut out = serde_json::Map::new();
    for r in registered {
        let uses = used_by_folder.get(&folder(r)).cloned().unwrap_or_default();
        let needs: Vec<Value> = uses
            .iter()
            .map(|name| {
                let by: Vec<String> = providers.get(name).map(|v| v.iter().filter(|w| low(w) != low(r)).cloned().collect()).unwrap_or_default();
                json!({"interface": name, "by": by})
            })
            .filter(|v| v["by"].as_array().is_some_and(|a| !a.is_empty()) || !providers.contains_key(v["interface"].as_str().unwrap_or("")))
            .collect();
        if !needs.is_empty() {
            out.insert(r.clone(), Value::Array(needs));
        }
    }
    Value::Object(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_are_sorted_into_what_the_person_can_do() {
        assert_eq!(classify("module scripts.SkillPerks.namespace not found", None).0, "dependency");
        assert_eq!(classify("no global variable detd_ash_amount", None), ("plugin", "global detd_ash_amount".into()));
        assert_eq!(
            classify("attempt to index a nil value (field 'ErnPerkFramework')", Some("interfaces.ErnPerkFramework.registerConstellation(d)")),
            ("dependency", "interface ErnPerkFramework".into())
        );
        assert_eq!(classify("attempt to index a nil value (field 'records')", Some("local x = core.mwscripts.records[1]")).0, "runtime");
        assert_eq!(classify("ERROR: Harvest Lights is not installed!", None).0, "dependency");
        assert_eq!(classify("unfinished string near ''", None).0, "mod");
        assert!(is_api("openmw.postprocessing.load") && is_api("tes3.claimSpellEffectId") && !is_api("mer.ashfall.interop"));
    }

    #[test]
    fn a_report_sorts_each_mod_and_ranks_the_gaps() {
        let runs = vec![
            ModRun {
                name: "perks.omwscripts".into(),
                kind: "openmw".into(),
                scripts: vec!["scripts/p/a.lua".into()],
                log: vec![
                    ("scripts/p/a.lua".into(), "error".into(), "scripts/p/a.lua:3: attempt to index a nil value (field 'ErnPerkFramework')".into()),
                    ("scripts/p/a.lua".into(), "skip".into(), "openmw.postprocessing.load is not run in the preview".into()),
                ],
                failed: None,
            },
            ModRun {
                name: "MWSE/mods/x".into(),
                kind: "mwse".into(),
                scripts: vec![],
                log: vec![("mwse/mods/x/main.lua".into(), "skip".into(), "mer.ashfall.interop is not run in the preview".into()), ("".into(), "skip".into(), "openmw.postprocessing.load is not run in the preview".into())],
                failed: None,
            },
        ];
        let r = report(&runs, &|p, n| (p == "scripts/p/a.lua" && n == 3).then(|| "I.ErnPerkFramework.add(x)".to_string()));
        assert_eq!(r["mods"][0]["status"], "dependency");
        assert_eq!(r["mods"][0]["found"][0]["about"], "interface ErnPerkFramework");
        assert_eq!(r["mods"][1]["found"][0]["about"], "module mer.ashfall.interop");
        assert_eq!((r["gaps"][0]["name"].clone(), r["gaps"][0]["mods"].clone()), (json!("openmw.postprocessing.load"), json!(2)));
    }

    #[test]
    fn a_script_needs_the_scripts_that_provide_what_its_mod_uses() {
        let mut s = HashMap::new();
        s.insert("scripts/perks/player.lua".to_string(), "local I = require('openmw.interfaces')\nreturn {}".to_string());
        s.insert("scripts/perks/lib.lua".to_string(), "local I = require('openmw.interfaces')\nI.ErnPerkFramework.add(x) I.UI.setMode()".to_string());
        s.insert("scripts/ern/framework.lua".to_string(), "return { interfaceName = 'ErnPerkFramework', interface = {} }".to_string());
        s.insert("scripts/omw/ui.lua".to_string(), "return { interfaceName = 'UI' }".to_string());
        let d = deps(&s, &["scripts/perks/player.lua".into(), "scripts/ern/framework.lua".into()], &["scripts/omw/ui.lua".into()]);
        assert_eq!(d["scripts/perks/player.lua"], json!([{"interface": "ErnPerkFramework", "by": ["scripts/ern/framework.lua"]}, {"interface": "UI", "by": ["@builtins"]}]), "{d}");
        assert!(d.get("scripts/ern/framework.lua").is_none());
    }

    #[test]
    fn the_lint_finds_interfaces_nobody_provides_groups_twice_and_copies() {
        let mut s = HashMap::new();
        s.insert("scripts/a/p.lua".to_string(), "local I = require('openmw.interfaces')\nI.Steward.register(x) I.UI.setMode() local y = AI.Foo".to_string());
        s.insert("scripts/omw/ui.lua".to_string(), "return { interfaceName = 'UI', interface = {} }".to_string());
        s.insert("scripts/a/s.lua".to_string(), "I.Settings.registerGroup {\n  key = 'SettingsA',\n}".to_string());
        s.insert("scripts/b/s.lua".to_string(), "I.Settings.registerGroup({ key = \"SettingsA\" })".to_string());
        let l = lint(&s, &[("harvest-lights.omwscripts".into(), vec!["E:/a".into(), "E:/b".into()]), ("one.omwscripts".into(), vec!["E:/c".into()])]);
        let kinds: Vec<(String, String)> = l.as_array().unwrap().iter().map(|x| (x["kind"].as_str().unwrap().to_string(), x["name"].as_str().unwrap().to_string())).collect();
        assert_eq!(kinds, vec![("interface".into(), "Steward".into()), ("settings".into(), "SettingsA".into()), ("copies".into(), "harvest-lights.omwscripts".into())], "{l}");
    }
}
