//! Export, as `wraithguard_toolkit.write_plan` and the `wraithguard.configurator` package write
//! it: the `openmw.cfg` with the sorted `content=` and `data=` lines spliced in, and the
//! `customizations.toml` that momw-configurator rebuilds the cfg from.
//!
//! Everything here is **text in, text out** except [`write_plan`], which is the only function
//! that touches the disk: a timestamped `.bak` of what it is about to overwrite (the bytes, not
//! a re-encoding), then the new file.
//!
//! The pieces, bottom up:
//!
//! - the `data=` line helpers of `cfglines.py` ([`format_data_line`], [`normalize_data_path`],
//!   [`toml_value`], ...): quoting that matches the file's own style, and the lossy key two
//!   spellings of one folder share;
//! - [`insert_data_paths`]: new `data=` lines at their anchors, never twice;
//! - [`generate_customizations_toml`]: the hand-edited file's blocks preserved, this run's
//!   inserts as `insertBlock` runs on an anchor chosen to be *unique* (momw-configurator treats
//!   an ambiguous anchor as fatal), warnings for the rest;
//! - [`cfg_segments`] and [`splice_cfg_lines`]: which lines of the cfg are replaced;
//! - [`write_plan`]: all of it, to disk.
//!
//! **Bytes.** The cfg is written as `"\n".join(lines) + "\n"` in UTF-8, with no newline
//! translation, whatever the platform: what was read is what is written. A cfg whose bytes are
//! not UTF-8 is declined ([`decode_cfg`]), because Python carries such bytes through as
//! `surrogateescape` and a Rust `String` cannot. The TOML is different: Python writes it in text
//! mode, so on Windows its lines end in `\r\n`; [`LineEnding`] says which, and the caller passes
//! the platform's.
//!
//! What is not here: the simulated apply of momw-configurator (`apply.py`), which only prints a
//! preview and never blocks the export, the planner that builds the [`Plan`], and the console
//! report.
//!
//! The Python is the reference and `tests/fixtures/export_cases.json` pins both. Known and left:
//! `lower()` is Rust's `to_lowercase`, which differs from Python's for no character a path or
//! plugin name holds.
//!
//! Written from the documented behaviour of OpenMW's cfg and of momw-configurator, not from
//! their code.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs;
use std::io;
use std::path::Path;
use std::sync::OnceLock;

use crate::patterns::Unsupported;
use crate::pyre::{Flags, Regex};
use crate::rules::{is_space, splitlines};

/// The base game's own masters: they lead every load order and are never a user's mod.
pub const BASE_GAME_MASTERS: [&str; 3] = ["morrowind.esm", "tribunal.esm", "bloodmoon.esm"];

fn strip(s: &str) -> &str {
    s.trim_matches(is_space)
}

fn compiled(cell: &'static OnceLock<Regex>, pattern: &str) -> &'static Regex {
    cell.get_or_init(|| {
        Regex::new(pattern, Flags { ignore_case: true, multiline: false })
            .expect("a fixed expression compiles")
    })
}

/// The value of a line `^\s*<key>\s*=\s*(.+?)\s*$` (ignoring case), if it is one.
fn keyed_value(
    cell: &'static OnceLock<Regex>,
    pattern: &str,
    line: &str,
) -> Result<Option<String>, Unsupported> {
    let chars: Vec<char> = line.chars().collect();
    Ok(compiled(cell, pattern)
        .match_at(&chars, 0)?
        .and_then(|m| m.group(1))
        .map(|(a, b)| chars[a..b].iter().collect()))
}

const DATA_LINE: &str = r"^\s*data\s*=\s*(.+?)\s*$";
const CONTENT_LINE: &str = r"^\s*content\s*=\s*(.+?)\s*$";
const GROUNDCOVER_LINE: &str = r"^\s*groundcover\s*=\s*(.+?)\s*$";

fn data_value(line: &str) -> Result<Option<String>, Unsupported> {
    static RE: OnceLock<Regex> = OnceLock::new();
    keyed_value(&RE, DATA_LINE, line)
}

// ---------------------------------------------------------------- the data= line helpers

/// Escape a path for an OpenMW *quoted* `data=` value: `&` is OpenMW's escape character.
pub fn escape_cfg_value(value: &str) -> String {
    value.replace('&', "&&").replace('"', "&\"")
}

/// Undo [`escape_cfg_value`] on the inside of a quoted value: `&` takes the next character
/// literally (a trailing lone `&` is dropped).
pub fn unescape_cfg_value(inner: &str) -> String {
    let mut out = String::new();
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c == '&' {
            out.extend(chars.next());
        } else {
            out.push(c);
        }
    }
    out
}

/// A raw `data=` value (quoted or bare) as its real path: a double-quoted value is unescaped, a
/// bare one is literal, so only stray single quotes are stripped.
fn unquote_cfg_value(raw: &str) -> String {
    let chars: Vec<char> = raw.chars().collect();
    if chars.len() >= 2 && chars[0] == '"' && chars[chars.len() - 1] == '"' {
        return unescape_cfg_value(&chars[1..chars.len() - 1].iter().collect::<String>());
    }
    raw.trim_matches('\'').to_string()
}

fn is_quoted(value: &str) -> bool {
    let chars: Vec<char> = value.chars().collect();
    chars.len() >= 2 && chars[0] == '"' && chars[chars.len() - 1] == '"'
}

/// Whether the cfg predominantly quotes its `data=` paths (ties and no lines: no). A cfg in the
/// classic bare style reads quotes as part of the path, so a new line must match the file.
///
/// # Errors
/// [`Unsupported`] only if the matcher declines a line.
pub fn detect_data_quoting<S: AsRef<str>>(data_lines: &[S]) -> Result<bool, Unsupported> {
    let (mut quoted, mut bare) = (0usize, 0usize);
    for line in data_lines {
        let Some(value) = data_value(line.as_ref())? else { continue };
        if is_quoted(strip(&value)) {
            quoted += 1;
        } else {
            bare += 1;
        }
    }
    Ok(quoted > bare)
}

/// A `data=` line in the file's own quoting convention.
pub fn format_data_line(path_value: &str, quoted: bool) -> String {
    let value = unquote_cfg_value(strip(path_value));
    if quoted {
        format!("data=\"{}\"", escape_cfg_value(&value))
    } else {
        format!("data={value}")
    }
}

/// The first line containing `anchor`, ignoring case (momw-configurator's own substring match).
pub fn find_anchor_index<S: AsRef<str>>(lines: &[S], anchor: &str) -> Option<usize> {
    let anchor = anchor.to_lowercase();
    lines.iter().position(|l| l.as_ref().to_lowercase().contains(&anchor))
}

/// The bare path of a raw `data=` line, or `None` if it is not one.
///
/// # Errors
/// [`Unsupported`] only if the matcher declines the line.
pub fn extract_data_path_value(line: &str) -> Result<Option<String>, Unsupported> {
    Ok(data_value(line)?.map(|v| unquote_cfg_value(strip(&v))))
}

/// A `data=` path for duplicate detection only: lower case, forward slashes, no trailing slash.
/// Never for display or writing back.
pub fn normalize_data_path(value: &str) -> String {
    if value.is_empty() {
        return String::new();
    }
    unquote_cfg_value(strip(value)).replace('\\', "/").trim_end_matches('/').to_lowercase()
}

/// A string as a TOML value, preferring a literal string (`'...'`, so a Windows path needs no
/// doubled backslashes); a `'` or a newline needs a multi-line literal; `'''` inside falls back
/// to an escaped basic string.
pub fn toml_value(value: &str) -> String {
    let has_newline = value.contains('\n');
    if value.contains("'''") {
        let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
        return if has_newline { format!("\"\"\"{escaped}\"\"\"") } else { format!("\"{escaped}\"") };
    }
    if value.contains('\'') || has_newline {
        return format!("'''{value}'''");
    }
    format!("'{value}'")
}

/// Whether a `data=` value is the base game's own `Data Files` folder (any case).
pub fn is_base_data_path(value: &str) -> bool {
    normalize_data_path(value).rsplit('/').next() == Some("data files")
}

/// The value of a cfg line, unquoted if matched quotes surround it (`cfgLineValue` in
/// momw-configurator); `None` for a line with no `=`.
pub fn cfg_line_value(line: &str) -> Option<String> {
    let (_, rest) = line.split_once('=')?;
    let value = strip(rest);
    let chars: Vec<char> = value.chars().collect();
    if chars.len() >= 2 && chars[0] == chars[chars.len() - 1] && (chars[0] == '"' || chars[0] == '\'') {
        return Some(chars[1..chars.len() - 1].iter().collect());
    }
    Some(value.to_string())
}

/// Whether a `remove*` entry matches a cfg line, as momw-configurator's `shouldRemoveLine` does:
/// a path-like value (a slash) compares with the line's value, exactly or by a `/` suffix;
/// anything else is a whole-line substring.
pub fn configurator_remove_matches(val: &str, line: &str) -> bool {
    if val.contains(['/', '\\']) {
        let Some(lv) = cfg_line_value(line) else { return false };
        let lvn = strip(&lv.replace('\\', "/")).to_string();
        let vn = strip(&val.replace('\\', "/")).to_string();
        let c: Vec<char> = vn.chars().collect();
        let is_abs = vn.starts_with('/') || (c.len() >= 3 && c[1] == ':' && c[2] == '/');
        return lvn == vn || (!is_abs && lvn.ends_with(&format!("/{vn}")));
    }
    line.contains(val)
}

/// Whether the curated list accounts for a cfg plugin, `clean_` prefix and all.
pub fn curated_covers(
    name: &str,
    curated_lower: &HashSet<String>,
    needs_cleaning_lower: &HashSet<String>,
) -> bool {
    let lowered = name.to_lowercase();
    if curated_lower.contains(&lowered) {
        return true;
    }
    match lowered.strip_prefix("clean_") {
        Some(base) => curated_lower.contains(base) && needs_cleaning_lower.contains(base),
        None => false,
    }
}

/// The orphaned (unmanaged) `content=` plugins and `data=` values of a cfg, in cfg order.
///
/// # Errors
/// [`Unsupported`] only if the matcher declines a line.
pub fn orphan_cfg_entries<S: AsRef<str>>(
    content_names: &[S],
    data_lines: &[S],
    curated_lower: &HashSet<String>,
    needs_cleaning_lower: &HashSet<String>,
    declared_plugins_lower: &HashSet<String>,
    declared_data_norms: &HashSet<String>,
) -> Result<(Vec<String>, Vec<String>), Unsupported> {
    let plugins = content_names
        .iter()
        .map(AsRef::as_ref)
        .filter(|name| {
            !curated_covers(name, curated_lower, needs_cleaning_lower)
                && !declared_plugins_lower.contains(&name.to_lowercase())
                && !BASE_GAME_MASTERS.contains(&name.to_lowercase().as_str())
        })
        .map(str::to_string)
        .collect();
    let mut data = Vec::new();
    for line in data_lines {
        let Some(value) = extract_data_path_value(line.as_ref())? else { continue };
        let norm = normalize_data_path(&value);
        if !norm.is_empty() && !declared_data_norms.contains(&norm) && !is_base_data_path(&value) {
            data.push(value);
        }
    }
    Ok((plugins, data))
}

// ---------------------------------------------------------------- inserting data= paths

/// One pending `data=` insertion: its value and optional anchors (empty counts as none).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DataInsert {
    /// The path.
    pub value: String,
    /// Insert after the first existing line containing this.
    pub after: Option<String>,
    /// Insert before the first existing line containing this.
    pub before: Option<String>,
}

fn nonempty(s: &Option<String>) -> Option<&str> {
    s.as_deref().filter(|v| !v.is_empty())
}

/// A `data=` line of the rewritten cfg: its text, whether this run added it, and the value of
/// the insert that did.
pub type DataLine = (String, bool, Option<String>);

/// Insert new `data=` lines at their anchors (`insert_data_paths`). Anchors match the
/// **existing** lines only, so an insert cannot anchor off another new one; an insert whose
/// path is already present (or claimed by an earlier insert) is skipped; nothing existing is
/// removed or reordered. Returns the lines and the notes the Python prints.
///
/// # Errors
/// [`Unsupported`] only if the matcher declines a line.
pub fn insert_data_paths(
    data_lines: &[String],
    data_inserts: &[DataInsert],
) -> Result<(Vec<DataLine>, Vec<String>), Unsupported> {
    let mut existing: HashSet<String> = HashSet::new();
    for line in data_lines {
        existing.insert(normalize_data_path(&extract_data_path_value(line)?.unwrap_or_default()));
    }
    existing.remove("");
    let quoted = detect_data_quoting(data_lines)?;
    let mut notes = Vec::new();
    let mut anchor_map: BTreeMap<usize, Vec<(&'static str, String, String)>> = BTreeMap::new();
    let mut leftover: Vec<(String, String)> = Vec::new();
    for item in data_inserts {
        let norm = normalize_data_path(&item.value);
        if !norm.is_empty() && existing.contains(&norm) {
            notes.push(format!(
                "NOTE: '{}' already present in data= list -- skipping duplicate insert.",
                item.value
            ));
            continue;
        }
        if !norm.is_empty() {
            existing.insert(norm);
        }
        let new_line = format_data_line(&item.value, quoted);
        let (anchor, mode) = match (nonempty(&item.after), nonempty(&item.before)) {
            (Some(a), _) => (Some(a), "after"),
            (None, Some(b)) => (Some(b), "before"),
            (None, None) => (None, "after"),
        };
        let Some(anchor) = anchor else {
            notes.push(format!(
                "NOTE: '{}' has no after/before anchor -- appending at end of data= list.",
                item.value
            ));
            leftover.push((new_line, item.value.clone()));
            continue;
        };
        match find_anchor_index(data_lines, anchor) {
            None => {
                notes.push(format!(
                    "WARNING: anchor '{anchor}' not found among existing data= paths for '{}' -- appending at end instead.",
                    item.value
                ));
                leftover.push((new_line, item.value.clone()));
            }
            Some(idx) => anchor_map.entry(idx).or_default().push((mode, new_line, item.value.clone())),
        }
    }
    let mut result: Vec<DataLine> = Vec::new();
    for (i, line) in data_lines.iter().enumerate() {
        let here = anchor_map.get(&i).map(Vec::as_slice).unwrap_or_default();
        for (mode, new_line, val) in here {
            if *mode == "before" {
                result.push((new_line.clone(), true, Some(val.clone())));
            }
        }
        result.push((line.clone(), false, None));
        for (mode, new_line, val) in here {
            if *mode == "after" {
                result.push((new_line.clone(), true, Some(val.clone())));
            }
        }
    }
    for (new_line, val) in leftover {
        result.push((new_line, true, Some(val)));
    }
    Ok((result, notes))
}

// ---------------------------------------------------------------- customizations.toml

/// A hand-written `[[Customizations.replace]]` entry.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Replace {
    /// `source`, if the entry has one (an empty string counts as present).
    pub source: Option<String>,
    /// `dest`, likewise.
    pub dest: Option<String>,
}

/// A hand-written `[[Customizations.append]]` entry.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Append {
    /// `append`, if present.
    pub append: Option<String>,
    /// `appendBlock`, if present.
    pub append_block: Option<String>,
}

/// One `[[Customizations]]` block of the source TOML, as far as the export carries it through.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CustomBlock {
    /// `listName`.
    pub list_name: Option<String>,
    /// `removeData`.
    pub remove_data: Vec<String>,
    /// `removeContent`.
    pub remove_content: Vec<String>,
    /// `removeFallback`.
    pub remove_fallback: Vec<String>,
    /// `removeGroundcover`.
    pub remove_groundcover: Vec<String>,
    /// `replace` entries.
    pub replace: Vec<Replace>,
    /// `append` entries.
    pub append: Vec<Append>,
}

/// What [`generate_customizations_toml`] is given besides the order.
#[derive(Clone, Debug)]
pub struct TomlInput<'a> {
    /// The source TOML's blocks (empty: one is made).
    pub original_blocks: &'a [CustomBlock],
    /// The whole sorted `content=` list.
    pub final_content_order: &'a [String],
    /// The user's own plugins.
    pub subset: &'a [String],
    /// `{plugin: the value originally written}`.
    pub original_content_values: &'a BTreeMap<String, String>,
    /// The sorted data result, when data paths were sorted.
    pub data_result: Option<&'a [DataLine]>,
    /// The original data inserts, when they were not.
    pub raw_data_inserts: Option<&'a [DataInsert]>,
    /// Plugins that arrived as a `replace` dest.
    pub replace_dest_names: &'a HashSet<String>,
    /// Raw paths of every data insert of this run, before duplicate skipping.
    pub user_data_values: &'a [String],
    /// Overrides `listName`.
    pub list_name: Option<&'a str>,
    /// `removeContent` entries to emit.
    pub remove_content: &'a [String],
    /// `removeData` entries to emit.
    pub remove_data: &'a [String],
    /// `{plugin in lower case: (how, anchor)}`, for the "why is it here" comments.
    pub custom_anchors: &'a BTreeMap<String, (String, Option<String>)>,
    /// Grass the cfg does not declare yet.
    pub new_groundcover: &'a [String],
}

/// The customizations TOML and the warnings the Python prints while writing it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Toml {
    /// The document, newline-terminated, lines joined with `\n`.
    pub text: String,
    /// The ambiguity warnings (an anchor or a removal that matches several cfg lines).
    pub warnings: Vec<String>,
}

/// Group the subset's plugins into runs of consecutive positions (`_subset_runs`); a plugin
/// handled by a `replace` breaks a run.
fn subset_runs(
    order: &[String],
    subset_lower: &HashSet<String>,
    replace_dest: &HashSet<String>,
) -> Vec<(usize, usize)> {
    let mut runs = Vec::new();
    let mut start: Option<usize> = None;
    for (i, name) in order.iter().enumerate() {
        let mine = subset_lower.contains(&name.to_lowercase()) && !replace_dest.contains(name);
        if mine && start.is_none() {
            start = Some(i);
        } else if !mine {
            if let Some(s) = start.take() {
                runs.push((s, i));
            }
        }
    }
    if let Some(s) = start {
        runs.push((s, order.len()));
    }
    runs
}

fn anchor_is_unique(anchor: &str, haystack: &[String]) -> bool {
    haystack.iter().filter(|l| l.contains(anchor)).count() == 1
}

/// The least specific form of an anchor that matches exactly one line: the bare value, else the
/// whole line (`_widen_anchor`).
fn widen_anchor(value: &str, line: &str, haystack: &[String]) -> Option<String> {
    [value, strip(line)]
        .into_iter()
        .find(|c| !c.is_empty() && anchor_is_unique(c, haystack))
        .map(str::to_string)
}

/// A classified `data=` entry: the line, whether it is this run's, and its path.
struct Classified<'a> {
    entry: &'a DataLine,
    path: Option<String>,
    ours: bool,
}

fn anchor_val(entry: &DataLine) -> Result<String, Unsupported> {
    if let Some(v) = extract_data_path_value(&entry.0)?.filter(|v| !v.is_empty()) {
        return Ok(v);
    }
    let after = entry.0.split_once('=').map_or(entry.0.as_str(), |(_, r)| r);
    Ok(strip(after).trim_matches('"').to_string())
}

/// Choose an unambiguous anchor for one run of `data=` inserts (`_pick_data_anchor`): prefer the
/// line that follows the run, widened if needed; if neither works, the natural one anyway.
fn pick_data_anchor(
    next_frozen: Option<&Classified<'_>>,
    prev_frozen: Option<&Classified<'_>>,
    haystack: &[String],
) -> Result<(&'static str, Option<String>), Unsupported> {
    let candidates = [("before", next_frozen), ("after", prev_frozen)];
    for (mode, neighbour) in candidates {
        let Some(n) = neighbour else { continue };
        if let Some(w) = widen_anchor(&anchor_val(n.entry)?, &n.entry.0, haystack) {
            return Ok((mode, Some(w)));
        }
    }
    for (mode, neighbour) in candidates {
        if let Some(n) = neighbour {
            return Ok((mode, Some(anchor_val(n.entry)?)));
        }
    }
    Ok(("before", None))
}

/// Choose an unambiguous anchor for one run of content inserts (`_pick_anchor`): after the
/// plugin before the run, else before the plugin after it, each widened to its `content=` line;
/// the natural one if none is unique.
fn pick_anchor(
    order: &[String],
    start: usize,
    end: usize,
    haystack: &[String],
) -> (&'static str, Option<String>) {
    let before_run = (start > 0).then(|| order[start - 1].as_str());
    let after_run = order.get(end).map(String::as_str);
    if let Some(b) = before_run {
        if let Some(w) = widen_anchor(b, &format!("content={b}"), haystack) {
            return ("after", Some(w));
        }
    }
    if let Some(a) = after_run {
        if let Some(w) = widen_anchor(a, &format!("content={a}"), haystack) {
            return ("before", Some(w));
        }
    }
    if let Some(b) = before_run {
        return ("after", Some(b.to_string()));
    }
    if let Some(a) = after_run {
        return ("before", Some(a.to_string()));
    }
    ("after", None)
}

/// The comments above a hand-written `replace` (`_replace_notes`).
fn replace_notes(rep: &Replace, order: &[String]) -> Vec<String> {
    let dest = rep.dest.as_deref().map(strip).unwrap_or_default();
    let source = rep.source.as_deref().map(strip).unwrap_or_default();
    if dest.is_empty() || source.is_empty() {
        return Vec::new();
    }
    let mut notes = vec![
        "# Yours: carried through unchanged. This tool never writes a replace of".to_string(),
        "# its own -- only insert, append and remove blocks are regenerated.".to_string(),
    ];
    let lowered: Vec<String> = order.iter().map(|n| n.to_lowercase()).collect();
    let Some(position) = lowered.iter().position(|n| *n == dest.to_lowercase()) else {
        return notes;
    };
    notes.push(format!("# mlox sorts {dest} to position {} of {}", position + 1, order.len()));
    if position > 0 {
        notes.push(format!("#   after: {}", order[position - 1]));
    }
    notes.push("# A replace has no after/before -- it inherits the position of source --".to_string());
    notes.push("# so if that is not where it sits now, only you can move it: change this".to_string());
    notes.push("# block to an insert, or leave it as it is on purpose.".to_string());
    notes
}

fn dedupe_ci(vals: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    vals.into_iter().filter(|x| seen.insert(x.to_lowercase())).collect()
}

/// Generate the `momw-customizations.toml` the Configurator consumes
/// (`generate_customizations_toml`). Everything this tool does not own (`replace`, `append`,
/// the `remove*` keys, `listName`) is carried through; this run's data inserts come first (the
/// paths must exist before a plugin looks for them), then the content inserts, as
/// `insertBlock` runs on an anchor that matches exactly one cfg line.
///
/// # Errors
/// [`Unsupported`]: a per-plugin anchor comment with no anchor, or a matcher decline.
pub fn generate_customizations_toml(inp: &TomlInput<'_>) -> Result<Toml, Unsupported> {
    let subset_lower: HashSet<String> = inp.subset.iter().map(|s| s.to_lowercase()).collect();
    let default_name = inp.list_name.filter(|n| !n.is_empty()).unwrap_or("generated");
    let made = [CustomBlock { list_name: Some(default_name.to_string()), ..CustomBlock::default() }];
    let blocks: &[CustomBlock] = if inp.original_blocks.is_empty() { &made } else { inp.original_blocks };

    let mut haystack: Vec<String> = inp.final_content_order.iter().map(|n| format!("content={n}")).collect();
    let data_result = inp.data_result.filter(|d| !d.is_empty());
    if let Some(dr) = data_result {
        haystack.extend(dr.iter().map(|(l, _, _)| l.clone()));
    }

    let mut out: Vec<String> = Vec::new();
    let mut anchors: Vec<String> = Vec::new();
    let mut removes: Vec<String> = Vec::new();
    for (bi, block) in blocks.iter().enumerate() {
        out.push("[[Customizations]]".to_string());
        let name =
            inp.list_name.filter(|n| !n.is_empty()).or(block.list_name.as_deref().filter(|n| !n.is_empty()));
        if let Some(name) = name {
            out.push(format!("listName = {}", toml_value(name)));
        }
        let keyed: [(&str, &Vec<String>, &[String]); 4] = [
            ("removeData", &block.remove_data, inp.remove_data),
            ("removeContent", &block.remove_content, inp.remove_content),
            ("removeFallback", &block.remove_fallback, &[]),
            ("removeGroundcover", &block.remove_groundcover, &[]),
        ];
        for (key, own, extra) in keyed {
            let mut vals: Vec<String> = own.clone();
            if bi == 0 {
                vals.extend(extra.iter().cloned());
            }
            let merged = dedupe_ci(vals);
            if !merged.is_empty() {
                out.push(format!("{key} = ["));
                out.extend(merged.iter().map(|x| format!("  {},", toml_value(x))));
                out.push("]".to_string());
                removes.extend(merged);
            }
        }
        out.push(String::new());

        // 1) data inserts first, in the first block only
        let raw = inp.raw_data_inserts.filter(|r| !r.is_empty());
        if let (0, Some(dr)) = (bi, data_result) {
            let user_norms: HashSet<String> = inp
                .user_data_values
                .iter()
                .map(|v| normalize_data_path(v))
                .filter(|n| !n.is_empty())
                .collect();
            let mut classified: Vec<Classified<'_>> = Vec::new();
            for entry in dr {
                let (line, is_new, value) = entry;
                let path = match value.as_deref().filter(|v| !v.is_empty()) {
                    Some(v) => Some(v.to_string()),
                    None => extract_data_path_value(line)?,
                };
                let norm =
                    path.as_deref().filter(|p| !p.is_empty()).map(normalize_data_path).unwrap_or_default();
                let has_path = path.as_deref().is_some_and(|p| !p.is_empty());
                let ours = has_path && (*is_new || (!norm.is_empty() && user_norms.contains(&norm)));
                classified.push(Classified { entry, path, ours });
            }
            let n = classified.len();
            let mut i = 0;
            while i < n {
                if !classified[i].ours {
                    i += 1;
                    continue;
                }
                let mut j = i;
                while j < n && classified[j].ours {
                    j += 1;
                }
                let next_frozen = classified.get(j);
                let prev_frozen = if i > 0 { classified.get(i - 1) } else { None };
                let (mode, anchor) = pick_data_anchor(next_frozen, prev_frozen, &haystack)?;
                let values: Vec<&str> = classified[i..j]
                    .iter()
                    .filter_map(|c| c.path.as_deref())
                    .filter(|v| !v.is_empty())
                    .collect();
                if !values.is_empty() {
                    out.push("[[Customizations.insert]]".to_string());
                    if values.len() == 1 {
                        out.push(format!("insert = {}", toml_value(values[0])));
                    } else {
                        out.push(format!("insertBlock = {}", toml_value(&values.join("\n"))));
                    }
                    match anchor {
                        Some(a) => {
                            out.push(format!("{mode} = {}", toml_value(&a)));
                            anchors.push(a);
                        }
                        None => out.push(
                            "# WARNING: no existing data= line anywhere in the cfg to anchor to".to_string(),
                        ),
                    }
                    out.push(String::new());
                }
                i = j;
            }
        } else if let (0, Some(raw)) = (bi, raw) {
            for d in raw {
                out.push("[[Customizations.insert]]".to_string());
                out.push(format!("insert = {}", toml_value(&d.value)));
                if let Some(a) = nonempty(&d.after) {
                    out.push(format!("after = {}", toml_value(a)));
                    anchors.push(a.to_string());
                } else if let Some(b) = nonempty(&d.before) {
                    out.push(format!("before = {}", toml_value(b)));
                    anchors.push(b.to_string());
                } else {
                    out.push(
                        "# WARNING: no anchor for this path -- pass --sort-data-paths (or tick 'Sort data= paths too') to place it"
                            .to_string(),
                    );
                }
                out.push(String::new());
            }
        }

        // 2) content inserts, in the first block only
        let runs = if bi == 0 {
            subset_runs(inp.final_content_order, &subset_lower, inp.replace_dest_names)
        } else {
            Vec::new()
        };
        for (start, end) in runs {
            let names = &inp.final_content_order[start..end];
            let values: Vec<&str> = names
                .iter()
                .map(|n| inp.original_content_values.get(n).map_or(n.as_str(), String::as_str))
                .collect();
            for name in names {
                let Some((how, anch)) = inp.custom_anchors.get(&name.to_lowercase()) else { continue };
                match how.as_str() {
                    "after" | "before" => {
                        let anch = anch.as_deref().ok_or(Unsupported)?;
                        out.push(format!("# {name}: must load {how} {}", toml_value(anch)));
                    }
                    "nearstart" => out.push(format!("# {name}: mlox [NearStart] hint")),
                    "nearend" => out.push(format!("# {name}: mlox [NearEnd] hint")),
                    _ => out.push(format!("# {name}: no ordering constraint -- positional only")),
                }
            }
            let (mode, anchor) = pick_anchor(inp.final_content_order, start, end, &haystack);
            out.push("[[Customizations.insert]]".to_string());
            if values.len() == 1 {
                out.push(format!("insert = {}", toml_value(values[0])));
            } else {
                out.push(format!("insertBlock = {}", toml_value(&values.join("\n"))));
            }
            match anchor {
                None => {
                    out.push("# WARNING: this is the only content= plugin -- no anchor to write".to_string())
                }
                Some(a) => {
                    out.push(format!("{mode} = {}", toml_value(&a)));
                    anchors.push(a);
                }
            }
            out.push(String::new());
        }

        for rep in &block.replace {
            out.extend(replace_notes(rep, inp.final_content_order));
            out.push("[[Customizations.replace]]".to_string());
            if let Some(s) = &rep.source {
                out.push(format!("source = {}", toml_value(s)));
                anchors.push(s.clone());
            }
            if let Some(d) = &rep.dest {
                out.push(format!("dest = {}", toml_value(d)));
            }
            out.push(String::new());
        }

        if bi == 0 {
            for name in inp.new_groundcover {
                out.push("[[Customizations.append]]".to_string());
                out.push(format!("append = {}", toml_value(&format!("groundcover={name}"))));
                out.push(String::new());
            }
        }

        for ap in &block.append {
            out.push("[[Customizations.append]]".to_string());
            if let Some(a) = &ap.append {
                out.push(format!("append = {}", toml_value(a)));
            }
            if let Some(b) = &ap.append_block {
                out.push(format!("appendBlock = {}", toml_value(b)));
            }
            out.push(String::new());
        }
    }

    // Ambiguity checks (warn only). An anchor that matches several cfg lines is an error for
    // momw-configurator; a remove entry that does silently removes them all.
    let mut warnings = Vec::new();
    let more = |hits: &[&String]| if hits.len() > 4 { " ..." } else { "" };
    let lines_of = |hits: &[&String]| hits.iter().take(4).map(|s| s.as_str()).collect::<Vec<_>>().join("; ");
    let mut seen = HashSet::new();
    for a in anchors.iter().filter(|a| seen.insert((*a).clone())) {
        let hits: Vec<&String> = haystack.iter().filter(|l| l.contains(a.as_str())).collect();
        if hits.len() > 1 {
            warnings.push(format!(
                "WARNING: anchor '{a}' in the emitted TOML matches {} openmw.cfg lines -- momw-configurator errors on ambiguous matches. Colliding lines: {}{}",
                hits.len(),
                lines_of(&hits),
                more(&hits)
            ));
        }
    }
    let mut seen = HashSet::new();
    for r in removes.iter().filter(|r| seen.insert((*r).clone())) {
        let hits: Vec<&String> = haystack.iter().filter(|l| configurator_remove_matches(r, l)).collect();
        if hits.len() > 1 {
            warnings.push(format!(
                "WARNING: remove entry '{r}' matches {} openmw.cfg lines -- momw-configurator removes ALL of them, silently. Colliding lines: {}{}",
                hits.len(),
                lines_of(&hits),
                more(&hits)
            ));
        }
    }

    let mut text = out.join("\n").trim_end_matches(is_space).to_string();
    text.push('\n');
    Ok(Toml { text, warnings })
}

// ---------------------------------------------------------------- the cfg

/// An `openmw.cfg` read into its lines and the positions of its `content=` and `data=` lines
/// (`read_cfg`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CfgRead {
    /// The lines.
    pub lines: Vec<String>,
    /// Where the `content=` lines are.
    pub content_positions: Vec<usize>,
    /// Each `content=` line's value and the line.
    pub content_order: Vec<(String, String)>,
    /// Where the `data=` lines are.
    pub data_positions: Vec<usize>,
    /// The raw `data=` lines.
    pub data_order: Vec<String>,
}

/// Decode a cfg's bytes as Python reads them (`utf-8-sig`, text mode): a leading byte-order mark
/// is dropped and every line ending becomes `\n`.
///
/// # Errors
/// [`Unsupported`] if the bytes are not UTF-8 (Python keeps them as `surrogateescape`).
pub fn decode_cfg(bytes: &[u8]) -> Result<String, Unsupported> {
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    let text = String::from_utf8(bytes.to_vec()).map_err(|_| Unsupported)?;
    // Python reads in text mode: `\r\n` and a lone `\r` both become `\n`
    Ok(text.replace("\r\n", "\n").replace('\r', "\n"))
}

/// Read a cfg's text into its lines and `content=` / `data=` positions (`read_cfg`).
///
/// # Errors
/// [`Unsupported`] only if the matcher declines a line.
pub fn read_cfg_text(text: &str) -> Result<CfgRead, Unsupported> {
    static CONTENT: OnceLock<Regex> = OnceLock::new();
    let mut read =
        CfgRead { lines: splitlines(text).into_iter().map(str::to_string).collect(), ..CfgRead::default() };
    for (i, line) in read.lines.iter().enumerate() {
        if let Some(value) = keyed_value(&CONTENT, CONTENT_LINE, line)? {
            read.content_positions.push(i);
            read.content_order.push((value, line.clone()));
        } else if data_value(line)?.is_some() {
            read.data_positions.push(i);
            read.data_order.push(line.clone());
        }
    }
    Ok(read)
}

/// The plugins declared on `groundcover=` lines, in file order (`read_groundcover_names`).
///
/// # Errors
/// [`Unsupported`] only if the matcher declines a line.
pub fn read_groundcover_names<S: AsRef<str>>(lines: &[S]) -> Result<Vec<String>, Unsupported> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let mut found = Vec::new();
    for line in lines {
        let Some(value) = keyed_value(&RE, GROUNDCOVER_LINE, line.as_ref())? else { continue };
        let name = basename_if_plugin(&value)
            .unwrap_or_else(|| strip(&value).trim_matches('"').trim_matches('\'').to_string());
        if !name.is_empty() {
            found.push(name);
        }
    }
    Ok(found)
}

/// The bare filename if the value names a plugin, else `None` (`basename_if_plugin`).
pub fn basename_if_plugin(value: &str) -> Option<String> {
    let v = strip(value).trim_matches('"').trim_matches('\'').replace('\\', "/");
    let name = v.rsplit('/').next().unwrap_or_default();
    let lower = name.to_lowercase();
    [".esp", ".esm", ".omwaddon", ".omwgame", ".omwscripts"]
        .iter()
        .any(|e| lower.ends_with(e))
        .then(|| name.to_string())
}

/// A block of the cfg's lines (`positions`) and what replaces it.
pub type Segment = (Vec<usize>, Vec<String>);

/// The cfg lines with the rebuilt `content=` / `data=` segments spliced in
/// (`splice_cfg_lines`): a segment's block is replaced at the position of its first line, other
/// lines are untouched, and a segment with no positions goes on the end.
pub fn splice_cfg_lines(lines: &[String], segments: &[Segment]) -> Vec<String> {
    let mut replace_at: HashMap<usize, &Vec<String>> = HashMap::new();
    let mut skip: HashSet<usize> = HashSet::new();
    let mut trailing: Vec<String> = Vec::new();
    for (positions, new_lines) in segments {
        let Some(&first) = positions.first() else {
            trailing.extend(new_lines.iter().cloned());
            continue;
        };
        replace_at.insert(first, new_lines);
        skip.extend(positions.iter().copied());
    }
    let mut out = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if let Some(new_lines) = replace_at.get(&i) {
            out.extend(new_lines.iter().cloned());
        }
        if skip.contains(&i) {
            continue;
        }
        out.push(line.clone());
    }
    out.extend(trailing);
    out
}

/// The part of a computed plan that export reads.
#[derive(Clone, Debug, Default)]
pub struct Plan {
    /// The cfg's lines as read.
    pub lines: Vec<String>,
    /// Where its `content=` lines are.
    pub content_positions: Vec<usize>,
    /// Where its `data=` lines are.
    pub data_positions: Vec<usize>,
    /// The sorted plugins.
    pub final_order: Vec<String>,
    /// The sorted data lines (`None`: data paths were not sorted).
    pub data_result: Option<Vec<DataLine>>,
    /// Grass declared this run that the cfg lacks.
    pub new_groundcover: Vec<String>,
    /// The user's own plugins.
    pub subset: Vec<String>,
    /// Lower-cased plugins the curated list owns (empty: unknown).
    pub curated_set: Vec<String>,
    /// The curated base order.
    pub base_order_names: Vec<String>,
    /// The cfg's raw `data=` lines.
    pub data_order: Vec<String>,
    /// This run's data inserts.
    pub data_inserts: Vec<DataInsert>,
    /// The source TOML's data inserts, unprocessed.
    pub raw_toml_data_inserts: Vec<DataInsert>,
    /// Data paths in the cfg that nothing declares.
    pub orphan_data_paths: Vec<String>,
    /// The source TOML's blocks.
    pub original_toml_blocks: Vec<CustomBlock>,
    /// `{plugin: the value originally written}`.
    pub original_content_values: BTreeMap<String, String>,
    /// Plugins that arrived as a `replace` dest.
    pub replace_dest_names: HashSet<String>,
    /// `{plugin in lower case: (how, anchor)}`.
    pub custom_anchors: BTreeMap<String, (String, Option<String>)>,
}

/// The cfg edits a plan makes (`cfg_segments`).
#[derive(Clone, Debug)]
pub struct CfgEdits {
    /// The cfg's lines with any new `groundcover=` declarations appended.
    pub lines: Vec<String>,
    /// The segments to splice in.
    pub segments: Vec<Segment>,
    /// The data result in the order being written.
    pub data_result: Option<Vec<DataLine>>,
}

/// The cfg edits a plan makes (`cfg_segments`): the lines with any new `groundcover=` declarations
/// appended, the segments to splice in, and the data result in the order being written.
///
/// # Errors
/// [`Unsupported`] only if the matcher declines a line.
pub fn cfg_segments(
    plan: &Plan,
    final_order: Option<&[String]>,
    data_order: Option<&[String]>,
) -> Result<CfgEdits, Unsupported> {
    let final_order = final_order.unwrap_or(&plan.final_order);
    let mut plan_lines = plan.lines.clone();
    if !plan.new_groundcover.is_empty() {
        let existing: HashSet<String> =
            read_groundcover_names(&plan_lines)?.into_iter().map(|n| n.to_lowercase()).collect();
        let additions: Vec<String> = plan
            .new_groundcover
            .iter()
            .filter(|n| !existing.contains(&n.to_lowercase()))
            .map(|n| format!("groundcover={n}"))
            .collect();
        plan_lines.extend(additions);
    }
    let mut data_result = plan.data_result.clone();
    if let (Some(order), Some(result)) = (data_order, &data_result) {
        let lookup: HashMap<&String, (bool, &Option<String>)> =
            result.iter().map(|(line, is_new, value)| (line, (*is_new, value))).collect();
        data_result = Some(
            order
                .iter()
                .map(|line| match lookup.get(line) {
                    Some((is_new, value)) => (line.clone(), *is_new, (*value).clone()),
                    None => (line.clone(), false, None),
                })
                .collect(),
        );
    }
    let mut segments: Vec<Segment> = Vec::new();
    if !final_order.is_empty() {
        segments.push((
            plan.content_positions.clone(),
            final_order.iter().map(|n| format!("content={n}")).collect(),
        ));
    }
    if let Some(result) = &data_result {
        segments.push((plan.data_positions.clone(), result.iter().map(|(l, _, _)| l.clone()).collect()));
    }
    Ok(CfgEdits { lines: plan_lines, segments, data_result })
}

/// The openmw.cfg a plan would write, without writing it (`render_cfg_lines`).
///
/// # Errors
/// [`Unsupported`] only if the matcher declines a line.
pub fn render_cfg_lines(
    plan: &Plan,
    final_order: Option<&[String]>,
    data_order: Option<&[String]>,
) -> Result<Vec<String>, Unsupported> {
    let edits = cfg_segments(plan, final_order, data_order)?;
    Ok(splice_cfg_lines(&edits.lines, &edits.segments))
}

/// The bytes a cfg is written as: the lines joined and terminated with `\n`, UTF-8, no newline
/// translation.
pub fn cfg_bytes(lines: &[String]) -> Vec<u8> {
    let mut text = lines.join("\n");
    text.push('\n');
    text.into_bytes()
}

// ---------------------------------------------------------------- removals

/// The opted-out plugins that need a durable `removeContent`: those the curated list owns (so
/// the Configurator would bring them back). With no curated set the base order stands in for it.
pub fn plugins_needing_removal(
    disabled: &[String],
    curated_set: &[String],
    base_order_names: &[String],
) -> Vec<String> {
    let curated: HashSet<String> = curated_set.iter().map(|n| n.to_lowercase()).collect();
    let base: HashSet<String> = base_order_names.iter().map(|n| n.to_lowercase()).collect();
    let owned = if curated.is_empty() { base } else { curated };
    disabled
        .iter()
        .filter(|p| owned.contains(&p.to_lowercase()))
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// The opted-out data paths that need a durable `removeData`: those in the cfg that this run
/// does not insert itself. Sorted and de-duplicated.
///
/// # Errors
/// [`Unsupported`] only if the matcher declines a line.
pub fn data_paths_needing_removal(
    disabled_data: &[String],
    our_values: &HashSet<String>,
    base_data_norms: &HashSet<String>,
) -> Result<Vec<String>, Unsupported> {
    let mut removals = BTreeSet::new();
    for line in disabled_data {
        let value = extract_data_path_value(line)?.filter(|v| !v.is_empty()).unwrap_or_else(|| line.clone());
        let norm = normalize_data_path(&value);
        if base_data_norms.contains(&norm) && !our_values.contains(&norm) {
            removals.insert(value);
        }
    }
    Ok(removals.into_iter().collect())
}

// ---------------------------------------------------------------- writing

/// How the TOML's lines end on disk: Python writes it in text mode, so on Windows `\r\n`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineEnding {
    /// `\n`.
    Lf,
    /// `\r\n`.
    Crlf,
}

impl LineEnding {
    /// What Python's text mode gives on this platform.
    pub fn platform() -> LineEnding {
        if cfg!(windows) {
            LineEnding::Crlf
        } else {
            LineEnding::Lf
        }
    }
}

/// What to export and where (the arguments `write_plan` reads).
#[derive(Clone, Debug)]
pub struct Targets<'a> {
    /// Patch this `openmw.cfg` (`--write-cfg`).
    pub write_cfg: Option<&'a Path>,
    /// Write this `customizations.toml` (`--emit-toml`).
    pub emit_toml: Option<&'a Path>,
    /// Preview only.
    pub dry_run: bool,
    /// Skip the `.bak` copy.
    pub no_backup: bool,
    /// Data paths were sorted (`--sort-data-paths`).
    pub sort_data_paths: bool,
    /// Overrides `listName`.
    pub list_name: Option<&'a str>,
    /// The `.bak` stamp, `YYYYMMDD-HHMMSS` in the user's local time.
    pub stamp: &'a str,
    /// The TOML's line endings.
    pub toml_line_ending: LineEnding,
}

/// What [`write_plan`] did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Written {
    /// The openmw.cfg was written.
    pub wrote_cfg: bool,
    /// The customizations.toml was written.
    pub wrote_toml: bool,
    /// The TOML text (also when only previewed).
    pub toml_text: Option<String>,
    /// The warnings and notes the Python prints while writing.
    pub messages: Vec<String>,
}

/// Why an export did not happen.
#[derive(Debug)]
pub enum ExportError {
    /// See the module.
    Unsupported,
    /// A file could not be read or written (Python raises it too).
    Io(io::Error),
}

impl From<Unsupported> for ExportError {
    fn from(_: Unsupported) -> Self {
        ExportError::Unsupported
    }
}

impl From<io::Error> for ExportError {
    fn from(e: io::Error) -> Self {
        ExportError::Io(e)
    }
}

/// A timestamped `.bak` copy of a file about to be overwritten (`backup_file`): the bytes, not a
/// re-encoding. Nothing if there is nothing to back up. A name Python would suffix differently
/// (one ending in a dot) is declined.
fn backup_file(path: &Path, no_backup: bool, stamp: &str) -> Result<(), ExportError> {
    if no_backup || !path.exists() {
        return Ok(());
    }
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    if name.ends_with('.') {
        return Err(ExportError::Unsupported);
    }
    fs::write(path.with_file_name(format!("{name}.bak-{stamp}")), fs::read(path)?)?;
    Ok(())
}

fn lines_of(text: &str) -> Vec<String> {
    splitlines(text).into_iter().map(str::to_string).collect()
}

/// Write out a computed plan (`write_plan`): the cfg with its segments spliced in and/or the
/// customizations TOML, each after a `.bak` of what it replaces. `final_order` and `data_order`
/// override the plan's (a GUI that let the person reorder), `disabled_plugins` and
/// `disabled_data` are items the user opted out of.
///
/// # Errors
/// [`ExportError`].
pub fn write_plan(
    targets: &Targets<'_>,
    plan: &Plan,
    final_order: Option<&[String]>,
    data_order: Option<&[String]>,
    disabled_plugins: &[String],
    disabled_data: &[String],
) -> Result<Written, ExportError> {
    let final_order_v: Vec<String> = final_order.map_or_else(|| plan.final_order.clone(), <[String]>::to_vec);
    let remove_content = plugins_needing_removal(disabled_plugins, &plan.curated_set, &plan.base_order_names);
    let mut base_data_norms = HashSet::new();
    for line in &plan.data_order {
        if let Some(value) = extract_data_path_value(line)?.filter(|v| !v.is_empty()) {
            base_data_norms.insert(normalize_data_path(&value));
        }
    }
    base_data_norms.remove("");
    let mut ours: HashSet<String> = plan
        .data_inserts
        .iter()
        .chain(&plan.raw_toml_data_inserts)
        .filter(|e| !e.value.is_empty())
        .map(|e| normalize_data_path(&e.value))
        .collect();
    ours.remove("");
    let remove_data = data_paths_needing_removal(disabled_data, &ours, &base_data_norms)?;

    let CfgEdits { lines: plan_lines, segments, data_result } = cfg_segments(plan, final_order, data_order)?;
    let mut messages = Vec::new();
    let mut wrote = Written { wrote_cfg: false, wrote_toml: false, toml_text: None, messages: Vec::new() };

    if let Some(path) = targets.write_cfg {
        if !targets.dry_run {
            let new_lines = splice_cfg_lines(&plan_lines, &segments);
            backup_file(path, targets.no_backup, targets.stamp)?;
            fs::write(path, cfg_bytes(&new_lines))?;
            wrote.wrote_cfg = true;
        }
    }

    if let Some(path) = targets.emit_toml {
        let orphan_data = &plan.orphan_data_paths;
        let declared_vals: Vec<String> = plan.data_inserts.iter().map(|d| d.value.clone()).collect();
        let raw_inserts = &plan.raw_toml_data_inserts;
        let (emit_result, emit_raw, emit_user_vals): (
            Option<Vec<DataLine>>,
            Option<Vec<DataInsert>>,
            Vec<String>,
        ) = if targets.sort_data_paths && data_result.is_some() {
            let mut vals = declared_vals.clone();
            vals.extend(orphan_data.iter().cloned());
            (data_result.clone(), None, vals)
        } else if !orphan_data.is_empty() {
            let src_lines: Vec<String> = match data_order {
                Some(order) => order.to_vec(),
                None => plan.data_order.clone(),
            };
            let mut src_norms = HashSet::new();
            for line in &src_lines {
                src_norms.insert(normalize_data_path(&extract_data_path_value(line)?.unwrap_or_default()));
            }
            src_norms.remove("");
            let brand_new: Vec<DataInsert> = raw_inserts
                .iter()
                .filter(|d| !src_norms.contains(&normalize_data_path(&d.value)))
                .cloned()
                .collect();
            let (result, notes) = insert_data_paths(&src_lines, &brand_new)?;
            messages.extend(notes);
            let mut vals = orphan_data.clone();
            vals.extend(raw_inserts.iter().map(|d| d.value.clone()));
            (Some(result), None, vals)
        } else if targets.sort_data_paths {
            (data_result.clone(), None, declared_vals)
        } else {
            (None, Some(raw_inserts.clone()), declared_vals)
        };
        let toml = generate_customizations_toml(&TomlInput {
            original_blocks: &plan.original_toml_blocks,
            final_content_order: &final_order_v,
            subset: &plan.subset,
            original_content_values: &plan.original_content_values,
            data_result: emit_result.as_deref(),
            raw_data_inserts: emit_raw.as_deref(),
            replace_dest_names: &plan.replace_dest_names,
            user_data_values: &emit_user_vals,
            list_name: targets.list_name,
            remove_content: &remove_content,
            remove_data: &remove_data,
            custom_anchors: &plan.custom_anchors,
            new_groundcover: &plan.new_groundcover,
        })?;
        messages.extend(toml.warnings.iter().cloned());
        if !targets.dry_run {
            backup_file(path, targets.no_backup, targets.stamp)?;
            let bytes = match targets.toml_line_ending {
                LineEnding::Lf => toml.text.clone(),
                LineEnding::Crlf => toml.text.replace('\n', "\r\n"),
            };
            fs::write(path, bytes)?;
            wrote.wrote_toml = true;
        }
        wrote.toml_text = Some(toml.text);
    }
    wrote.messages = messages;
    Ok(wrote)
}

/// The lines of a TOML or cfg text, as the Python splits them.
pub fn split_lines(text: &str) -> Vec<String> {
    lines_of(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(list: &[&str]) -> Vec<String> {
        list.iter().map(|x| (*x).to_string()).collect()
    }

    #[test]
    fn a_quoted_path_escapes_ampersands_and_quotes() {
        assert_eq!(format_data_line("C:\\Mods\\A & B", true), "data=\"C:\\Mods\\A && B\"");
        assert_eq!(format_data_line("\"C:\\x&&y\"", false), "data=C:\\x&y");
        assert_eq!(unescape_cfg_value("a&&b&\"c&"), "a&b\"c");
    }

    #[test]
    fn two_spellings_of_a_folder_share_a_key() {
        assert_eq!(normalize_data_path("\"C:\\Games\\Mods\\\""), "c:/games/mods");
        assert_eq!(normalize_data_path(""), "");
        assert!(is_base_data_path("D:\\Morrowind\\Data Files\\"));
    }

    #[test]
    fn toml_values_prefer_a_literal_string() {
        assert_eq!(toml_value("C:\\a\\b"), "'C:\\a\\b'");
        assert_eq!(toml_value("it's"), "'''it's'''");
        assert_eq!(toml_value("a\nb"), "'''a\nb'''");
        assert_eq!(toml_value("a'''b"), "\"a'''b\"");
        assert_eq!(toml_value("a'''b\n\"c\\"), "\"\"\"a'''b\n\\\"c\\\\\"\"\"");
    }

    #[test]
    fn splicing_replaces_a_block_at_its_first_line() {
        let lines = s(&["a", "content=x", "b", "content=y", "c"]);
        let seg = vec![(vec![1, 3], s(&["content=1", "content=2", "content=3"]))];
        assert_eq!(
            splice_cfg_lines(&lines, &seg),
            s(&["a", "content=1", "content=2", "content=3", "b", "c"])
        );
        let none = vec![(vec![], s(&["data=z"]))];
        assert_eq!(splice_cfg_lines(&s(&["a"]), &none), s(&["a", "data=z"]));
    }

    #[test]
    fn a_new_path_goes_after_its_anchor_and_never_twice() {
        let lines = s(&["data=C:\\A", "data=C:\\B"]);
        let inserts = vec![
            DataInsert { value: "C:\\New".into(), after: Some("c:\\a".into()), before: None },
            DataInsert { value: "c:/b/".into(), ..DataInsert::default() },
            DataInsert { value: "C:\\Loose".into(), ..DataInsert::default() },
        ];
        let (got, notes) = insert_data_paths(&lines, &inserts).unwrap();
        let texts: Vec<&str> = got.iter().map(|g| g.0.as_str()).collect();
        assert_eq!(texts, ["data=C:\\A", "data=C:\\New", "data=C:\\B", "data=C:\\Loose"]);
        assert_eq!(notes.len(), 2);
    }

    #[test]
    fn an_ambiguous_anchor_is_widened_to_its_whole_line() {
        let order = s(&["Wares.esp", "Better Wares.esp", "Mine.esp"]);
        let hay: Vec<String> = order.iter().map(|n| format!("content={n}")).collect();
        assert_eq!(pick_anchor(&order, 1, 3, &hay), ("after", Some("content=Wares.esp".to_string())));
    }

    #[test]
    fn a_cfg_is_written_as_its_lines_joined_and_terminated() {
        assert_eq!(cfg_bytes(&s(&["a", "b"])), b"a\nb\n");
        assert_eq!(decode_cfg(b"\xEF\xBB\xBFa\n").unwrap(), "a\n");
        assert_eq!(decode_cfg(b"a\r\nb\rc\n").unwrap(), "a\nb\nc\n");
        assert!(decode_cfg(b"caf\xe9").is_err());
    }
}
