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

/// Writes `order` into an `openmw.cfg` (the one of the cfgs read that holds the `content=`
/// lines, `cfg`): its `content=` lines, `off` written as `#content=`; the file copied first.
/// The customizations TOML is not written: MOMW's configurator, run again, would put its
/// own order back.
///
/// # Errors
/// The cfg cannot be read, copied or written.
pub fn write_cfg(cfg: &Path, order: &[String], off: &[String], dry: bool, stamp: &str) -> Result<Written, String> {
    let text = latin1(&std::fs::read(cfg).map_err(|e| format!("{} could not be read: {e}", cfg.display()))?);
    let offs = lowered(off);
    let mut said = Vec::new();
    let what = format!("content= ({} on, {} off)", order.len() - order.iter().filter(|n| offs.contains(&n.to_lowercase())).count(), off.len());
    put(cfg, &text, &cfg_with_order(&text, order, &offs), &what, dry, stamp, &mut said)?;
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
