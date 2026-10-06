//! Wraithguard: the load order's OpenMW Lua scripts, checked by the engine itself.
//!
//! The scan is `luacore`'s (the same code the Python side runs through `native/`): every
//! `.omwscripts` file and `.omwaddon` LUAL record the cfg's `content=` lines name, the
//! scripts they register read and analysed, and the checks between them. What this adds
//! is the overlay: files come from [`Vfs`], so a script packed in an archive is read
//! where it is, and a data folder's place in the overlay decides which copy runs, exactly
//! as for meshes. The API rules are the install's own when its `resources` folder is
//! found (its documentation's packages and interfaces), else OpenMW 0.51's.
//!
//! Not here: the Teal type checks (an embedded Lua, Python-side only so far).

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};

use luacore::report::{self, Header, Row, ScriptLine};
use luacore::scan::{self, ScriptFiles};

use crate::json::{escape_into, string_array, J};
use crate::vfs::{Loc, Vfs};

/// The overlay as `luacore` asks for files: a loose file by its path, a packed one by
/// `archive.bsa::key` (what [`Loc::ident`] calls it).
pub struct VfsScripts<'a> {
    vfs: &'a Vfs,
    locs: HashMap<PathBuf, Loc>,
}

impl<'a> VfsScripts<'a> {
    pub fn new(vfs: &'a Vfs) -> Self {
        VfsScripts { vfs, locs: HashMap::new() }
    }
}

impl ScriptFiles for VfsScripts<'_> {
    fn providers(&mut self, rel: &str) -> Vec<PathBuf> {
        // The overlay answers winner first (folders by priority, then archives
        // last-loaded first); the scan wants the winner last.
        let mut out = Vec::new();
        for (_, loc) in self.vfs.providers(rel, &[]).into_iter().rev() {
            let name = match &loc {
                Loc::Disk(p) => p.clone(),
                Loc::Packed { .. } => PathBuf::from(loc.ident(self.vfs)),
            };
            self.locs.insert(name.clone(), loc);
            out.push(name);
        }
        out
    }

    fn read(&self, file: &Path) -> io::Result<Vec<u8>> {
        match self.locs.get(file) {
            Some(loc) => self
                .vfs
                .load(loc)
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("{} could not be read", file.display()))),
            None => std::fs::read(file),
        }
    }
}

/// Every `content=` name of an openmw.cfg's text, in order - scripts lists and addons as
/// well as plugins (the load order's own reader keeps only plugins).
pub fn content_of(cfg_text: &str) -> Vec<String> {
    cfg_text
        .lines()
        .filter_map(|l| {
            let (k, v) = l.split_once('=')?;
            if !k.trim().eq_ignore_ascii_case("content") {
                return None;
            }
            let v = v.trim();
            let b = v.as_bytes();
            let v = if b.len() >= 2 && b[0] == b[b.len() - 1] && (b[0] == b'"' || b[0] == b'\'') { &v[1..v.len() - 1] } else { v };
            (!v.is_empty()).then(|| v.to_string())
        })
        .collect()
}

/// The scan as the page reads it: `{openmw, revision, resources, note, scripts,
/// findings, report}`. `order_file` is what the install's load order came from; a setup
/// that is not OpenMW's (a Morrowind.ini) has no Lua and gets an empty answer and a note.
pub fn scan_json(vfs: &Vfs, order_file: Option<&Path>) -> Result<String, String> {
    // OpenMW's by what it is, not by its name: Wraithguard hands the viewer its setup as
    // `wraithguard_cellviewer_setup.cfg`, and any cfg with `data=`/`content=` lines is one.
    // A Morrowind.ini (an .ini, `GameFileN=` lines) is the only other kind.
    let cfg = order_file.filter(|p| is_openmw_cfg(p));
    let Some(cfg) = cfg else {
        let mut o = J::obj();
        o.str("note", "Not an OpenMW setup: Morrowind.ini load orders have no Lua scripts.")
            .raw("scripts", "[]")
            .raw("findings", "[]")
            .str("report", "");
        return Ok(o.done());
    };
    let text = std::fs::read_to_string(cfg).map_err(|e| format!("{}: {e}", cfg.display()))?;
    let content = content_of(&text);
    let resources = luacore::findings::find_resources(Some(cfg), None);
    let api = luacore::api::for_install(resources.as_deref());
    let mut files = VfsScripts::new(vfs);
    let got = scan::scan_files(&mut files, &content, &api.rules).map_err(|e| e.to_string())?;

    let mut rows: Vec<Row> = got
        .findings
        .iter()
        .map(|(p, s, c, l, m)| (p.clone(), s.to_string(), c.to_string(), *l, m.clone()))
        .collect();
    let mut lines: Vec<ScriptLine> = Vec::new();
    let mut scripts = String::from("[");
    for (i, rec) in got.scripts.iter().enumerate() {
        let info = rec.info.as_ref().map(|(info, _)| info);
        if let Some(info) = info {
            rows.extend(info.findings.iter().map(|(s, c, l, m)| (rec.path.clone(), s.to_string(), c.to_string(), *l, m.clone())));
        }
        if let Some(e) = &rec.read_error {
            rows.push((rec.path.clone(), "error".into(), "UNREADABLE".into(), 0, e.clone()));
        }
        let handlers: Vec<String> = info.map(|i| i.engine_handlers.items.iter().map(|(k, _)| k.clone()).collect()).unwrap_or_default();
        let interface = info.and_then(|i| i.interface_name.clone());
        let file = rec.providers.last().map(|p| p.to_string_lossy().to_string());
        let mut o = J::obj();
        o.str("path", &rec.path)
            .raw("flags", &string_array(&rec.flags))
            .raw("file", &file.as_deref().map(quoted).unwrap_or_else(|| "null".into()))
            .int("providers", rec.providers.len() as u64)
            .raw("interface", &interface.as_deref().map(quoted).unwrap_or_else(|| "null".into()))
            .raw("handlers", &string_array(&handlers));
        if i > 0 {
            scripts.push(',');
        }
        scripts.push_str(&o.done());
        lines.push((rec.path.clone(), rec.flags.clone(), interface, handlers, rec.providers.is_empty()));
    }
    scripts.push(']');

    let header = Header {
        openmw: api.openmw.clone(),
        revision: api.revision,
        omwscripts: got.omwscripts.len(),
        scripts: got.scripts.len(),
        data_dirs: vfs.roots.len(),
        api_source: match &resources {
            Some(r) => format!("API rules from {} (Teal checks: the toolkit's Lua window)", r.display()),
            None => "no OpenMW install found: OpenMW 0.51's API rules (Teal checks: the toolkit's Lua window)".into(),
        },
        omwaddons: got.omwaddons.len(),
        lual_files: got.lual_files.len(),
    };
    report::sort_findings(&mut rows);
    let mut found = String::from("[");
    for (i, (path, sev, code, line, message)) in rows.iter().enumerate() {
        if i > 0 {
            found.push(',');
        }
        let mut o = J::obj();
        o.str("path", path).str("severity", sev).str("code", code).int("line", *line as u64).str("message", message);
        found.push_str(&o.done());
    }
    found.push(']');
    let text = report::render(&header, rows, &lines, true);

    let mut out = J::obj();
    out.str("openmw", &api.openmw)
        .int("revision", api.revision.max(0) as u64)
        .raw("resources", &resources.as_deref().map(|r| quoted(&r.to_string_lossy())).unwrap_or_else(|| "null".into()))
        .str("note", &header.api_source)
        .raw("scripts", &scripts)
        .raw("findings", &found)
        .str("report", &text);
    Ok(out.done())
}

/// Whether a load-order file is an OpenMW cfg: a `.cfg`, or any file with a `content=`
/// or `data=` line.
pub fn is_openmw_cfg(p: &Path) -> bool {
    if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("cfg")) {
        return true;
    }
    std::fs::read_to_string(p).is_ok_and(|t| {
        t.lines().any(|l| {
            let k = l.split_once('=').map(|(k, _)| k.trim().to_ascii_lowercase()).unwrap_or_default();
            k == "content" || k == "data"
        })
    })
}

fn quoted(s: &str) -> String {
    let mut out = String::from("\"");
    escape_into(s, &mut out);
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_lines_of_every_kind() {
        let cfg = "data=\"x\"\ncontent=Morrowind.esm\ncontent = My.omwscripts\nCONTENT=\"Q.omwaddon\"\ngroundcover=g.esp\n";
        assert_eq!(content_of(cfg), ["Morrowind.esm", "My.omwscripts", "Q.omwaddon"]);
    }

    #[test]
    fn scans_through_the_overlay() {
        let root = std::env::temp_dir().join(format!("wg_viewcore_lua_{}", std::process::id()));
        let (a, b) = (root.join("a"), root.join("b"));
        std::fs::create_dir_all(a.join("scripts")).unwrap();
        std::fs::create_dir_all(b.join("Scripts")).unwrap();
        std::fs::write(a.join("m.omwscripts"), "NPC: scripts/x.lua\n").unwrap();
        std::fs::write(a.join("scripts").join("x.lua"), "return { engineHandlers = { onUpdate = function() end } }").unwrap();
        std::fs::write(b.join("Scripts").join("X.lua"), "return { engineHandlers = { onUpdate = function() end } }").unwrap();
        let cfg = root.join("openmw.cfg");
        std::fs::write(&cfg, "content=m.omwscripts\n").unwrap();
        // Highest priority first, as the overlay holds its roots: b wins.
        let vfs = Vfs::with_roots(&[("b".into(), b), ("a".into(), a)]);
        let got = scan_json(&vfs, Some(&cfg)).unwrap();
        std::fs::remove_dir_all(&root).ok();
        assert!(got.contains("\"OVERRIDDEN_FILE\""), "{got}");
        assert!(got.contains("\"PER_ACTOR_FRAME\""), "{got}");
        assert!(got.contains("\"providers\":2"), "{got}");
        let mw = scan_json(&Vfs::default(), Some(Path::new("Morrowind.ini"))).unwrap();
        assert!(is_openmw_cfg(Path::new("wraithguard_cellviewer_setup.cfg")));
        assert!(!is_openmw_cfg(Path::new("Morrowind.ini")));
        assert!(mw.contains("\"scripts\":[]"));
    }
}
