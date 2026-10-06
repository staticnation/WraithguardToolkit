//! Teal's diagnostics as the toolkit's findings, and where a setup's Teal pieces are (was
//! the mapping half of `wraithguard/lua/openmw_api.py`, which stays as the fallback and
//! the reference this is held to). The module docstring there lists the codes.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;

use super::analysis::Finding;
use super::lexer::py_repr;
use super::omwscripts::{py_splitlines, py_strip};
use super::scan::read_text;

static INVALID_KEY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^invalid key '([^']*)' in .*?\b(?:type|of type) (?:record )?([A-Za-z0-9_]+__[A-Za-z0-9_]+)\b")
        .expect("valid")
});
static ARITY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"given (\d+), expects (?:at least \d+ and at most (\d+)|(\d+) or (\d+)|(?:at most )?(\d+))")
        .expect("valid")
});
static UNKNOWN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^unknown variable: ([A-Za-z_][A-Za-z0-9_]*)").expect("valid"));
static MODULE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"module not found: '([^']+)'").expect("valid"));
static TLCONFIG_FIELD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?s)\b(include_dir|source_dir)\s*=\s*(\{[^}]*\}|"[^"]*"|'[^']*')"#).expect("valid")
});
static LUA_STRING: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#""([^"]*)"|'([^']*)'"#).expect("valid"));
static TLCONFIG_GLOBALS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"\bglobal_env_def\s*=\s*("[^"]*"|'[^']*')"#).expect("valid"));

/// Python's `difflib.SequenceMatcher(None, a, b).ratio()` (no junk: the names are far
/// shorter than its 200-element autojunk threshold), block for block.
pub fn ratio(a: &[char], b: &[char]) -> f64 {
    let total = a.len() + b.len();
    if total == 0 {
        return 1.0;
    }
    let mut b2j: HashMap<char, Vec<usize>> = HashMap::new();
    for (j, c) in b.iter().enumerate() {
        b2j.entry(*c).or_default().push(j);
    }
    let longest = |alo: usize, ahi: usize, blo: usize, bhi: usize| -> (usize, usize, usize) {
        let (mut besti, mut bestj, mut bestsize) = (alo, blo, 0);
        let mut j2len: HashMap<usize, usize> = HashMap::new();
        for (i, c) in a.iter().enumerate().take(ahi).skip(alo) {
            let mut next: HashMap<usize, usize> = HashMap::new();
            if let Some(js) = b2j.get(c) {
                for &j in js {
                    if j < blo {
                        continue;
                    }
                    if j >= bhi {
                        break;
                    }
                    let k = j.checked_sub(1).and_then(|p| j2len.get(&p)).copied().unwrap_or(0) + 1;
                    next.insert(j, k);
                    if k > bestsize {
                        (besti, bestj, bestsize) = (i + 1 - k, j + 1 - k, k);
                    }
                }
            }
            j2len = next;
        }
        (besti, bestj, bestsize)
    };
    let mut matches = 0;
    let mut queue = vec![(0, a.len(), 0, b.len())];
    while let Some((alo, ahi, blo, bhi)) = queue.pop() {
        let (i, j, k) = longest(alo, ahi, blo, bhi);
        if k > 0 {
            matches += k;
            if alo < i && blo < j {
                queue.push((alo, i, blo, j));
            }
            if i + k < ahi && j + k < bhi {
                queue.push((i + k, ahi, j + k, bhi));
            }
        }
    }
    2.0 * matches as f64 / total as f64
}

/// Python's `difflib.get_close_matches(word, possibilities, n, cutoff)`.
pub fn close_matches<'a>(word: &str, possibilities: &'a [String], n: usize, cutoff: f64) -> Vec<&'a str> {
    let w: Vec<char> = word.chars().collect();
    let mut scored: Vec<(f64, &str)> = possibilities
        .iter()
        .filter_map(|x| {
            let r = ratio(&x.chars().collect::<Vec<_>>(), &w);
            (r >= cutoff).then_some((r, x.as_str()))
        })
        .collect();
    scored.sort_by(|p, q| q.0.total_cmp(&p.0).then_with(|| q.1.cmp(p.1)));
    scored.into_iter().take(n).map(|(_, x)| x).collect()
}

/// A leading verb taken off: `getX` and `setX` are two functions, not a typo of each other.
fn strip_verb(s: &str) -> &str {
    for verb in ["get", "set", "is", "has", "add", "remove", "can", "to"] {
        if let Some(rest) = s.strip_prefix(verb)
            && rest.starts_with(|c: char| c.is_ascii_uppercase() || c == '_')
        {
            return rest;
        }
    }
    s
}

/// The documented name `key` is likely a typo of, if any.
pub fn typo_of<'a>(key: &str, names: &'a [String]) -> Option<&'a str> {
    let stem = strip_verb(key);
    // Skipped: the same word under another verb (only when `key` has a verb at all).
    close_matches(key, names, 3, 0.75).into_iter().find(|cand| stem == key || strip_verb(cand) != stem)
}

/// A declared type's Teal name for a message: `nearby__nearby` is `openmw.nearby`,
/// `core__GameObject` is `openmw.core GameObject`, `I_AI__AI` is `I.AI`.
pub fn type_label(tid: &str) -> String {
    let (key, name) = tid.split_once("__").unwrap_or((tid, ""));
    let (package, short) = if let Some(k) = key.strip_prefix("I_") {
        (format!("I.{k}"), k)
    } else if let Some(k) = key.strip_prefix("aux_") {
        (format!("openmw_aux.{k}"), k)
    } else {
        (format!("openmw.{key}"), key)
    };
    if name.to_lowercase() == short.to_lowercase() { package } else { format!("{package} {name}") }
}

/// Whether an arity message is about too many arguments (not too few).
fn too_many(message: &str) -> bool {
    let Some(m) = ARITY.captures(message) else { return false };
    let given: u64 = m[1].parse().unwrap_or(0);
    let most = [2, 4, 5].iter().find_map(|&g| m.get(g)).and_then(|g| g.as_str().parse::<u64>().ok());
    most.is_some_and(|most| given > most)
}

/// One Teal diagnostic: kind, severity, line, message.
pub struct DiagRef<'a> {
    pub kind: &'a str,
    pub severity: &'a str,
    pub line: usize,
    pub message: &'a str,
}

/// Teal's diagnostics for one file as findings, in line order. `teal`: the file is Teal
/// (every type error counts); `bound`: the names the script binds; `members`: each
/// declared type's member names; `stubs`: the declarations are stubs (no install).
pub fn findings_for(
    diags: &[DiagRef<'_>],
    teal: bool,
    bound: &HashSet<String>,
    members: &HashMap<String, Vec<String>>,
    stubs: bool,
) -> Vec<Finding> {
    let mut out: Vec<Finding> = Vec::new();
    let mut seen: HashSet<(&'static str, usize, String)> = HashSet::new();
    let mut add = |sev: &'static str, code: &'static str, line: usize, msg: String| {
        if seen.insert((code, line, msg.clone())) {
            out.push((sev, code, line, msg));
        }
    };
    let empty: Vec<String> = Vec::new();
    for d in diags {
        let (msg, line) = (d.message, d.line);
        match d.kind {
            "invalid_key" => {
                let Some(m) = INVALID_KEY.captures(msg) else {
                    // A record Teal made up from a table in the script: its inference, not the API.
                    if teal {
                        add("warn", "TEAL", line, msg.to_string());
                    }
                    continue;
                };
                let (key, tid) = (&m[1], &m[2]);
                let label = type_label(tid);
                match typo_of(key, members.get(tid).unwrap_or(&empty)) {
                    Some(close) => add(
                        "warn",
                        "API_TYPO",
                        line,
                        format!("{label} has no {} - did you mean {}?", py_repr(key), py_repr(close)),
                    ),
                    None if key.starts_with('_') => {
                        add("info", "API_INTERNAL", line, format!("{label}: {key} is internal to OpenMW"))
                    }
                    None => add("info", "API_UNDOCUMENTED", line, format!("{label}: {key} is not in the documented API")),
                }
            }
            "arity" => {
                let many = too_many(msg);
                if many || teal {
                    add("warn", if many { "TOO_MANY_ARGS" } else { "TEAL" }, line, msg.to_string());
                }
            }
            "unknown_variable" => {
                if let Some(m) = UNKNOWN.captures(msg)
                    && !bound.contains(&m[1])
                {
                    add("warn", "UNKNOWN_GLOBAL", line, format!("{} is not defined (a typo, or a global set elsewhere)", &m[1]));
                }
            }
            "module_not_found" => {
                let name = MODULE.captures(msg).map(|m| m[1].to_string()).unwrap_or_else(|| msg.to_string());
                if name.starts_with("openmw.") || name.starts_with("openmw_aux.") {
                    if !stubs {
                        add("error", "REQUIRE_NOT_FOUND", line, format!("{name} is not a package of this OpenMW"));
                    }
                } else {
                    add(
                        "warn",
                        "REQUIRE_NOT_FOUND",
                        line,
                        format!("require({}): no data folder has that module", py_repr(&name)),
                    );
                }
            }
            // "unused variable x: <type> (inferred at ...)" -> "unused variable x".
            "unused_variable" | "unused_function" => {
                add("info", "UNUSED_LOCAL", line, msg.split(':').next().unwrap_or("").to_string())
            }
            "type" | "syntax" if teal => {
                add(if d.severity == "error" { "error" } else { "warn" }, "TEAL", line, msg.to_string())
            }
            _ => {}
        }
    }
    out.sort_by_key(|f| f.2);
    out
}

/// A data folder's `tlconfig.lua` files: its own, then one a folder in (sorted).
fn tlconfig_files(d: &Path) -> Vec<PathBuf> {
    let mut subs: Vec<PathBuf> = std::fs::read_dir(d)
        .map(|rd| rd.flatten().map(|e| e.path().join("tlconfig.lua")).filter(|p| p.is_file()).collect())
        .unwrap_or_default();
    subs.sort();
    let mut out = vec![d.join("tlconfig.lua")];
    out.extend(subs);
    out
}

/// The folders the Teal projects among the mods name (`source_dir`, `include_dir`),
/// relative to their `tlconfig.lua`, in the order found - not yet resolved or checked to
/// exist (the caller does, as Python's `Path.resolve` does).
pub fn tlconfig_dirs(data_dirs: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for d in data_dirs {
        for cfg in tlconfig_files(d) {
            let Ok(text) = read_text(&cfg) else { continue };
            let parent = cfg.parent().unwrap_or(Path::new(""));
            for m in TLCONFIG_FIELD.captures_iter(&text) {
                for s in LUA_STRING.captures_iter(&m[2]) {
                    let rel = s.get(1).or(s.get(2)).map(|g| g.as_str()).unwrap_or("");
                    out.push(parent.join(rel));
                }
            }
        }
    }
    out
}

/// The modules the Cyan projects among the mods declare their globals in
/// (`global_env_def`), in the order found, each once.
pub fn tlconfig_globals(data_dirs: &[PathBuf]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for d in data_dirs {
        for cfg in tlconfig_files(d) {
            let Ok(text) = read_text(&cfg) else { continue };
            for m in TLCONFIG_GLOBALS.captures_iter(&text) {
                let quoted = &m[1];
                let name = py_strip(&quoted[1..quoted.len() - 1]).to_string();
                if !name.is_empty() && !out.contains(&name) {
                    out.push(name);
                }
            }
        }
    }
    out
}

/// A file name matched as the platform's glob matches it (no case on Windows).
fn name_is(name: &str, want: &str) -> bool {
    if cfg!(windows) { name.eq_ignore_ascii_case(want) } else { name == want }
}

fn has_declarations(dir: &Path) -> bool {
    let any = |d: &Path| {
        std::fs::read_dir(d).is_ok_and(|rd| {
            rd.flatten().any(|e| {
                let n = e.file_name().to_string_lossy().to_string();
                let n = if cfg!(windows) { n.to_ascii_lowercase() } else { n };
                n.ends_with(".d.tl")
            })
        })
    };
    any(dir) || any(&dir.join("openmw"))
}

/// A folder of OpenMW's published Teal declarations (`teal_declarations`) in or beside
/// one of these folders: one with `.d.tl` files (in it, or its `openmw` folder).
pub fn find_teal_declarations(near: &[PathBuf]) -> Option<PathBuf> {
    let mut seen: HashSet<PathBuf> = HashSet::new();
    for base in near {
        let mut places = vec![base.join("teal_declarations")];
        if let Some(parent) = base.parent() {
            places.push(parent.join("teal_declarations"));
        }
        for place in places {
            if seen.insert(place.clone()) && place.is_dir() && has_declarations(&place) {
                return Some(place);
            }
        }
    }
    None
}

/// Whether a folder is an OpenMW `resources` folder with a Lua API.
pub fn is_resources(path: &Path) -> bool {
    path.join("lua_api").join("openmw").is_dir()
}

/// Where OpenMW installs put `resources`, tried after the cfg and the environment.
const INSTALL_GLOBS: [&str; 10] = [
    "C:/Program Files/OpenMW*/resources",
    "C:/Program Files (x86)/OpenMW*/resources",
    "C:/Games/OpenMW*/resources",
    "/usr/share/games/openmw/resources",
    "/usr/share/openmw/resources",
    "/usr/local/share/games/openmw/resources",
    "/usr/local/share/openmw/resources",
    "/var/lib/flatpak/app/org.openmw.OpenMW/current/active/files/share/games/openmw/resources",
    "~/.local/share/flatpak/app/org.openmw.OpenMW/current/active/files/share/games/openmw/resources",
    "/Applications/OpenMW.app/Contents/Resources/resources",
];

fn home() -> Option<PathBuf> {
    let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(var).filter(|v| !v.is_empty()).map(PathBuf::from)
}

fn expand_user(p: &str) -> PathBuf {
    match (p.strip_prefix("~/"), home()) {
        (Some(rest), Some(h)) => h.join(rest),
        _ => PathBuf::from(p),
    }
}

/// `<dir>/<prefix>*/<rest>`: the matches, newest (greatest) first.
fn glob_one(pattern: &str) -> Vec<PathBuf> {
    let (head, rest) = pattern.split_once('*').unwrap_or((pattern, ""));
    let (dir, prefix) = head.rsplit_once('/').unwrap_or(("", head));
    let rest = rest.trim_start_matches('/');
    let mut out: Vec<PathBuf> = std::fs::read_dir(if dir.is_empty() { "/" } else { dir })
        .map(|rd| {
            rd.flatten()
                .filter(|e| {
                    let n = e.file_name().to_string_lossy().to_string();
                    n.len() >= prefix.len() && n.is_char_boundary(prefix.len()) && name_is(&n[..prefix.len()], prefix)
                })
                .map(|e| e.path().join(rest))
                .filter(|p| p.exists())
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out.reverse();
    out
}

/// `key=value` as `cfglines.cfg_line_value` reads it: matched surrounding quotes taken off.
fn cfg_line_value(line: &str) -> Option<&str> {
    let (_, v) = line.split_once('=')?;
    let v = py_strip(v);
    let b = v.as_bytes();
    if b.len() >= 2 && b[0] == b[b.len() - 1] && (b[0] == b'"' || b[0] == b'\'') {
        return Some(&v[1..v.len() - 1]);
    }
    Some(v)
}

/// `cfglines.unescape_cfg_value`: `&` escapes the character after it.
fn unescape_cfg_value(inner: &str) -> String {
    let mut out = String::new();
    let mut it = inner.chars();
    while let Some(c) = it.next() {
        if c == '&' {
            if let Some(n) = it.next() {
                out.push(n);
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// The `resources` folder of the OpenMW install a setup runs on: `explicit` (or its
/// `resources`), `WG_OPENMW_RESOURCES`, the cfg's `resources=` lines, then the usual
/// install places (the newest first). Only a folder with `lua_api/openmw` counts.
pub fn find_resources(cfg: Option<&Path>, explicit: Option<&Path>) -> Option<PathBuf> {
    let mut tried: Vec<PathBuf> = Vec::new();
    if let Some(e) = explicit {
        tried.push(e.to_path_buf());
        tried.push(e.join("resources"));
    }
    if let Some(env) = std::env::var_os("WG_OPENMW_RESOURCES").filter(|v| !v.is_empty()) {
        tried.push(PathBuf::from(env));
    }
    if let Some(cfg) = cfg
        && let Ok(text) = read_text(cfg)
    {
        for line in py_splitlines(&text) {
            let t = line.trim_start_matches(super::omwscripts::py_space);
            let is_key = t.len() >= 9
                && t.is_char_boundary(9)
                && t[..9].eq_ignore_ascii_case("resources")
                && t[9..].trim_start_matches(super::omwscripts::py_space).starts_with('=');
            if is_key {
                let p = PathBuf::from(unescape_cfg_value(cfg_line_value(line).unwrap_or("")));
                tried.push(if p.is_absolute() { p } else { cfg.parent().unwrap_or(Path::new("")).join(p) });
            }
        }
    }
    for pattern in INSTALL_GLOBS {
        let base = if pattern.starts_with('~') { expand_user(pattern).to_string_lossy().replace('\\', "/") } else { pattern.to_string() };
        if base.contains('*') {
            tried.extend(glob_one(&base));
        } else {
            tried.push(PathBuf::from(base));
        }
    }
    if cfg!(target_os = "macos") {
        tried.push(expand_user("~/Applications/OpenMW.app/Contents/Resources/resources"));
    }
    tried.into_iter().find(|p| is_resources(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ratio_matches_difflib() {
        let r = |a: &str, b: &str| ratio(&a.chars().collect::<Vec<_>>(), &b.chars().collect::<Vec<_>>());
        // difflib.SequenceMatcher(None, a, b).ratio(), from CPython.
        assert!((r("recordId", "recrdId") - 0.9333333333333333).abs() < 1e-12);
        assert!((r("abcd", "bcde") - 0.75).abs() < 1e-12);
        assert_eq!(r("", ""), 1.0);
        let names: Vec<String> = ["recordId", "position", "rotation", "cell"].map(String::from).into();
        assert_eq!(close_matches("recrdId", &names, 3, 0.75), ["recordId"]);
    }

    #[test]
    fn typos_labels_and_arity() {
        let names: Vec<String> = ["setConsoleMode", "getConsoleMode"].map(String::from).into();
        assert_eq!(typo_of("getConsoleMod", &names), Some("getConsoleMode"));
        let only_set: Vec<String> = vec!["setConsoleMode".into()];
        assert_eq!(typo_of("getConsoleMode", &only_set), None);
        assert_eq!(type_label("nearby__nearby"), "openmw.nearby");
        assert_eq!(type_label("core__GameObject"), "openmw.core GameObject");
        assert_eq!(type_label("I_AI__AI"), "I.AI");
        assert_eq!(type_label("aux_time__time"), "openmw_aux.time");
        assert!(too_many("wrong number of arguments (given 3, expects 2)"));
        assert!(!too_many("wrong number of arguments (given 1, expects at least 2 and at most 3)"));
    }

    #[test]
    fn diagnostics_to_findings() {
        let members: HashMap<String, Vec<String>> = [("self__self".to_string(), vec!["recordId".to_string()])].into();
        let diags = [
            DiagRef { kind: "invalid_key", severity: "error", line: 3, message: "invalid key 'recrdId' in type self__self" },
            DiagRef { kind: "unknown_variable", severity: "error", line: 1, message: "unknown variable: foo" },
            DiagRef { kind: "unknown_variable", severity: "error", line: 2, message: "unknown variable: mine" },
            DiagRef { kind: "module_not_found", severity: "error", line: 4, message: "module not found: 'openmw.nope'" },
        ];
        let bound: HashSet<String> = ["mine".to_string()].into();
        let got = findings_for(&diags, false, &bound, &members, false);
        let codes: Vec<&str> = got.iter().map(|f| f.1).collect();
        assert_eq!(codes, ["UNKNOWN_GLOBAL", "API_TYPO", "REQUIRE_NOT_FOUND"]);
        assert_eq!(got[1].3, "openmw.self has no 'recrdId' - did you mean 'recordId'?");
        assert!(findings_for(&diags[3..], false, &bound, &members, true).is_empty());
    }
}
