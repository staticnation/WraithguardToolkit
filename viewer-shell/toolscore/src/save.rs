//! Save Check (port step 4.4, slice 5): what an OpenMW save depends on, whether the load order
//! has it, and how much space each mod takes in it - `openmw-save-stats` (MIT), ported whole:
//! the bytes of the save credited to the mod that owns them, what a mod's Lua data is made of,
//! the hints to shrink it, and what grew between two saves.
//!
//! An `.omwsave` is an ESM3 file: records (4-byte tag, u32 size, 8 bytes of flags) of
//! subrecords (4-byte tag, u32 size, data). Every subrecord is read once and its bytes go to
//! one owner, so the totals per mod add up to the size of the file:
//!
//! - Lua: each script's data (global, on objects, on the player) goes to the mod declaring the
//!   script, in an `.omwscripts` file or a plugin's `LUAL` records;
//! - saved objects: attaching a local script marks an object changed, so it is saved in full
//!   from then on. An object carrying scripts of mods (and none the engine attaches by type)
//!   goes to those mods; any other to the plugin that placed it, or, created in game, to the
//!   plugin defining its record;
//! - the journal, quests, known topics, globals and global mwscripts go to the plugin defining
//!   the dialogue, variable or script;
//! - the rest (the player, the map, cell headers, fog of war, records created in game) is
//!   OpenMW's.
//!
//! Plugins and `.omwscripts` from one mod folder are grouped under the folder's name.
//!
//! openmw-save-stats: Copyright (c) its author, MIT License (the copy Static sent is the
//! reference). Its format notes cite OpenMW's `components/esm/luascripts.cpp`,
//! `components/lua/serialization.cpp`, `components/lua/configuration.cpp`,
//! `apps/openmw/mwlua/luamanagerimp.cpp`, `apps/openmw/mwworld/worldmodel.cpp`, `cellstore.cpp`,
//! `components/esm3/objectstate.cpp`, `cellref.cpp`, `journalentry.cpp`, `loadglob.cpp`.

use std::collections::{BTreeSet, HashMap};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use indexmap::IndexMap;

pub const ENGINE: &str = "OpenMW";
pub const UNKNOWN_MOD: &str = "(unknown)";

/// Record types an `.omwscripts` tag attaches a local script to.
const TYPE_TAGS: [(&str, &str); 17] = [
    ("ACTIVATOR", "ACTI"),
    ("ARMOR", "ARMO"),
    ("BOOK", "BOOK"),
    ("CLOTHING", "CLOT"),
    ("CONTAINER", "CONT"),
    ("CREATURE", "CREA"),
    ("DOOR", "DOOR"),
    ("INGREDIENT", "INGR"),
    ("LIGHT", "LIGH"),
    ("MISC_ITEM", "MISC"),
    ("NPC", "NPC_"),
    ("POTION", "ALCH"),
    ("WEAPON", "WEAP"),
    ("APPARATUS", "APPA"),
    ("LOCKPICK", "LOCK"),
    ("PROBE", "PROB"),
    ("REPAIR", "REPA"),
];
const FLAG_TAGS: [&str; 5] = ["GLOBAL", "CUSTOM", "PLAYER", "MENU", "LOAD"];
const LUAL_FLAGS: [(u32, &str); 5] = [(1, "GLOBAL"), (2, "CUSTOM"), (4, "PLAYER"), (16, "MENU"), (32, "LOAD")];

pub const LUA_GLOBAL: &str = "Lua global data";
pub const LUA_OBJECTS: &str = "Lua on objects";
pub const LUA_PLAYER: &str = "Lua on player";
pub const LUA_LIST: &str = "Lua script list";
pub const SAVED_BY_SCRIPTS: &str = "objects saved by its scripts";
pub const SAVED_BY_ENGINE: &str = "its objects saved by OpenMW scripts";
pub const PLACED_OBJECTS: &str = "its objects changed";
pub const SPAWNED_OBJECTS: &str = "objects spawned from its records";
pub const CHANGED_RECORDS: &str = "its records changed";
pub const JOURNAL: &str = "journal";
pub const QUESTS: &str = "quests";
pub const TOPICS: &str = "known topics";
pub const GLOBALS: &str = "global variables";
pub const MWSCRIPTS: &str = "global mwscripts";
pub const LOAD_ORDER: &str = "load order entry";

fn engine_record(tag: &str) -> Option<&'static str> {
    Some(match tag {
        "SAVE" => "save header and screenshot",
        "PLAY" => "player",
        "GMAP" => "world map",
        "WTHR" => "weather",
        "DCOU" => "death counts",
        "STLN" => "stolen items",
        "DIAS" => "dialogue state",
        "PROJ" => "projectiles",
        "SPLM" => "magic projectiles",
        "ACTC" => "actor ids",
        "CAM_" => "camera",
        "INPU" => "input",
        "KEYS" => "quick keys",
        "MARK" => "Mark location",
        "ASPL" => "active spells",
        "RAND" => "random seed",
        "DYNA" => "dynamic record counters",
        "ENAB" => "teleport and levitation flags",
        _ => return None,
    })
}

// Lua serialization (components/lua/serialization.cpp).
const NUMBER: u8 = 0x0;
const LONG_STRING: u8 = 0x1;
const BOOLEAN: u8 = 0x2;
const TABLE_START: u8 = 0x3;
const TABLE_END: u8 = 0x4;
const SHORT_STRING_FLAG: u8 = 0x20;
const CUSTOM_FULL_FLAG: u8 = 0x40;
const CUSTOM_COMPACT_FLAG: u8 = 0x80;
fn fixed_size(t: u8) -> Option<usize> {
    Some(match t {
        0x10 => 16,
        0x11 => 24,
        0x12 => 128,
        0x13 => 32,
        0x14 => 32,
        0x15 => 16,
        _ => return None,
    })
}
/// A table with more entries is a map (by record id, cell name...): its keys are grouped.
const FOLD_KEYS: usize = 20;
/// The serialized data of an `onSave` that returned `{}`.
const EMPTY_TABLE: &[u8] = b"\x00\x03\x04";
/// What a hint must save to be listed among the hints of the whole save.
const MIN_HINT: f64 = 16.0 * 1024.0;

/// A path as the engine compares it: trimmed, `/`, lower case.
pub fn normalize(path: &str) -> String {
    path.trim().replace('\\', "/").to_lowercase()
}

fn latin1(b: &[u8]) -> String {
    b.iter().map(|&c| c as char).collect()
}

fn u32_at(d: &[u8], p: usize) -> Option<u32> {
    d.get(p..p + 4).map(|b| u32::from_le_bytes(b.try_into().expect("4 bytes")))
}

fn i32_at(d: &[u8], p: usize) -> Option<i32> {
    d.get(p..p + 4).map(|b| i32::from_le_bytes(b.try_into().expect("4 bytes")))
}

fn slice(d: &[u8], start: usize, size: usize) -> &[u8] {
    let s = start.min(d.len());
    &d[s..start.saturating_add(size).min(d.len())]
}

// ---------------------------------------------------------------- the data folders

/// The data folders as the engine builds its VFS (`resources/vfs`, the `data=` folders,
/// `data-local`), the last winning, and the names of the folders that have a fixed one.
#[derive(Clone, Debug, Default)]
pub struct DataDirs {
    pub dirs: Vec<PathBuf>,
    pub labels: HashMap<PathBuf, String>,
}

impl DataDirs {
    /// From a setup's cfg (and those it chains), read as `ordercore::cfg` reads it.
    pub fn from_cfg(cfg: &Path) -> std::io::Result<DataDirs> {
        let setup = ordercore::cfg::read(cfg, &ordercore::cfg::Tokens::for_cfg(cfg))?;
        let mut labels = HashMap::new();
        if let Some(local) = &setup.data_local {
            labels.insert(local.clone(), "data-local".to_string());
        }
        let mut dirs = Vec::new();
        if let Some(r) = &setup.resources {
            let vfs = r.join("vfs");
            labels.insert(vfs.clone(), ENGINE.to_string());
            let root = std::fs::canonicalize(r).unwrap_or_else(|_| r.clone());
            for d in &setup.data {
                let canon = std::fs::canonicalize(d).unwrap_or_else(|_| d.clone());
                if canon.starts_with(&root) {
                    labels.insert(d.clone(), ENGINE.to_string());
                }
            }
            dirs.push(vfs);
        }
        dirs.extend(setup.data.iter().cloned());
        Ok(DataDirs { dirs, labels })
    }
}

/// Whether a folder's name is one of the generic subfolders mod packages are split into
/// (`00 Core`, `Data Files`...): `\d+\b.*|data files|data|files|core|...`, without case.
fn generic_dir(name: &str) -> bool {
    let digits = name.chars().take_while(char::is_ascii_digit).count();
    if digits > 0 {
        let next = name.chars().nth(digits);
        if next.is_none_or(|c| !(c.is_alphanumeric() || c == '_')) {
            return true;
        }
    }
    matches!(
        name.to_lowercase().as_str(),
        "data files" | "data" | "files" | "core" | "main" | "mod" | "base" | "base mod" | "patch" | "patches" | "optional" | "options" | "extras" | "hd" | "modular"
    )
}

/// The mod a data folder belongs to: its own name without the generic subfolders.
pub fn mod_name(directory: &Path, labels: &HashMap<PathBuf, String>) -> String {
    if let Some(l) = labels.get(directory) {
        return l.clone();
    }
    let parts: Vec<String> = directory.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
    let mut n = parts.len();
    while n > 1 && generic_dir(&parts[n - 1]) {
        n -= 1;
    }
    parts.get(n.saturating_sub(1)).cloned().unwrap_or_default()
}

// ---------------------------------------------------------------- the load order

/// A Lua script's declaration: the content file declaring it, its flags and record types.
#[derive(Clone, Debug, Default)]
pub struct ScriptDecl {
    pub content: usize,
    pub flags: BTreeSet<String>,
    pub types: BTreeSet<String>,
}

/// Which content file declares which script and defines which record.
#[derive(Debug, Default)]
pub struct LoadOrder {
    pub content_files: Vec<String>,
    pub mods: Vec<String>,
    pub missing: Vec<String>,
    pub decls: HashMap<String, ScriptDecl>,
    /// Record id (lower case) to the first content file defining it: the one that created it.
    pub dialogues: HashMap<String, usize>,
    pub globals: HashMap<String, usize>,
    pub mwscripts: HashMap<String, usize>,
    pub objects: HashMap<String, usize>,
    pub resolved: bool,
}

impl LoadOrder {
    pub fn new(content_files: &[String], data_dirs: Option<&DataDirs>) -> LoadOrder {
        let mut lo = LoadOrder { content_files: content_files.to_vec(), resolved: data_dirs.is_some(), ..Default::default() };
        let mut cache: HashMap<PathBuf, HashMap<String, PathBuf>> = HashMap::new();
        for (index, name) in content_files.iter().enumerate() {
            let path = data_dirs.and_then(|dd| find(name, dd, &mut cache));
            if name.eq_ignore_ascii_case("builtin.omwscripts") {
                lo.mods.push(ENGINE.to_string());
            } else if let (Some(p), Some(dd)) = (&path, data_dirs) {
                lo.mods.push(mod_name(p.parent().unwrap_or(Path::new("")), &dd.labels));
            } else {
                lo.mods.push(Path::new(name).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default());
            }
            let Some(path) = path else {
                if data_dirs.is_some() {
                    lo.missing.push(name.clone());
                }
                continue;
            };
            if name.to_lowercase().ends_with(".omwscripts") {
                lo.read_omwscripts(&path, index);
            } else {
                lo.read_plugin(&path, index);
            }
        }
        lo
    }

    fn read_omwscripts(&mut self, path: &Path, index: usize) {
        let Ok(bytes) = std::fs::read(path) else { return };
        let text = String::from_utf8_lossy(&bytes);
        for line in luacore::omwscripts::py_splitlines(&text) {
            let line = luacore::omwscripts::py_strip(line);
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((tags, script)) = line.split_once(':') else { continue };
            let mut decl = ScriptDecl { content: index, ..Default::default() };
            for tag in tags.split(|c: char| c.is_whitespace() || c == ',').filter(|t| !t.is_empty()) {
                if FLAG_TAGS.contains(&tag) {
                    decl.flags.insert(tag.to_string());
                } else if let Some((_, t)) = TYPE_TAGS.iter().find(|(k, _)| *k == tag) {
                    decl.types.insert((*t).to_string());
                }
            }
            self.decls.insert(normalize(script), decl);
        }
    }

    fn read_plugin(&mut self, path: &Path, index: usize) {
        let Ok(d) = std::fs::read(path) else { return };
        if d.get(..4) != Some(b"TES3") {
            return;
        }
        let end = d.len();
        let mut p = 0usize;
        while p + 16 <= end {
            let tag = &d[p..p + 4];
            let size = u32_at(&d, p + 4).unwrap_or(0) as usize;
            let start = p + 16;
            p = start + size;
            if tag == b"LUAL" {
                self.read_lual(&d, start, p.min(end), index);
                continue;
            }
            if matches!(tag, b"CELL" | b"LAND" | b"PGRD" | b"INFO" | b"TES3" | b"LTEX") || start + 8 > end {
                continue;
            }
            let sub = &d[start..start + 4];
            let sub_size = u32_at(&d, start + 4).unwrap_or(0) as usize;
            let name = if tag == b"SCPT" && sub == b"SCHD" {
                slice(&d, start + 8, sub_size.min(32))
            } else if sub == b"NAME" {
                slice(&d, start + 8, sub_size)
            } else {
                continue;
            };
            let key = latin1(name.split(|&b| b == 0).next().unwrap_or(&[])).to_lowercase();
            let table = match tag {
                b"DIAL" => &mut self.dialogues,
                b"GLOB" => &mut self.globals,
                b"SCPT" => &mut self.mwscripts,
                _ => &mut self.objects,
            };
            table.entry(key).or_insert(index);
        }
    }

    fn read_lual(&mut self, d: &[u8], mut p: usize, end: usize, index: usize) {
        let mut current: Option<String> = None;
        while p + 8 <= end {
            let tag = &d[p..p + 4];
            let size = u32_at(d, p + 4).unwrap_or(0) as usize;
            let data = slice(d, p + 8, size);
            match tag {
                b"LUAS" => {
                    let key = normalize(latin1(data).trim_end_matches('\0'));
                    self.decls.insert(key.clone(), ScriptDecl { content: index, ..Default::default() });
                    current = Some(key);
                }
                b"LUAF" if size >= 4 => {
                    if let Some(decl) = current.as_ref().and_then(|k| self.decls.get_mut(k)) {
                        let flags = u32_at(data, 0).unwrap_or(0);
                        decl.flags = LUAL_FLAGS.iter().filter(|(bit, _)| flags & bit != 0).map(|(_, n)| (*n).to_string()).collect();
                        decl.types = (4..size.saturating_sub(3)).step_by(4).filter_map(|i| data.get(i..i + 4)).map(latin1).collect();
                    }
                }
                b"LUAR" | b"LUAI" => {
                    if let Some(decl) = current.as_ref().and_then(|k| self.decls.get_mut(k)) {
                        decl.flags.insert(if tag == b"LUAR" { "per record" } else { "per instance" }.to_string());
                    }
                }
                _ => {}
            }
            p += 8 + size;
        }
    }

    pub fn content_mod(&self, index: Option<i64>) -> Option<&str> {
        let i = usize::try_from(index?).ok()?;
        self.mods.get(i).map(String::as_str)
    }

    /// Record types the engine attaches scripts to: their objects are saved anyway.
    pub fn engine_types(&self) -> BTreeSet<String> {
        self.decls.iter().filter(|(p, _)| self.script_mod(p) == ENGINE).flat_map(|(_, d)| d.types.iter().cloned()).collect()
    }

    pub fn script_mod(&self, path: &str) -> String {
        if let Some(d) = self.decls.get(path) {
            return self.mods[d.content].clone();
        }
        if path.starts_with("scripts/omw/") {
            return ENGINE.to_string();
        }
        if !self.resolved {
            let parts: Vec<&str> = path.split('/').collect();
            return if parts.len() > 2 { parts[1].to_string() } else { UNKNOWN_MOD.to_string() };
        }
        UNKNOWN_MOD.to_string()
    }

    fn mod_of_id(&self, table: &HashMap<String, usize>, refid: &str) -> Option<String> {
        table.get(&refid.to_lowercase()).and_then(|&i| self.mods.get(i)).cloned()
    }
}

fn find(name: &str, dd: &DataDirs, cache: &mut HashMap<PathBuf, HashMap<String, PathBuf>>) -> Option<PathBuf> {
    let low = name.to_lowercase();
    for d in dd.dirs.iter().rev() {
        let listing = cache.entry(d.clone()).or_insert_with(|| {
            let mut m = HashMap::new();
            if let Ok(rd) = std::fs::read_dir(d) {
                for e in rd.flatten() {
                    if e.path().is_file() {
                        m.insert(e.file_name().to_string_lossy().to_lowercase(), e.path());
                    }
                }
            }
            m
        });
        if let Some(p) = listing.get(&low) {
            return Some(p.clone());
        }
    }
    None
}

// ---------------------------------------------------------------- low level decoding

/// A RefId as `ESMWriter::writeRefId` writes it.
pub fn decode_refid(data: &[u8]) -> String {
    let Some(&t) = data.first() else { return String::new() };
    let hex_i = |v: i64| if v < 0 { format!("0x-{:x}", -v) } else { format!("0x{v:x}") };
    match t {
        0 => String::new(),
        1 => {
            let n = u32_at(data, 1).unwrap_or(0) as usize;
            latin1(slice(data, 5, n))
        }
        2 => latin1(&data[1..]),
        3 => {
            let index = u32_at(data, 1).unwrap_or(0);
            let content = i32_at(data, 5).unwrap_or(0);
            format!("FormId:{}:0x{index:x}", hex_i(i64::from(content)))
        }
        4 => format!("Generated:0x{:x}", data.get(1..9).map(|b| u64::from_le_bytes(b.try_into().expect("8 bytes"))).unwrap_or(0)),
        5 => format!("Index:{}:{}", latin1(slice(data, 1, 4)), u32_at(data, 5).unwrap_or(0)),
        6 => format!("Exterior {}, {}", i32_at(data, 1).unwrap_or(0), i32_at(data, 5).unwrap_or(0)),
        _ => latin1(data),
    }
}

/// A read past the end, or a type the format does not have.
#[derive(Debug)]
pub struct Bad;

fn byte(d: &[u8], p: usize) -> Result<u8, Bad> {
    d.get(p).copied().ok_or(Bad)
}

/// The position just after the serialized Lua value at `p`.
pub fn lua_value_end(d: &[u8], p: usize) -> Result<usize, Bad> {
    let t = byte(d, p)?;
    let p = p + 1;
    if t & (CUSTOM_COMPACT_FLAG | CUSTOM_FULL_FLAG) != 0 {
        if t & CUSTOM_COMPACT_FLAG != 0 {
            return Ok(p + usize::from(t & 7) + 1 + usize::from((t >> 3) & 15));
        }
        let size = u32_at(d, p).ok_or(Bad)? as usize;
        return Ok(p + 4 + usize::from(t & 63) + 1 + size);
    }
    if t & SHORT_STRING_FLAG != 0 {
        return Ok(p + usize::from(t & 0x1F));
    }
    match t {
        NUMBER => Ok(p + 8),
        LONG_STRING => Ok(p + 4 + u32_at(d, p).ok_or(Bad)? as usize),
        BOOLEAN => Ok(p + 1),
        TABLE_START => {
            let mut q = p;
            while byte(d, q)? != TABLE_END {
                q = lua_value_end(d, lua_value_end(d, q)?)?;
            }
            Ok(q + 1)
        }
        _ => fixed_size(t).map(|n| p + n).ok_or(Bad),
    }
}

/// `@?(0x[0-9a-fA-F]+|-?\d+(\.\d+)?)`, whole.
fn id_key(k: &str) -> bool {
    let k = k.strip_prefix('@').unwrap_or(k);
    if let Some(h) = k.strip_prefix("0x") {
        return !h.is_empty() && h.chars().all(|c| c.is_ascii_hexdigit());
    }
    let k = k.strip_prefix('-').unwrap_or(k);
    let (a, b) = match k.split_once('.') {
        Some((a, b)) => (a, Some(b)),
        None => (k, None),
    };
    let digits = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit());
    digits(a) && b.is_none_or(digits)
}

/// A table key, the keys that are ids grouped together.
fn lua_key_label(d: &[u8], p: usize) -> String {
    let Some(&t) = d.get(p) else { return "<table>".into() };
    if t & (CUSTOM_COMPACT_FLAG | CUSTOM_FULL_FLAG) != 0 {
        return "<object>".into();
    }
    let key = if t & SHORT_STRING_FLAG != 0 {
        String::from_utf8_lossy(slice(d, p + 1, usize::from(t & 0x1F))).into_owned()
    } else if t == LONG_STRING {
        let n = u32_at(d, p + 1).unwrap_or(0) as usize;
        String::from_utf8_lossy(slice(d, p + 5, n)).into_owned()
    } else if t == NUMBER {
        return "[n]".into();
    } else if t == BOOLEAN {
        return "<bool>".into();
    } else {
        return "<table>".into();
    };
    if id_key(&key) { "<id>".into() } else { key }
}

/// A key path's `[count, bytes, entries]`.
pub type KeyStats = IndexMap<Vec<String>, [u64; 3]>;

/// For each key path up to `depth`, how often it occurs, the bytes of its key and value, the
/// entries of its value when a table. Answers the entries of the table at `p`.
pub fn lua_key_sizes(d: &[u8], p: usize, depth: usize, prefix: &[String], out: &mut KeyStats) -> Result<u64, Bad> {
    if byte(d, p)? != TABLE_START {
        return Ok(0);
    }
    let p = p + 1;
    let mut count = 0usize;
    let mut q = p;
    while byte(d, q)? != TABLE_END {
        q = lua_value_end(d, lua_value_end(d, q)?)?;
        count += 1;
    }
    let mut entries = 0u64;
    let mut p = p;
    while byte(d, p)? != TABLE_END {
        let value = lua_value_end(d, p)?;
        let end = lua_value_end(d, value)?;
        let label = lua_key_label(d, p);
        let mut path = prefix.to_vec();
        path.push(if count > FOLD_KEYS && label != "[n]" && label != "<id>" { "<key>".to_string() } else { label });
        {
            let stat = out.entry(path.clone()).or_insert([0, 0, 0]);
            stat[0] += 1;
            stat[1] += (end - p) as u64;
        }
        if depth > 1 {
            let n = lua_key_sizes(d, value, depth - 1, &path, out)?;
            out.get_mut(&path).expect("just made")[2] += n;
        }
        entries += 1;
        p = end;
    }
    Ok(entries)
}

fn lua_kind(t: u8) -> &'static str {
    if t & (CUSTOM_COMPACT_FLAG | CUSTOM_FULL_FLAG) != 0 {
        "object"
    } else if t & SHORT_STRING_FLAG != 0 || t == LONG_STRING {
        "string"
    } else if t == NUMBER {
        "number"
    } else if t == BOOLEAN {
        "boolean"
    } else if t == TABLE_START {
        "table"
    } else {
        "vector"
    }
}

fn lua_string(d: &[u8], p: usize) -> &[u8] {
    let t = d.get(p).copied().unwrap_or(0);
    if t == LONG_STRING {
        let n = u32_at(d, p + 1).unwrap_or(0) as usize;
        return slice(d, p + 5, n);
    }
    slice(d, p + 1, usize::from(t & 0x1F))
}

/// `[count, bytes]` by key, in the order first met.
pub type Counter = IndexMap<String, (u64, f64)>;

fn add(c: &mut Counter, key: &str, size: f64, count: u64) {
    let e = c.entry(key.to_string()).or_insert((0, 0.0));
    e.0 += count;
    e.1 += size;
}

/// What the saved data of a script is made of, over all its objects.
#[derive(Debug, Default)]
pub struct LuaScan {
    pub kinds: Counter,
    pub key_names: IndexMap<Vec<u8>, u64>,
    pub strings: IndexMap<Vec<u8>, u64>,
    pub xyz_tables: u64,
    pub depth: usize,
}

/// Walks the value at `p` into `scan`; the position after it.
pub fn lua_scan(d: &[u8], p: usize, scan: &mut LuaScan, depth: usize) -> Result<usize, Bad> {
    let t = byte(d, p)?;
    if t != TABLE_START {
        let end = lua_value_end(d, p)?;
        let kind = lua_kind(t);
        add(&mut scan.kinds, &format!("{kind} values"), (end - p) as f64, 1);
        if kind == "string" && end - p > 4 {
            *scan.strings.entry(lua_string(d, p).to_vec()).or_default() += 1;
        }
        return Ok(end);
    }
    add(&mut scan.kinds, "table markers", 2.0, 1);
    scan.depth = scan.depth.max(depth + 1);
    let mut names: BTreeSet<Vec<u8>> = BTreeSet::new();
    let mut q = p + 1;
    while byte(d, q)? != TABLE_END {
        let value = lua_value_end(d, q)?;
        let kind = lua_kind(byte(d, q)?);
        add(&mut scan.kinds, &format!("{kind} keys"), (value - q) as f64, 1);
        if kind == "string" {
            let name = lua_string(d, q).to_vec();
            *scan.key_names.entry(name.clone()).or_default() += 1;
            names.insert(name);
        }
        q = lua_scan(d, value, scan, depth + 1)?;
    }
    if names.len() == 3 && names.contains(&b"x"[..]) && names.contains(&b"y"[..]) && names.contains(&b"z"[..]) {
        scan.xyz_tables += 1;
    }
    Ok(q + 1)
}

/// Python's `f"{v:g}"`.
fn py_g(v: f64) -> String {
    if v.is_nan() {
        return "nan".into();
    }
    if v.is_infinite() {
        return if v > 0.0 { "inf" } else { "-inf" }.into();
    }
    if v == 0.0 {
        return if v.is_sign_negative() { "-0" } else { "0" }.into();
    }
    let exp = format!("{:.5e}", v);
    let (mant, e) = exp.split_once('e').expect("an exponent");
    let e: i32 = e.parse().unwrap_or(0);
    let strip = |s: &str| if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s.to_string() };
    if (-4..6).contains(&e) {
        let decimals = usize::try_from(5 - e).unwrap_or(0);
        strip(&format!("{v:.decimals$}"))
    } else {
        format!("{}e{}{:02}", strip(mant), if e < 0 { '-' } else { '+' }, e.abs())
    }
}

fn lua_scalar(d: &[u8], p: usize) -> String {
    let t = d.get(p).copied().unwrap_or(0);
    let kind = lua_kind(t);
    match kind {
        "string" => {
            let text = String::from_utf8_lossy(lua_string(d, p)).into_owned();
            let text = if text.chars().count() <= 80 { text } else { text.chars().take(77).collect::<String>() + "..." };
            luacore::lexer::py_repr(&text)
        }
        "number" => {
            let v = d.get(p + 1..p + 9).map(|b| f64::from_le_bytes(b.try_into().expect("8 bytes"))).unwrap_or(0.0);
            if v.is_finite() && v == v.trunc() && v.abs() < 1e15 { format!("{}", v as i64) } else { py_g(v) }
        }
        "boolean" => if d.get(p + 1).copied().unwrap_or(0) != 0 { "true" } else { "false" }.into(),
        "object" if t & CUSTOM_COMPACT_FLAG != 0 && d.get(p + 1) == Some(&b'o') && (t >> 3) & 15 == 8 => {
            let index = u32_at(d, p + 2).unwrap_or(0);
            let content = i32_at(d, p + 6).unwrap_or(0);
            format!("<object {content}:0x{index:x}>")
        }
        k => format!("<{k}>"),
    }
}

fn lua_dump(d: &[u8], p: usize, indent: &str, budget: &mut i64, out: &mut String) -> Result<(), Bad> {
    let mut q = p + 1;
    while byte(d, q)? != TABLE_END {
        if *budget <= 0 {
            if *budget == 0 {
                let _ = writeln!(out, "{indent}...");
                *budget = -1;
            }
            return Ok(());
        }
        let value = lua_value_end(d, q)?;
        let end = lua_value_end(d, value)?;
        let key = lua_scalar(d, q);
        let kq = byte(d, q)?;
        let key = if kq & SHORT_STRING_FLAG != 0 && kq & (CUSTOM_COMPACT_FLAG | CUSTOM_FULL_FLAG) == 0 {
            let mut c = key.chars();
            c.next();
            c.next_back();
            c.collect()
        } else {
            format!("[{key}]")
        };
        *budget -= 1;
        if byte(d, value)? == TABLE_START {
            let _ = writeln!(out, "{indent}{key} = {{  -- {} B", commas((end - value) as u64));
            lua_dump(d, value, &format!("{indent}  "), budget, out)?;
            if *budget > 0 {
                let _ = writeln!(out, "{indent}}}");
            }
        } else {
            let _ = writeln!(out, "{indent}{key} = {}", lua_scalar(d, value));
        }
        q = end;
    }
    Ok(())
}

// ---------------------------------------------------------------- statistics

/// One Lua script's share of the save.
#[derive(Clone, Debug, Default)]
pub struct ScriptStat {
    pub attached: u64,
    pub with_data: u64,
    pub header_bytes: u64,
    pub data_bytes: u64,
    pub timers: u64,
    pub timer_bytes: u64,
    pub max_data: u64,
    pub contexts: BTreeSet<String>,
    /// The instances with data: where the data is in the save, its size, the object, the cell.
    pub instances: Vec<(usize, usize, String, String)>,
}

impl ScriptStat {
    pub fn total(&self) -> u64 {
        self.header_bytes + self.data_bytes + self.timer_bytes
    }
}

/// A save, read and credited.
#[derive(Debug, Default)]
pub struct SaveStats {
    pub name: String,
    pub data: Vec<u8>,
    pub size: u64,
    pub format_version: u32,
    pub player: String,
    pub cell: String,
    pub content_files: Vec<String>,
    pub script_paths: Vec<String>,
    pub load_order: LoadOrder,
    /// mod -> part -> [count, bytes].
    pub owned: IndexMap<String, Counter>,
    pub mod_object_types: IndexMap<String, Counter>,
    pub mod_object_ids: IndexMap<String, Counter>,
    pub mod_object_cells: IndexMap<String, Counter>,
    pub mod_attachers: IndexMap<String, Counter>,
    pub mod_carriers: Counter,
    pub records: Counter,
    pub scripts: IndexMap<i64, ScriptStat>,
    pub script_keys: IndexMap<i64, KeyStats>,
    pub events: Counter,
    pub object_types: Counter,
    pub attachers: Counter,
    pub cells: Counter,
    pub inventory_items: u64,
}

impl SaveStats {
    fn own(&mut self, m: &str, part: &str, size: f64, count: u64) {
        add(self.owned.entry(m.to_string()).or_default(), part, size, count);
    }

    pub fn mod_total(&self, m: &str) -> f64 {
        self.owned.get(m).map(|c| c.values().map(|v| v.1).sum()).unwrap_or(0.0)
    }

    pub fn script_path(&self, id: i64) -> String {
        usize::try_from(id).ok().and_then(|i| self.script_paths.get(i)).cloned().unwrap_or_else(|| format!("<script #{id}>"))
    }

    pub fn script_mod(&self, id: i64) -> String {
        self.load_order.script_mod(&self.script_path(id))
    }

    pub fn script_decl(&self, id: i64) -> Option<&ScriptDecl> {
        self.load_order.decls.get(&self.script_path(id))
    }
}

type Sub = ([u8; 4], usize, usize);

struct Parser {
    s: SaveStats,
    key_depth: usize,
    carrier_mods: BTreeSet<String>,
}

fn subrecords(d: &[u8], start: usize, end: usize) -> Vec<Sub> {
    let mut out = Vec::new();
    let mut p = start;
    while p + 8 <= end.min(d.len()) {
        let size = u32_at(d, p + 4).unwrap_or(0) as usize;
        out.push((d[p..p + 4].try_into().expect("4 bytes"), p + 8, size));
        p += 8 + size;
    }
    out
}

fn records(d: &[u8]) -> Vec<([u8; 4], usize, Vec<Sub>)> {
    let mut out = Vec::new();
    let mut p = 0usize;
    while p + 16 <= d.len() {
        let tag: [u8; 4] = d[p..p + 4].try_into().expect("4 bytes");
        let size = u32_at(d, p + 4).unwrap_or(0) as usize;
        out.push((tag, size, subrecords(d, p + 16, p + 16 + size)));
        p += 16 + size;
    }
    out
}

fn sub_total(subs: &[Sub]) -> f64 {
    16.0 + subs.iter().map(|s| 8.0 + s.2 as f64).sum::<f64>()
}

impl Parser {
    fn text(&self, start: usize, size: usize) -> String {
        let b = slice(&self.s.data, start, size);
        let end = b.iter().rposition(|&c| c != 0).map_or(0, |i| i + 1);
        String::from_utf8_lossy(&b[..end]).into_owned()
    }

    fn refid(&self, start: usize, size: usize) -> String {
        decode_refid(slice(&self.s.data, start, size))
    }

    fn flush_carrier(&mut self) {
        let mods: Vec<String> = std::mem::take(&mut self.carrier_mods).into_iter().collect();
        for m in mods {
            add(&mut self.s.mod_carriers, &m, 0.0, 1);
        }
    }

    fn read_metadata(&mut self) {
        let d = std::mem::take(&mut self.s.data);
        for (tag, _, subs) in records(&d) {
            if !matches!(&tag, b"TES3" | b"SAVE" | b"LUAM") {
                continue;
            }
            for &(sub, start, size) in &subs {
                let text = || {
                    let b = slice(&d, start, size);
                    let end = b.iter().rposition(|&c| c != 0).map_or(0, |i| i + 1);
                    String::from_utf8_lossy(&b[..end]).into_owned()
                };
                match &sub {
                    b"FORM" if &tag == b"TES3" => self.s.format_version = u32_at(&d, start).unwrap_or(0),
                    b"MAST" => self.s.content_files.push(text()),
                    b"PLNA" => self.s.player = text(),
                    b"PLCE" => self.s.cell = text(),
                    b"LUAP" => self.s.script_paths.push(normalize(&text())),
                    _ => {}
                }
            }
        }
        self.s.data = d;
    }

    fn parse(&mut self) {
        let recs = records(&self.s.data);
        for (tag, size, subs) in recs {
            let name = latin1(&tag);
            add(&mut self.s.records, &name, 16.0 + size as f64, 1);
            match &tag {
                b"TES3" => self.tes3(&subs),
                b"LUAM" => self.luam(&subs),
                b"CSTA" => self.csta(&subs),
                b"PLAY" => self.play(&subs),
                b"JOUR" | b"QUES" => self.by_topic(&name, &subs),
                b"DIAS" => self.dias(&subs),
                b"GLOB" | b"GSCR" => self.by_name(&name, &subs),
                _ => self.other(&name, &subs),
            }
        }
    }

    // -- Lua --

    fn lua_block(&mut self, subs: &[Sub], mut i: usize, context: &str, part: &str, holder: &str, cell: &str) -> (usize, i64, f64) {
        let (_, start, size) = subs[i];
        let script_id: i64 = if size == 4 {
            i64::from(i32_at(&self.s.data, start).unwrap_or(0))
        } else {
            let path = normalize(&self.text(start, size));
            let at = match self.s.script_paths.iter().position(|p| *p == path) {
                Some(at) => at,
                None => {
                    self.s.script_paths.push(path);
                    self.s.script_paths.len() - 1
                }
            };
            at as i64
        };
        let m = self.s.script_mod(script_id);
        if context != "global" {
            self.carrier_mods.insert(m.clone());
        }
        let mut total = 8 + size;
        {
            let stat = self.s.scripts.entry(script_id).or_default();
            stat.attached += 1;
            stat.header_bytes += (8 + size) as u64;
            stat.contexts.insert(context.to_string());
        }
        i += 1;
        if i < subs.len() && &subs[i].0 == b"LUAD" {
            let (_, dstart, dsize) = subs[i];
            {
                let stat = self.s.scripts.get_mut(&script_id).expect("just made");
                stat.with_data += 1;
                stat.data_bytes += (8 + dsize) as u64;
                stat.max_data = stat.max_data.max(dsize as u64);
                stat.instances.push((dstart, dsize, holder.to_string(), cell.to_string()));
            }
            total += 8 + dsize;
            if self.key_depth > 0 && dsize > 1 {
                let mut keys = self.s.script_keys.shift_remove(&script_id).unwrap_or_default();
                let _ = lua_key_sizes(&self.s.data, dstart + 1, self.key_depth, &[], &mut keys);
                self.s.script_keys.insert(script_id, keys);
            }
            i += 1;
        }
        while i < subs.len() && &subs[i].0 == b"LUAT" {
            self.s.scripts.get_mut(&script_id).expect("just made").timers += 1;
            for expected in [b"LUAT", b"LUAC", b"LUAD"] {
                if i < subs.len() && &subs[i].0 == expected {
                    self.s.scripts.get_mut(&script_id).expect("just made").timer_bytes += (8 + subs[i].2) as u64;
                    total += 8 + subs[i].2;
                    i += 1;
                }
            }
        }
        self.s.own(&m, part, total as f64, 0);
        (i, script_id, total as f64)
    }

    fn luam(&mut self, subs: &[Sub]) {
        self.s.own(ENGINE, "record headers", 16.0, 0);
        let mut i = 0;
        while i < subs.len() {
            let (tag, start, size) = subs[i];
            match &tag {
                b"LUAS" => {
                    let (next, id, _) = self.lua_block(subs, i, "global", LUA_GLOBAL, "", "");
                    let m = self.s.script_mod(id);
                    add(self.s.owned.entry(m).or_default(), LUA_GLOBAL, 0.0, 1);
                    i = next;
                    continue;
                }
                b"LUAP" => {
                    let m = self.s.load_order.script_mod(&normalize(&self.text(start, size)));
                    self.s.own(&m, LUA_LIST, (8 + size) as f64, 0);
                }
                b"LUAE" => {
                    let name = self.text(start, size);
                    let mut total = 8 + size;
                    i += 1;
                    while i < subs.len() && matches!(&subs[i].0, b"FRMR" | b"LUAD") {
                        total += 8 + subs[i].2;
                        i += 1;
                    }
                    add(&mut self.s.events, &name, total as f64, 1);
                    self.s.own(ENGINE, "Lua: pending events", total as f64, 1);
                    continue;
                }
                _ => self.s.own(ENGINE, "Lua: simulation time", (8 + size) as f64, 0),
            }
            i += 1;
        }
    }

    fn play(&mut self, subs: &[Sub]) {
        self.s.own(ENGINE, "record headers", 16.0, 0);
        let mut i = 0;
        let mut holder = "player".to_string();
        while i < subs.len() {
            if &subs[i].0 == b"NAME" && i > 0 && &subs[i - 1].0 == b"FRMR" {
                holder = self.refid(subs[i].1, subs[i].2);
            }
            if &subs[i].0 == b"LUAS" {
                i = self.lua_block(subs, i, "player", LUA_PLAYER, &holder.clone(), "").0;
            } else {
                self.s.own(ENGINE, "player", (8 + subs[i].2) as f64, 0);
                i += 1;
            }
        }
        self.flush_carrier();
    }

    fn credit_record(&mut self, m: Option<String>, part: &str, name: &str, subs: &[Sub]) {
        let size = sub_total(subs);
        match m {
            Some(m) => self.s.own(&m, part, size, 1),
            None => {
                let label = if part != name { format!("{part} (defined in game)") } else { format!("{name} records") };
                self.s.own(ENGINE, &label, size, 1);
            }
        }
    }

    fn by_topic(&mut self, name: &str, subs: &[Sub]) {
        let topic = subs.iter().find(|s| &s.0 == b"YETO").map(|s| self.refid(s.1, s.2)).unwrap_or_default();
        let part = if name == "JOUR" { JOURNAL } else { QUESTS };
        let m = self.s.load_order.mod_of_id(&self.s.load_order.dialogues, &topic);
        self.credit_record(m, part, name, subs);
    }

    fn by_name(&mut self, name: &str, subs: &[Sub]) {
        let refid = match subs.first() {
            Some(s) if &s.0 == b"NAME" => self.refid(s.1, s.2),
            _ => String::new(),
        };
        let lo = &self.s.load_order;
        if name == "GLOB" {
            let m = lo.mod_of_id(&lo.globals, &refid);
            self.credit_record(m, GLOBALS, name, subs);
        } else {
            let m = lo.mod_of_id(&lo.mwscripts, &refid);
            self.credit_record(m, MWSCRIPTS, name, subs);
        }
    }

    fn dias(&mut self, subs: &[Sub]) {
        self.s.own(ENGINE, "dialogue state", 16.0, 0);
        for &(tag, start, size) in subs {
            let m = if &tag == b"TOPI" { self.s.load_order.mod_of_id(&self.s.load_order.dialogues, &self.refid(start, size)) } else { None };
            match m {
                Some(m) => self.s.own(&m, TOPICS, (8 + size) as f64, 1),
                None => self.s.own(ENGINE, "dialogue state", (8 + size) as f64, 0),
            }
        }
    }

    fn tes3(&mut self, subs: &[Sub]) {
        self.s.own(ENGINE, "save header and screenshot", 16.0, 0);
        let (mut i, mut index) = (0usize, 0i64);
        while i < subs.len() {
            let (tag, _, size) = subs[i];
            if &tag == b"MAST" {
                let next = if i + 1 < subs.len() && &subs[i + 1].0 == b"DATA" { 8 + subs[i + 1].2 } else { 0 };
                let m = self.s.load_order.content_mod(Some(index)).unwrap_or(ENGINE).to_string();
                self.s.own(&m, LOAD_ORDER, (8 + size + next) as f64, 1);
                index += 1;
                i += 2;
                continue;
            }
            self.s.own(ENGINE, "save header and screenshot", (8 + size) as f64, 0);
            i += 1;
        }
    }

    fn other(&mut self, name: &str, subs: &[Sub]) {
        let size = sub_total(subs);
        if engine_record(name).is_some() || subs.first().is_none_or(|s| &s.0 != b"NAME") {
            let label = engine_record(name).map(str::to_string).unwrap_or_else(|| format!("{name} records"));
            self.s.own(ENGINE, &label, size, 1);
            return;
        }
        let refid = self.refid(subs[0].1, subs[0].2);
        match self.s.load_order.mod_of_id(&self.s.load_order.objects, &refid) {
            Some(m) => self.s.own(&m, CHANGED_RECORDS, size, 1),
            None => self.s.own(ENGINE, "records created in game (enchanting, potions, Lua)", size, 1),
        }
    }

    // -- cells --

    fn csta(&mut self, subs: &[Sub]) {
        self.s.own(ENGINE, "record headers", 16.0, 0);
        let cell = match subs.first() {
            Some(s) if &s.0 == b"NAME" => self.refid(s.1, s.2),
            _ => "?".to_string(),
        };
        let mut i = 0;
        while i < subs.len() {
            let (tag, _, size) = subs[i];
            if &tag == b"OBJE" {
                i = self.object(subs, i, &cell);
                continue;
            }
            if &tag == b"MVRF" {
                let n = 8 + size + if i + 1 < subs.len() { 8 + subs[i + 1].2 } else { 0 };
                self.s.own(ENGINE, "cells: moved references", n as f64, 1);
                add(&mut self.s.cells, &cell, n as f64, 0);
                i += 2;
                continue;
            }
            let part = if &tag == b"FTEX" { "cells: fog of war" } else { "cells: header" };
            self.s.own(ENGINE, part, (8 + size) as f64, 0);
            add(&mut self.s.cells, &cell, (8 + size) as f64, 0);
            i += 1;
        }
    }

    fn object(&mut self, subs: &[Sub], mut i: usize, cell: &str) -> usize {
        let type_tag = latin1(slice(&self.s.data, subs[i].1, 4));
        let mut own_bytes = (8 + subs[i].2) as f64;
        let mut lua_bytes = 0.0;
        i += 1;
        let mut refid = "?".to_string();
        let mut content: Option<i64> = None;
        let mut holder = refid.clone();
        let mut own_scripts: Vec<i64> = Vec::new();
        let (mut in_inventory, mut seen_ref) = (false, false);
        while i < subs.len() && !matches!(&subs[i].0, b"OBJE" | b"MVRF") {
            let (tag, start, size) = subs[i];
            if &tag == b"FRMR" && i + 1 < subs.len() && &subs[i + 1].0 == b"NAME" {
                self.flush_carrier();
                holder = self.refid(subs[i + 1].1, subs[i + 1].2);
                if seen_ref {
                    in_inventory = true;
                    self.s.inventory_items += 1;
                } else {
                    seen_ref = true;
                    content = if size == 8 { i32_at(&self.s.data, start + 4).map(i64::from) } else { None };
                    refid = holder.clone();
                }
            }
            if &tag == b"LUAS" {
                let context = if in_inventory { "inventory item" } else { "object" };
                let (next, id, n) = self.lua_block(subs, i, context, LUA_OBJECTS, &holder.clone(), cell);
                if !in_inventory {
                    own_scripts.push(id);
                }
                lua_bytes += n;
                i = next;
                continue;
            }
            own_bytes += (8 + size) as f64;
            i += 1;
        }
        self.flush_carrier();

        let total = own_bytes + lua_bytes;
        add(&mut self.s.object_types, &type_tag, total, 1);
        add(&mut self.s.cells, cell, total, 1);

        let attachers = self.attachers(&type_tag, &own_scripts);
        let (owners, part): (Vec<String>, &str) = if !attachers.is_empty() {
            add(&mut self.s.attachers, &attachers.join(" + "), total, 1);
            (attachers.clone(), SAVED_BY_SCRIPTS)
        } else {
            if !own_scripts.is_empty() {
                add(&mut self.s.attachers, &format!("{ENGINE}, by type {type_tag}"), total, 1);
            }
            let lo = &self.s.load_order;
            match content {
                Some(c) if c >= 0 => {
                    (vec![lo.content_mod(Some(c)).unwrap_or(ENGINE).to_string()], if own_scripts.is_empty() { PLACED_OBJECTS } else { SAVED_BY_ENGINE })
                }
                _ => match lo.mod_of_id(&lo.objects, &refid) {
                    Some(m) => (vec![m], SPAWNED_OBJECTS),
                    None => (vec![ENGINE.to_string()], "objects created in game from records created in game"),
                },
            }
        };
        let share = own_bytes / owners.len() as f64;
        for m in &owners {
            self.s.own(m, part, share, 1);
            add(self.s.mod_object_types.entry(m.clone()).or_default(), &type_tag, share, 1);
            add(self.s.mod_object_ids.entry(m.clone()).or_default(), &refid, share, 1);
            add(self.s.mod_object_cells.entry(m.clone()).or_default(), cell, share, 1);
            if !attachers.is_empty() {
                add(self.s.mod_attachers.entry(m.clone()).or_default(), &attachers.join(" + "), share, 1);
            }
        }
        i
    }

    /// The mods whose scripts made this object be saved, or none when the engine attaches
    /// scripts to its type itself (it would be saved anyway).
    fn attachers(&self, type_tag: &str, scripts: &[i64]) -> Vec<String> {
        let mut mods = BTreeSet::new();
        for &id in scripts {
            let m = self.s.script_mod(id);
            if m == ENGINE {
                if self.s.script_decl(id).is_some_and(|d| d.types.contains(type_tag)) {
                    return Vec::new();
                }
                continue;
            }
            mods.insert(m);
        }
        mods.into_iter().collect()
    }
}

/// A save read and credited (`analyse`): `name` for the reports, `data_dirs` to find the load
/// order's files (`None`: mods guessed from names), `key_depth` how deep the Lua data's keys are
/// broken down (0: not).
pub fn analyse(name: &str, data: Vec<u8>, data_dirs: Option<&DataDirs>, key_depth: usize) -> SaveStats {
    let mut p = Parser {
        s: SaveStats { name: name.to_string(), size: data.len() as u64, data, ..Default::default() },
        key_depth,
        carrier_mods: BTreeSet::new(),
    };
    p.read_metadata();
    p.s.load_order = LoadOrder::new(&p.s.content_files, data_dirs);
    p.parse();
    p.s
}

/// The content files a save names in its `SAVE` record (`DEPE`; `read_savegame_content_files`),
/// or why they cannot be read.
pub fn content_files(data: &[u8]) -> Result<Vec<String>, String> {
    if data.get(..4) != Some(b"TES3") {
        return Err("not an OpenMW save (no TES3 header)".into());
    }
    for (tag, _, subs) in records(data) {
        if &tag == b"SAVE" {
            return Ok(subs
                .iter()
                .filter(|s| &s.0 == b"DEPE")
                .map(|s| {
                    let b = slice(data, s.1, s.2);
                    let end = b.iter().rposition(|&c| c != 0).map_or(0, |i| i + 1);
                    String::from_utf8_lossy(&b[..end]).into_owned()
                })
                .collect());
        }
    }
    Err("no SAVE record found -- not a savegame?".into())
}

/// Those of a save's content files the load order lacks (`check_savegame_against_order`).
pub fn missing_from(files: &[String], order: &[String]) -> Vec<String> {
    let have: std::collections::HashSet<String> = order.iter().map(|n| n.to_lowercase()).collect();
    files.iter().filter(|f| !have.contains(&f.to_lowercase())).cloned().collect()
}

// ---------------------------------------------------------------- reports

fn commas(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn fixed1(v: f64) -> String {
    let s = format!("{:.1}", v.abs());
    let (int, frac) = s.split_once('.').unwrap_or((&s, "0"));
    let int: u64 = int.parse().unwrap_or(0);
    format!("{}{}.{frac}", if v < 0.0 && s != "0.0" { "-" } else { "" }, commas(int))
}

/// `f"{n / 1024:,.1f}"`.
pub fn kb(n: f64) -> String {
    fixed1(n / 1024.0)
}

fn signed_kb(n: f64) -> String {
    format!("{}{}", if n >= 0.0 { "+" } else { "" }, kb(n))
}

fn pct(n: f64, total: u64) -> String {
    if total == 0 { String::new() } else { format!("{:.1}%", 100.0 * n / total as f64) }
}

/// `[+-]?[\d,.]+%?`, whole.
fn numeric(s: &str) -> bool {
    let s = s.strip_prefix(['+', '-']).unwrap_or(s);
    let s = s.strip_suffix('%').unwrap_or(s);
    !s.is_empty() && s.chars().all(|c| c.is_ascii_digit() || c == ',' || c == '.')
}

struct Out(String);

impl Out {
    fn line(&mut self, s: &str) {
        self.0.push_str(s);
        self.0.push('\n');
    }

    fn table(&mut self, header: &[&str], rows: &[Vec<String>], indent: &str) {
        if rows.is_empty() {
            self.line(&format!("{indent}(none)"));
            return;
        }
        let widths: Vec<usize> = header
            .iter()
            .enumerate()
            .map(|(i, h)| rows.iter().map(|r| r.get(i).map_or(0, |c| c.chars().count())).max().unwrap_or(0).max(h.chars().count()))
            .collect();
        let fmt = |r: &[String]| -> String {
            let cells: Vec<String> = r
                .iter()
                .zip(&widths)
                .enumerate()
                .map(|(i, (c, &w))| if i == 0 || !numeric(c) { format!("{c:<w$}") } else { format!("{c:>w$}") })
                .collect();
            format!("{indent}{}", cells.join("  ")).trim_end().to_string()
        };
        let head: Vec<String> = header.iter().map(|h| (*h).to_string()).collect();
        self.line(&fmt(&head));
        self.line(&format!("{indent}{}", widths.iter().map(|w| "-".repeat(*w)).collect::<Vec<_>>().join("  ")));
        for r in rows {
            self.line(&fmt(r));
        }
    }

    fn section(&mut self, title: &str, note: &str) {
        self.line(&format!("\n## {title}\n"));
        if !note.is_empty() {
            self.line(&format!("{note}\n"));
        }
    }
}

/// A counter's entries, largest first (ties in the order first met).
fn by_size(c: &Counter) -> Vec<(&String, &(u64, f64))> {
    let mut v: Vec<(&String, &(u64, f64))> = c.iter().collect();
    v.sort_by(|a, b| b.1.1.partial_cmp(&a.1.1).unwrap_or(std::cmp::Ordering::Equal));
    v
}

fn scripts_by<F: Fn(&ScriptStat) -> u64>(s: &SaveStats, f: F) -> Vec<(i64, &ScriptStat)> {
    let mut v: Vec<(i64, &ScriptStat)> = s.scripts.iter().map(|(k, v)| (*k, v)).collect();
    v.sort_by_key(|(_, st)| std::cmp::Reverse(f(st)));
    v
}

fn print_key_tree(o: &mut Out, keys: &KeyStats, prefix: &[String], rows: usize, indent: &str) {
    let mut children: Vec<(&Vec<String>, &[u64; 3])> =
        keys.iter().filter(|(k, _)| k.len() == prefix.len() + 1 && k[..prefix.len()] == *prefix).collect();
    children.sort_by_key(|(_, v)| std::cmp::Reverse(v[1]));
    for (k, v) in children.iter().take(rows) {
        let (count, size, entries) = (v[0], v[1], v[2]);
        let mut label = format!("{}{}", "  ".repeat(prefix.len()), k[k.len() - 1]);
        if label.chars().count() > 40 {
            label = label.chars().take(39).collect::<String>() + "~";
        }
        let extra = if entries > 0 { format!("  {} entries", commas(entries)) } else { String::new() };
        o.line(&format!("{indent}{label:<40} {:>10} KB  x{}{extra}", kb(size as f64), commas(count)));
        print_key_tree(o, keys, k, rows, indent);
    }
    if children.len() > rows {
        let rest: u64 = children[rows..].iter().map(|(_, v)| v[1]).sum();
        let label = format!("{}({} more keys)", "  ".repeat(prefix.len()), children.len() - rows);
        o.line(&format!("{indent}{label:<40} {:>10} KB", kb(rest as f64)));
    }
}

fn script_hints(s: &SaveStats, id: i64, st: &ScriptStat, scan: Option<&LuaScan>) -> Vec<(f64, String)> {
    let mut hints: Vec<(f64, String)> = Vec::new();
    if st.instances.len() >= 20 {
        let mut payloads: IndexMap<&[u8], u64> = IndexMap::new();
        for (start, size, _, _) in &st.instances {
            *payloads.entry(slice(&s.data, *start, *size)).or_default() += 1;
        }
        let empty = payloads.shift_remove(EMPTY_TABLE).unwrap_or(0);
        if empty >= 10 {
            hints.push(((empty * (8 + EMPTY_TABLE.len() as u64)) as f64, format!("{} objects save an empty table: onSave can return nil instead", commas(empty))));
        }
        // `max` keeps the first of the largest.
        let mut best: Option<(&[u8], u64)> = None;
        for (k, c) in &payloads {
            if best.is_none_or(|(_, bc)| *c > bc) {
                best = Some((k, *c));
            }
        }
        if let Some((common, count)) = best
            && count >= 10
            && count as f64 >= st.instances.len() as f64 * 0.2
        {
            hints.push((
                (count * (8 + common.len() as u64)) as f64,
                format!(
                    "{} of {} objects save the same {} bytes, probably default values: onSave need not save them",
                    commas(count),
                    commas(st.instances.len() as u64),
                    common.len()
                ),
            ));
        }
    }
    let engine_types = s.load_order.engine_types();
    let saved_types: Vec<String> =
        s.script_decl(id).map(|d| d.types.iter().filter(|t| !engine_types.contains(*t)).cloned().collect()).unwrap_or_default();
    if st.attached >= 1000 && !saved_types.is_empty() && s.script_mod(id) != ENGINE && st.header_bytes > st.data_bytes {
        hints.push((
            st.header_bytes as f64,
            format!(
                "attached to {} objects, {} KB of script headers for {} KB of data, and the objects of type {} are saved in full because of it: attach it on demand (addScript) rather than to every object of a type",
                commas(st.attached),
                kb(st.header_bytes as f64),
                kb(st.data_bytes as f64),
                saved_types.join(", ")
            ),
        ));
    }
    if let Some(scan) = scan {
        let data: u64 = st.instances.iter().map(|i| i.1 as u64).sum();
        let long_keys: Vec<(&Vec<u8>, u64)> = scan.key_names.iter().filter(|(k, c)| k.len() > 6 && **c >= 50).map(|(k, c)| (k, *c)).collect();
        let cost: u64 = long_keys.iter().map(|(k, c)| (k.len() as u64 + 1) * c).sum();
        if cost as f64 > data as f64 * 0.1 {
            hints.push((
                long_keys.iter().map(|(k, c)| (k.len() as u64 - 2) * c).sum::<u64>() as f64,
                format!("{} KB go to key names longer than 6 characters, repeated 50 times or more: shorter names in the saved tables, mapped back on load, would save most of it", kb(cost as f64)),
            ));
        }
        let number_keys = scan.kinds.get("number keys").map_or(0.0, |v| v.1);
        if number_keys > data as f64 * 0.1 {
            hints.push((0.0, format!("{} KB go to array indexes, each stored as a 9-byte number: a list of small values packed in a string, or a set keyed by string, is smaller", kb(number_keys))));
        }
        if scan.xyz_tables >= 20 {
            hints.push(((scan.xyz_tables * 10) as f64, format!("{} tables {{x=, y=, z=}}: a util.vector3 takes 25 bytes instead of 35", commas(scan.xyz_tables))));
        }
    }
    hints.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    hints
}

fn print_hints(o: &mut Out, hints: &[(f64, String)], indent: &str) {
    for (saving, text) in hints {
        let tail = if *saving != 0.0 { format!(" (saves {} KB)", kb(*saving)) } else { String::new() };
        o.line(&format!("{indent}- {text}{tail}"));
    }
}

fn print_script_data(o: &mut Out, s: &SaveStats, id: i64, st: &ScriptStat, rows: usize, indent: &str) {
    let mut scan = LuaScan::default();
    for (start, size, _, _) in &st.instances {
        if *size > 1 {
            let _ = lua_scan(&s.data, start + 1, &mut scan, 0);
        }
    }
    let data: u64 = st.instances.iter().map(|i| i.1 as u64).sum();
    let kinds: Vec<String> =
        by_size(&scan.kinds).into_iter().filter(|(_, v)| v.1 >= data as f64 * 0.01).map(|(k, v)| format!("{k} {}", pct(v.1, data))).collect();
    o.line(&format!("\n{indent}Made of: {}; nesting depth {}.", kinds.join(", "), scan.depth));

    let mut names: Vec<(&Vec<u8>, u64)> = scan.key_names.iter().filter(|(_, c)| **c >= 10).map(|(k, c)| (k, *c)).collect();
    names.sort_by_key(|(k, c)| std::cmp::Reverse((k.len() as u64 + 1) * c));
    names.truncate(rows);
    if !names.is_empty() {
        o.line(&format!("\n{indent}Costliest key names (each occurrence stores the name):\n"));
        let rows: Vec<Vec<String>> = names
            .iter()
            .map(|(k, c)| vec![String::from_utf8_lossy(k).into_owned(), commas(*c), kb(((k.len() as u64 + 1) * c) as f64)])
            .collect();
        o.table(&["key", "count", "KB"], &rows, indent);
    }
    let mut strings: Vec<(&Vec<u8>, u64)> = scan.strings.iter().filter(|(_, c)| **c >= 10).map(|(k, c)| (k, *c)).collect();
    strings.sort_by_key(|(v, c)| std::cmp::Reverse((v.len() as u64 + 1) * c));
    strings.truncate(rows);
    if !strings.is_empty() {
        o.line(&format!("\n{indent}Most repeated strings:\n"));
        let rows: Vec<Vec<String>> = strings
            .iter()
            .map(|(v, c)| {
                let text: String = String::from_utf8_lossy(v).chars().take(60).collect();
                vec![luacore::lexer::py_repr(&text), commas(*c), kb(((v.len() as u64 + 1) * c) as f64)]
            })
            .collect();
        o.table(&["value", "count", "KB"], &rows, indent);
    }
    if st.instances.len() > 1 {
        let mut largest: Vec<&(usize, usize, String, String)> = st.instances.iter().collect();
        largest.sort_by_key(|i| std::cmp::Reverse(i.1));
        let parts: Vec<String> = largest
            .iter()
            .take(3)
            .map(|(_, size, holder, cell)| format!("{} B {holder}{}", commas(*size as u64), if cell.is_empty() { String::new() } else { format!(" in {cell}") }))
            .collect();
        o.line(&format!("\n{indent}Largest: {}", parts.join(", ")));
    }
    let hints = script_hints(s, id, st, Some(&scan));
    if !hints.is_empty() {
        o.line(&format!("\n{indent}Hints:"));
        print_hints(o, &hints, &format!("{indent}  "));
    }
}

fn report_hints(o: &mut Out, s: &SaveStats, top: usize) {
    let mut hints: Vec<(f64, String)> = Vec::new();
    for (id, st) in &s.scripts {
        for (saving, text) in script_hints(s, *id, st, None) {
            if saving >= MIN_HINT {
                hints.push((saving, format!("{} ({}): {text}", s.script_path(*id), s.script_mod(*id))));
            }
        }
    }
    if !hints.is_empty() {
        o.section("Hints", "Ways to shrink what Lua scripts save, by the space they would save (estimates). Details per mod give a mod's data, with more hints.");
        hints.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        hints.truncate(top);
        print_hints(o, &hints, "");
    }
}

fn report_dump(o: &mut Out, s: &SaveStats, pattern: &str, lines: i64) {
    let mut found = false;
    for (id, st) in scripts_by(s, |st| st.data_bytes) {
        if !s.script_path(id).contains(&pattern.to_lowercase()) || st.instances.is_empty() {
            continue;
        }
        found = true;
        let mut best = &st.instances[0];
        for inst in &st.instances {
            if inst.1 > best.1 {
                best = inst;
            }
        }
        let (start, size, holder, cell) = best;
        let wher = if holder.is_empty() { "global".to_string() } else { format!("{holder}{}", if cell.is_empty() { String::new() } else { format!(" in {cell}") }) };
        o.section(
            &format!("Content of {}", s.script_path(id)),
            &format!("Largest of {} instances with data: {} B, {wher}. First {lines} lines.", commas(st.instances.len() as u64), commas(*size as u64)),
        );
        if s.data.get(start + 1) == Some(&TABLE_START) {
            let mut text = String::new();
            let mut budget = lines;
            let _ = lua_dump(&s.data, start + 1, "", &mut budget, &mut text);
            o.0.push_str(&text);
        } else {
            o.line(&lua_scalar(&s.data, start + 1));
        }
    }
    if !found {
        o.line(&format!("\nNo script with data has a path containing {}.", luacore::lexer::py_repr(pattern)));
    }
}

fn mods_by_size(s: &SaveStats) -> Vec<String> {
    let mut mods: Vec<String> = s.owned.keys().cloned().collect();
    mods.sort_by(|a, b| s.mod_total(b).partial_cmp(&s.mod_total(a)).unwrap_or(std::cmp::Ordering::Equal));
    mods
}

fn report_mods(o: &mut Out, s: &SaveStats, top: usize) -> Vec<String> {
    let total = s.size;
    let mods = mods_by_size(s);
    o.section(
        "Space per mod",
        "Every byte of the save is credited to one mod, or to OpenMW itself. Objects saved because of scripts of several mods are split between them.",
    );
    let mut rows = Vec::new();
    for m in mods.iter().take(top) {
        let parts = by_size(&s.owned[m]);
        let main: Vec<String> = parts.iter().take(3).filter(|(_, v)| v.1 >= 1024.0).map(|(p, v)| format!("{p} {}", kb(v.1))).collect();
        rows.push(vec![m.clone(), kb(s.mod_total(m)), pct(s.mod_total(m), total), main.join(", ")]);
    }
    if mods.len() > top {
        let size: f64 = mods[top..].iter().map(|m| s.mod_total(m)).sum();
        rows.push(vec![format!("({} other mods)", mods.len() - top), kb(size), pct(size, total), String::new()]);
    }
    let all: f64 = mods.iter().map(|m| s.mod_total(m)).sum();
    rows.push(vec!["total".into(), kb(all), pct(all, total), String::new()]);
    o.table(&["mod", "KB", "share", "main parts (KB)"], &rows, "");
    mods
}

fn script_tags(decl: Option<&ScriptDecl>) -> String {
    let Some(d) = decl else { return String::new() };
    let mut tags: BTreeSet<String> = d.flags.clone();
    for (k, v) in TYPE_TAGS {
        if d.types.contains(v) {
            tags.insert(k.to_string());
        }
    }
    let t = tags.into_iter().collect::<Vec<_>>().join(",");
    if t.chars().count() <= 40 { t } else { t.chars().take(39).collect::<String>() + "~" }
}

fn report_mod(o: &mut Out, s: &SaveStats, m: &str, key_rows: usize) {
    let total = s.mod_total(m);
    o.line(&format!("\n### {m} - {} KB ({} of the save)\n", kb(total), pct(total, s.size)));
    let lo = &s.load_order;
    let files: Vec<&String> = lo.content_files.iter().enumerate().filter(|(i, _)| lo.mods.get(*i).map(String::as_str) == Some(m)).map(|(_, f)| f).collect();
    if !files.is_empty() {
        o.line(&format!("Content files: {}\n", files.iter().map(|f| f.as_str()).collect::<Vec<_>>().join(", ")));
    }
    let rows: Vec<Vec<String>> = by_size(&s.owned[m])
        .into_iter()
        .map(|(p, (c, b))| vec![p.clone(), if *c > 0 { commas(*c) } else { String::new() }, kb(*b)])
        .collect();
    o.table(&["part", "count", "KB"], &rows, "  ");

    let scripts: Vec<(i64, &ScriptStat)> = scripts_by(s, ScriptStat::total).into_iter().filter(|(id, _)| s.script_mod(*id) == m).collect();
    if !scripts.is_empty() {
        let carriers = s.mod_carriers.get(m).map_or(0, |v| v.0);
        o.line(&format!("\n  Lua scripts{}:\n", if carriers > 0 { format!(", on {} objects and items", commas(carriers)) } else { String::new() }));
        let rows: Vec<Vec<String>> = scripts
            .iter()
            .map(|(id, st)| {
                vec![
                    s.script_path(*id),
                    script_tags(s.script_decl(*id)),
                    st.contexts.iter().cloned().collect::<Vec<_>>().join(","),
                    commas(st.attached),
                    commas(st.with_data),
                    kb(st.total() as f64),
                    kb(st.data_bytes as f64),
                    commas(st.max_data),
                    st.timers.to_string(),
                ]
            })
            .collect();
        o.table(&["script", "tags", "context", "objects", "with data", "KB", "data KB", "max B", "timers"], &rows, "  ");
        for (id, st) in &scripts {
            let keys = s.script_keys.get(id);
            if let Some(keys) = keys.filter(|k| !k.is_empty() && st.data_bytes >= 1024 && key_rows > 0) {
                o.line(&format!("\n  Data of {} ({} KB):\n", s.script_path(*id), kb(st.data_bytes as f64)));
                print_key_tree(o, keys, &[], key_rows, "    ");
                print_script_data(o, s, *id, st, key_rows, "    ");
            } else {
                let hints: Vec<(f64, String)> = script_hints(s, *id, st, None).into_iter().filter(|h| h.0 >= 1024.0).collect();
                if !hints.is_empty() {
                    o.line(&format!("\n  Hints for {}:", s.script_path(*id)));
                    print_hints(o, &hints, "    ");
                }
            }
        }
    }
    if let Some(att) = s.mod_attachers.get(m) {
        o.line("\n  Objects saved because of its scripts, by the mods attaching scripts to them:\n");
        let rows: Vec<Vec<String>> = by_size(att).into_iter().map(|(k, (c, b))| vec![k.clone(), commas(*c), kb(*b)]).collect();
        o.table(&["attached by", "objects", "its share KB"], &rows, "  ");
    }
    if s.mod_object_types.contains_key(m) {
        o.line("\n  Its saved objects by type, id and cell (excluding their Lua data):\n");
        for (title, counter) in [("type", &s.mod_object_types), ("id", &s.mod_object_ids), ("cell", &s.mod_object_cells)] {
            let empty = Counter::new();
            let c = counter.get(m).unwrap_or(&empty);
            let rows: Vec<Vec<String>> = by_size(c).into_iter().take(5).map(|(k, (n, b))| vec![k.clone(), commas(*n), kb(*b)]).collect();
            o.table(&[title, "objects", "KB"], &rows, "  ");
            o.line("");
        }
    }
}

fn report_global(o: &mut Out, s: &SaveStats, top: usize) {
    let total = s.size;
    o.section("Records", "");
    let rows: Vec<Vec<String>> = by_size(&s.records).into_iter().take(top).map(|(k, (c, b))| vec![k.clone(), commas(*c), kb(*b), pct(*b, total)]).collect();
    o.table(&["record", "count", "KB", "share"], &rows, "");

    o.section("Lua scripts", "");
    let rows: Vec<Vec<String>> = scripts_by(s, ScriptStat::total)
        .into_iter()
        .take(top)
        .map(|(id, st)| {
            vec![
                s.script_path(id),
                s.script_mod(id),
                st.contexts.iter().cloned().collect::<Vec<_>>().join(","),
                commas(st.attached),
                commas(st.with_data),
                kb(st.total() as f64),
                kb(st.data_bytes as f64),
                commas(st.max_data),
                st.timers.to_string(),
            ]
        })
        .collect();
    o.table(&["script", "mod", "context", "objects", "with data", "KB", "data KB", "max B", "timers"], &rows, "");
    if !s.events.is_empty() {
        o.section("Pending Lua events", "");
        let rows: Vec<Vec<String>> = by_size(&s.events).into_iter().take(top).map(|(k, (c, b))| vec![k.clone(), c.to_string(), kb(*b)]).collect();
        o.table(&["event", "count", "KB"], &rows, "");
    }
    let objects: u64 = s.object_types.values().map(|v| v.0).sum();
    o.section(
        "Saved objects by type",
        &format!("{} objects with {} inventory items, inventories and Lua data included.", commas(objects), commas(s.inventory_items)),
    );
    let rows: Vec<Vec<String>> = by_size(&s.object_types).into_iter().take(top).map(|(k, (c, b))| vec![k.clone(), commas(*c), kb(*b)]).collect();
    o.table(&["type", "objects", "KB"], &rows, "");
    o.section(
        "Saved objects carrying Lua scripts",
        "Attaching a local script marks an object as changed, so it is saved in full from then on. Objects of a type the engine attaches scripts to are saved anyway.",
    );
    let rows: Vec<Vec<String>> = by_size(&s.attachers).into_iter().take(top).map(|(k, (c, b))| vec![k.clone(), commas(*c), kb(*b)]).collect();
    o.table(&["attached by", "objects", "KB"], &rows, "");
    o.section("Largest cells", "");
    let rows: Vec<Vec<String>> = by_size(&s.cells).into_iter().take(top).map(|(k, (c, b))| vec![k.clone(), commas(*c), kb(*b)]).collect();
    o.table(&["cell", "objects", "KB"], &rows, "");
}

/// A pattern selects the mod of that exact name if there is one, else the mods containing it.
pub fn matches(m: &str, patterns: &[String], mods: &[String]) -> bool {
    let names: std::collections::HashSet<String> = mods.iter().map(|m| m.to_lowercase()).collect();
    let low = m.to_lowercase();
    patterns.iter().any(|p| {
        let p = p.to_lowercase();
        low == p || (!names.contains(&p) && low.contains(&p))
    })
}

/// What a report shows.
#[derive(Clone, Debug)]
pub struct ReportOptions {
    /// Rows per table.
    pub top: usize,
    /// Detail the N largest mods.
    pub details: usize,
    /// Detail these mods instead.
    pub patterns: Vec<String>,
    /// Keys listed per table (0: no key breakdown).
    pub key_rows: usize,
    /// The tables of the whole save too.
    pub full: bool,
    /// The saved data of the largest instance of the scripts whose path holds these.
    pub dump: Vec<String>,
    pub dump_lines: i64,
}

impl Default for ReportOptions {
    fn default() -> Self {
        ReportOptions { top: 30, details: 10, patterns: Vec::new(), key_rows: 5, full: false, dump: Vec::new(), dump_lines: 40 }
    }
}

/// One save's report (`report`), as `openmw-save-stats` prints it.
pub fn report(s: &SaveStats, opt: &ReportOptions) -> String {
    let mut o = Out(String::new());
    o.line(&format!("# {}", s.name));
    o.line(&format!("\nPlayer: {} - cell: {} - save format {} - {} KB", s.player, s.cell, s.format_version, kb(s.size as f64)));
    let lo = &s.load_order;
    if !lo.missing.is_empty() {
        o.line(&format!("Not found in the data directories: {}", lo.missing.join(", ")));
    }
    if !lo.resolved {
        o.line("Load order not read: mods are guessed from file and script names.");
    }
    let mods = report_mods(&mut o, s, opt.top);
    let shown: Vec<String> =
        if opt.patterns.is_empty() { mods.iter().take(opt.details).cloned().collect() } else { mods.iter().filter(|m| matches(m, &opt.patterns, &mods)).cloned().collect() };
    if !shown.is_empty() {
        o.section("Details per mod", "");
        for m in &shown {
            report_mod(&mut o, s, m, opt.key_rows);
        }
    }
    report_hints(&mut o, s, opt.top);
    let unknown: BTreeSet<String> = s.scripts.keys().filter(|id| s.script_mod(**id) == UNKNOWN_MOD).map(|id| s.script_path(*id)).collect();
    if !unknown.is_empty() {
        o.line("\nScripts not declared in any .omwscripts or plugin of the load order:");
        for p in unknown {
            o.line(&format!("  {p}"));
        }
    }
    if opt.full {
        report_global(&mut o, s, opt.top);
    }
    for pattern in &opt.dump {
        report_dump(&mut o, s, pattern, opt.dump_lines);
    }
    o.0
}

fn diff_rows<K: Clone + Eq + std::hash::Hash>(a: &IndexMap<K, (u64, f64)>, b: &IndexMap<K, (u64, f64)>) -> Vec<(K, u64, u64, f64, f64)> {
    // Python iterates `set(a) | set(b)` (no order); here a's then b's, then sorted by the change.
    let mut keys: Vec<K> = a.keys().cloned().collect();
    for k in b.keys() {
        if !a.contains_key(k) {
            keys.push(k.clone());
        }
    }
    let mut rows = Vec::new();
    for k in keys {
        let (ca, sa) = a.get(&k).copied().unwrap_or((0, 0.0));
        let (cb, sb) = b.get(&k).copied().unwrap_or((0, 0.0));
        if sa.round() != sb.round() || ca != cb {
            rows.push((k, ca, cb, sa, sb));
        }
    }
    rows.sort_by(|x, y| (y.4 - y.3).abs().partial_cmp(&(x.4 - x.3).abs()).unwrap_or(std::cmp::Ordering::Equal));
    rows
}

/// What changed between two saves, the older first (`report_diff`).
pub fn report_diff(a: &SaveStats, b: &SaveStats, top: usize, patterns: &[String]) -> String {
    let mut o = Out(String::new());
    o.line(&format!("# {} -> {}", a.name, b.name));
    o.line(&format!("\n{} KB -> {} KB ({} KB)", kb(a.size as f64), kb(b.size as f64), signed_kb(b.size as f64 - a.size as f64)));
    let mut names: Vec<String> = a.owned.keys().cloned().collect();
    names.extend(b.owned.keys().filter(|k| !a.owned.contains_key(*k)).cloned());
    let keep = |m: &String| patterns.is_empty() || matches(m, patterns, &names);
    let ma: IndexMap<String, (u64, f64)> = a.owned.keys().filter(|m| keep(m)).map(|m| (m.clone(), (0, a.mod_total(m)))).collect();
    let mb: IndexMap<String, (u64, f64)> = b.owned.keys().filter(|m| keep(m)).map(|m| (m.clone(), (0, b.mod_total(m)))).collect();
    o.section("Space per mod", "");
    let rows: Vec<Vec<String>> =
        diff_rows(&ma, &mb).into_iter().take(top).map(|(k, _, _, sa, sb)| vec![k, kb(sa), kb(sb), signed_kb(sb - sa)]).collect();
    o.table(&["mod", "KB A", "KB B", "delta KB"], &rows, "");

    let parts = |s: &SaveStats, ms: &IndexMap<String, (u64, f64)>| -> IndexMap<(String, String), (u64, f64)> {
        let mut out = IndexMap::new();
        for m in ms.keys() {
            if let Some(c) = s.owned.get(m) {
                for (p, v) in c {
                    out.insert((m.clone(), p.clone()), *v);
                }
            }
        }
        out
    };
    o.section("What changed, per mod and part", "");
    let rows: Vec<Vec<String>> = diff_rows(&parts(a, &ma), &parts(b, &mb))
        .into_iter()
        .take(top)
        .map(|((m, p), ca, cb, sa, sb)| vec![m, p, commas(ca), commas(cb), kb(sa), kb(sb), signed_kb(sb - sa)])
        .collect();
    o.table(&["mod", "part", "count A", "count B", "KB A", "KB B", "delta KB"], &rows, "");

    let scripts = |s: &SaveStats| -> IndexMap<String, (u64, f64)> { s.scripts.iter().map(|(id, st)| (s.script_path(*id), (st.attached, st.total() as f64))).collect() };
    o.section("Lua scripts", "");
    let rows: Vec<Vec<String>> = diff_rows(&scripts(a), &scripts(b))
        .into_iter()
        .take(top)
        .map(|(k, ca, cb, x, y)| vec![k, commas(ca), commas(cb), kb(x), kb(y), signed_kb(y - x)])
        .collect();
    o.table(&["script", "objects A", "objects B", "KB A", "KB B", "delta KB"], &rows, "");
    o.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sub(tag: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut v = tag.to_vec();
        v.extend(u32::try_from(data.len()).unwrap().to_le_bytes());
        v.extend(data);
        v
    }
    fn rec(tag: &[u8; 4], subs: &[Vec<u8>]) -> Vec<u8> {
        let body: Vec<u8> = subs.concat();
        let mut v = tag.to_vec();
        v.extend(u32::try_from(body.len()).unwrap().to_le_bytes());
        v.extend([0u8; 8]);
        v.extend(body);
        v
    }

    #[test]
    fn numbers_print_as_python_prints_them() {
        assert_eq!(kb(1_536_000.0), "1,500.0");
        assert_eq!(kb(51.2), "0.1");
        assert_eq!(signed_kb(-2048.0), "-2.0");
        assert_eq!(pct(1.0, 3), "33.3%");
        assert_eq!(commas(1234567), "1,234,567");
        assert_eq!(py_g(0.0001), "0.0001");
        assert_eq!(py_g(1.5e20), "1.5e+20");
        assert_eq!(py_g(123456789.0), "1.23457e+08");
        assert_eq!(py_g(3.25), "3.25");
        assert!(id_key("@0x1F") && id_key("-12.5") && !id_key("12.") && !id_key("name"));
        assert!(generic_dir("00 Core") && generic_dir("Data Files") && !generic_dir("01Core") && !generic_dir("Tamriel Data"));
        assert_eq!(decode_refid(b"\x03\x05\x00\x00\x00\xff\xff\xff\xff"), "FormId:0x-1:0x5");
    }

    #[test]
    fn every_byte_goes_to_one_owner() {
        let save = [
            rec(b"TES3", &[sub(b"FORM", &40u32.to_le_bytes()), sub(b"MAST", b"Morrowind.esm\0"), sub(b"DATA", &[0; 8]), sub(b"MAST", b"Mine.esp\0"), sub(b"DATA", &[0; 8])]),
            rec(b"SAVE", &[sub(b"PLNA", b"Nerevar\0"), sub(b"PLCE", b"Balmora\0"), sub(b"DEPE", b"Morrowind.esm\0"), sub(b"DEPE", b"Mine.esp\0")]),
            rec(b"GLOB", &[sub(b"NAME", b"\x02mine_var"), sub(b"FLTV", &[0; 4])]),
            rec(b"WTHR", &[sub(b"DATA", &[0; 10])]),
        ]
        .concat();
        assert_eq!(content_files(&save).unwrap(), ["Morrowind.esm", "Mine.esp"]);
        assert_eq!(missing_from(&content_files(&save).unwrap(), &["morrowind.esm".into()]), ["Mine.esp"]);
        let s = analyse("a.omwsave", save.clone(), None, 4);
        assert_eq!((s.player.as_str(), s.cell.as_str(), s.format_version), ("Nerevar", "Balmora", 40));
        let credited: f64 = s.owned.keys().map(|m| s.mod_total(m)).sum();
        assert_eq!(credited, save.len() as f64);
        let text = report(&s, &ReportOptions::default());
        assert!(text.starts_with("# a.omwsave\n\nPlayer: Nerevar - cell: Balmora - save format 40 - "), "{text}");
        assert!(text.contains("## Space per mod"));
        let d = report_diff(&s, &s, 30, &[]);
        assert!(d.contains("(none)"));
    }
}
