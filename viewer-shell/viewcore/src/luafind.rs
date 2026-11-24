//! Where a Lua mod's missing dependency is, if the person has it (Static 2026-11-24, "lets do
//! all this": "Lua mods one-click turn-on of missing dependencies, plus links for missing
//! mods").
//!
//! The checker says what a mod needs and the load order does not have: an interface (`I.X`),
//! a module (`require`), or a global variable a plugin defines. Often the mod that has it is
//! installed and not turned on: its content file sits in a data folder and is not in
//! `content=`, or its folder is in the mods folder and not in `data=`. [`find`] looks for it:
//!
//! - the content files in the data folders that are not loaded (`.omwscripts`, `.omwaddon`,
//!   `.esp`, `.esm`): a script one of them registers that sets `interfaceName = 'X'`, or a
//!   global variable record named X;
//! - the extra folders the caller names (the mods folder's, not in `data=` yet): the same, and
//!   a module's file (`scripts/a/b.lua` for `scripts.a.b`, or its `init.lua`).
//!
//! Each finding says what to turn on: the content file (and, for an extra folder, the folder
//! as a data path), with the path of the file a person would drop on the Load order page.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::esp::{cstring, Records, Subs};
use crate::vfs::Vfs;

/// A file name that is a content file the checker can look into.
fn is_content(name: &str) -> bool {
    let l = name.to_ascii_lowercase();
    [".omwscripts", ".omwaddon", ".omwgame", ".esp", ".esm"].iter().any(|e| l.ends_with(e))
}

/// The scripts a content file registers: `.omwscripts` lines, an `.omwaddon`'s LUAL records.
fn registered(name: &str, bytes: &[u8]) -> Vec<String> {
    let l = name.to_ascii_lowercase();
    if l.ends_with(".omwscripts") {
        luacore::omwscripts::parse_omwscripts(&luacore::scan::text_of(bytes), name).entries.into_iter().map(|e| e.path).collect()
    } else if l.ends_with(".omwaddon") || l.ends_with(".omwgame") {
        luacore::lual_scripts_of(bytes).map(|v| v.into_iter().map(|e| e.0).collect()).unwrap_or_default()
    } else {
        Vec::new()
    }
}

/// The global variables (GLOB) a plugin defines, in lower case.
fn globals(bytes: &[u8]) -> HashSet<String> {
    let mut out = HashSet::new();
    let mut recs = Records::new(bytes);
    while let Some(rec) = recs.next_filtered(&|t| &t == b"GLOB") {
        for s in Subs::new(rec.body) {
            if &s.tag == b"NAME" {
                out.insert(cstring(s.data).to_ascii_lowercase());
            }
        }
    }
    out
}

/// A module's file under a folder: `scripts.a.b` as `scripts/a/b.lua` or `scripts/a/b/init.lua`.
fn module_file(dir: &Path, module: &str) -> Option<PathBuf> {
    let rel = module.trim().trim_matches(|c| c == '\'' || c == '"').replace('.', "/");
    if rel.is_empty() || rel.contains("..") {
        return None;
    }
    [format!("{rel}.lua"), format!("{rel}/init.lua")].into_iter().map(|r| dir.join(r)).find(|p| p.is_file())
}

/// One place a content file can be looked into: its name, where it is, and how its scripts read.
struct Candidate {
    name: String,
    path: PathBuf,
    /// The folder it is in, when that folder is not in `data=` yet.
    extra: Option<PathBuf>,
}

/// What can be turned on for each need. `needs`: `(kind, name)`, kind `interface`, `module` or
/// `global`. `content`: the load order's content files. `extra`: folders not in `data=` yet.
/// Answers `[{kind, name, found: [{content, path, data, via}]}]`: `content` the file to add to
/// `content=` (null for a module), `path` its full path (what the Load order page takes), `data`
/// the folder to add to `data=` (null when it is there already), `via` how it was found.
pub fn find(needs: &[(String, String)], vfs: &Vfs, content: &[String], extra: &[PathBuf]) -> Value {
    let loaded: HashSet<String> = content.iter().map(|c| c.to_ascii_lowercase()).collect();
    let mut cands: Vec<Candidate> = vfs
        .plugin_files()
        .into_iter()
        .chain(vfs.list("", &["omwscripts"]).into_iter().filter(|p| !p.contains('/')).filter_map(|p| vfs.resolve_disk(&p).map(|d| (p.clone(), d.to_path_buf()))))
        .filter(|(n, _)| is_content(n) && !loaded.contains(&n.to_ascii_lowercase()))
        .map(|(name, path)| Candidate { name, path, extra: None })
        .collect();
    for dir in extra {
        let Ok(rd) = std::fs::read_dir(dir) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            if p.is_file() && is_content(&name) && !loaded.contains(&name.to_ascii_lowercase()) {
                cands.push(Candidate { name, path: p, extra: Some(dir.clone()) });
            }
        }
    }
    let mut seen = HashSet::new();
    cands.retain(|c| seen.insert(c.path.clone()));
    // What each candidate offers, read once.
    let mut offers: Vec<(Vec<String>, HashSet<String>)> = Vec::new();
    for c in &cands {
        let bytes = std::fs::read(&c.path).unwrap_or_default();
        let mut ifaces = Vec::new();
        for script in registered(&c.name, &bytes) {
            let text = match &c.extra {
                Some(dir) => std::fs::read(dir.join(script.replace('\\', "/"))).ok().map(|b| luacore::scan::text_of(&b)),
                None => crate::luascan::source(vfs, &script),
            };
            if let Some(t) = text {
                ifaces.extend(crate::luacheck::provides(&t));
            }
        }
        let l = c.name.to_ascii_lowercase();
        let globs = if l.ends_with(".esp") || l.ends_with(".esm") || l.ends_with(".omwaddon") || l.ends_with(".omwgame") { globals(&bytes) } else { HashSet::new() };
        offers.push((ifaces, globs));
    }
    let shown = |p: &Path| p.display().to_string();
    let mut out = Vec::new();
    for (kind, name) in needs {
        let mut found = Vec::new();
        match kind.as_str() {
            "interface" | "global" => {
                for (c, (ifaces, globs)) in cands.iter().zip(&offers) {
                    let hit = if kind == "interface" { ifaces.iter().any(|i| i == name) } else { globs.contains(&name.to_ascii_lowercase()) };
                    if hit {
                        let via = if kind == "interface" { format!("a script it registers provides I.{name}") } else { format!("it defines the global {name}") };
                        found.push(json!({"content": c.name, "path": shown(&c.path), "data": c.extra.as_deref().map(shown), "via": via}));
                    }
                }
            }
            "module" => {
                for dir in extra {
                    if let Some(f) = module_file(dir, name) {
                        found.push(json!({"content": null, "path": shown(dir), "data": shown(dir), "via": format!("the folder has {}", shown(&f))}));
                    }
                }
            }
            _ => {}
        }
        out.push(json!({"kind": kind, "name": name, "found": found}));
    }
    Value::Array(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_module_s_file_by_its_dotted_name() {
        let dir = std::env::temp_dir().join(format!("wg_luafind_{}", std::process::id()));
        std::fs::create_dir_all(dir.join("scripts/ern/framework")).unwrap();
        std::fs::write(dir.join("scripts/ern/util.lua"), "return {}").unwrap();
        std::fs::write(dir.join("scripts/ern/framework/init.lua"), "return {}").unwrap();
        assert!(module_file(&dir, "scripts.ern.util").is_some_and(|p| p.ends_with("util.lua")));
        assert!(module_file(&dir, "'scripts.ern.framework'").is_some_and(|p| p.ends_with("init.lua")));
        assert!(module_file(&dir, "scripts.ern.gone").is_none());
        assert!(module_file(&dir, "..secret").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A record: its tag, size and two zero words, then its subrecords.
    fn rec(tag: &[u8; 4], subs: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
        let mut body = Vec::new();
        for (t, d) in subs {
            body.extend_from_slice(*t);
            body.extend_from_slice(&(d.len() as u32).to_le_bytes());
            body.extend_from_slice(d);
        }
        let mut out = tag.to_vec();
        out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        out.extend_from_slice(&[0; 8]);
        out.extend(body);
        out
    }

    #[test]
    fn what_is_installed_and_not_turned_on() {
        let root = std::env::temp_dir().join(format!("wg_luafind_vfs_{}", std::process::id()));
        let data = root.join("data");
        let mods = root.join("mods");
        std::fs::create_dir_all(data.join("scripts/perks")).unwrap();
        std::fs::create_dir_all(mods.join("scripts/ern")).unwrap();
        // In data=, not in content=: a scripts file whose script provides I.Perks, and a plugin
        // with the global ern_ready.
        std::fs::write(data.join("Perks.omwscripts"), "PLAYER: scripts/perks/player.lua\n").unwrap();
        std::fs::write(data.join("scripts/perks/player.lua"), "return { interfaceName = 'Perks', interface = {} }").unwrap();
        let mut esp = rec(b"TES3", &[(b"HEDR", vec![0; 300])]);
        esp.extend(rec(b"GLOB", &[(b"NAME", b"ern_ready\0".to_vec()), (b"FNAM", b"s".to_vec()), (b"FLTV", vec![0; 4])]));
        std::fs::write(data.join("Ern.esp"), &esp).unwrap();
        std::fs::write(data.join("Loaded.esp"), &esp).unwrap();
        // The mods folder, not in data=: a module.
        std::fs::write(mods.join("scripts/ern/util.lua"), "return {}").unwrap();
        let vfs = Vfs::with_roots(&[("data".into(), data.clone())]);
        let needs = vec![
            ("interface".to_string(), "Perks".to_string()),
            ("global".to_string(), "ERN_READY".to_string()),
            ("module".to_string(), "scripts.ern.util".to_string()),
            ("interface".to_string(), "Nobody".to_string()),
        ];
        let v = find(&needs, &vfs, &["Loaded.esp".to_string()], std::slice::from_ref(&mods));
        let f = |i: usize| v[i]["found"].as_array().cloned().unwrap_or_default();
        assert_eq!(f(0).len(), 1, "{v}");
        assert_eq!(f(0)[0]["content"], "Perks.omwscripts");
        assert!(f(0)[0]["data"].is_null());
        // The loaded copy is not offered: only the one not turned on.
        assert_eq!(f(1).iter().map(|x| x["content"].as_str().unwrap_or("")).collect::<Vec<_>>(), ["Ern.esp"]);
        assert_eq!(f(2).len(), 1);
        assert!(f(2)[0]["content"].is_null() && f(2)[0]["data"].as_str().is_some_and(|d| d.ends_with("mods")));
        assert!(f(3).is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }
}
