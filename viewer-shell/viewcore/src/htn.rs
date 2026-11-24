//! The HTN AI tool's file side (Tools → HTN AI, `ui/src/82_wg_htn.js`; Static 2026-11-23:
//! "a tool/frontend that would make [FluadHTN] usable, easier, automated, logical").
//!
//! - [`deps`]: what a mod the tool exports needs, and whether the load order has it: FluadHTN's
//!   OpenMW build (`scripts/sb_htn/interface.lua`), its MWSE build (`mwse/lib/sb_htn/interop.lua`) and,
//!   for OpenMW, Rubic0n's LuaJIT in place of the stock one (the OpenMW build calls
//!   `table.size`, `table.new` and `table.removevalue`, which only Rubic0n gives an OpenMW
//!   script). Rubic0n is told from stock LuaJIT by the MWSE-style table helpers it carries.
//! - [`write_mod`]: the exported mod's files, written into the folder the person chose, each
//!   whole (beside it, then renamed over), and only below that folder.
//! - [`read_domain`]: a saved domain (`*.wghtn.json`) read back.
//!
//! The tool never carries FluadHTN: the mods it writes `require` the copy the player installed.

use std::path::{Component, Path, PathBuf};

use serde_json::{json, Value};

use crate::vfs::Vfs;

/// What a saved domain's file name ends with.
pub const DOMAIN_EXT: &str = ".wghtn.json";
/// The largest domain file read back.
pub const MAX_DOMAIN: u64 = 4 << 20;

/// Where FluadHTN's OpenMW build keeps the file every other one is required through.
pub const OMW_FILE: &str = "scripts/sb_htn/interface.lua";
/// Where its MWSE build keeps the file mods require (`require("sb_htn.interop")`): MWSE/lib in
/// the Nexus build (52111, 2026-06-18), MWSE/mods in a copy laid out as the GitHub repository is.
pub const MWSE_FILES: [&str; 2] = ["mwse/lib/sb_htn/interop.lua", "mwse/mods/sb_htn/interop.lua"];

/// Whether a LuaJIT library is Rubic0n's: it embeds MWSE-style table helpers stock LuaJIT has
/// no trace of.
pub fn is_rubic0n(lib: &[u8]) -> bool {
    let has = |needle: &[u8]| lib.windows(needle.len()).any(|w| w == needle);
    has(b"removevalue") && has(b"copymissing")
}

/// What an exported mod needs, and what is there: `{omw, mwse, mwse_path, luajit, rubic0n}`.
/// `luajit` is the library OpenMW will load (null when none was found); `rubic0n` is null then.
pub fn deps(vfs: &Vfs, luajit: Option<&Path>) -> Value {
    let omw = vfs.resolve(OMW_FILE).is_some();
    let mwse_path = MWSE_FILES.iter().find(|p| vfs.resolve(p).is_some()).copied();
    let rubic0n = luajit.and_then(|p| std::fs::read(p).ok()).map(|b| is_rubic0n(&b));
    let roots: Vec<PathBuf> = vfs.roots.iter().map(|r| r.dir.clone()).collect();
    json!({
        "omw": omw,
        "mwse": mwse_path.is_some(),
        "mwse_path": mwse_path,
        "luajit": luajit.map(|p| p.display().to_string()),
        "rubic0n": rubic0n,
        // Why it may not be found (Static 2026-11-23: "its not detecting odaihtn").
        "hint": if omw && mwse_path.is_some() { Value::Null } else { json!(why_missing(&roots)) },
    })
}

/// FluadHTN in data folders a customizations TOML adds but openmw.cfg does not have yet (Static
/// 2026-11-23: "its in the customizations toml not the cfg"): `{omw, mwse}`, each the folder that
/// holds that build, or null.
pub fn in_toml(data: &[String]) -> Value {
    let mut omw = None;
    let mut mwse = None;
    for d in data {
        let dir = PathBuf::from(d.trim().trim_matches('"'));
        if omw.is_none() && dir.join("scripts").join("sb_htn").join("interface.lua").is_file() {
            omw = Some(dir.display().to_string());
        }
        if mwse.is_none() && (dir.join("MWSE").join("lib").join("sb_htn").join("interop.lua").is_file() || dir.join("MWSE").join("mods").join("sb_htn").join("interop.lua").is_file()) {
            mwse = Some(dir.display().to_string());
        }
    }
    json!({"omw": omw, "mwse": mwse})
}

/// Where FluadHTN's files are under the data folders that look like it, when they are not where
/// the game looks: `sb_htn` folders found up to four levels into any data folder whose path
/// names OdaiHTN, FluadHTN or HTN, said with the data= line that would make it load. When no
/// data folder names it, that is said: the mod is not in the load order's data folders.
pub fn why_missing(roots: &[PathBuf]) -> String {
    let like = |p: &Path| {
        let s = p.to_string_lossy().to_ascii_lowercase();
        s.contains("odai") || s.contains("fluad") || s.contains("htn")
    };
    let mut found = Vec::new();
    for root in roots.iter().filter(|r| like(r)) {
        let mut stack = vec![(root.clone(), 0usize)];
        while let Some((d, depth)) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(&d) else { continue };
            for e in rd.flatten() {
                let p = e.path();
                if !p.is_dir() {
                    continue;
                }
                let name = e.file_name().to_string_lossy().to_ascii_lowercase();
                if name == "sb_htn" {
                    // Its parent should be `scripts` (OpenMW) or `MWSE/lib` (MWSE), right under the root.
                    let rel = p.strip_prefix(root).map(|r| r.to_string_lossy().replace('\\', "/").to_ascii_lowercase()).unwrap_or_default();
                    if rel != "scripts/sb_htn" && rel != "mwse/lib/sb_htn" && rel != "mwse/mods/sb_htn" && rel != "data files/mwse/lib/sb_htn" {
                        let want = p.parent().and_then(|s| if s.file_name().is_some_and(|n| n.eq_ignore_ascii_case("scripts")) { s.parent() } else { None });
                        found.push(match want {
                            Some(w) => format!("{} is in {}, one folder too deep: the data= line should be {}", p.display(), root.display(), w.display()),
                            None => format!("{} is under {}, not where the game looks (scripts/sb_htn for OpenMW, MWSE/lib/sb_htn for MWSE)", p.display(), root.display()),
                        });
                    } else if rel == "data files/mwse/lib/sb_htn" {
                        found.push(format!("{} is in {}: the MWSE build goes in Morrowind's Data Files, or the data= line should end in Data Files", p.display(), root.display()));
                    }
                } else if depth < 4 {
                    stack.push((p, depth + 1));
                }
            }
        }
    }
    if !found.is_empty() {
        return found.join("\n");
    }
    if roots.iter().any(|r| like(r)) {
        return "A data folder names it, but no sb_htn folder is in it: is FluadHTN unpacked there?".into();
    }
    "No data folder of the load order is FluadHTN's: add its folder to openmw.cfg (a data= line), or enable it in your mod manager, then reload.".into()
}

/// Whether `rel` is a path below the export folder: relative, with no `..`, no drive, and no
/// empty part.
pub fn rel_ok(rel: &str) -> bool {
    if rel.is_empty() || rel.len() > 260 || rel.contains(':') || rel.contains('\0') {
        return false;
    }
    let p = Path::new(rel);
    if p.is_absolute() || rel.starts_with('/') || rel.starts_with('\\') {
        return false;
    }
    rel.split(['/', '\\']).all(|s| !s.is_empty() && s != "." && s != "..")
        && p.components().all(|c| matches!(c, Component::Normal(_)))
}

/// The mod's files written below `dir`: every path checked before any is written, the folders
/// made, each file written whole. The paths written, in order.
pub fn write_mod(dir: &Path, files: &[(String, String)]) -> Result<Vec<PathBuf>, String> {
    if !dir.is_dir() {
        return Err(format!("{} is not a folder", dir.display()));
    }
    if files.is_empty() {
        return Err("there is nothing to write".into());
    }
    for (rel, _) in files {
        if !rel_ok(rel) {
            return Err(format!("not a path inside the folder: {rel}"));
        }
    }
    let mut out = Vec::new();
    for (rel, text) in files {
        let path = rel.split(['/', '\\']).fold(dir.to_path_buf(), |p, s| p.join(s));
        if let Some(up) = path.parent() {
            std::fs::create_dir_all(up).map_err(|e| format!("{}: {e}", up.display()))?;
        }
        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let tmp = path.with_file_name(format!(".{name}.wgnew"));
        std::fs::write(&tmp, text.as_bytes()).map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            format!("{}: {e}", tmp.display())
        })?;
        std::fs::rename(&tmp, &path).map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            format!("{}: {e}", path.display())
        })?;
        out.push(path);
    }
    Ok(out)
}

/// A saved domain read back: a `*.wghtn.json` file, JSON, of a sensible size.
pub fn read_domain(path: &Path) -> Result<Value, String> {
    let name = path.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    if !name.ends_with(DOMAIN_EXT) {
        return Err(format!("not an HTN domain file (*{DOMAIN_EXT}): {}", path.display()));
    }
    let meta = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if meta.len() > MAX_DOMAIN {
        return Err(format!("{} is too large to be a domain", path.display()));
    }
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rubic0n_is_told_from_stock_luajit() {
        assert!(is_rubic0n(b"..LuaJIT 2.1..function table.removevalue(t, v)..table.copymissing.."));
        assert!(!is_rubic0n(b"..LuaJIT 2.1.1767980792..table.new..table.clear.."));
    }

    #[test]
    fn a_misplaced_or_missing_fluadhtn_is_explained() {
        let dir = std::env::temp_dir().join(format!("wg_why_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let root = dir.join("OdaiHTN - The AI Framework - OMW");
        std::fs::create_dir_all(root.join("OdaiHTN/scripts/sb_htn")).unwrap();
        let why = why_missing(&[dir.join("Other"), root.clone()]);
        assert!(why.contains("one folder too deep") && why.contains(&root.join("OdaiHTN").display().to_string()), "{why}");
        assert!(why_missing(&[dir.join("Other")]).contains("No data folder"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn fluadhtn_in_a_customizations_toml_only_is_found() {
        let dir = std::env::temp_dir().join(format!("wg_toml_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("omw/scripts/sb_htn")).unwrap();
        std::fs::write(dir.join("omw/scripts/sb_htn/interface.lua"), "return {}").unwrap();
        let v = in_toml(&["C:/nowhere".into(), format!("\"{}\"", dir.join("omw").display())]);
        assert_eq!(v["omw"], dir.join("omw").display().to_string());
        assert!(v["mwse"].is_null());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn only_paths_below_the_folder_are_written() {
        for ok in ["scripts/wg_htn/guard.lua", "MWSE/mods/wg_htn/guard/main.lua", "guard.omwscripts", "a\\b.lua"] {
            assert!(rel_ok(ok), "{ok}");
        }
        for bad in ["", "../x.lua", "scripts/../../x", "/etc/x", "\\x", "C:/x", "a//b", "a/./b", "a/b/"] {
            assert!(!rel_ok(bad), "{bad}");
        }
        let dir = std::env::temp_dir().join(format!("wg_htn_write_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // One bad path: nothing written at all.
        assert!(write_mod(&dir, &[("ok.txt".into(), "x".into()), ("../out.txt".into(), "y".into())]).is_err());
        assert!(!dir.join("ok.txt").exists());
        let w = write_mod(&dir, &[("scripts/wg_htn/a.lua".into(), "print(1)".into()), ("a.omwscripts".into(), "NPC: scripts/wg_htn/a.lua".into())]).unwrap();
        assert_eq!(w.len(), 2);
        assert_eq!(std::fs::read_to_string(dir.join("scripts/wg_htn/a.lua")).unwrap(), "print(1)");
        // Written again over the same file, whole.
        write_mod(&dir, &[("scripts/wg_htn/a.lua".into(), "print(2)".into())]).unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("scripts/wg_htn/a.lua")).unwrap(), "print(2)");
        assert!(std::fs::read_dir(dir.join("scripts/wg_htn")).unwrap().all(|e| !e.unwrap().file_name().to_string_lossy().ends_with(".wgnew")));
        let f = dir.join("guard.wghtn.json");
        std::fs::write(&f, r#"{"format":"wraithguard-htn","version":1}"#).unwrap();
        assert_eq!(read_domain(&f).unwrap()["version"], 1);
        std::fs::write(dir.join("x.json"), "{}").unwrap();
        assert!(read_domain(&dir.join("x.json")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
