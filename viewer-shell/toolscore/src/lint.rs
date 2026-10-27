//! The load-order lint (port step 4.4, slice 2): `wraithguard_toolkit.lint_plugins`, whole.
//! One pass over each plugin's raw records pulls out the facts (the evil GMSTs, interior cells
//! and their fog, interior path grids, header gaps, masters, expansion-only script calls),
//! the plugins side by side; then the load order is folded in order (which plugin introduced
//! an interior first, whether any plugin gives it a path grid) and the warnings are worded.
//!
//! The record scan is the one `native/src/lint.rs` ported step for step from the Python
//! `_lint_*` helpers (how a truncated record is clamped, which duplicate subrecord wins,
//! Python's `str.strip()` whitespace and `re`'s word boundaries over Latin-1).

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::lint_tables::{BLOODMOON_FUNCS, EVIL_GMSTS, SKIP, SKIP_CELLS, TRIBUNAL_FUNCS};

/// Top-level records: `(tag, body)`, the body clamped to what the file holds.
pub fn records(raw: &[u8]) -> impl Iterator<Item = (&[u8], &[u8])> {
    frames(raw, 16)
}

/// Subrecords of a record body: `(tag, data)`, clamped likewise.
pub fn subrecords(body: &[u8]) -> impl Iterator<Item = (&[u8], &[u8])> {
    frames(body, 8)
}

fn frames(buf: &[u8], header: usize) -> impl Iterator<Item = (&[u8], &[u8])> {
    let mut i = 0usize;
    std::iter::from_fn(move || {
        if i + header > buf.len() {
            return None;
        }
        let tag = &buf[i..i + 4];
        let size = u32::from_le_bytes(buf[i + 4..i + 8].try_into().expect("4 bytes")) as usize;
        let start = i + header;
        let end = start.saturating_add(size).min(buf.len());
        i = start.saturating_add(size);
        Some((tag, &buf[start.min(buf.len())..end]))
    })
}

/// Python's `str.isspace()` over Latin-1.
fn is_py_space(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\x0b' | '\x0c' | '\r' | '\x1c'..='\x1f' | ' ' | '\u{85}' | '\u{a0}')
}

/// Python `re`'s `\w` over Latin-1 text: `str.isalnum()` or underscore.
pub fn is_py_word(b: u8) -> bool {
    let c = b as char;
    c == '_' || c.is_alphanumeric()
}

fn latin1(b: &[u8]) -> String {
    b.iter().map(|&x| x as char).collect()
}

/// A NUL-terminated string field: up to the first NUL, Latin-1, stripped.
pub fn zstr(b: &[u8]) -> String {
    let end = b.iter().position(|&x| x == 0).unwrap_or(b.len());
    latin1(&b[..end]).trim_matches(is_py_space).to_string()
}

fn strip_nuls(v: &[u8]) -> &[u8] {
    let end = v.iter().rposition(|&b| b != 0).map_or(0, |p| p + 1);
    &v[..end]
}

/// The expansion-function matcher: `^[^;\n]*?\b(NAME|...)\b`, case-insensitive, per line. At
/// most one match per line - the leftmost, and at one position the first name in list order -
/// returned as spelled in the script.
pub struct Calls {
    names: Vec<Vec<u8>>,
}

impl Calls {
    pub fn new<S: AsRef<str>>(names: &[S]) -> Calls {
        Calls { names: names.iter().map(|n| n.as_ref().as_bytes().to_vec()).collect() }
    }

    pub fn scan(&self, text: &[u8], found: &mut BTreeSet<String>) {
        let mut line_start = 0usize;
        while line_start <= text.len() {
            let line_end = text[line_start..].iter().position(|&c| c == b'\n').map_or(text.len(), |p| line_start + p);
            let stop = text[line_start..line_end].iter().position(|&c| c == b';').map_or(line_end, |p| line_start + p);
            'pos: for p in line_start..stop {
                let before_word = p > 0 && is_py_word(text[p - 1]);
                if before_word || !is_py_word(text[p]) {
                    continue;
                }
                for name in &self.names {
                    let end = p + name.len();
                    if end > text.len() || !text[p..end].eq_ignore_ascii_case(name) {
                        continue;
                    }
                    if end < text.len() && is_py_word(text[end]) {
                        continue;
                    }
                    found.insert(latin1(&text[p..end]));
                    break 'pos;
                }
            }
            if line_end == text.len() {
                break;
            }
            line_start = line_end + 1;
        }
    }
}

/// A plugin's facts, in record order where order matters.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Facts {
    pub events: Vec<Event>,
    pub evil_gmsts: Vec<String>,
    pub pathgrids: Vec<String>,
    pub masters: BTreeSet<String>,
    pub tribunal: BTreeSet<String>,
    pub bloodmoon: BTreeSet<String>,
}

/// What the header or a cell says, in record order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// The header leaves these blank (`author`, `description`).
    Header(Vec<&'static str>),
    /// An interior cell: its name, its path grid id (empty: left out of that check), and
    /// whether its fog density is exactly 0 outside "behave like exterior".
    Cell { name: String, cell_id: String, fog_bug: bool },
}

/// Reads one plugin's records into the facts the checks consume.
pub struct Linter {
    evil: HashMap<String, (Vec<u8>, Vec<u8>)>,
    tribunal: Calls,
    bloodmoon: Calls,
    skip_cells: BTreeSet<String>,
}

impl Default for Linter {
    /// The lint's own tables ([`crate::lint_tables`]).
    fn default() -> Self {
        Linter::new(
            EVIL_GMSTS.iter().map(|(n, t, v)| ((*n).to_string(), (t.to_vec(), v.to_vec()))).collect(),
            TRIBUNAL_FUNCS,
            BLOODMOON_FUNCS,
            SKIP_CELLS.iter().map(|s| (*s).to_string()).collect(),
        )
    }
}

impl Linter {
    /// `evil` maps a lower-cased setting name to `(value tag, raw value)`; the function lists
    /// are the Tribunal and Bloodmoon script functions; `skip_cells` are lower-cased cell
    /// names left out of the path grid check.
    pub fn new<S: AsRef<str>>(evil: HashMap<String, (Vec<u8>, Vec<u8>)>, tribunal: &[S], bloodmoon: &[S], skip_cells: BTreeSet<String>) -> Linter {
        Linter {
            evil: evil.into_iter().map(|(k, (t, v))| (k, (t, strip_nuls(&v).to_vec()))).collect(),
            tribunal: Calls::new(tribunal),
            bloodmoon: Calls::new(bloodmoon),
            skip_cells,
        }
    }

    /// One plugin's facts. `is_custom` turns on the header and expansion-call checks, which
    /// apply only to the user's own mods.
    pub fn facts(&self, raw: &[u8], is_custom: bool) -> Facts {
        let mut f = Facts::default();
        for (tag, body) in records(raw) {
            match tag {
                b"SCPT" | b"INFO" if is_custom => {
                    let want: &[u8] = if tag == b"SCPT" { b"SCTX" } else { b"BNAM" };
                    for (st, data) in subrecords(body) {
                        if st == want && !data.is_empty() {
                            self.tribunal.scan(data, &mut f.tribunal);
                            self.bloodmoon.scan(data, &mut f.bloodmoon);
                        }
                    }
                }
                b"TES3" => {
                    for (st, data) in subrecords(body) {
                        if st == b"MAST" {
                            f.masters.insert(zstr(data).to_lowercase());
                        }
                    }
                    if is_custom {
                        let missing = header_gaps(body);
                        if !missing.is_empty() {
                            f.events.push(Event::Header(missing));
                        }
                    }
                }
                b"GMST" => {
                    if let Some(name) = self.evil_gmst(body) {
                        f.evil_gmsts.push(name);
                    }
                }
                b"CELL" => {
                    if let Some(e) = self.cell(body) {
                        f.events.push(e);
                    }
                }
                b"PGRD" => {
                    if let Some(name) = interior_pathgrid(body) {
                        f.pathgrids.push(name);
                    }
                }
                _ => {}
            }
        }
        f
    }

    /// The setting's name when both name and value match the evil table.
    fn evil_gmst(&self, body: &[u8]) -> Option<String> {
        let mut name: Option<String> = None;
        let mut value: Option<(&[u8], &[u8])> = None;
        for (st, data) in subrecords(body) {
            match st {
                b"NAME" => name = Some(zstr(data).to_lowercase()),
                b"STRV" | b"INTV" | b"FLTV" => value = Some((st, data)),
                _ => {}
            }
        }
        let name = name.filter(|n| !n.is_empty())?;
        let (want_tag, want_value) = self.evil.get(&name)?;
        let (tag, data) = value?;
        (tag == want_tag.as_slice() && strip_nuls(data) == want_value.as_slice()).then_some(name)
    }

    /// An interior cell's name, path grid id and fog-bug flag; `None` for an exterior or a
    /// `DATA` too short to read.
    fn cell(&self, body: &[u8]) -> Option<Event> {
        let mut name = String::new();
        let mut data: Option<&[u8]> = None;
        let mut ambience: Option<&[u8]> = None;
        for (st, payload) in subrecords(body) {
            match st {
                b"NAME" => name = zstr(payload),
                b"DATA" if data.is_none() => data = Some(payload),
                b"AMBI" => ambience = Some(payload),
                _ => {}
            }
        }
        let data = data.filter(|d| d.len() >= 12)?;
        let flags = u32::from_le_bytes(data[0..4].try_into().expect("4 bytes"));
        if flags & 1 == 0 {
            return None;
        }
        let cell_id = name.to_lowercase();
        let mut fog_bug = false;
        if flags & 128 == 0 {
            let fog = match ambience {
                Some(a) if a.len() == 16 => f32::from_le_bytes(a[12..16].try_into().expect("4 bytes")),
                _ => f32::from_le_bytes(data[8..12].try_into().expect("4 bytes")),
            };
            fog_bug = fog == 0.0;
        }
        let cell_id = if self.skip_cells.contains(&cell_id) { String::new() } else { cell_id };
        Some(Event::Cell { name, cell_id, fog_bug })
    }
}

/// Which of author and description the first full `HEDR` leaves blank.
fn header_gaps(body: &[u8]) -> Vec<&'static str> {
    for (st, data) in subrecords(body) {
        if st == b"HEDR" && data.len() >= 296 {
            let mut out = Vec::new();
            if zstr(&data[8..40]).is_empty() {
                out.push("author");
            }
            if zstr(&data[40..296]).is_empty() {
                out.push("description");
            }
            return out;
        }
    }
    Vec::new()
}

/// The lower-cased cell name of an interior path grid (grid `(0, 0)`, named).
fn interior_pathgrid(body: &[u8]) -> Option<String> {
    let mut name = String::new();
    let mut grid: Option<(i32, i32)> = None;
    for (st, data) in subrecords(body) {
        match st {
            b"NAME" => name = zstr(data),
            b"DATA" if data.len() >= 8 => {
                grid = Some((
                    i32::from_le_bytes(data[0..4].try_into().expect("4 bytes")),
                    i32::from_le_bytes(data[4..8].try_into().expect("4 bytes")),
                ));
            }
            _ => {}
        }
    }
    (grid == Some((0, 0)) && !name.is_empty()).then(|| name.to_lowercase())
}

/// The lint's counts (`stats`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    pub scanned: usize,
    pub unreadable: usize,
    pub warnings: usize,
    pub interior_cells: usize,
    pub pathgrids: usize,
}

/// What reading one plugin gave.
enum Read {
    Skip,
    Unreadable,
    Ok(Box<Facts>),
}

/// The lint over a load order (`lint_plugins`): `find` locates a plugin's file, `subset` names
/// the user's own plugins and `origins` each plugin's provenance (lower-case name to the tag
/// the warnings carry). `threads` reads that many plugins at once (0: one). Answers the
/// warnings and the counts.
pub fn lint_order(
    order: &[String],
    find: &(dyn Fn(&str) -> Option<PathBuf> + Sync),
    subset: &[String],
    origins: &BTreeMap<String, String>,
    threads: usize,
) -> (Vec<String>, Stats) {
    let mine: HashSet<String> = subset.iter().map(|s| s.to_lowercase()).collect();
    let tag = |p: &str| origins.get(&p.to_lowercase()).map(|o| format!(" [{o}]")).unwrap_or_default();
    let linter = Linter::default();
    let skip: HashSet<&str> = SKIP.iter().copied().collect();
    let read_one = |plugin: &String| -> Read {
        let low = plugin.to_lowercase();
        if low.ends_with(".omwscripts") || skip.contains(low.as_str()) {
            return Read::Skip;
        }
        let Some(path) = find(plugin) else { return Read::Skip };
        let Ok(raw) = std::fs::read(&path) else { return Read::Unreadable };
        if raw.get(..4) != Some(b"TES3") {
            return Read::Skip;
        }
        Read::Ok(Box::new(linter.facts(&raw, mine.contains(&low))))
    };
    let read: Vec<Read> = par_map(order, threads.max(1), read_one);

    let mut warnings = Vec::new();
    let mut stats = Stats::default();
    let mut interior_first: BTreeMap<String, (String, String)> = BTreeMap::new();
    let mut pathgrids: BTreeSet<String> = BTreeSet::new();
    for (plugin, got) in order.iter().zip(read) {
        let facts = match got {
            Read::Skip => continue,
            Read::Unreadable => {
                stats.unreadable += 1;
                continue;
            }
            Read::Ok(f) => f,
        };
        stats.scanned += 1;
        let t = tag(plugin);
        for e in &facts.events {
            match e {
                Event::Header(missing) => warnings.push(format!("[HEADER] '{plugin}'{t}: header has no {}.", missing.join(" and no "))),
                Event::Cell { name, cell_id, fog_bug } => {
                    if !cell_id.is_empty() && !interior_first.contains_key(cell_id) {
                        interior_first.insert(cell_id.clone(), (plugin.clone(), name.clone()));
                    }
                    if *fog_bug {
                        warnings.push(format!(
                            "[FOGBUG] '{plugin}'{t}: interior cell '{name}' has fog density 0.0 -- renders as a black void on some GPUs. Fix by setting any nonzero fog density on the cell."
                        ));
                    }
                }
            }
        }
        pathgrids.extend(facts.pathgrids.iter().cloned());
        if !facts.evil_gmsts.is_empty() {
            let mut names = facts.evil_gmsts.clone();
            names.sort();
            warnings.push(format!(
                "[EVLGMST] '{plugin}'{t}: {} evil GMST(s): {} -- stale expansion defaults copied in by an old Construction Set; tes3cmd clean removes them.",
                facts.evil_gmsts.len(),
                names.join(", ")
            ));
        }
        let has = |m: &str| facts.masters.contains(m);
        if !facts.tribunal.is_empty() && !has("tribunal.esm") && !has("bloodmoon.esm") {
            warnings.push(format!(
                "[EXP-DEP] '{plugin}'{t}: scripts use Tribunal function(s) {} but the plugin doesn't master Tribunal.esm -- fragile on non-expansion setups (tes3lint !TB-FUN).",
                facts.tribunal.iter().cloned().collect::<Vec<_>>().join(", ")
            ));
        }
        if !facts.bloodmoon.is_empty() && !has("bloodmoon.esm") {
            warnings.push(format!(
                "[EXP-DEP] '{plugin}'{t}: scripts use Bloodmoon function(s) {} but the plugin doesn't master Bloodmoon.esm -- fragile on non-expansion setups (tes3lint !BM-FUN).",
                facts.bloodmoon.iter().cloned().collect::<Vec<_>>().join(", ")
            ));
        }
    }
    for (cell_id, (plugin, name)) in &interior_first {
        if !pathgrids.contains(cell_id) {
            warnings.push(format!(
                "[NO PATHGRID] '{plugin}'{}: new interior cell '{name}' has no pathgrid anywhere in the load order -- NPCs can't pathfind there.",
                tag(plugin)
            ));
        }
    }
    warnings.extend(twin_warnings(order, find, &mine, &tag));
    stats.warnings = warnings.len();
    stats.interior_cells = interior_first.len();
    stats.pathgrids = pathgrids.len();
    (warnings, stats)
}

/// The user's plugins whose `.omwscripts` / content twin sits beside them but is not loaded
/// (`_lint_twin_warnings`).
fn twin_warnings(
    order: &[String],
    find: &(dyn Fn(&str) -> Option<PathBuf> + Sync),
    mine: &HashSet<String>,
    tag: &dyn Fn(&str) -> String,
) -> Vec<String> {
    let active: HashSet<String> = order.iter().map(|p| p.to_lowercase()).collect();
    let mut out = Vec::new();
    for plugin in order {
        let low = plugin.to_lowercase();
        if !mine.contains(&low) {
            continue;
        }
        let Some(path) = find(plugin) else { continue };
        let file = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let stem = file.rsplit_once('.').map_or(file.as_str(), |(s, _)| s).to_string();
        let twin = |ext: &str| -> Option<String> {
            let name = format!("{stem}{ext}");
            let p: PathBuf = path.with_file_name(&name);
            (Path::new(&p).exists() && !active.contains(&name.to_lowercase())).then_some(name)
        };
        if low.ends_with(".omwaddon") || low.ends_with(".esp") {
            if let Some(name) = twin(".omwscripts") {
                out.push(format!(
                    "[TWIN] '{plugin}'{}: '{name}' sits in the same folder but isn't in the load order -- the mod's Lua half is disabled. Add it (or confirm it's optional).",
                    tag(plugin)
                ));
            }
        } else if low.ends_with(".omwscripts")
            && let Some(name) = [".omwaddon", ".esp"].iter().find_map(|e| twin(e))
        {
            out.push(format!(
                "[TWIN] '{plugin}'{}: '{name}' sits in the same folder but isn't in the load order -- scripts may reference content that never loads. Add it (or confirm it's optional).",
                tag(plugin)
            ));
        }
    }
    out
}

/// `f` over `items` on up to `threads` threads, the answers in order.
fn par_map<T: Sync, R: Send>(items: &[T], threads: usize, f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    if threads <= 1 || items.len() < 2 {
        return items.iter().map(&f).collect();
    }
    let next = std::sync::atomic::AtomicUsize::new(0);
    let mut out: Vec<Option<R>> = (0..items.len()).map(|_| None).collect();
    let slots: Vec<std::sync::Mutex<&mut Option<R>>> = out.iter_mut().map(std::sync::Mutex::new).collect();
    std::thread::scope(|s| {
        for _ in 0..threads.min(items.len()) {
            s.spawn(|| {
                loop {
                    let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    if i >= items.len() {
                        break;
                    }
                    let r = f(&items[i]);
                    **slots[i].lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(r);
                }
            });
        }
    });
    drop(slots);
    out.into_iter().map(|r| r.expect("every item was mapped")).collect()
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
    fn header(author: &str, masters: &[&str]) -> Vec<u8> {
        let mut hedr = vec![0u8; 300];
        hedr[8..8 + author.len()].copy_from_slice(author.as_bytes());
        let mut subs = vec![sub(b"HEDR", &hedr)];
        for m in masters {
            subs.push(sub(b"MAST", format!("{m}\0").as_bytes()));
            subs.push(sub(b"DATA", &[0u8; 8]));
        }
        rec(b"TES3", &subs)
    }

    #[test]
    fn a_load_order_is_linted_and_worded_as_the_python_words_it() {
        let dir = std::env::temp_dir().join(format!("wg_lint_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut cell_data = vec![1u8, 0, 0, 0];
        cell_data.extend([0u8; 8]);
        let mine = [
            header("", &["Morrowind.esm"]),
            rec(b"CELL", &[sub(b"NAME", b"Dark Room\0"), sub(b"DATA", &cell_data)]),
            rec(b"GMST", &[sub(b"NAME", b"fFleeDistance\0"), sub(b"FLTV", b"\x00\x80;E")]),
            rec(b"SCPT", &[sub(b"SCTX", b"begin x\nif ( GetPCJumping ) ; ForceJump\nendif\nPlaceAtMe foo\nend")]),
        ]
        .concat();
        std::fs::write(dir.join("Mine.esp"), mine).unwrap();
        std::fs::write(dir.join("Mine.omwscripts"), "").unwrap();
        let order = vec!["Morrowind.esm".to_string(), "Mine.esp".to_string()];
        let find = |n: &str| Some(dir.join(n)).filter(|p| p.exists());
        let origins: BTreeMap<String, String> = [("mine.esp".to_string(), "custom".to_string())].into();
        let (w, s) = lint_order(&order, &find, &["Mine.esp".into()], &origins, 2);
        assert_eq!(s.scanned, 1);
        assert_eq!(
            w,
            [
                "[HEADER] 'Mine.esp' [custom]: header has no author and no description.",
                "[FOGBUG] 'Mine.esp' [custom]: interior cell 'Dark Room' has fog density 0.0 -- renders as a black void on some GPUs. Fix by setting any nonzero fog density on the cell.",
                "[EVLGMST] 'Mine.esp' [custom]: 1 evil GMST(s): ffleedistance -- stale expansion defaults copied in by an old Construction Set; tes3cmd clean removes them.",
                "[EXP-DEP] 'Mine.esp' [custom]: scripts use Tribunal function(s) GetPCJumping but the plugin doesn't master Tribunal.esm -- fragile on non-expansion setups (tes3lint !TB-FUN).",
                "[EXP-DEP] 'Mine.esp' [custom]: scripts use Bloodmoon function(s) PlaceAtMe but the plugin doesn't master Bloodmoon.esm -- fragile on non-expansion setups (tes3lint !BM-FUN).",
                "[NO PATHGRID] 'Mine.esp' [custom]: new interior cell 'Dark Room' has no pathgrid anywhere in the load order -- NPCs can't pathfind there.",
                "[TWIN] 'Mine.esp' [custom]: 'Mine.omwscripts' sits in the same folder but isn't in the load order -- the mod's Lua half is disabled. Add it (or confirm it's optional).",
            ]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
