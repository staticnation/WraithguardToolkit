//! The whole load order sorted by the mlox rules, as plain mlox sorts it: no curated list, no
//! customizations file, every plugin the user's (TODO.md, Next, 3; Static, 2026-11-10). For
//! the installs MOMW does not manage - a Mod Organizer 2 profile, the game's own Morrowind.ini.
//!
//! The sort is [`crate::order::build_and_sort`] with nothing frozen: each plugin goes where the
//! rules and its header's masters put it, and one with no rule keeps its place. What it
//! writes is the install's own: a Morrowind.ini's `[Game Files]` and the plugins' dates (the
//! game loads its plugins by date, masters first; the ini is the record of it), an MO2
//! profile's `loadorder.txt` and `plugins.txt` and its Morrowind.ini. Every file is copied
//! first (`name.bak-<stamp>`), the dates set kept to put back (`Morrowind.ini.dates-<stamp>.json`).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::graph::is_master_file;

/// What the sort made of a load order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sorted {
    /// The load order as it is.
    pub was: Vec<String>,
    /// As the rules have it.
    pub order: Vec<String>,
    /// The plugins that move: `(name, place it had, place it takes)`, from 1.
    pub moved: Vec<(String, usize, usize)>,
    /// Rules refused for closing a cycle, `(before, after)`.
    pub conflicts: Vec<(String, String)>,
    /// What the rule files said along the way (lines bridged over).
    pub notes: Vec<String>,
}

/// Sorts `order` (the plugins, as the install loads them) by the rule files `rules`
/// (lowest priority first, as the Load order page lists them); `find` says where a plugin is,
/// for its header's masters.
///
/// # Errors
/// A rule file that cannot be read, or a rule the patterns decline.
pub fn sort(order: &[String], rules: &[String], find: &dyn Fn(&str) -> Option<PathBuf>) -> Result<Sorted, String> {
    let mut report = crate::plan::Report::default();
    let blocks = crate::plan::load_rule_blocks(rules, &mut report).map_err(|_| "a rule file could not be read".to_string())?;
    let mut masters: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for name in order {
        if let Some(p) = find(name) {
            let ms = crate::inputs::read_masters(&p);
            if !ms.is_empty() {
                masters.insert(name.to_lowercase(), ms);
            }
        }
    }
    let got = crate::order::build_and_sort(&[], order, &blocks.chains, &masters, &blocks.nearstart, &blocks.nearend)
        .map_err(|_| "a rule names a plugin in a form the sort does not read".to_string())?;
    // Only the plugins of the load order (the sort answers in the spelling it was given).
    let known: HashMap<String, &String> = order.iter().map(|n| (n.to_lowercase(), n)).collect();
    let sorted: Vec<String> = got.order.iter().filter_map(|n| known.get(&n.to_lowercase()).map(|s| (*s).clone())).collect();
    let at: HashMap<String, usize> = order.iter().enumerate().map(|(i, n)| (n.to_lowercase(), i + 1)).collect();
    let moved = sorted
        .iter()
        .enumerate()
        .filter_map(|(i, n)| at.get(&n.to_lowercase()).filter(|&&was| was != i + 1).map(|&was| (n.clone(), was, i + 1)))
        .collect();
    Ok(Sorted { was: order.to_vec(), order: sorted, moved, conflicts: got.conflicts, notes: report.text() })
}

/// A text file's bytes as characters one to one (Windows-1252's bytes are what a Morrowind.ini
/// holds; read as Latin-1 and written back the same, nothing in a line not changed moves).
fn latin1(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| b as char).collect()
}

fn unlatin1(text: &str) -> Vec<u8> {
    text.chars().map(|c| if (c as u32) < 256 { c as u32 as u8 } else { b'?' }).collect()
}

/// A Morrowind.ini's text with its `[Game Files]` naming `order` (`GameFile0=` on), the rest of
/// the file as it was: its line ends, the other sections, the lines of the section that are
/// not `GameFileN=`. A file with no `[Game Files]` gets one at the end.
pub fn ini_with_order(text: &str, order: &[String]) -> String {
    let nl = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let header = |l: &str| l.trim().starts_with('[');
    let start = lines.iter().position(|l| l.trim().eq_ignore_ascii_case("[Game Files]"));
    let entries: String = order.iter().enumerate().map(|(i, n)| format!("GameFile{i}={n}{nl}")).collect();
    let Some(start) = start else {
        let mut out = text.to_string();
        if !out.is_empty() && !out.ends_with('\n') {
            out.push_str(nl);
        }
        out.push_str(&format!("[Game Files]{nl}{entries}"));
        return out;
    };
    let end = lines.iter().enumerate().skip(start + 1).find(|(_, l)| header(l)).map_or(lines.len(), |(i, _)| i);
    let is_entry = |l: &str| {
        let t = l.trim_start();
        t.len() > 8 && t[..8].eq_ignore_ascii_case("gamefile") && t[8..].split('=').next().is_some_and(|d| !d.is_empty() && d.trim().chars().all(|c| c.is_ascii_digit()))
    };
    let mut out = String::with_capacity(text.len() + entries.len());
    for l in &lines[..=start] {
        out.push_str(l);
    }
    if !lines[start].ends_with('\n') {
        out.push_str(nl);
    }
    out.push_str(&entries);
    for l in &lines[start + 1..end] {
        if !is_entry(l) {
            out.push_str(l);
        }
    }
    for l in &lines[end..] {
        out.push_str(l);
    }
    out
}

/// The dates that make the game load `order` (masters first, each kind by date): walking the
/// order, a plugin keeps its date when it is later than the one before it of its kind, else
/// takes that one's and a minute. `(name, date it has, date it takes)` for the ones that change.
pub fn dates_for(order: &[String], dated: &dyn Fn(&str) -> Option<SystemTime>) -> Vec<(String, SystemTime, SystemTime)> {
    let mut out = Vec::new();
    for masters in [true, false] {
        let mut last: Option<SystemTime> = None;
        for n in order.iter().filter(|n| is_master_file(n) == masters) {
            let Some(t) = dated(n) else { continue };
            let want = match last {
                Some(l) if t <= l => l + Duration::from_secs(60),
                _ => t,
            };
            if want != t {
                out.push((n.clone(), t, want));
            }
            last = Some(want);
        }
    }
    out
}

fn secs(t: SystemTime) -> f64 {
    t.duration_since(SystemTime::UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64())
}

/// A copy of `path` beside it, `name.bak-<stamp>`, before it is written over.
fn backup(path: &Path, stamp: &str) -> Result<PathBuf, String> {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let to = path.with_file_name(format!("{name}.bak-{stamp}"));
    std::fs::copy(path, &to).map_err(|e| format!("{} could not be copied ({e}): nothing was written", path.display()))?;
    Ok(to)
}

/// What [`write`] did, in lines (or would do, on a dry run).
pub type Written = Vec<String>;

/// One file written (or not): the same text says so, `dry` says what would be, else the file
/// is copied first and written. `what` names the change in the line said.
fn put(path: &Path, was: &str, now: &str, what: &str, dry: bool, stamp: &str, said: &mut Written) -> Result<(), String> {
    let name = path.display();
    if now == was {
        said.push(format!("{name}: {what} already as sorted"));
    } else if dry {
        said.push(format!("{name}: {what} would be written"));
    } else {
        let b = backup(path, stamp)?;
        std::fs::write(path, unlatin1(now)).map_err(|e| format!("{name} could not be written: {e}"))?;
        said.push(format!("{name}: {what} written; as it was: {}", b.display()));
    }
    Ok(())
}

fn lowered(off: &[String]) -> HashSet<String> {
    off.iter().map(|n| n.to_lowercase()).collect()
}

/// Writes `order` into the install: its Morrowind.ini's `[Game Files]` (`ini`; the plugins of
/// `off` left out: the game loads what it lists), the plugins' dates (`find` says where each
/// is; a Morrowind.ini install: the game loads by date), and for an MO2 profile
/// (`mo2_profile`, its folder) `loadorder.txt` and `plugins.txt` as well (`off` switched off
/// there) - in MO2 the dates are its own business, and are not set. `dry`: nothing written,
/// the lines say what would be. `stamp` names the backups.
///
/// # Errors
/// A file that cannot be read or written: what was written before it stays, and is said.
pub fn write(ini: &Path, order: &[String], off: &[String], mo2_profile: Option<&Path>, find: &dyn Fn(&str) -> Option<PathBuf>, dry: bool, stamp: &str) -> Result<Written, String> {
    let mut said: Written = Vec::new();
    let offs = lowered(off);
    let on: Vec<String> = order.iter().filter(|n| !offs.contains(&n.to_lowercase())).cloned().collect();
    let text = latin1(&std::fs::read(ini).map_err(|e| format!("{} could not be read: {e}", ini.display()))?);
    put(ini, &text, &ini_with_order(&text, &on), &format!("[Game Files] ({} plugins on)", on.len()), dry, stamp, &mut said)?;
    match mo2_profile {
        Some(dir) => {
            for file in ["loadorder.txt", "plugins.txt"] {
                let p = dir.join(file);
                let Ok(bytes) = std::fs::read(&p) else { continue };
                let was = latin1(&bytes);
                let now = if file == "plugins.txt" { plugins_txt_with_order(&was, order, &offs) } else { list_with_order(&was, order) };
                put(&p, &was, &now, "the order", dry, stamp, &mut said)?;
            }
        }
        None => {
            let dated = |n: &str| find(n).and_then(|p| std::fs::metadata(p).ok()?.modified().ok());
            let changes = dates_for(order, &dated);
            if changes.is_empty() {
                said.push("the plugins' dates already load them in this order".into());
            } else if dry {
                said.push(format!("{} plugin date(s) would be set: {}", changes.len(), changes.iter().take(8).map(|c| c.0.as_str()).collect::<Vec<_>>().join(", ")));
            } else {
                // The dates as they were, kept first, to put back.
                let keep = ini.with_file_name(format!("{DATES}{stamp}.json"));
                let doc: Vec<serde_json::Value> = changes
                    .iter()
                    .filter_map(|(n, was, _)| Some(serde_json::json!({"plugin": n, "path": find(n)?.to_string_lossy(), "modified": secs(*was)})))
                    .collect();
                std::fs::write(&keep, serde_json::to_string_pretty(&doc).unwrap_or_default()).map_err(|e| format!("{} could not be written ({e}): no date was set", keep.display()))?;
                let mut set = 0;
                for (n, _, want) in &changes {
                    let Some(p) = find(n) else { continue };
                    set_date(&p, *want).map_err(|e| format!("{e}; {set} set before it"))?;
                    set += 1;
                }
                said.push(format!("{set} plugin date(s) set so the game loads them in this order; as they were: {}", keep.display()));
            }
        }
    }
    Ok(said)
}

/// What the dates kept before a write are named, beside the Morrowind.ini: this and the stamp.
const DATES: &str = "Morrowind.ini.dates-";

fn set_date(p: &Path, t: SystemTime) -> Result<(), String> {
    let f = std::fs::File::options().write(true).open(p).map_err(|e| format!("{} could not be opened to date it ({e})", p.display()))?;
    f.set_modified(t).map_err(|e| format!("{} could not be dated ({e})", p.display()))
}

/// Puts back the plugin dates the last write set (the newest `Morrowind.ini.dates-<stamp>.json`
/// beside `ini`), and renames that file `.restored` so the one before it is next. `dry`:
/// nothing set, the lines say what would be.
///
/// # Errors
/// No dates kept, a file that does not read, or a plugin that cannot be dated (those before it are).
pub fn restore_dates(ini: &Path, dry: bool) -> Result<Written, String> {
    let dir = ini.parent().ok_or("the Morrowind.ini has no folder")?;
    let newest = std::fs::read_dir(dir)
        .map_err(|e| format!("{} could not be read: {e}", dir.display()))?
        .flatten()
        .filter(|e| {
            let n = e.file_name().to_string_lossy().into_owned();
            n.starts_with(DATES) && n.ends_with(".json")
        })
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .max_by_key(|(t, _)| *t)
        .map(|(_, p)| p)
        .ok_or_else(|| format!("no dates kept beside {}: Export has not set any", ini.display()))?;
    let doc: Vec<serde_json::Value> = serde_json::from_slice(&std::fs::read(&newest).map_err(|e| format!("{} could not be read: {e}", newest.display()))?)
        .map_err(|e| format!("{} does not read: {e}", newest.display()))?;
    let mut said = Vec::new();
    let mut set = 0;
    let mut gone = Vec::new();
    for d in &doc {
        let (Some(path), Some(at)) = (d.get("path").and_then(|v| v.as_str()), d.get("modified").and_then(serde_json::Value::as_f64)) else { continue };
        let p = Path::new(path);
        if !p.is_file() {
            gone.push(path.to_string());
            continue;
        }
        if !dry {
            set_date(p, SystemTime::UNIX_EPOCH + Duration::from_secs_f64(at.max(0.0))).map_err(|e| format!("{e}; {set} put back before it"))?;
        }
        set += 1;
    }
    if dry {
        said.push(format!("{set} plugin date(s) would be put back from {}", newest.display()));
    } else {
        let done = newest.with_extension("json.restored");
        let _ = std::fs::rename(&newest, &done);
        said.push(format!("{set} plugin date(s) put back from {} (now {})", newest.display(), done.display()));
    }
    if !gone.is_empty() {
        said.push(format!("{} plugin(s) no longer there: {}", gone.len(), gone.join(", ")));
    }
    Ok(said)
}

/// The lines of `text` that name a plugin of `order` (by `name`, `None` for a line naming none),
/// given the plugins of `order` in its order, one to a place: `(line index, plugin)`.
fn slots(lines: &[String], order: &[String], name: &dyn Fn(&str) -> Option<String>) -> Vec<(usize, String)> {
    let wanted: HashMap<String, usize> = order.iter().enumerate().map(|(i, n)| (n.to_lowercase(), i)).collect();
    let at: Vec<usize> = (0..lines.len()).filter(|&i| name(&lines[i]).is_some_and(|n| wanted.contains_key(&n.to_lowercase()))).collect();
    let mut names: Vec<String> = at.iter().filter_map(|&i| name(&lines[i])).collect();
    names.sort_by_key(|n| wanted[&n.to_lowercase()]);
    at.into_iter().zip(names).collect()
}

fn join(lines: &[String], text: &str) -> String {
    let nl = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut s = lines.join(nl);
    if text.ends_with('\n') {
        s.push_str(nl);
    }
    s
}

fn mo2_name(l: &str) -> Option<String> {
    let t = l.trim();
    (!t.is_empty() && !t.starts_with('#')).then(|| t.trim_start_matches('*').trim().to_string())
}

/// An MO2 `loadorder.txt` with the plugins of `order` in that order, each in one of the places
/// the file had a plugin of `order` in: what else it lists (a plugin not loaded, a comment)
/// stays where it was.
pub fn list_with_order(text: &str, order: &[String]) -> String {
    let mut out: Vec<String> = text.lines().map(str::to_string).collect();
    for (i, n) in slots(&out.clone(), order, &mo2_name) {
        out[i] = n;
    }
    join(&out, text)
}

/// An MO2 `plugins.txt` the same way, the plugins of `off` switched off: where the file marks
/// the ones on with `*`, an off one loses its `*` (and one on gets it); where it lists only the
/// ones on (Morrowind's own form), an off one's line goes.
pub fn plugins_txt_with_order(text: &str, order: &[String], off: &HashSet<String>) -> String {
    let lines: Vec<String> = text.lines().map(str::to_string).collect();
    let starred = lines.iter().any(|l| l.trim_start().starts_with('*'));
    let mut out: Vec<Option<String>> = lines.iter().cloned().map(Some).collect();
    for (i, n) in slots(&lines, order, &mo2_name) {
        let is_off = off.contains(&n.to_lowercase());
        out[i] = match (starred, is_off) {
            (true, false) => Some(format!("*{n}")),
            (true, true) => Some(n),
            (false, false) => Some(n),
            (false, true) => None,
        };
    }
    join(&out.into_iter().flatten().collect::<Vec<_>>(), text)
}

fn cfg_content(l: &str) -> Option<String> {
    let (k, v) = l.trim().split_once('=')?;
    (k.trim() == "content").then(|| v.trim().to_string()).filter(|v| !v.is_empty())
}

/// An `openmw.cfg`'s text with its `content=` lines in the order of `order`, each in one of
/// the places the file had one; one of `off` written `#content=` (OpenMW passes over a `#`
/// line, and the next sort leaves it where it is). Every other line as it was.
pub fn cfg_with_order(text: &str, order: &[String], off: &HashSet<String>) -> String {
    let mut out: Vec<String> = text.lines().map(str::to_string).collect();
    for (i, n) in slots(&out.clone(), order, &cfg_content) {
        out[i] = if off.contains(&n.to_lowercase()) { format!("#content={n}") } else { format!("content={n}") };
    }
    join(&out, text)
}

/// A path as compared: the same folder however it was spelled.
fn same_path(p: &Path) -> String {
    std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf()).to_string_lossy().to_lowercase()
}

/// The data folders (`data`, lowest priority first) in the order the plugins load: a folder
/// that holds plugins of `order` (`find` says where each is) goes as its first plugin does,
/// into one of the places such folders had; a folder with none (textures, meshes) keeps its
/// place. `(order, moved: (folder, place it had, place it takes))`, places from 1.
pub fn data_order(data: &[PathBuf], order: &[String], find: &dyn Fn(&str) -> Option<PathBuf>) -> (Vec<PathBuf>, Vec<(String, usize, usize)>) {
    let mut first: HashMap<String, usize> = HashMap::new();
    for (i, n) in order.iter().enumerate() {
        if let Some(dir) = find(n).as_deref().and_then(Path::parent) {
            first.entry(same_path(dir)).or_insert(i);
        }
    }
    let key = |d: &PathBuf| first.get(&same_path(d)).copied();
    let at: Vec<usize> = (0..data.len()).filter(|&i| key(&data[i]).is_some()).collect();
    let mut holding: Vec<&PathBuf> = at.iter().map(|&i| &data[i]).collect();
    holding.sort_by_key(|d| key(d));
    let mut out = data.to_vec();
    for (slot, d) in at.iter().zip(holding) {
        out[*slot] = d.clone();
    }
    let was: HashMap<String, usize> = data.iter().enumerate().map(|(i, d)| (same_path(d), i + 1)).collect();
    let moved = out
        .iter()
        .enumerate()
        .filter_map(|(i, d)| was.get(&same_path(d)).filter(|&&w| w != i + 1).map(|&w| (d.to_string_lossy().into_owned(), w, i + 1)))
        .collect();
    (out, moved)
}

/// An `openmw.cfg`'s `data=` lines (of the file in `dir`) in the order of `data`, each in one
/// of the places the file had one (a line as written: its quotes, its token); one of `off`
/// written `#data=`. A line naming a folder not in `data` stays where it is.
fn cfg_with_data(text: &str, data: &[PathBuf], off: &HashSet<String>, dir: &Path, tokens: &crate::cfg::Tokens) -> String {
    let want: HashMap<String, usize> = data.iter().enumerate().map(|(i, d)| (same_path(d), i)).collect();
    let mut out: Vec<String> = text.lines().map(str::to_string).collect();
    let key = |l: &str| -> Option<String> {
        let (k, v) = l.trim().split_once('=')?;
        if k.trim() != "data" {
            return None;
        }
        let p = crate::cfg::resolve(v.trim(), dir, tokens).ok()?;
        Some(same_path(&p)).filter(|k| want.contains_key(k))
    };
    let at: Vec<(usize, String)> = (0..out.len()).filter_map(|i| key(&out[i]).map(|k| (i, k))).collect();
    let mut lines: Vec<(String, String)> = at.iter().map(|(i, k)| (k.clone(), out[*i].trim().to_string())).collect();
    lines.sort_by_key(|(k, _)| want[k]);
    for ((slot, _), (k, line)) in at.iter().zip(lines) {
        out[*slot] = if off.contains(&k) { format!("#{line}") } else { line };
    }
    join(&out, text)
}

/// Writes `order` into an `openmw.cfg` (the one of the cfgs read that holds the `content=`
/// lines, `cfg`): its `content=` lines, `off` written as `#content=`; the file copied first.
/// The customizations TOML is not written: MOMW's configurator, run again, would put its
/// own order back.
///
/// # Errors
/// The cfg cannot be read, copied or written.
pub fn write_cfg(cfg: &Path, order: &[String], off: &[String], data: Option<(&[PathBuf], &[PathBuf])>, dry: bool, stamp: &str) -> Result<Written, String> {
    let text = latin1(&std::fs::read(cfg).map_err(|e| format!("{} could not be read: {e}", cfg.display()))?);
    let offs = lowered(off);
    let mut said = Vec::new();
    let mut now = cfg_with_order(&text, order, &offs);
    let mut what = format!("content= ({} on, {} off)", order.len() - order.iter().filter(|n| offs.contains(&n.to_lowercase())).count(), off.len());
    if let Some((data, data_off)) = data {
        let dir = cfg.parent().unwrap_or(Path::new("."));
        let doff: HashSet<String> = data_off.iter().map(|d| same_path(d)).collect();
        now = cfg_with_data(&now, data, &doff, dir, &crate::cfg::Tokens::for_cfg(cfg));
        what.push_str(&format!(" and data= ({} off)", data_off.len()));
    }
    put(cfg, &text, &now, &what, dry, stamp, &mut said)?;
    Ok(said)
}

/// The plugins of `was` to move to make it `now`: those outside a longest run they share in
/// the same order (a longest common subsequence, ignoring case), so the fewest move.
pub fn to_move(was: &[String], now: &[String]) -> Vec<String> {
    let a: Vec<String> = was.iter().map(|n| n.to_lowercase()).collect();
    let b: Vec<String> = now.iter().map(|n| n.to_lowercase()).collect();
    let (n, m) = (a.len(), b.len());
    let mut t = vec![0u32; (n + 1) * (m + 1)];
    let at = |i: usize, j: usize| i * (m + 1) + j;
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            t[at(i, j)] = if a[i] == b[j] { t[at(i + 1, j + 1)] + 1 } else { t[at(i + 1, j)].max(t[at(i, j + 1)]) };
        }
    }
    let mut keep: HashSet<String> = HashSet::new();
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if a[i] == b[j] {
            keep.insert(b[j].clone());
            i += 1;
            j += 1;
        } else if t[at(i + 1, j)] >= t[at(i, j + 1)] {
            i += 1;
        } else {
            j += 1;
        }
    }
    now.iter().filter(|x| !keep.contains(&x.to_lowercase())).cloned().collect()
}

/// A customizations TOML that makes MOMW's configurator build `order` (the plugins on, sorted)
/// where it built `was` (the cfg's `content=` as it is): the source TOML (`source`; none: a new
/// one) with its own plugins placed again where `order` has them, and the curated list's
/// plugins that move (the fewest, [`to_move`]) or are switched off (`off`) given a
/// `removeContent`, the moved ones inserted again in their new places. `(text, warnings)`.
///
/// # Errors
/// A source TOML that does not read, or that holds what the writer does not write.
pub fn customizations_toml(source: Option<&str>, was: &[String], order: &[String], off: &[String], list_name: Option<&str>) -> Result<(String, Vec<String>), String> {
    use crate::export::{generate_customizations_toml, TomlInput};
    let bad = |_| "the customizations TOML holds what the writer does not write".to_string();
    let (cust, blocks) = match source {
        Some(t) => (crate::inputs::customizations(t).map_err(bad)?, crate::plan::customization_blocks(t).map_err(bad)?),
        None => Default::default(),
    };
    let offs = lowered(off);
    let on: Vec<String> = order.iter().filter(|n| !offs.contains(&n.to_lowercase())).cloned().collect();
    let was_on: Vec<String> = was.iter().filter(|n| !offs.contains(&n.to_lowercase())).cloned().collect();
    let theirs: HashSet<String> = cust.subset.iter().map(|n| n.to_lowercase()).collect();
    let moved: HashSet<String> = to_move(&was_on, &on).iter().map(|n| n.to_lowercase()).collect();
    // The plugins placed by inserts: the TOML's own (on) and the list's that move.
    let subset: Vec<String> = on.iter().filter(|n| theirs.contains(&n.to_lowercase()) || moved.contains(&n.to_lowercase())).cloned().collect();
    let remove: Vec<String> = order
        .iter()
        .filter(|n| !theirs.contains(&n.to_lowercase()) && (moved.contains(&n.to_lowercase()) || offs.contains(&n.to_lowercase())))
        .cloned()
        .collect();
    let values: BTreeMap<String, String> = order.iter().map(|n| (n.clone(), n.clone())).collect();
    let replace_dest: HashSet<String> = cust.replace_dest.iter().cloned().collect();
    let user_data: Vec<String> = cust.data.iter().map(|d| d.value.clone()).collect();
    let toml = generate_customizations_toml(&TomlInput {
        original_blocks: &blocks,
        final_content_order: &on,
        subset: &subset,
        original_content_values: &values,
        data_result: None,
        raw_data_inserts: Some(&cust.data),
        replace_dest_names: &replace_dest,
        user_data_values: &user_data,
        list_name,
        remove_content: &remove,
        remove_data: &[],
        custom_anchors: &BTreeMap::new(),
        new_groundcover: &[],
    })
    .map_err(|_| "the order could not be written as a customizations TOML".to_string())?;
    Ok((toml.text, toml.warnings))
}

/// [`customizations_toml`] written to `to` (copied first when it is there), its line ends as
/// the source's. `dry`: not written.
///
/// # Errors
/// As [`customizations_toml`], or `to` cannot be copied or written.
#[allow(clippy::too_many_arguments)]
pub fn write_toml(from: Option<&Path>, to: &Path, was: &[String], order: &[String], off: &[String], list_name: Option<&str>, dry: bool, stamp: &str) -> Result<Written, String> {
    let source = match from.filter(|p| p.is_file()) {
        Some(p) => Some(String::from_utf8(std::fs::read(p).map_err(|e| format!("{} could not be read: {e}", p.display()))?).map_err(|_| format!("{} is not UTF-8", p.display()))?),
        None => None,
    };
    let (text, warnings) = customizations_toml(source.as_deref(), was, order, off, list_name)?;
    let crlf = source.as_deref().is_some_and(|t| t.contains("\r\n"));
    let text = if crlf { text.replace('\n', "\r\n") } else { text };
    let mut said: Written = warnings;
    let was_text = std::fs::read(to).map(|b| latin1(&b)).unwrap_or_default();
    let name = to.display();
    if was_text == latin1(text.as_bytes()) {
        said.push(format!("{name}: the customizations already as sorted"));
    } else if dry {
        said.push(format!("{name}: the customizations would be written"));
    } else {
        let copy = if to.is_file() { Some(backup(to, stamp)?) } else { None };
        std::fs::write(to, text.as_bytes()).map_err(|e| format!("{name} could not be written: {e}"))?;
        said.push(format!("{name}: the customizations written{}", copy.map(|b| format!("; as it was: {}", b.display())).unwrap_or_default()));
    }
    Ok(said)
}

/// Which of `files` (the cfgs read) holds the `content=` lines: the one to write.
/// An error when none does, or more than one (which file takes which line is OpenMW's to say).
pub fn cfg_holding_content(files: &[PathBuf]) -> Result<PathBuf, String> {
    let holding: Vec<&PathBuf> = files
        .iter()
        .filter(|f| std::fs::read(f).is_ok_and(|b| latin1(&b).lines().any(|l| cfg_content(l).is_some())))
        .collect();
    match holding.as_slice() {
        [one] => Ok((*one).clone()),
        [] => Err("no cfg read has content= lines".into()),
        many => Err(format!(
            "content= lines are in {} cfgs ({}): move them into one to write the order",
            many.len(),
            many.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", ")
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_game_files_are_rewritten_and_the_rest_of_the_ini_stays() {
        let ini = "[General]\r\nA=1\r\n[Game Files]\r\nGameFile0=Morrowind.esm\r\nGameFile1=B.esp\r\nGameFile2=A.esp\r\n; a note\r\n[Archives]\r\nArchive 0=Tribunal.bsa\r\n";
        let out = ini_with_order(ini, &["Morrowind.esm".into(), "A.esp".into(), "B.esp".into()]);
        assert_eq!(out, "[General]\r\nA=1\r\n[Game Files]\r\nGameFile0=Morrowind.esm\r\nGameFile1=A.esp\r\nGameFile2=B.esp\r\n; a note\r\n[Archives]\r\nArchive 0=Tribunal.bsa\r\n");
        assert!(ini_with_order("[General]\n", &["M.esm".into()]).ends_with("[Game Files]\nGameFile0=M.esm\n"));
    }

    #[test]
    fn dates_move_only_what_is_out_of_order() {
        let t = |s: u64| SystemTime::UNIX_EPOCH + Duration::from_secs(s);
        let dates: HashMap<&str, SystemTime> = [("M.esm", t(100)), ("A.esp", t(500)), ("B.esp", t(300)), ("C.esp", t(900))].into();
        let order: Vec<String> = ["M.esm", "A.esp", "B.esp", "C.esp"].iter().map(|s| s.to_string()).collect();
        let got = dates_for(&order, &|n| dates.get(n).copied());
        assert_eq!(got, vec![("B.esp".to_string(), t(300), t(560))]);
        // Two plugins with one date (a mod's files unpacked together; the game's own load
        // between them is the file system's): the later in the order a minute on, and the one
        // after it moved on only when that puts it behind; masters by themselves.
        let one: HashMap<&str, SystemTime> =
            [("M.esm", t(100)), ("T.esm", t(100)), ("A.esp", t(500)), ("B.esp", t(500)), ("C.esp", t(530)), ("D.esp", t(900))].into();
        let order: Vec<String> = ["M.esm", "T.esm", "B.esp", "A.esp", "C.esp", "D.esp"].iter().map(|s| s.to_string()).collect();
        let got = dates_for(&order, &|n| one.get(n).copied());
        assert_eq!(
            got,
            vec![
                ("T.esm".to_string(), t(100), t(160)),
                ("A.esp".to_string(), t(500), t(560)),
                ("C.esp".to_string(), t(530), t(620)),
            ]
        );
        // A plugin with no date (not found) is passed over, not counted.
        let got = dates_for(&order, &|n| (n != "A.esp").then(|| one.get(n).copied()).flatten());
        assert_eq!(got.iter().map(|g| g.0.as_str()).collect::<Vec<_>>(), ["T.esm"]);
    }

    #[test]
    fn an_mo2_list_keeps_what_it_does_not_order() {
        let txt = "# This file was automatically generated by Mod Organizer.\r\n*Morrowind.esm\r\n*B.esp\r\nOff.esp\r\n*A.esp\r\n";
        let order: Vec<String> = ["Morrowind.esm", "A.esp", "B.esp"].iter().map(|s| s.to_string()).collect();
        let none = HashSet::new();
        assert_eq!(plugins_txt_with_order(txt, &order, &none), "# This file was automatically generated by Mod Organizer.\r\n*Morrowind.esm\r\n*A.esp\r\nOff.esp\r\n*B.esp\r\n");
        assert_eq!(list_with_order("Morrowind.esm\nB.esp\nA.esp\n", &order), "Morrowind.esm\nA.esp\nB.esp\n");
        // Switched off: the `*` goes; in Morrowind's own form (only the ones on) the line goes.
        let b: HashSet<String> = ["b.esp".to_string()].into();
        assert_eq!(plugins_txt_with_order(txt, &order, &b), "# This file was automatically generated by Mod Organizer.\r\n*Morrowind.esm\r\n*A.esp\r\nOff.esp\r\nB.esp\r\n");
        assert_eq!(plugins_txt_with_order("Morrowind.esm\nB.esp\nA.esp\n", &order, &b), "Morrowind.esm\nA.esp\n");
    }

    #[test]
    fn an_openmw_cfg_gets_its_content_lines_in_order() {
        let cfg = "data=\"D\"\ncontent=Morrowind.esm\ncontent=B.esp\n# a note\ncontent = A.esp\nfallback-archive=Morrowind.bsa\n";
        let order: Vec<String> = ["Morrowind.esm", "A.esp", "B.esp"].iter().map(|s| s.to_string()).collect();
        let b: HashSet<String> = ["b.esp".to_string()].into();
        assert_eq!(cfg_with_order(cfg, &order, &b), "data=\"D\"\ncontent=Morrowind.esm\ncontent=A.esp\n# a note\n#content=B.esp\nfallback-archive=Morrowind.bsa\n");
    }

    #[test]
    fn data_folders_follow_their_plugins() {
        let d = std::env::temp_dir().join(format!("wg_plain_data_{}", std::process::id()));
        let (base, a, tex, b) = (d.join("base"), d.join("a"), d.join("tex"), d.join("b"));
        for (dir, plugin) in [(&base, Some("Morrowind.esm")), (&a, Some("A.esp")), (&tex, None), (&b, Some("B.esp"))] {
            std::fs::create_dir_all(dir).unwrap();
            if let Some(p) = plugin {
                std::fs::write(dir.join(p), b"").unwrap();
            }
        }
        let find = |n: &str| [&base, &a, &b].iter().map(|dir| dir.join(n)).find(|p| p.is_file());
        let order: Vec<String> = ["Morrowind.esm", "B.esp", "A.esp"].iter().map(|s| s.to_string()).collect();
        let data = vec![base.clone(), a.clone(), tex.clone(), b.clone()];
        let (now, moved) = data_order(&data, &order, &find);
        assert_eq!(now, [base.clone(), b.clone(), tex.clone(), a.clone()]);
        assert_eq!(moved.len(), 2);
        let cfg = d.join("openmw.cfg");
        let text = format!("data=\"{}\"\ndata={}\ndata=\"{}\"\ndata=\"{}\"\ncontent=Morrowind.esm\ncontent=A.esp\ncontent=B.esp\n", base.display(), a.display(), tex.display(), b.display());
        std::fs::write(&cfg, &text).unwrap();
        write_cfg(&cfg, &order, &[], Some((&now, std::slice::from_ref(&tex))), false, "s").unwrap();
        let got = std::fs::read_to_string(&cfg).unwrap();
        let want = format!("data=\"{}\"\ndata=\"{}\"\n#data=\"{}\"\ndata={}\ncontent=Morrowind.esm\ncontent=B.esp\ncontent=A.esp\n", base.display(), b.display(), tex.display(), a.display());
        assert_eq!(got, want);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn the_fewest_move() {
        let v = |x: &[&str]| x.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(to_move(&v(&["M", "A", "B", "C", "D"]), &v(&["M", "B", "C", "A", "D"])), ["A"]);
        assert!(to_move(&v(&["M", "A"]), &v(&["m", "a"])).is_empty());
    }

    #[test]
    fn a_customizations_toml_moves_list_plugins_and_places_its_own() {
        let was: Vec<String> = ["Morrowind.esm", "List1.esp", "List2.esp", "Mine.esp", "List3.esp"].iter().map(|s| s.to_string()).collect();
        let now: Vec<String> = ["Morrowind.esm", "List2.esp", "List1.esp", "List3.esp", "Mine.esp"].iter().map(|s| s.to_string()).collect();
        let src = "[[Customizations]]\nlistName = \"total-overhaul\"\n\n[[Customizations.insert]]\ninsert = \"Mine.esp\"\nafter = \"List2.esp\"\n";
        let (text, _) = customizations_toml(Some(src), &was, &now, &[], None).unwrap();
        assert!(text.contains("listName = 'total-overhaul'"), "{text}");
        assert!(text.contains("removeContent = [\n  'List1.esp',\n]"), "{text}");
        assert!(text.contains("insert = 'List1.esp'\nafter = 'List2.esp'"), "{text}");
        assert!(text.contains("insert = 'Mine.esp'\nafter = 'List3.esp'"), "{text}");
        assert!(!text.contains("  'Mine.esp',"), "Mine.esp is the TOML's own: inserted, not removed\n{text}");
    }

    #[test]
    fn a_morrowind_ini_install_is_sorted_and_written() {
        let d = std::env::temp_dir().join(format!("wg_plain_{}", std::process::id()));
        let data = d.join("Data Files");
        std::fs::create_dir_all(&data).unwrap();
        for n in ["Morrowind.esm", "A.esp", "B.esp"] {
            std::fs::write(data.join(n), b"").unwrap();
        }
        let ini = d.join("Morrowind.ini");
        std::fs::write(&ini, "[General]\r\n[Game Files]\r\nGameFile0=Morrowind.esm\r\nGameFile1=A.esp\r\nGameFile2=B.esp\r\n").unwrap();
        let rules = d.join("mlox_base.txt");
        std::fs::write(&rules, "[Order]\nB.esp\nA.esp\n").unwrap();
        let find = |n: &str| Some(data.join(n)).filter(|p| p.is_file());
        let order: Vec<String> = ["Morrowind.esm", "A.esp", "B.esp"].iter().map(|s| s.to_string()).collect();
        let got = sort(&order, &[rules.to_string_lossy().into_owned()], &find).unwrap();
        assert_eq!(got.order, ["Morrowind.esm", "B.esp", "A.esp"]);
        assert_eq!(got.moved.len(), 2);
        // Dates: A later than B, so B keeps its date only if before A; set them the wrong way.
        let t = |s: u64| SystemTime::UNIX_EPOCH + Duration::from_secs(s);
        for (n, at) in [("Morrowind.esm", 100), ("A.esp", 200), ("B.esp", 300)] {
            std::fs::File::options().write(true).open(data.join(n)).unwrap().set_modified(t(at)).unwrap();
        }
        let dry = write(&ini, &got.order, &[], None, &find, true, "s1").unwrap();
        assert!(dry.iter().any(|l| l.contains("would be written")));
        assert!(std::fs::read_to_string(&ini).unwrap().contains("GameFile1=A.esp"));
        write(&ini, &got.order, &[], None, &find, false, "s1").unwrap();
        assert!(std::fs::read_to_string(&ini).unwrap().contains("GameFile1=B.esp\r\nGameFile2=A.esp\r\n"));
        assert!(d.join("Morrowind.ini.bak-s1").is_file() && d.join("Morrowind.ini.dates-s1.json").is_file());
        let m = |n: &str| std::fs::metadata(data.join(n)).unwrap().modified().unwrap();
        assert!(m("B.esp") < m("A.esp"));
        // Put back: the dates before the write, the kept file set aside.
        assert!(restore_dates(&ini, true).unwrap()[0].contains("1 plugin date(s) would be put back"));
        restore_dates(&ini, false).unwrap();
        assert_eq!(m("B.esp"), t(300));
        assert!(!d.join("Morrowind.ini.dates-s1.json").exists() && restore_dates(&ini, false).is_err());
        // Switched off: out of [Game Files].
        write(&ini, &got.order, &["A.esp".into()], None, &find, false, "s2").unwrap();
        assert!(!std::fs::read_to_string(&ini).unwrap().contains("A.esp"));
        let _ = std::fs::remove_dir_all(&d);
    }
}
