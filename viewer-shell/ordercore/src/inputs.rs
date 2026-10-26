//! The load order's inputs, as `wraithguard_toolkit.compute_plan` gathers them before it sorts
//! (port step 4.2 slice 2k, part 1): what to sort and from where, which plugins are grass, which
//! cfg entries nobody manages, where each new `data=` folder belongs, and whether every plugin's
//! masters are there and load first.
//!
//! - the subset: [`subset_from_lines`] (a plain-text subset file or a scan held in memory),
//!   [`subset_from_toml`] (the subset file's minimal TOML form) and [`customizations`] (a
//!   momw-configurator `customizations.toml`);
//! - grass: [`groundcover_declarations`] and [`hold_back_groundcover`];
//! - the mods-folder scan: [`mod_folders`] and [`scan_mod_directories`];
//! - the folders a plugin may be in: [`all_scan_dirs`], [`pending_custom_dirs`],
//!   [`list_plugins_in_dir`] and [`PluginIndex`];
//! - a MOMW list's data paths (`momw_datapaths.py`): [`parse_data_paths_cache`],
//!   [`managed_cfg_data_path_norms`] and [`reconcile_data_paths`];
//! - [`cfg_orphans`]: the `--subset-from-cfg` pull;
//! - [`infer_data_path_anchors`]: where a new `data=` folder goes, from the plugins in it;
//! - [`check_missing_masters`]: every plugin's header masters against the order.
//!
//! What each says is returned rather than printed: the names a warning is about, the notes as
//! values, the master check's lines in the Python's English (it does not translate them).
//!
//! The Python is the reference and `tests/fixtures/input_cases.json` pins both. Declined
//! ([`Unsupported`]): a TOML value of a type the Python would not read as a string (it would
//! stringify it, iterate it, or fail), and a TOML file with a byte-order mark. Known and left:
//! `lower()` is Rust's `to_lowercase`; a file name that is not UTF-8 is read lossily (as the
//! native scan always has); a plugin header's master names are read as the Python's tolerant
//! scan reads them (cp1252, cut at the first NUL, stripped), which is what its strict reader
//! gives for every header it can read.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;

use crate::export::{
    basename_if_plugin, extract_data_path_value, normalize_data_path, orphan_cfg_entries, DataInsert,
};
use crate::patterns::Unsupported;
use crate::pyre::{Flags, Regex};
use crate::rules::{is_space, splitlines};

/// What a subset entry or a folder's file must end in to be a plugin (`.omwscripts` too).
pub const PLUGIN_EXTS: [&str; 5] = [".esp", ".esm", ".omwaddon", ".omwgame", ".omwscripts"];

/// The folder names that make a folder a mod's data folder in the scan.
pub const SCAN_ASSET_FOLDERS: [&str; 10] =
    ["icons", "meshes", "scripts", "sound", "textures", "bookart", "music", "fonts", "splash", "video"];

/// Plugin endings that carry a TES3 header (a `.omwscripts` file does not).
const HEADER_EXTS: [&str; 4] = [".esp", ".esm", ".omwaddon", ".omwgame"];

/// Folder links followed this deep at most in the scan, so a link back up cannot walk forever.
const MAX_LINK_DEPTH: usize = 64;

/// The most of a plugin's header record read (it is tiny; a larger size is a damaged file).
const HEADER_READ_CAP: usize = 1 << 20;

fn strip(s: &str) -> &str {
    s.trim_matches(is_space)
}

fn ends_with_any(name: &str, exts: &[&str]) -> bool {
    let lower = name.to_lowercase();
    exts.iter().any(|e| lower.ends_with(e))
}

fn compiled(cell: &'static OnceLock<Regex>, pattern: &str, multiline: bool) -> &'static Regex {
    cell.get_or_init(|| {
        Regex::new(pattern, Flags { ignore_case: !multiline, multiline }).expect("a fixed expression compiles")
    })
}

/// `^\s*groundcover\s*=\s*(.+?)\s*$`, ignoring case: the name a line declares as grass.
fn groundcover_decl(line: &str) -> Result<Option<String>, Unsupported> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let chars: Vec<char> = line.chars().collect();
    let re = compiled(&RE, r"^\s*groundcover\s*=\s*(.+?)\s*$", false);
    Ok(re.match_at(&chars, 0)?.and_then(|m| m.group(1)).map(|(a, b)| chars[a..b].iter().collect()))
}

// ---------------------------------------------------------------- the subset

/// A subset file's lines sorted out (`extract_subset_from_lines`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SubsetEntries {
    /// Plugin filenames, in order.
    pub plugins: Vec<String>,
    /// Data folder paths, in order (inserts with no anchor).
    pub data: Vec<String>,
    /// Entries that are neither (no plugin ending, no slash): the Python warns and skips them.
    pub skipped: Vec<String>,
}

/// A subset-file line without its `#` comment (`_strip_line_comment`): a `#` starts one only at
/// the line's start (after spaces) or after a space, so `FMI_#NotAllDunmer.ESP` stays whole.
pub fn strip_line_comment(line: &str) -> &str {
    if line.trim_start_matches(is_space).starts_with('#') {
        return "";
    }
    let mut space_at = None;
    for (i, c) in line.char_indices() {
        if c == '#' {
            if let Some(at) = space_at {
                return &line[..at];
            }
        }
        space_at = if is_space(c) { Some(i) } else { None };
    }
    line
}

/// One raw entry sorted into `out` (`_classify_subset_entry`).
fn classify(raw: &str, out: &mut SubsetEntries) -> Result<(), Unsupported> {
    let raw = strip(raw);
    if raw.is_empty() || groundcover_decl(raw)?.is_some() {
        return Ok(()); // a grass declaration is read by groundcover_declarations, never as content
    }
    if let Some(name) = basename_if_plugin(raw) {
        out.plugins.push(name);
    } else if raw.replace('\\', "/").contains('/') {
        out.data.push(raw.to_string());
    } else {
        out.skipped.push(raw.to_string());
    }
    Ok(())
}

/// The plain-text subset form: one plugin or data folder a line, `#` comments
/// (`extract_subset_from_lines`).
///
/// # Errors
/// [`Unsupported`] only if the matcher declines a line.
pub fn subset_from_lines<S: AsRef<str>>(lines: &[S]) -> Result<SubsetEntries, Unsupported> {
    let mut out = SubsetEntries::default();
    for line in lines {
        classify(strip_line_comment(line.as_ref()), &mut out)?;
    }
    Ok(out)
}

/// The text's lines, as Python's `str.splitlines` cuts them.
pub fn text_lines(text: &str) -> Vec<String> {
    splitlines(text).into_iter().map(str::to_string).collect()
}

/// The plugins a subset's lines declare as grass (`extract_groundcover_declarations`): the
/// names, once each (ignoring case), and the declared entries that are not plugin filenames (the
/// Python warns and skips them).
///
/// # Errors
/// [`Unsupported`] only if the matcher declines a line.
pub fn groundcover_declarations<S: AsRef<str>>(lines: &[S]) -> Result<(Vec<String>, Vec<String>), Unsupported> {
    let mut found = Vec::new();
    let mut not_plugins = Vec::new();
    let mut seen = HashSet::new();
    for line in lines {
        let Some(value) = groundcover_decl(strip_line_comment(line.as_ref()))? else { continue };
        let name = basename_if_plugin(&value).unwrap_or_else(|| strip(&value).to_string());
        if name.is_empty() {
            continue;
        }
        if !ends_with_any(&name, &PLUGIN_EXTS) {
            not_plugins.push(name);
            continue;
        }
        if seen.insert(name.to_lowercase()) {
            found.push(name);
        }
    }
    Ok((found, not_plugins))
}

/// A subset file's minimal TOML form (`subset = [...]`, `data = [...]`, `groundcover = [...]`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SubsetToml {
    /// `subset`, sorted as the plain-text lines are, then `data` as given.
    pub entries: SubsetEntries,
    /// `groundcover`, as given (`declared_groundcover` reads it).
    pub groundcover: Vec<String>,
}

fn parse_toml(text: &str) -> Result<toml::Table, Unsupported> {
    if text.starts_with('\u{feff}') {
        return Err(Unsupported);
    }
    // a syntax error is the Python's to report, in tomllib's words
    text.parse::<toml::Table>().map_err(|_| Unsupported)
}

/// The strings of an array value (missing is empty); anything else is declined.
fn strings(value: Option<&toml::Value>) -> Result<Vec<String>, Unsupported> {
    let Some(value) = value else { return Ok(Vec::new()) };
    let toml::Value::Array(items) = value else { return Err(Unsupported) };
    items.iter().map(|v| v.as_str().map(str::to_string).ok_or(Unsupported)).collect()
}

/// Read the subset file's TOML form (`extract_subset_from_subset_file`, and the `groundcover`
/// key `declared_groundcover` reads).
///
/// # Errors
/// [`Unsupported`] for a file tomllib would refuse (its message is the Python's to give), a
/// byte-order mark, or a value that is not a list of strings.
pub fn subset_from_toml(text: &str) -> Result<SubsetToml, Unsupported> {
    let table = parse_toml(text)?;
    let mut out = SubsetToml::default();
    for raw in strings(table.get("subset"))? {
        classify(&raw, &mut out.entries)?;
    }
    out.entries.data.extend(strings(table.get("data"))?);
    out.groundcover = strings(table.get("groundcover"))?;
    Ok(out)
}

/// What a `customizations.toml` adds to the sort (`extract_subset_from_toml`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Customizations {
    /// Plugin filenames, in the file's order, once each (ignoring case).
    pub subset: Vec<String>,
    /// The data folders, with the anchors given.
    pub data: Vec<DataInsert>,
    /// The plugins that came from a `replace` block's `dest` (sorted, not emitted as inserts).
    pub replace_dest: Vec<String>,
    /// Which `[[Customizations]]` block (`listName`) each plugin came from.
    pub list_names: BTreeMap<String, String>,
}

/// Python's truth of a TOML value.
fn truthy(v: &toml::Value) -> bool {
    match v {
        toml::Value::String(s) => !s.is_empty(),
        toml::Value::Integer(i) => *i != 0,
        toml::Value::Float(f) => *f != 0.0,
        toml::Value::Boolean(b) => *b,
        toml::Value::Datetime(_) => true,
        toml::Value::Array(a) => !a.is_empty(),
        toml::Value::Table(t) => !t.is_empty(),
    }
}

/// An optional string key; another type is declined.
fn opt_str(t: &toml::Table, key: &str) -> Result<Option<String>, Unsupported> {
    match t.get(key) {
        None => Ok(None),
        Some(toml::Value::String(s)) => Ok(Some(s.clone())),
        Some(_) => Err(Unsupported),
    }
}

/// The tables of an array key (missing is none); anything else is declined.
fn tables<'a>(t: &'a toml::Table, key: &str) -> Result<Vec<&'a toml::Table>, Unsupported> {
    match t.get(key) {
        None => Ok(Vec::new()),
        Some(toml::Value::Array(items)) => items.iter().map(|v| v.as_table().ok_or(Unsupported)).collect(),
        Some(_) => Err(Unsupported),
    }
}

/// Read a momw-configurator `customizations.toml` (`extract_subset_from_toml`): each block's
/// `insert` / `insertBlock` values (a plugin, or a folder with its `after` / `before`), its
/// `replace` destinations (anchored before what they replace) and the `content=` lines of its
/// `append` / `appendBlock`.
///
/// # Errors
/// [`Unsupported`] for a file tomllib would refuse, a byte-order mark, or a value of a type the
/// Python does not read as a string.
pub fn customizations(text: &str) -> Result<Customizations, Unsupported> {
    static APPEND: OnceLock<Regex> = OnceLock::new();
    let table = parse_toml(text)?;
    let mut subset: Vec<String> = Vec::new();
    let mut out = Customizations::default();
    let mut replace_seen = HashSet::new();

    for block in tables(&table, "Customizations")? {
        let list_name = opt_str(block, "listName")?.filter(|n| !n.is_empty());
        let mut handle = |value: &str, after: Option<String>, before: Option<String>, replace: bool| {
            if value.is_empty() {
                return;
            }
            if let Some(name) = basename_if_plugin(value) {
                if replace && replace_seen.insert(name.clone()) {
                    out.replace_dest.push(name.clone());
                }
                if let Some(list) = &list_name {
                    out.list_names.insert(name.clone(), list.clone());
                }
                subset.push(name);
            } else if value.replace('\\', "/").contains('/') {
                out.data.push(DataInsert { value: value.to_string(), after, before });
            }
        };
        for ins in tables(block, "insert")? {
            let after = opt_str(ins, "after")?;
            let before = opt_str(ins, "before")?;
            match ins.get("insert") {
                // a value that is not a string is passed over, as the Python's isinstance check does
                Some(single) => {
                    if let Some(v) = single.as_str() {
                        handle(v, after, before, false);
                    }
                }
                None => {
                    let lines = match ins.get("insertBlock") {
                        None => String::new(),
                        Some(toml::Value::String(s)) => s.clone(),
                        Some(v) if !truthy(v) => String::new(),
                        Some(_) => return Err(Unsupported),
                    };
                    for line in lines.split('\n').filter(|l| !l.is_empty()) {
                        handle(&line.replace('\r', ""), after.clone(), before.clone(), false);
                    }
                }
            }
        }
        for rep in tables(block, "replace")? {
            let source = opt_str(rep, "source")?;
            if let Some(dest) = rep.get("dest").and_then(toml::Value::as_str) {
                handle(dest, None, source, true);
            }
        }
        for ap in tables(block, "append")? {
            let chosen = [ap.get("append"), ap.get("appendBlock")].into_iter().flatten().find(|v| truthy(v));
            let text = match chosen {
                None => String::new(),
                Some(toml::Value::String(s)) => s.clone(),
                Some(_) => return Err(Unsupported),
            };
            let chars: Vec<char> = text.chars().collect();
            for m in compiled(&APPEND, r"^\s*content\s*=\s*(\S+)", true).find_all(&chars)? {
                let Some((a, b)) = m.group(1) else { continue };
                let name: String = chars[a..b].iter().collect();
                if let Some(list) = &list_name {
                    out.list_names.insert(name.clone(), list.clone());
                }
                subset.push(name);
            }
        }
    }
    let mut seen = HashSet::new();
    out.subset = subset.into_iter().filter(|s| seen.insert(s.to_lowercase())).collect();
    Ok(out)
}

// ---------------------------------------------------------------- grass

/// The subset without the plugins the cfg (or this run) declares as grass, and those held back
/// in the subset's order (`hold_back_groundcover`). Their `data=` folders are still written:
/// OpenMW has to find the file for the `groundcover=` line to work.
pub fn hold_back_groundcover<S: AsRef<str>>(subset: &[S], groundcover: &[S]) -> (Vec<String>, Vec<String>) {
    let blocked: HashSet<String> = groundcover.iter().map(|n| n.as_ref().to_lowercase()).collect();
    let (mut kept, mut held) = (Vec::new(), Vec::new());
    for name in subset.iter().map(AsRef::as_ref) {
        if blocked.contains(&name.to_lowercase()) {
            held.push(name.to_string());
        } else {
            kept.push(name.to_string());
        }
    }
    (kept, held)
}

// ---------------------------------------------------------------- the mods-folder scan

/// The lower-case extension of a file name, with its dot (`".esp"`), or `""`.
fn ext_of(name: &str) -> String {
    match name.rfind('.') {
        Some(i) if i > 0 => name[i..].to_lowercase(),
        _ => String::new(),
    }
}

/// Every folder under `start` (itself included) that holds one of `asset_dirs` (by lower-case
/// name) or a file ending in one of `plugin_exts` (lower case, with the dot): its path, and the
/// plugin names directly inside it. A match is not looked into further. In the order a top-down
/// walk meets them, sub-folders in the order the system lists them.
pub fn mod_folders(start: &Path, asset_dirs: &HashSet<String>, plugin_exts: &HashSet<String>) -> Vec<(String, Vec<String>)> {
    let mut out = Vec::new();
    let mut seen: HashSet<PathBuf> = HashSet::new();
    if let Ok(c) = fs::canonicalize(start) {
        seen.insert(c);
    }
    let mut stack: Vec<(PathBuf, usize)> = vec![(start.to_path_buf(), 0)];
    while let Some((dir, depth)) = stack.pop() {
        let Ok(rd) = fs::read_dir(&dir) else { continue };
        let mut subdirs: Vec<(PathBuf, bool)> = Vec::new();
        let mut plugins = Vec::new();
        let mut has_assets = false;
        for entry in rd.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let Ok(ft) = entry.file_type() else { continue };
            let path = entry.path();
            let link = ft.is_symlink();
            let is_dir = if link { fs::metadata(&path).map(|m| m.is_dir()).unwrap_or(false) } else { ft.is_dir() };
            if is_dir {
                if asset_dirs.contains(&name.to_lowercase()) {
                    has_assets = true;
                }
                subdirs.push((path, link));
            } else if plugin_exts.contains(&ext_of(&name)) {
                plugins.push(name);
            }
        }
        if has_assets || !plugins.is_empty() {
            out.push((dir.to_string_lossy().into_owned(), plugins));
            continue;
        }
        for (p, link) in subdirs.into_iter().rev() {
            if link {
                if depth >= MAX_LINK_DEPTH {
                    continue;
                }
                if !fs::canonicalize(&p).is_ok_and(|c| seen.insert(c)) {
                    continue;
                }
            }
            stack.push((p, depth + 1));
        }
    }
    out
}

/// Python's `os.path.abspath`: absolute against the working folder, `.` and `..` folded away
/// by the text alone (a link is not followed, so a junctioned mod folder keeps its path).
pub fn abspath(path: &str) -> String {
    let p = Path::new(path);
    let abs = std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf());
    let mut out = PathBuf::new();
    for c in abs.components() {
        match c {
            Component::ParentDir => {
                if !out.pop() {
                    out.push(c);
                }
            }
            Component::CurDir => {}
            other => out.push(other),
        }
    }
    out.to_string_lossy().into_owned()
}

/// The mods-folder scan as a subset (`scan_mod_directories`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Scan {
    /// Per mod folder: its absolute path, its plugins (ignoring case, sorted), a blank line.
    pub lines: Vec<String>,
    /// How many mod folders.
    pub folders: usize,
    /// How many plugins.
    pub plugins: usize,
    /// The folder scanned, absolute.
    pub start: String,
}

impl Scan {
    /// The subset file's text: the lines, each ending in `\n`.
    pub fn text(&self) -> String {
        if self.lines.is_empty() { String::new() } else { format!("{}\n", self.lines.join("\n")) }
    }
}

/// Scan `start` for mod folders (`scan_mod_directories`): each folder holding an asset folder or
/// a plugin gives its path (a `data=` entry) and its plugins (`content=` entries).
pub fn scan_mod_directories(start: &str) -> Scan {
    let assets: HashSet<String> = SCAN_ASSET_FOLDERS.iter().map(|s| (*s).to_string()).collect();
    let exts: HashSet<String> = PLUGIN_EXTS.iter().map(|s| (*s).to_string()).collect();
    let mut scan = Scan { start: abspath(start), ..Scan::default() };
    for (root, mut found) in mod_folders(Path::new(start), &assets, &exts) {
        found.sort_by_key(|n| n.to_lowercase());
        scan.lines.push(abspath(&root));
        scan.plugins += found.len();
        scan.lines.extend(found);
        scan.lines.push(String::new());
        scan.folders += 1;
    }
    scan
}

// ---------------------------------------------------------------- folders and plugin files

/// Every folder a plugin may be in this run (`all_scan_dirs`): the cfg's `data=` folders, then
/// the pending inserts (`data_inserts` first) that are not among them, ignoring case.
///
/// # Errors
/// [`Unsupported`] only if the matcher declines a line.
pub fn all_scan_dirs(
    data_order: &[String],
    raw_toml_data_inserts: &[String],
    data_inserts: &[String],
) -> Result<Vec<String>, Unsupported> {
    let mut dirs = Vec::new();
    for line in data_order {
        if let Some(v) = extract_data_path_value(line)?.filter(|v| !v.is_empty()) {
            dirs.push(v);
        }
    }
    let mut seen: HashSet<String> = dirs.iter().map(|d| d.to_lowercase()).collect();
    for v in data_inserts.iter().chain(raw_toml_data_inserts) {
        if !v.is_empty() && seen.insert(v.to_lowercase()) {
            dirs.push(v.clone());
        }
    }
    Ok(dirs)
}

/// The pending custom folders alone, once each, in declared order (`pending_custom_dirs`).
pub fn pending_custom_dirs(raw_toml_data_inserts: &[String], data_inserts: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    data_inserts
        .iter()
        .chain(raw_toml_data_inserts)
        .filter(|v| !v.is_empty() && seen.insert(v.to_lowercase()))
        .cloned()
        .collect()
}

/// The plugin files directly in a `data=` folder, sorted (`list_plugins_in_dir`). A relative
/// folder is tried as given and then against `base_dir`; one that cannot be read gives nothing.
pub fn list_plugins_in_dir(path_value: &str, base_dir: Option<&Path>) -> Vec<String> {
    let raw = strip(path_value).trim_matches('"').trim_matches('\'');
    if raw.is_empty() {
        return Vec::new();
    }
    let p = PathBuf::from(raw);
    let mut candidates = vec![p.clone()];
    if let Some(base) = base_dir {
        if !p.is_absolute() {
            candidates.push(base.join(&p));
        }
    }
    for c in candidates {
        if !c.is_dir() {
            continue;
        }
        let Ok(rd) = fs::read_dir(&c) else { continue };
        let mut names: Vec<String> = rd
            .flatten()
            .filter(|e| e.path().is_file())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| ends_with_any(n, &PLUGIN_EXTS))
            .collect();
        names.sort();
        return names;
    }
    Vec::new()
}

/// The plugin files of a run's folders by lower-case name (`PluginFileIndex`). A later folder
/// wins a name both hold, as a later `data=` line shadows an earlier one in OpenMW.
#[derive(Clone, Debug, Default)]
pub struct PluginIndex {
    files: HashMap<String, PathBuf>,
}

impl PluginIndex {
    /// Index the plugin files directly in each folder; one that cannot be read is passed over.
    pub fn build<S: AsRef<str>>(dirs: &[S]) -> PluginIndex {
        let mut files = HashMap::new();
        for dir in dirs {
            let path = Path::new(dir.as_ref());
            if !path.is_dir() {
                continue;
            }
            let Ok(rd) = fs::read_dir(path) else { continue };
            for entry in rd.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if ends_with_any(&name, &PLUGIN_EXTS) && entry.path().is_file() {
                    files.insert(name.to_lowercase(), entry.path());
                }
            }
        }
        PluginIndex { files }
    }

    /// A plugin's file, by name, ignoring case. `None` does not prove it is absent: see
    /// [`PluginIndex::usable`].
    pub fn find(&self, name: &str) -> Option<&Path> {
        self.files.get(&name.to_lowercase()).map(PathBuf::as_path)
    }

    /// Whether any folder held a plugin, so a "not found" means absent rather than unseen.
    pub fn usable(&self) -> bool {
        !self.files.is_empty()
    }
}

/// cp1252, as Python decodes it with `errors="replace"` (its five unused bytes become U+FFFD).
pub fn cp1252(bytes: &[u8]) -> String {
    const HIGH: [u32; 32] = [
        0x20AC, 0xFFFD, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160, 0x2039, 0x0152, 0xFFFD,
        0x017D, 0xFFFD, 0xFFFD, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014, 0x02DC, 0x2122, 0x0161, 0x203A,
        0x0153, 0xFFFD, 0x017E, 0x0178,
    ];
    bytes
        .iter()
        .map(|&b| match b {
            0x80..=0x9F => char::from_u32(HIGH[usize::from(b - 0x80)]).unwrap_or('\u{fffd}'),
            _ => char::from(b),
        })
        .collect()
}

/// A plugin's header masters with the sizes recorded beside them
/// (`read_plugin_masters_with_sizes`): each `MAST` name and its `DATA` size, `None` when that is
/// missing or short. Empty for a file that is not a TES3 plugin or cannot be read.
pub fn read_masters_with_sizes(path: &Path) -> Vec<(String, Option<u64>)> {
    let Ok(mut f) = fs::File::open(path) else { return Vec::new() };
    let mut head = [0u8; 16];
    if f.read_exact(&mut head).is_err() || &head[..4] != b"TES3" {
        return Vec::new();
    }
    let size = u32::from_le_bytes([head[4], head[5], head[6], head[7]]) as usize;
    let mut data = Vec::new();
    if f.take(size.min(HEADER_READ_CAP) as u64).read_to_end(&mut data).is_err() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut pending: Option<String> = None;
    let mut i = 0;
    while i + 8 <= data.len() {
        let tag = &data[i..i + 4];
        let sz = u32::from_le_bytes([data[i + 4], data[i + 5], data[i + 6], data[i + 7]]) as usize;
        i += 8;
        let chunk = &data[i..(i.saturating_add(sz)).min(data.len())];
        i = i.saturating_add(sz);
        if tag == b"MAST" {
            if let Some(p) = pending.take() {
                out.push((p, None));
            }
            let raw = chunk.split(|&b| b == 0).next().unwrap_or_default();
            let name = strip(&cp1252(raw)).to_string();
            pending = (!name.is_empty()).then_some(name);
        } else if tag == b"DATA" {
            if let Some(p) = pending.take() {
                let size = (chunk.len() >= 8).then(|| {
                    u64::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3], chunk[4], chunk[5], chunk[6], chunk[7]])
                });
                out.push((p, size));
            }
        }
    }
    if let Some(p) = pending {
        out.push((p, None));
    }
    out
}

/// What [`check_missing_masters`] found.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MasterCheck {
    /// `[MISSING MASTER]` lines: a master not in the order (installed or not found at all).
    pub missing: Vec<String>,
    /// `[MASTER ORDER]` lines: a master that loads after its dependent.
    pub order_problems: Vec<String>,
    /// `[MASTER SIZE]` lines: the installed master's size is not the one the header recorded.
    pub size_notes: Vec<String>,
    /// How many plugin files were read (0: the mod folders are not reachable from here).
    pub checked: usize,
    /// The plugins behind `missing` and `order_problems`, sorted ignoring case.
    pub problem_plugins: Vec<String>,
}

/// Check every active plugin's header masters against the order (`check_missing_masters`): each
/// must be in the order and load first, and the installed copy's size should be the one the
/// header recorded. Plugins whose files `index` cannot find are passed over.
pub fn check_missing_masters<S: AsRef<str>>(
    active_order: &[S],
    index: &PluginIndex,
    origins: &HashMap<String, String>,
) -> MasterCheck {
    let pos: HashMap<String, usize> =
        active_order.iter().enumerate().map(|(i, p)| (p.as_ref().to_lowercase(), i)).collect();
    let mut out = MasterCheck::default();
    let mut problems: HashSet<String> = HashSet::new();
    let size_of = |name: &str| index.find(name).and_then(|p| fs::metadata(p).ok()).map(|m| m.len());
    for p in active_order.iter().map(AsRef::as_ref) {
        let Some(path) = index.find(p) else { continue };
        let pairs = read_masters_with_sizes(path);
        if pairs.is_empty() && !ends_with_any(p, &HEADER_EXTS) {
            continue;
        }
        out.checked += 1;
        let tag = origins.get(&p.to_lowercase()).map(|o| format!(" [{o}]")).unwrap_or_default();
        for (m, recorded) in pairs {
            let Some(&mpos) = pos.get(&m.to_lowercase()) else {
                out.missing.push(if index.find(&m).is_none() {
                    format!(
                        "[MISSING MASTER] '{p}'{tag} requires '{m}' -- NOT FOUND in any data folder. \
                         The game will fail to load with this plugin enabled."
                    )
                } else {
                    format!(
                        "[MISSING MASTER] '{p}'{tag} requires '{m}' -- installed but not in the load \
                         order. Enable/add it (it must load before '{p}')."
                    )
                });
                problems.insert(p.to_string());
                continue;
            };
            if mpos > pos[&p.to_lowercase()] {
                out.order_problems
                    .push(format!("[MASTER ORDER] '{p}'{tag} loads BEFORE its master '{m}' -- '{m}' must come first."));
                problems.insert(p.to_string());
            }
            // 0 counts: a failed tes3cmd sync zeroes these
            if let (Some(recorded), Some(actual)) = (recorded, size_of(&m)) {
                if actual != recorded {
                    let hint = if recorded == 0 {
                        "header records 0 bytes -- likely damaged by a tes3cmd sync that couldn't find the master"
                    } else {
                        "made against a different version of the master (usually fine)"
                    };
                    out.size_notes.push(format!(
                        "[MASTER SIZE] '{p}'{tag}: header says '{m}' was {recorded} bytes, installed copy is \
                         {actual} -- {hint}. The tes3cmd window's in-app resync fixes this."
                    ));
                }
            }
        }
    }
    out.problem_plugins = problems.into_iter().collect();
    out.problem_plugins.sort_by_key(|n| n.to_lowercase());
    out
}

// ---------------------------------------------------------------- a MOMW list's data paths

/// Python's `str.casefold` as far as [`compact`] can tell: the letters whose folding gives an
/// ASCII letter that lower-casing does not.
fn casefold_char(c: char, out: &mut String) {
    match c {
        'ß' | 'ẞ' => out.push_str("ss"),
        'ſ' => out.push('s'),
        'ﬀ' => out.push_str("ff"),
        'ﬁ' => out.push_str("fi"),
        'ﬂ' => out.push_str("fl"),
        'ﬃ' => out.push_str("ffi"),
        'ﬄ' => out.push_str("ffl"),
        'ﬅ' | 'ﬆ' => out.push_str("st"),
        'ŉ' => out.push_str("\u{2bc}n"),
        'ǰ' => out.push_str("j\u{30c}"),
        'ẖ' => out.push_str("h\u{331}"),
        'ẗ' => out.push_str("t\u{308}"),
        'ẘ' => out.push_str("w\u{30a}"),
        'ẙ' => out.push_str("y\u{30a}"),
        'ẚ' => out.push_str("a\u{2be}"),
        _ => out.extend(c.to_lowercase()),
    }
}

/// A path's key for matching a list's tails (`_compact`): case-folded, letters `a`-`z` and
/// digits only, so case, slashes and punctuation do not matter.
pub fn compact(value: &str) -> String {
    let mut folded = String::new();
    for c in value.chars() {
        casefold_char(c, &mut folded);
    }
    folded.chars().filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit()).collect()
}

/// A cached list of data paths (`parse_data_paths_cache`): one relative tail a line, in load
/// order, forward-slashed; blank lines and `#` comments passed over. `text` is the file's, a
/// leading byte-order mark allowed.
pub fn parse_data_paths_cache(text: &str) -> Vec<String> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    splitlines(text)
        .into_iter()
        .map(strip)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| l.replace('\\', "/"))
        .collect()
}

/// The tails by their last folder's [`compact`] key: `(the whole tail's key, its place)`.
type TailIndex = HashMap<String, Vec<(String, usize)>>;

fn index_tails<S: AsRef<str>>(tails: &[S]) -> TailIndex {
    let mut idx: TailIndex = HashMap::new();
    for (i, tail) in tails.iter().enumerate() {
        let tail = tail.as_ref();
        let Some(last) = tail.split('/').rfind(|c| !c.is_empty()) else { continue };
        idx.entry(compact(last)).or_default().push((compact(tail), i));
    }
    idx
}

/// The place of the tail a cfg `data=` value is (its key ends with the tail's), or `None`.
fn match_index(value: &str, idx: &TailIndex) -> Option<usize> {
    let slashed = value.replace('\\', "/");
    let last = slashed.split('/').rfind(|c| !c.is_empty())?;
    let candidates = idx.get(&compact(last))?;
    let whole = compact(value);
    candidates.iter().find(|(tail, _)| !tail.is_empty() && whole.ends_with(tail.as_str())).map(|&(_, i)| i)
}

/// The [`normalize_data_path`] keys of the cfg's `data=` values the list's tails account for
/// (`managed_cfg_data_path_norms`).
pub fn managed_cfg_data_path_norms<S: AsRef<str>, T: AsRef<str>>(tails: &[S], cfg_data_paths: &[T]) -> HashSet<String> {
    if tails.is_empty() {
        return HashSet::new();
    }
    let idx = index_tails(tails);
    let mut managed: HashSet<String> = cfg_data_paths
        .iter()
        .map(AsRef::as_ref)
        .filter(|v| match_index(v, &idx).is_some())
        .map(normalize_data_path)
        .collect();
    managed.remove("");
    managed
}

/// The list's data paths against the cfg's (`reconcile_data_paths`): the ones not in the cfg (a
/// count, or each with `report_missing`), and the first place the cfg's order is not the list's.
pub fn reconcile_data_paths<S: AsRef<str>, T: AsRef<str>>(
    tails: &[S],
    cfg_data_paths: &[T],
    report_missing: bool,
) -> Vec<String> {
    if tails.is_empty() {
        return Vec::new();
    }
    let idx = index_tails(tails);
    let matched: Vec<usize> = cfg_data_paths.iter().filter_map(|v| match_index(v.as_ref(), &idx)).collect();
    let present: HashSet<usize> = matched.iter().copied().collect();
    let missing: Vec<&str> =
        tails.iter().enumerate().filter(|(i, _)| !present.contains(i)).map(|(_, t)| t.as_ref()).collect();
    let mut findings = Vec::new();
    if !missing.is_empty() {
        if report_missing {
            findings.extend(missing.iter().map(|t| format!("[DATA PATH] the list's '{t}' is not in openmw.cfg.")));
        } else {
            findings.push(format!(
                "[DATA PATH] {} of the list's data= path(s) are not in openmw.cfg (usually mods you haven't \
                 installed -- not enforced). Re-run with -v to list them.",
                missing.len()
            ));
        }
    }
    if matched.windows(2).any(|w| w[1] < w[0]) {
        findings.push(
            "[DATA PATH ORDER] a data= path appears earlier in openmw.cfg than one the list's order puts \
             before it."
                .to_string(),
        );
    }
    findings
}

// ---------------------------------------------------------------- the cfg's orphans

/// What [`cfg_orphans`] is asked about.
#[derive(Clone, Debug, Default)]
pub struct OrphanQuery<'a> {
    /// The subset so far.
    pub subset: &'a [String],
    /// The cfg's `content=` names, in order.
    pub base_order_names: &'a [String],
    /// The cfg's raw `data=` lines.
    pub data_order: &'a [String],
    /// Lower-case plugins the curated list owns.
    pub curated_lower: HashSet<String>,
    /// Lower-case plugins the list flags for cleaning (the `clean_` alias).
    pub needs_cleaning_lower: HashSet<String>,
    /// Lower-case plugins this run declared.
    pub declared_lower: HashSet<String>,
    /// Lower-case plugins the cfg declares as grass.
    pub groundcover_lower: HashSet<String>,
    /// This run's data-path insert values (both kinds).
    pub declared_data: &'a [String],
    /// The list's cached data-path tails, when a cache was given (`--data-paths-cache`).
    pub tails: Option<&'a [String]>,
}

/// The `--subset-from-cfg` pull (`_pull_cfg_orphans`): the cfg's `content=` plugins that are
/// neither curated nor declared (nor a base master, nor grass), in cfg order, without those
/// already in the subset; and with a data-paths cache, the cfg's `data=` folders the list and
/// this run do not account for (`None` without one: nothing can tell them apart).
///
/// # Errors
/// [`Unsupported`] only if the matcher declines a line.
pub fn cfg_orphans(q: &OrphanQuery<'_>) -> Result<(Vec<String>, Option<Vec<String>>), Unsupported> {
    let mut cfg_values = Vec::new();
    for line in q.data_order {
        if let Some(v) = extract_data_path_value(line)?.filter(|v| !v.is_empty()) {
            cfg_values.push(v);
        }
    }
    let managed: HashSet<String> = match q.tails {
        Some(tails) => {
            let mut m = managed_cfg_data_path_norms(tails, &cfg_values);
            m.extend(q.declared_data.iter().map(|d| normalize_data_path(d)).filter(|n| !n.is_empty()));
            m
        }
        // without the cache every data= path counts as managed (no orphans)
        None => cfg_values.iter().map(|v| normalize_data_path(v)).collect(),
    };
    let (plugins, data) = orphan_cfg_entries(
        q.base_order_names,
        q.data_order,
        &q.curated_lower,
        &q.needs_cleaning_lower,
        &q.declared_lower,
        &managed,
    )?;
    let already: HashSet<String> = q.subset.iter().map(|s| s.to_lowercase()).collect();
    let plugins = plugins
        .into_iter()
        .filter(|p| !q.groundcover_lower.contains(&p.to_lowercase()) && !already.contains(&p.to_lowercase()))
        .collect();
    Ok((plugins, q.tails.map(|_| data)))
}

// ---------------------------------------------------------------- data= anchors

/// An anchor [`infer_data_path_anchors`] chose: the insert's value, `after` or `before`, the
/// existing folder, and the plugin it went by.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InferredAnchor {
    pub value: String,
    pub mode: &'static str,
    pub anchor: String,
    pub via: String,
}

/// Anchor the `data=` inserts that came with none (`infer_data_path_anchors`): next to the
/// existing folder holding the nearest plugin, before the folder's own plugins in
/// `final_order` (after it) or failing that after them (before it). Written into `inserts`.
///
/// # Errors
/// [`Unsupported`] only if the matcher declines a line.
pub fn infer_data_path_anchors(
    inserts: &mut [DataInsert],
    data_order: &[String],
    final_order: &[String],
    base_dir: Option<&Path>,
) -> Result<Vec<InferredAnchor>, Unsupported> {
    let mut notes = Vec::new();
    if final_order.is_empty() {
        return Ok(notes);
    }
    let mut owner: HashMap<String, String> = HashMap::new();
    for line in data_order {
        let Some(val) = extract_data_path_value(line)?.filter(|v| !v.is_empty()) else { continue };
        for plugin in list_plugins_in_dir(&val, base_dir) {
            owner.entry(plugin.to_lowercase()).or_insert_with(|| val.clone());
        }
    }
    let order_index: HashMap<String, usize> =
        final_order.iter().enumerate().map(|(i, n)| (n.to_lowercase(), i)).collect();
    let set = |s: &Option<String>| s.as_deref().is_some_and(|v| !v.is_empty());
    for item in inserts.iter_mut() {
        if set(&item.after) || set(&item.before) {
            continue; // the user's anchor stands
        }
        let own: HashSet<String> =
            list_plugins_in_dir(&item.value, base_dir).iter().map(|p| p.to_lowercase()).collect();
        let mut positions: Vec<usize> = own.iter().filter_map(|p| order_index.get(p).copied()).collect();
        positions.sort_unstable();
        let (Some(&lo), Some(&hi)) = (positions.first(), positions.last()) else { continue };
        let owned = |i: usize| owner.get(&final_order[i].to_lowercase()).cloned();
        let found = (0..lo)
            .rev()
            .find_map(|i| owned(i).map(|o| ("after", o)))
            .or_else(|| (hi + 1..final_order.len()).find_map(|i| owned(i).map(|o| ("before", o))));
        if let Some((mode, anchor)) = found {
            if mode == "after" {
                item.after = Some(anchor.clone());
            } else {
                item.before = Some(anchor.clone());
            }
            let mut via: Vec<&String> = own.iter().filter(|p| order_index.contains_key(*p)).collect();
            via.sort();
            notes.push(InferredAnchor { value: item.value.clone(), mode, anchor, via: via[0].clone() });
        }
    }
    Ok(notes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hash_in_a_name_is_not_a_comment() {
        assert_eq!(strip_line_comment("FMI_#NotAllDunmer.ESP"), "FMI_#NotAllDunmer.ESP");
        assert_eq!(strip_line_comment("a.esp  # note"), "a.esp ");
        assert_eq!(strip_line_comment("   # all"), "");
    }

    #[test]
    fn compact_folds_as_python_does() {
        assert_eq!(compact("Straße/Mod_01"), "strassemod01");
        assert_eq!(compact("C:\\Games\\ÄB"), "cgamesb");
    }

    #[test]
    fn abspath_folds_dots() {
        let got = abspath("a/./b/../c");
        assert!(got.ends_with(&format!("a{}c", std::path::MAIN_SEPARATOR)), "{got}");
    }

    #[test]
    fn cp1252_has_holes() {
        assert_eq!(cp1252(&[0x80, 0x81, b'a', 0xE9]), "\u{20ac}\u{fffd}a\u{e9}");
    }
}
