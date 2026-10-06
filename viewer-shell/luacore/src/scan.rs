//! The load order's Lua scripts, found and checked together (was `wraithguard/lua/scan.py`,
//! which stays as the fallback and the reference this is held to - findings, their order
//! and their wording included). The module docstring there says what each check is for.
//!
//! Where the files come from is a [`ScriptFiles`]: [`Vfs`] here is loose data folders (the
//! Python side's, the archives' scripts already unpacked into folders of their own), and
//! the viewer's engine has its own over its overlay, archives included
//! (`viewcore::luascan`).

use std::collections::{HashMap, HashSet};
use std::io;
use std::path::{Path, PathBuf};

use super::analysis::{self, Info, Rules};
use super::lexer::py_repr;
use super::omwscripts::{Entry, OmwScripts, normalize_vfs_path, parse_omwscripts};
use super::parser::Node;

/// One release's rules: the per-script ones, and what the engine's own scripts offer.
#[derive(Clone, Default)]
pub struct ScanRules {
    pub rules: Rules,
    pub builtin_interfaces: HashSet<String>,
    pub builtin_events: HashSet<String>,
}

/// One script of the load order (the Python `ScriptRecord`).
pub struct Record {
    pub path: String,
    pub registrations: Vec<Entry>,
    pub flags: Vec<String>,
    pub providers: Vec<PathBuf>,
    pub info: Option<(Info, Option<Node>)>,
    pub read_error: Option<String>,
}

impl Record {
    fn file(&self) -> Option<&PathBuf> {
        self.providers.last()
    }

    fn contexts(&self) -> HashSet<&'static str> {
        contexts_for_flags(&self.flags)
    }
}

/// A load-order finding: script path ("" for none), severity, code, line, message.
pub type ScanFinding = (String, &'static str, &'static str, usize, String);

/// Everything `scan_load_order` found (the Python `LuaScan`, its folders and API aside).
#[derive(Default)]
pub struct Scan {
    pub omwscripts: Vec<OmwScripts>,
    pub omwaddons: Vec<String>,
    pub lual_files: Vec<String>,
    pub scripts: Vec<Record>,
    pub findings: Vec<ScanFinding>,
}

/// The contexts a script with these flags runs in (the Python `api.contexts_for_flags`).
pub fn contexts_for_flags(flags: &[String]) -> HashSet<&'static str> {
    let mut out = HashSet::new();
    for f in flags {
        match f.as_str() {
            "GLOBAL" => {
                out.insert("global");
            }
            "MENU" => {
                out.insert("menu");
            }
            "LOAD" => {
                out.insert("load");
            }
            "PLAYER" => {
                out.insert("player");
            }
            // Attached by a global script to any object - the player included.
            "CUSTOM" => {
                out.insert("local");
                out.insert("player");
            }
            _ => {
                out.insert("local");
            }
        }
    }
    out
}

/// A file's text as Python's `read_text(encoding="utf-8", errors="replace")` gives it:
/// invalid bytes replaced, line ends made `\n`.
pub fn read_text(path: &Path) -> io::Result<String> {
    Ok(text_of(&std::fs::read(path)?))
}

/// Bytes as text, as [`read_text`] reads a file.
pub fn text_of(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    if text.contains('\r') { text.replace("\r\n", "\n").replace('\r', "\n") } else { text.into_owned() }
}

/// Where a scan finds its files: every provider of a VFS path, and their bytes. A
/// provider is named by a path - a file on disk, or any stable name for one packed in an
/// archive (it is only ever shown, and handed back to [`ScriptFiles::read`]).
pub trait ScriptFiles: Sync {
    /// Every provider of `rel` (`/` separated, any case), lowest priority first: the last
    /// one is what runs.
    fn providers(&mut self, rel: &str) -> Vec<PathBuf>;
    /// One provider's bytes.
    fn read(&self, file: &Path) -> io::Result<Vec<u8>>;
}

/// Case-insensitive lookups across loose data folders, with listings cached.
pub struct Vfs {
    dirs: Vec<PathBuf>,
    listings: HashMap<PathBuf, HashMap<String, Vec<PathBuf>>>,
}

impl Vfs {
    pub fn new(dirs: Vec<PathBuf>) -> Self {
        Vfs { dirs, listings: HashMap::new() }
    }

    /// A folder's entries by lower-case name (empty if it cannot be read). On a
    /// case-sensitive file system `Scripts` and `scripts` can both exist; OpenMW's VFS
    /// merges them, so both are kept.
    fn listing(&mut self, folder: &Path) -> &HashMap<String, Vec<PathBuf>> {
        self.listings.entry(folder.to_path_buf()).or_insert_with(|| {
            let mut m: HashMap<String, Vec<PathBuf>> = HashMap::new();
            if let Ok(rd) = std::fs::read_dir(folder) {
                for e in rd.flatten() {
                    m.entry(e.file_name().to_string_lossy().to_lowercase()).or_default().push(e.path());
                }
            }
            m
        })
    }

    /// Every data folder's file at a VFS path (`/` separated, any case), in load order.
    pub fn files(&mut self, rel: &str) -> Vec<PathBuf> {
        let low = rel.replace('\\', "/").to_lowercase();
        let parts: Vec<&str> = low.split('/').filter(|p| !p.is_empty()).collect();
        let mut found = Vec::new();
        let dirs = self.dirs.clone();
        for root in dirs {
            let mut frontier = vec![root];
            for part in &parts {
                let mut next = Vec::new();
                for cur in &frontier {
                    if let Some(hits) = self.listing(cur).get(*part) {
                        next.extend(hits.iter().cloned());
                    }
                }
                frontier = next;
                if frontier.is_empty() {
                    break;
                }
            }
            let mut files: Vec<PathBuf> = frontier.into_iter().filter(|p| p.is_file()).collect();
            files.sort();
            if let Some(f) = files.into_iter().next() {
                found.push(f);
            }
        }
        found
    }
}

impl ScriptFiles for Vfs {
    fn providers(&mut self, rel: &str) -> Vec<PathBuf> {
        self.files(rel)
    }

    fn read(&self, file: &Path) -> io::Result<Vec<u8>> {
        std::fs::read(file)
    }
}

/// LuaScriptCfg flag bits -> the `.omwscripts` flag they mean.
const FLAG_BITS: [(u32, &str); 4] = [(1, "GLOBAL"), (1 << 1, "CUSTOM"), (1 << 2, "PLAYER"), (1 << 4, "MENU")];
const LOAD_BIT: u32 = 1 << 5;

/// A record tag a LUAF attaches to -> the `.omwscripts` flag.
fn type_flag(tag: &str) -> String {
    let flag = match tag {
        "ACTI" => "ACTIVATOR",
        "ALCH" => "POTION",
        "APPA" => "APPARATUS",
        "ARMO" => "ARMOR",
        "BOOK" => "BOOK",
        "CLOT" => "CLOTHING",
        "CONT" => "CONTAINER",
        "CREA" => "CREATURE",
        "DOOR" => "DOOR",
        "INGR" => "INGREDIENT",
        "LIGH" => "LIGHT",
        "LOCK" => "LOCKPICK",
        "MISC" => "MISC_ITEM",
        "NPC_" => "NPC",
        "PROB" => "PROBE",
        "REPA" => "REPAIR",
        "WEAP" => "WEAPON",
        _ => return tag.trim_matches(|c| c == '\0' || c == ' ' || c == '_').to_string(),
    };
    flag.to_string()
}

/// A LUAF flags word and type list as `.omwscripts` flags (the Python `lual._flags`):
/// `per_object` (LUAR/LUAI attachments) makes it a local script.
pub fn lual_flags(word: u32, types: &[String], per_object: bool) -> Vec<String> {
    let mut out: Vec<String> = FLAG_BITS.iter().filter(|(b, _)| word & b != 0).map(|(_, n)| n.to_string()).collect();
    if word & LOAD_BIT != 0 {
        out.push("LOAD".into());
    }
    out.extend(types.iter().map(|t| type_flag(t)));
    if per_object && !out.iter().any(|f| f == "CUSTOM") {
        out.push("CUSTOM".into());
    }
    let mut seen = HashSet::new();
    out.retain(|f| seen.insert(f.clone()));
    out
}

/// The scripts a content file registers in its LUAL records (the Python `read_lual`).
pub fn read_lual(path: &Path, name: &str) -> OmwScripts {
    lual_entries(std::fs::read(path).and_then(|b| crate::lual_scripts_of(&b)), name)
}

/// [`read_lual`] for what [`crate::lual_scripts_of`] made of a file.
pub fn lual_entries(got: io::Result<Vec<(String, u32, Vec<String>, bool)>>, name: &str) -> OmwScripts {
    let mut out = OmwScripts { name: name.to_string(), ..Default::default() };
    match got {
        Err(e) if crate::lual_error_is_os(&e) => out.problems.push((0, format!("cannot read: {e}"))),
        Err(e) => out.problems.push((0, format!("LUAL records that cannot be read: {e}"))),
        Ok(found) => {
            for (script, word, types, per_object) in found {
                let flags = lual_flags(word, &types, per_object);
                if flags.is_empty() {
                    out.problems.push((0, format!("script {} has no flags (it never starts)", py_repr(&script))));
                    continue;
                }
                out.entries.push(Entry { path: script, flags, source: name.to_string(), line: 0 });
            }
        }
    }
    out
}

fn flag(out: &mut Vec<ScanFinding>, path: &str, sev: &'static str, code: &'static str, line: usize, msg: String) {
    out.push((path.to_string(), sev, code, line, msg));
}

fn handler_line(info: &Info, name: &str) -> Option<usize> {
    info.engine_handlers.items.iter().find(|(k, _)| k == name).map(|(_, v)| *v)
}

/// Read and analyse one script's winning file, in place.
fn read_script(rec: &mut Record, rules: &Rules, files: &dyn ScriptFiles) {
    let Some(path) = rec.file() else { return };
    match files.read(path).map(|b| text_of(&b)) {
        Err(e) => rec.read_error = Some(e.to_string()),
        Ok(src) => {
            let mut ctx: Vec<String> = rec.contexts().into_iter().map(String::from).collect();
            ctx.sort();
            rec.info = Some(analysis::analyze(&src, &ctx, rules));
        }
    }
}

/// Find, read and check the scripts of a load order. `data_dirs` in load order (the
/// archives' folders first), `content` the cfg's `content=` names. An `.omwscripts` file
/// that is found but cannot be read is an error (as the Python `read_text` raises).
pub fn scan_load_order(data_dirs: &[PathBuf], content: &[String], rules: &ScanRules) -> io::Result<Scan> {
    scan_files(&mut Vfs::new(data_dirs.to_vec()), content, rules)
}

/// [`scan_load_order`] over any [`ScriptFiles`].
pub fn scan_files<F: ScriptFiles>(vfs: &mut F, content: &[String], rules: &ScanRules) -> io::Result<Scan> {
    let mut out = Scan::default();
    let mut by_key: HashMap<String, usize> = HashMap::new();
    for name in content {
        let low = name.to_lowercase();
        let addon = low.ends_with(".omwaddon") || low.ends_with(".omwgame");
        if addon {
            out.omwaddons.push(name.clone());
        } else if !low.ends_with(".omwscripts") {
            continue;
        }
        let files = vfs.providers(name);
        let Some(file) = files.last() else {
            if !addon {
                // A missing plugin is the load order's business, not Lua's.
                flag(&mut out.findings, "", "error", "MISSING_OMWSCRIPTS", 0, format!("{name}: in no data folder"));
            }
            continue;
        };
        let (parsed, code) = if addon {
            let p = lual_entries(vfs.read(file).and_then(|b| crate::lual_scripts_of(&b)), name);
            if !p.entries.is_empty() || !p.problems.is_empty() {
                out.lual_files.push(name.clone());
            }
            (p, "LUAL_RECORD")
        } else {
            let text = vfs
                .read(file)
                .map(|b| text_of(&b))
                .map_err(|e| io::Error::new(e.kind(), format!("{}: {e}", file.display())))?;
            let p = parse_omwscripts(&text, name);
            out.omwscripts.push(p.clone());
            (p, "OMWSCRIPTS_LINE")
        };
        for (line, message) in &parsed.problems {
            flag(&mut out.findings, "", "warn", code, *line, format!("{name}: {message}"));
        }
        for entry in parsed.entries {
            let key = normalize_vfs_path(&entry.path);
            let idx = match by_key.get(&key) {
                Some(&i) => i,
                None => {
                    out.scripts.push(Record {
                        path: entry.path.clone(),
                        registrations: Vec::new(),
                        flags: Vec::new(),
                        providers: Vec::new(),
                        info: None,
                        read_error: None,
                    });
                    by_key.insert(key, out.scripts.len() - 1);
                    out.scripts.len() - 1
                }
            };
            let rec = &mut out.scripts[idx];
            for f in &entry.flags {
                if !rec.flags.contains(f) {
                    rec.flags.push(f.clone());
                }
            }
            rec.registrations.push(entry);
        }
    }
    for rec in &mut out.scripts {
        rec.providers = vfs.providers(&rec.path);
    }
    // Read on threads: analysis is per script and needs nothing shared but the rules.
    let n = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(1, 16);
    let size = out.scripts.len().div_ceil(n).max(1);
    let files: &F = vfs;
    std::thread::scope(|s| {
        for chunk in out.scripts.chunks_mut(size) {
            let rules = &rules.rules;
            s.spawn(move || chunk.iter_mut().for_each(|rec| read_script(rec, rules, files)));
        }
    });
    let mut found = std::mem::take(&mut out.findings);
    cross_checks(&out.scripts, rules, &mut found);
    out.findings = found;
    Ok(out)
}

const ACTOR_FLAGS: [&str; 2] = ["NPC", "CREATURE"];

fn cross_checks(scripts: &[Record], rules: &ScanRules, out: &mut Vec<ScanFinding>) {
    for rec in scripts {
        if rec.providers.is_empty() {
            let mut sources: Vec<&str> = Vec::new();
            for e in &rec.registrations {
                if !sources.contains(&e.source.as_str()) {
                    sources.push(&e.source);
                }
            }
            let msg = format!("registered by {} but in no data folder (BSAs are not searched)", sources.join(", "));
            flag(out, &rec.path, "error", "MISSING_SCRIPT", 0, msg);
        }
        if rec.registrations.len() > 1 {
            let regs: Vec<String> = rec
                .registrations
                .iter()
                .map(|e| format!("{} line {} ({})", e.source, e.line, e.flags.join(", ")))
                .collect();
            flag(out, &rec.path, "warn", "DUPLICATE_REGISTRATION", 0, format!("registered twice: {}", regs.join("; ")));
        }
        if rec.providers.len() > 1 {
            let (last, rest) = rec.providers.split_last().expect("more than one");
            let losers: Vec<String> = rest.iter().map(|p| p.display().to_string()).collect();
            let msg = format!("{} data folders have it; {} runs", rec.providers.len(), last.display());
            flag(out, &rec.path, "info", "OVERRIDDEN_FILE", 0, format!("{msg} (over {})", losers.join(", ")));
        }
        if let Some((info, _)) = &rec.info
            && rec.flags.iter().any(|f| ACTOR_FLAGS.contains(&f.as_str()))
        {
            let mut names: Vec<&str> = info
                .engine_handlers
                .items
                .iter()
                .map(|(k, _)| k.as_str())
                .filter(|k| rules.rules.per_frame.contains(*k))
                .collect();
            names.sort();
            names.dedup();
            for name in names {
                let msg = format!("{name} runs on every active NPC/creature, every frame");
                let line = handler_line(info, name).unwrap_or(0);
                flag(out, &rec.path, "warn", "PER_ACTOR_FRAME", line, msg);
            }
        }
    }
    interface_checks(scripts, rules, out);
    event_checks(scripts, rules, out);
}

/// Interfaces offered twice where both scripts run, or replacing a built-in.
fn interface_checks(scripts: &[Record], rules: &ScanRules, out: &mut Vec<ScanFinding>) {
    let mut seen: HashMap<&str, Vec<&Record>> = HashMap::new();
    for rec in scripts {
        let Some((info, _)) = &rec.info else { continue };
        let Some(name) = info.interface_name.as_deref().filter(|n| !n.is_empty()) else { continue };
        let line = info.interface_line;
        let extends = handler_line(info, "onInterfaceOverride").is_some();
        let how = if extends { "extends" } else { "replaces (it has no onInterfaceOverride)" };
        let mine = rec.contexts();
        if let Some(earlier) = seen.get(name) {
            for e in earlier {
                if !e.contexts().is_disjoint(&mine) {
                    let msg = format!("interface {} {how} the one from {}", py_repr(name), e.path);
                    flag(out, &rec.path, if extends { "info" } else { "warn" }, "INTERFACE_OVERRIDE", line, msg);
                }
            }
        }
        if rules.builtin_interfaces.contains(name) && !seen.contains_key(name) {
            let msg = format!("interface {} {how} OpenMW's built-in one", py_repr(name));
            flag(out, &rec.path, "info", "BUILTIN_OVERRIDE", line, msg);
        }
        seen.entry(name).or_default().push(rec);
    }
}

/// Events sent that no script handles.
fn event_checks(scripts: &[Record], rules: &ScanRules, out: &mut Vec<ScanFinding>) {
    let mut handled: HashSet<&str> = rules.builtin_events.iter().map(String::as_str).collect();
    for rec in scripts {
        if let Some((info, _)) = &rec.info {
            handled.extend(info.event_handlers.items.iter().map(|(k, _)| k.as_str()));
        }
    }
    for rec in scripts {
        let Some((info, _)) = &rec.info else { continue };
        let mut sent: Vec<&(String, usize)> = info.sent_events.items.iter().collect();
        sent.sort();
        for (event, line) in sent {
            if !handled.contains(event.as_str()) {
                let msg = format!(
                    "sends {}, which no script handles that this can see (a typo, a missing mod, or handlers built at \
                     run time)",
                    py_repr(event)
                );
                flag(out, &rec.path, "warn", "UNHANDLED_EVENT", *line, msg);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules() -> ScanRules {
        let mut handlers = HashMap::new();
        for h in ["onUpdate", "onInterfaceOverride", "onInit"] {
            handlers.insert(h.to_string(), vec!["global".into(), "local".into(), "player".into()]);
        }
        ScanRules {
            rules: Rules { handlers, per_frame: ["onUpdate".to_string()].into(), packages: HashMap::new() },
            builtin_interfaces: ["AI".to_string()].into(),
            builtin_events: HashSet::new(),
        }
    }

    #[test]
    fn scans_a_load_order() {
        let root = std::env::temp_dir().join(format!("wg_lua_scan_{}", std::process::id()));
        let (a, b) = (root.join("a"), root.join("b"));
        std::fs::create_dir_all(a.join("Scripts")).unwrap();
        std::fs::create_dir_all(b.join("scripts")).unwrap();
        std::fs::write(a.join("m.omwscripts"), "NPC: scripts/x.lua\nGLOBAL: scripts/gone.lua\n").unwrap();
        std::fs::write(b.join("n.omwscripts"), "PLAYER: Scripts/X.lua\n").unwrap();
        let x = "return { interfaceName = 'AI', interface = {}, engineHandlers = { onUpdate = function() end } }";
        std::fs::write(a.join("Scripts").join("x.lua"), x).unwrap();
        std::fs::write(b.join("scripts").join("x.lua"), x).unwrap();
        let content: Vec<String> = ["m.omwscripts", "n.omwscripts", "missing.omwscripts", "a.esp"].map(String::from).into();
        let got = scan_load_order(&[a.clone(), b.clone()], &content, &rules()).unwrap();
        std::fs::remove_dir_all(&root).ok();
        assert_eq!(got.scripts.len(), 2);
        assert_eq!(got.scripts[0].flags, ["NPC", "PLAYER"]);
        assert_eq!(got.scripts[0].providers.len(), 2);
        let codes: Vec<&str> = got.findings.iter().map(|f| f.2).collect();
        assert_eq!(
            codes,
            [
                "MISSING_OMWSCRIPTS",
                "DUPLICATE_REGISTRATION",
                "OVERRIDDEN_FILE",
                "PER_ACTOR_FRAME",
                "MISSING_SCRIPT",
                "BUILTIN_OVERRIDE"
            ]
        );
    }

    #[test]
    fn lual_flags_as_python() {
        assert_eq!(lual_flags(1 | 32, &[], false), ["GLOBAL", "LOAD"]);
        assert_eq!(lual_flags(0, &["NPC_".into(), "XYZ_".into()], true), ["NPC", "XYZ", "CUSTOM"]);
    }
}
