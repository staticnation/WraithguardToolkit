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
///
/// Read by `ordercore::cfg`, OpenMW's documented rules (option names exact, as OpenMW's
/// are), as the load order is.
pub fn content_of(cfg_text: &str) -> Vec<String> {
    ordercore::cfg::read_text(cfg_text, Path::new(""), &ordercore::cfg::Tokens::default()).content
}

/// The load order's `content=` names (every chained cfg's, in order), for a Lua run's
/// `core.contentFiles`; none for a setup that is not OpenMW's.
pub fn content(order_file: Option<&Path>) -> Vec<String> {
    let Some(cfg) = order_file.filter(|p| is_openmw_cfg(p)) else { return Vec::new() };
    ordercore::cfg::read(cfg, &ordercore::cfg::Tokens::for_cfg(cfg)).map(|c| c.content).unwrap_or_default()
}

/// One registered script, as the Lua panel lists it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Script {
    pub path: String,
    pub flags: Vec<String>,
    /// The winning copy (a loose file, or `archive.bsa::key`); `None` when nothing has it.
    pub file: Option<String>,
    pub providers: usize,
    pub interface: Option<String>,
    pub handlers: Vec<String>,
}

/// The install's OpenMW, when the setup is OpenMW's: its version, Lua API revision and the
/// `resources` folder the API rules came from.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Api {
    pub openmw: String,
    pub revision: u64,
    pub resources: Option<String>,
}

/// The scan: the API (`None` for a setup that is not OpenMW's), a note, the scripts, the
/// findings (path, severity, code, line, message) and the text report.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Scan {
    pub api: Option<Api>,
    pub note: String,
    pub scripts: Vec<Script>,
    pub findings: Vec<(String, String, String, u64, String)>,
    pub report: String,
}

/// The scan. `order_file` is what the install's load order came from; a setup that is not
/// OpenMW's (a Morrowind.ini) has no Lua and gets an empty scan and a note.
pub fn scan(vfs: &Vfs, order_file: Option<&Path>) -> Result<Scan, String> {
    // OpenMW's by what it is, not by its name: Wraithguard hands the viewer its setup as
    // `wraithguard_cellviewer_setup.cfg`, and any cfg with `data=`/`content=` lines is one.
    // A Morrowind.ini (an .ini, `GameFileN=` lines) is the only other kind.
    let cfg = order_file.filter(|p| is_openmw_cfg(p));
    let Some(cfg) = cfg else {
        return Ok(Scan { note: "Not an OpenMW setup: Morrowind.ini load orders have no Lua scripts.".into(), ..Default::default() });
    };
    // The cfg and every cfg it chains, as the load order is read (`ordercore::cfg`).
    let content = ordercore::cfg::read(cfg, &ordercore::cfg::Tokens::for_cfg(cfg))
        .map_err(|e| format!("{}: {e}", cfg.display()))?
        .content;
    let resources = luacore::findings::find_resources(Some(cfg), None);
    let api = luacore::api::for_install(resources.as_deref());
    let mut files = VfsScripts::new(vfs);
    let got = scan::scan_files(&mut files, &content, &api.rules).map_err(|e| e.to_string())?;

    let mut rows: Vec<Row> = got
        .findings
        .iter()
        .map(|(p, s, c, l, m)| (p.clone(), s.to_string(), c.to_string(), *l, m.clone()))
        .collect();
    // The Teal checks (luacore::check, from native/ in 4.6): each loose script as its
    // winner on disk, the data folders lowest priority first. Packed scripts have no file
    // for the checker and are left to the scan's own rules.
    let winners: Vec<Option<PathBuf>> = got.scripts.iter().map(|r| r.providers.last().cloned()).collect();
    let data_dirs: Vec<PathBuf> = vfs.roots.iter().rev().map(|r| r.dir.clone()).collect();
    let teal = luacore::check::teal_findings(&winners, &data_dirs, resources.as_deref());
    let teal_note = match &teal {
        Ok(t) => format!("Teal checked {} script(s) against {}", t.checked, t.description),
        Err(e) => format!("Teal checks off: the checker did not start ({e:#})"),
    };
    if let Ok(t) = &teal {
        for (rec, found) in got.scripts.iter().zip(&t.findings) {
            if let Some(found) = found {
                rows.extend(found.iter().map(|(s, c, l, m)| (rec.path.clone(), s.to_string(), c.to_string(), *l, m.clone())));
            }
        }
    }
    let mut lines: Vec<ScriptLine> = Vec::new();
    let mut scripts = Vec::new();
    for rec in got.scripts.iter() {
        let info = rec.info.as_ref().map(|(info, _)| info);
        if let Some(info) = info {
            rows.extend(info.findings.iter().map(|(s, c, l, m)| (rec.path.clone(), s.to_string(), c.to_string(), *l, m.clone())));
        }
        if let Some(e) = &rec.read_error {
            rows.push((rec.path.clone(), "error".into(), "UNREADABLE".into(), 0, e.clone()));
        }
        let handlers: Vec<String> = info.map(|i| i.engine_handlers.items.iter().map(|(k, _)| k.clone()).collect()).unwrap_or_default();
        let interface = info.and_then(|i| i.interface_name.clone());
        scripts.push(Script {
            path: rec.path.clone(),
            flags: rec.flags.clone(),
            file: rec.providers.last().map(|p| p.to_string_lossy().to_string()),
            providers: rec.providers.len(),
            interface: interface.clone(),
            handlers: handlers.clone(),
        });
        lines.push((rec.path.clone(), rec.flags.clone(), interface, handlers, rec.providers.is_empty()));
    }

    let header = Header {
        openmw: api.openmw.clone(),
        revision: api.revision,
        omwscripts: got.omwscripts.len(),
        scripts: got.scripts.len(),
        data_dirs: vfs.roots.len(),
        api_source: match &resources {
            Some(r) => format!("API rules from {}; {teal_note}", r.display()),
            None => format!("no OpenMW install found: OpenMW 0.51's API rules; {teal_note}"),
        },
        omwaddons: got.omwaddons.len(),
        lual_files: got.lual_files.len(),
    };
    report::sort_findings(&mut rows);
    let findings = rows.iter().map(|(p, s, c, l, m)| (p.clone(), s.clone(), c.clone(), *l as u64, m.clone())).collect();
    let text = report::render(&header, rows, &lines, true);
    Ok(Scan {
        api: Some(Api {
            openmw: api.openmw.clone(),
            revision: api.revision.max(0) as u64,
            resources: resources.as_deref().map(|r| r.to_string_lossy().into_owned()),
        }),
        note: header.api_source,
        scripts,
        findings,
        report: text,
    })
}

/// The scan as bytes (`WGTB`, Static 2026-11-17: JSON only where it is needed), read by the
/// Editor's Lua panel (`WgEditor.readLuaScan`, ui/src/50_wg_editor_lua.js):
///
/// - `head` `usuusss`: OpenMW's setup 0/1, its version, Lua API revision, a resources folder
///   0/1, that folder, the note, the text report
/// - `scripts` `ssuusu`: path, the winning file, a file 0/1, providers, interface, one 0/1;
///   with `script_flags` `us` and `script_handlers` `us` (its row, a flag or a handler)
/// - `findings` `sssus`: path, severity, code, line, message
pub fn scan_tables(s: &Scan) -> Vec<u8> {
    use crate::wgtb::Tables;
    let mut tb = Tables::new();
    tb.table("head", "usuusss", 1);
    match &s.api {
        Some(a) => tb
            .u(1)
            .s(&a.openmw)
            .u(a.revision)
            .u(a.resources.is_some() as u64)
            .s(a.resources.as_deref().unwrap_or("")),
        None => tb.u(0).s("").u(0).u(0).s(""),
    };
    tb.s(&s.note).s(&s.report);
    tb.table("scripts", "ssuusu", s.scripts.len());
    for x in &s.scripts {
        tb.s(&x.path)
            .s(x.file.as_deref().unwrap_or(""))
            .u(x.file.is_some() as u64)
            .u(x.providers as u64)
            .s(x.interface.as_deref().unwrap_or(""))
            .u(x.interface.is_some() as u64);
    }
    tb.table("script_flags", "us", s.scripts.iter().map(|x| x.flags.len()).sum());
    for (row, x) in s.scripts.iter().enumerate() {
        for f in &x.flags {
            tb.u(row as u64).s(f);
        }
    }
    tb.table("script_handlers", "us", s.scripts.iter().map(|x| x.handlers.len()).sum());
    for (row, x) in s.scripts.iter().enumerate() {
        for h in &x.handlers {
            tb.u(row as u64).s(h);
        }
    }
    tb.table("findings", "sssus", s.findings.len());
    for (p, sev, code, line, msg) in &s.findings {
        tb.s(p).s(sev).s(code).u(*line).s(msg);
    }
    tb.done()
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

/// The load order's registered scripts, for running them (the Preview's scripts,
/// `ui/src/74_wg_lua.js`): every `.omwscripts` line and LUAL record the cfg's `content=`
/// names, in load order, as `(path, flags)` - a script registered twice has its flags
/// joined, as the scan has them. Nothing is read or checked; `None` for a setup that is not
/// OpenMW's.
pub fn registered(vfs: &Vfs, order_file: Option<&Path>) -> Result<Option<Vec<(String, Vec<String>)>>, String> {
    use luacore::omwscripts::{normalize_vfs_path, parse_omwscripts};
    let Some(cfg) = order_file.filter(|p| is_openmw_cfg(p)) else { return Ok(None) };
    let content = ordercore::cfg::read(cfg, &ordercore::cfg::Tokens::for_cfg(cfg))
        .map_err(|e| format!("{}: {e}", cfg.display()))?
        .content;
    let mut files = VfsScripts::new(vfs);
    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    let mut at: HashMap<String, usize> = HashMap::new();
    for name in &content {
        let low = name.to_lowercase();
        let addon = low.ends_with(".omwaddon") || low.ends_with(".omwgame");
        if !addon && !low.ends_with(".omwscripts") {
            continue;
        }
        let found = files.providers(name);
        let Some(file) = found.last() else { continue };
        let Ok(bytes) = files.read(file) else { continue };
        let parsed = if addon {
            scan::lual_entries(luacore::lual_scripts_of(&bytes), name)
        } else {
            parse_omwscripts(&scan::text_of(&bytes), name)
        };
        for e in parsed.entries {
            let key = normalize_vfs_path(&e.path);
            match at.get(&key) {
                Some(&i) => {
                    for f in e.flags {
                        if !out[i].1.contains(&f) {
                            out[i].1.push(f);
                        }
                    }
                }
                None => {
                    at.insert(key, out.len());
                    out.push((e.path, e.flags));
                }
            }
        }
    }
    Ok(Some(out))
}

/// A script's or module's text from the overlay (the winning copy), by its path in the
/// data folders (`scripts/my/mod.lua`).
pub fn source(vfs: &Vfs, path: &str) -> Option<String> {
    let mut files = VfsScripts::new(vfs);
    let found = files.providers(&path.replace('\\', "/"));
    let file = found.last()?;
    files.read(file).ok().map(|b| scan::text_of(&b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_lines_of_every_kind() {
        // OpenMW's option names are exact: `CONTENT=` is not one of its lines.
        let cfg = "data=\"x\"\ncontent=Morrowind.esm\ncontent = My.omwscripts\nCONTENT=Q.omwaddon\ncontent=Q2.omwaddon\ngroundcover=g.esp\n";
        assert_eq!(content_of(cfg), ["Morrowind.esm", "My.omwscripts", "Q2.omwaddon"]);
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
        let got = scan(&vfs, Some(&cfg)).unwrap();
        std::fs::remove_dir_all(&root).ok();
        let codes: Vec<&str> = got.findings.iter().map(|f| f.2.as_str()).collect();
        assert!(codes.contains(&"OVERRIDDEN_FILE"), "{codes:?}");
        assert!(codes.contains(&"PER_ACTOR_FRAME"), "{codes:?}");
        assert_eq!(got.scripts.iter().map(|s| s.providers).collect::<Vec<_>>(), [2], "{:?}", got.scripts);
        assert!(got.api.is_some());
        let mw = scan(&Vfs::default(), Some(Path::new("Morrowind.ini"))).unwrap();
        assert!(is_openmw_cfg(Path::new("wraithguard_cellviewer_setup.cfg")));
        assert!(!is_openmw_cfg(Path::new("Morrowind.ini")));
        assert!(mw.scripts.is_empty() && mw.api.is_none() && mw.note.contains("Not an OpenMW setup"));
        // And as bytes: the tables read back to the same scan.
        let t = crate::wgtb::read(&scan_tables(&got)).unwrap();
        assert_eq!(crate::wgtb::rows(&t, "scripts").len(), 1);
        assert_eq!(crate::wgtb::rows(&t, "findings").len(), got.findings.len());
        assert_eq!(crate::wgtb::rows(&t, "head")[0][0], crate::wgtb::Val::U(1));
    }

    #[test]
    fn registered_scripts_and_their_source() {
        let root = std::env::temp_dir().join(format!("wg_viewcore_luareg_{}", std::process::id()));
        std::fs::create_dir_all(root.join("d").join("scripts")).unwrap();
        std::fs::write(root.join("d").join("m.omwscripts"), "GLOBAL: scripts/g.lua\nNPC, CREATURE: scripts/a.lua\nPLAYER: scripts/a.lua\n").unwrap();
        std::fs::write(root.join("d").join("scripts").join("g.lua"), "return {}").unwrap();
        let cfg = root.join("openmw.cfg");
        std::fs::write(&cfg, "content=m.omwscripts\n").unwrap();
        let vfs = Vfs::with_roots(&[("d".into(), root.join("d"))]);
        let got = registered(&vfs, Some(&cfg)).unwrap().unwrap();
        let src = source(&vfs, "scripts/g.lua");
        std::fs::remove_dir_all(&root).ok();
        assert_eq!(got, vec![("scripts/g.lua".to_string(), vec!["GLOBAL".to_string()]), ("scripts/a.lua".to_string(), vec!["NPC".into(), "CREATURE".into(), "PLAYER".into()])]);
        assert_eq!(src.as_deref(), Some("return {}"));
        assert_eq!(registered(&vfs, Some(Path::new("Morrowind.ini"))).unwrap(), None);
    }
}
