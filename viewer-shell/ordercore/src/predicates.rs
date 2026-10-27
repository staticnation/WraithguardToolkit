//! The predicate rules, as `wraithguard/rules/expressions.py` and `predicates.py` read them:
//! `[Requires]`, `[Conflict]`, `[Patch]` and `[Note]` bodies are a small lisp-like language of
//! plugin patterns and `ALL`/`ANY`/`NOT` groups, plus the atomic forms `[VER]`, `[SIZE]`,
//! `[DESC]` and `[MWSE-LUA]` that test a plugin's version, file size or header description.
//! This turns the rule text and the sorted load order into the warnings that fire.
//!
//! Three steps: [`tokenize`] and [`parse`] (text to a tree; [`describe`] renders a node back),
//! [`evaluate`] and [`triggered`] (does it hold; which plugins made it hold), and
//! [`check_predicates`] (every block of a rule text against a load order).
//!
//! **Plugin files are not read here.** What a plugin's version, size and header description are
//! comes through [`Plugins`], which the caller implements over the cfg's `data=` folders (the
//! Python's `PluginFileIndex`); the version is then read from the description or the file name
//! as `plugin_version` does. An unreadable or unreachable plugin means *unknown*, not *absent*:
//! a predicate that cannot be settled errs towards not warning, as mlox does.
//!
//! What is not read is **declined**, as in [`crate::patterns`]: every function that can meet it
//! answers [`Unsupported`], and the caller keeps Python's answer. That is a `/regex/` of a `[DESC]` that
//! [`crate::pyre`] does not read (an invalid one is not declined: Python treats it as not
//! matching), and a number or version too long for the Python's integers to be ported as text.
//! The evaluation is as lazy as Python's, so a `[DESC]` Python never reaches is never declined.
//!
//! Written from mlox's documented behaviour, not its code.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

use crate::patterns::{has_meta, Pattern, Unsupported};
use crate::pyre::{Flags, Match, PatError, Regex};
use crate::rules::{is_space, is_word, splitlines, strip_comment};

/// How deep a rule's brackets may nest before it is declined (Python's own recursion limit
/// would stop it first).
const MAX_DEPTH: usize = 200;

const VER: &str = r"(\d+(?:[_.-]?\d+)*[a-zA-Z]?)";
const FUNC_FORM: &str = concat!(
    r"\[\s*VER\b[^\]\n]*\]|",
    r"\[\s*SIZE\b[^\]\n]*\]|",
    r"\[\s*DESC\s*!?\s*/[^/\n]*/[^\]\n]*\]|",
    r"\[\s*MWSE-LUA\s*!?\s*/[^/\n]*/[^\]\n]*\]"
);
const KEYWORD: &str = r"\bALL\b|\bANY\b|\bNOT\b";
const MESSAGE: &str = r"/[^/]+/";
/// The extensions a plugin name ends in, in the order the Python's alternation tries them.
const EXTENSIONS: [&str; 5] = ["esp", "esm", "omwaddon", "omwgame", "omwscripts"];
const TOP: &str = r"^\[(Order|NearStart|NearEnd|Requires|Conflict|Patch|Note|Version)\b([^\]\n]*)\]";
const FILENAME_VERSION: &str = r"\D(\d+(?:[_.-]?\d+)*[a-zA-Z]?)\D*\.es[mp]";
const HEADER_VERSION: &str = r"\b(?:version\b\D+|v(?:er)?\.?\s*)(\d+(?:[_.-]?\d+)*[a-zA-Z]?)";
const ALPHA_TAIL: &str = r"(\d+)([a-zA-Z])";

const ICASE: Flags = Flags { ignore_case: true, multiline: false };

fn compiled(cell: &'static OnceLock<Regex>, pattern: &str, flags: Flags) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pattern, flags).expect("a fixed expression compiles"))
}

fn text_of(chars: &[char], span: (usize, usize)) -> String {
    chars[span.0..span.1].iter().collect()
}

fn group(chars: &[char], m: &Match, n: usize) -> String {
    m.group(n).map(|s| text_of(chars, s)).unwrap_or_default()
}

fn strip(s: &str) -> &str {
    s.trim_matches(is_space)
}

// ---------------------------------------------------------------- text to tree

/// A rule's expression: a plugin pattern, an atomic form or a `/message/` (a leaf), or a
/// bracketed group of nodes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Node {
    /// A string leaf.
    Leaf(String),
    /// A bracketed group.
    Group(Vec<Node>),
}

/// The length of the extension (`\.(?:esp|esm|omwaddon|omwgame|omwscripts)`, any ASCII case)
/// at `i`, if one starts there.
fn ext_at(chars: &[char], i: usize) -> Option<usize> {
    if chars.get(i) != Some(&'.') {
        return None;
    }
    EXTENSIONS.into_iter().find_map(|ext| {
        let fits = ext.chars().enumerate().all(|(k, e)| chars.get(i + 1 + k).is_some_and(|c| c.eq_ignore_ascii_case(&e)));
        fits.then_some(1 + ext.len())
    })
}

/// Whether the bracket at `i` opens a function or a group, which no name may hold:
/// `\[(?:ALL|ANY|NOT|DESC|VER|SIZE|MWSE-LUA)\b` or `\[\s*(?:DESC|VER|SIZE|MWSE-LUA)\b`.
fn opens_a_form(chars: &[char], i: usize) -> bool {
    let word_at = |at: usize, words: &[&str]| {
        words.iter().any(|w| {
            let fits = w.chars().enumerate().all(|(k, e)| chars.get(at + k).is_some_and(|c| c.eq_ignore_ascii_case(&e)));
            // `\b`: each word ends in a word character, so the next must not be one.
            fits && !chars.get(at + w.len()).copied().is_some_and(is_word)
        })
    };
    const FORMS: [&str; 4] = ["DESC", "VER", "SIZE", "MWSE-LUA"];
    if word_at(i + 1, &["ALL", "ANY", "NOT"]) || word_at(i + 1, &FORMS) {
        return true;
    }
    let mut at = i + 1;
    while chars.get(at).copied().is_some_and(is_space) {
        at += 1;
    }
    word_at(at, &FORMS)
}

/// A plugin name at `i` (the Python's `_NAME_RE`): the shortest run of name characters
/// (anything but `[`, `]` and a line break) and bracketed parts that do not open a form or
/// hold an extension, followed by an extension and an optional `*`. Its end, if one is there.
fn name_at(chars: &[char], i: usize) -> Option<usize> {
    let mut at = i;
    loop {
        // one unit, then the extension is tried (the lazy `+?`)
        match chars.get(at)? {
            '\n' | ']' => return None,
            '[' => {
                if opens_a_form(chars, at) {
                    return None;
                }
                let mut j = at + 1;
                loop {
                    match chars.get(j)? {
                        ']' => break,
                        '[' | '\n' => return None,
                        _ if ext_at(chars, j).is_some() => return None,
                        _ => j += 1,
                    }
                }
                at = j + 1;
            }
            _ => at += 1,
        }
        if let Some(len) = ext_at(chars, at) {
            let end = at + len;
            return Some(if chars.get(end) == Some(&'*') { end + 1 } else { end });
        }
    }
}

/// Split a predicate body into tokens (`tokenize_mlox_logic`): brackets, `ALL`/`ANY`/`NOT`,
/// `/message/` strings, plugin patterns and whole atomic forms. At each place, in this order:
/// an atomic form, a name that starts with a bracketed part (`[Official]Master Index.esp`), a
/// bracket, a keyword, a `/message/`, a name; a place that starts none is passed over. A name
/// may hold a bracketed part, as mlox reads one (`Morrowind Rebirth [Main].ESP`).
///
/// # Errors
/// [`Unsupported`] only if the matcher declines the text.
pub fn tokenize(text: &str) -> Result<Vec<String>, Unsupported> {
    static FUNC_RE: OnceLock<Regex> = OnceLock::new();
    static KEYWORD_RE: OnceLock<Regex> = OnceLock::new();
    static MESSAGE_RE: OnceLock<Regex> = OnceLock::new();
    let func = compiled(&FUNC_RE, FUNC_FORM, ICASE);
    let keyword = compiled(&KEYWORD_RE, KEYWORD, ICASE);
    let message = compiled(&MESSAGE_RE, MESSAGE, Flags::default());
    let chars: Vec<char> = text.chars().collect();
    let mut tokens: Vec<String> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let end = match chars[i] {
            '[' => match func.match_at(&chars, i)? {
                Some(m) => Some(m.end()),
                None => name_at(&chars, i).or(Some(i + 1)),
            },
            ']' => Some(i + 1),
            _ => match keyword.match_at(&chars, i)? {
                Some(m) => Some(m.end()),
                None => match message.match_at(&chars, i)? {
                    Some(m) => Some(m.end()),
                    None => name_at(&chars, i),
                },
            },
        };
        match end {
            Some(end) => {
                tokens.push(text_of(&chars, (i, end)));
                i = end;
            }
            None => i += 1,
        }
    }
    Ok(tokens.iter().map(|t| strip(t).to_string()).filter(|t| !t.is_empty()).collect())
}

/// Build the nested tree from tokens (`parse_mlox_lisp`). Unbalanced input degrades the way the
/// Python does: a missing `]` ends the group at the end of the input, and a stray `]` ends the
/// parse there.
///
/// # Errors
/// [`Unsupported`] for brackets nested deeper than [`MAX_DEPTH`].
pub fn parse(tokens: &[String]) -> Result<Vec<Node>, Unsupported> {
    fn go(tokens: &[String], pos: &mut usize, depth: usize) -> Result<Vec<Node>, Unsupported> {
        if depth > MAX_DEPTH {
            return Err(Unsupported);
        }
        let mut node = Vec::new();
        while *pos < tokens.len() {
            let token = &tokens[*pos];
            *pos += 1;
            match token.as_str() {
                "[" => node.push(Node::Group(go(tokens, pos, depth + 1)?)),
                "]" => return Ok(node),
                _ => node.push(Node::Leaf(token.clone())),
            }
        }
        Ok(node)
    }
    go(tokens, &mut 0, 0)
}

/// Render a node as a short readable string (`describe_node`), to name a missing dependency.
/// A group with no operator is described as `ANY`, as the evaluator treats it.
pub fn describe(node: &Node) -> String {
    match node {
        Node::Leaf(s) => s.clone(),
        Node::Group(items) if items.is_empty() => "?".to_string(),
        Node::Group(items) => {
            let (op, rest) = match &items[0] {
                Node::Leaf(s) if matches!(s.to_uppercase().as_str(), "ALL" | "ANY" | "NOT" | "DESC") => {
                    (s.to_uppercase(), &items[1..])
                }
                _ => ("ANY".to_string(), &items[..]),
            };
            format!("{op}({})", rest.iter().map(describe).collect::<Vec<_>>().join(", "))
        }
    }
}

// ---------------------------------------------------------------- plugin metadata

/// What the rules need to know of the plugin files (the Python's `PluginFileIndex`).
pub trait Plugins {
    /// Whether a "not found" can be trusted: at least one data folder gave a plugin.
    fn usable(&self) -> bool;
    /// Whether the plugin is in the data folders (found by name without regard to case).
    fn exists(&self, name: &str) -> bool;
    /// The file's size in bytes, or `None` if it cannot be read. Asked only if it [exists].
    ///
    /// [exists]: Plugins::exists
    fn size(&self, name: &str) -> Option<u64>;
    /// The header description (`read_plugin_description`: empty on any problem). Asked only if
    /// the plugin [exists](Plugins::exists).
    fn description(&self, name: &str) -> String;
}

/// `format_version`: a version as a fixed-width string that compares as the version does.
///
/// Returns `""` for a string with no parseable version (the caller treats that as unknown).
///
/// # Errors
/// [`Unsupported`] for a number the Python would read from text this does not (signs,
/// whitespace, non-ASCII digits).
pub fn format_version(version: &str) -> Result<String, Unsupported> {
    static TAIL: OnceLock<Regex> = OnceLock::new();
    let mut parts: Vec<String> = Vec::new();
    let mut current = String::new();
    for c in version.chars() {
        if matches!(c, '_' | '.' | '-') && parts.len() < 3 {
            parts.push(std::mem::take(&mut current));
        } else {
            current.push(c);
        }
    }
    parts.push(current);
    let last: Vec<char> = parts[parts.len() - 1].chars().collect();
    let mut alpha = "_".to_string();
    if let Some(m) = compiled(&TAIL, ALPHA_TAIL, ICASE).match_at(&last, 0)? {
        alpha = group(&last, &m, 2);
        let digits = group(&last, &m, 1);
        let n = parts.len();
        parts[n - 1] = digits;
    }
    let mut numbers: Vec<String> = Vec::new();
    for part in &parts {
        if part.chars().any(|c| !c.is_ascii() || is_space(c) || c == '+') {
            return Err(Unsupported);
        }
        if part.is_empty() || !part.chars().all(|c| c.is_ascii_digit()) {
            return Ok(String::new()); // int() raises ValueError
        }
        let trimmed = part.trim_start_matches('0');
        numbers.push(if trimmed.is_empty() { "0".to_string() } else { trimmed.to_string() });
    }
    while numbers.len() < 3 {
        numbers.push("0".to_string());
    }
    Ok(format!("{:0>5}.{:0>5}.{:0>5}.{alpha}", numbers[0], numbers[1], numbers[2]))
}

/// `plugin_version`: the version in the plugin's own header description, else the one in its
/// file name, else `None` (unknowable).
fn plugin_version(name: &str, plugins: Option<&dyn Plugins>) -> Result<Option<String>, Unsupported> {
    static HEADER: OnceLock<Regex> = OnceLock::new();
    static FILE: OnceLock<Regex> = OnceLock::new();
    if let Some(p) = plugins.filter(|p| p.exists(name)) {
        let desc: Vec<char> = p.description(name).chars().collect();
        if let Some(m) = compiled(&HEADER, HEADER_VERSION, ICASE).search(&desc, 0)? {
            return format_version(&group(&desc, &m, 1)).map(Some);
        }
    }
    let file: Vec<char> = name.chars().collect();
    if let Some(m) = compiled(&FILE, FILENAME_VERSION, ICASE).search(&file, 0)? {
        return format_version(&group(&file, &m, 1)).map(Some);
    }
    Ok(None)
}

// ---------------------------------------------------------------- the atomic forms

/// A function form read from its token: which form, and its parts.
pub(crate) enum Form {
    Ver { op: String, want: String, plugin: String },
    Size { bang: bool, want: String, plugin: String },
    Desc { bang: bool, pat: String, plugin: String },
    MwseLua { plugin: String },
}

impl Form {
    fn plugin(&self) -> &str {
        match self {
            Form::Ver { plugin, .. }
            | Form::Size { plugin, .. }
            | Form::Desc { plugin, .. }
            | Form::MwseLua { plugin } => plugin,
        }
    }
}

/// Which atomic form a whole token is (the four `_re_*_fun` expressions, tried in order).
pub(crate) fn read_form(token: &str) -> Result<Option<Form>, Unsupported> {
    static VERSION: OnceLock<Regex> = OnceLock::new();
    static SIZE: OnceLock<Regex> = OnceLock::new();
    static DESC: OnceLock<Regex> = OnceLock::new();
    static LUA: OnceLock<Regex> = OnceLock::new();
    let ver = format!(r"^\[\s*VER\s*([=<>])\s*{VER}\s*([^\]]+?)\s*\]$");
    let size = r"^\[\s*SIZE\s*(!?)(\d+)\s+(\S.*?\.(?:es[mp]|omwaddon|omwgame|omwscripts)\b)\s*\]$";
    let desc = r"^\[\s*DESC\s*(!?)/([^/]+)/\s+([^\]]+?)\s*\]$";
    let lua = r"^\[\s*MWSE-LUA\s*(!?)/([^/]+)/\s+([^\]]+?)\s*\]$";
    let t: Vec<char> = token.chars().collect();
    let plugin = |m: &Match, n: usize| strip(&group(&t, m, n)).to_string();
    if let Some(m) = compiled(&VERSION, &ver, ICASE).match_at(&t, 0)? {
        return Ok(Some(Form::Ver { op: group(&t, &m, 1), want: group(&t, &m, 2), plugin: plugin(&m, 3) }));
    }
    if let Some(m) = compiled(&SIZE, size, ICASE).match_at(&t, 0)? {
        return Ok(Some(Form::Size {
            bang: group(&t, &m, 1) == "!",
            want: group(&t, &m, 2),
            plugin: plugin(&m, 3),
        }));
    }
    if let Some(m) = compiled(&DESC, desc, ICASE).match_at(&t, 0)? {
        return Ok(Some(Form::Desc {
            bang: group(&t, &m, 1) == "!",
            pat: group(&t, &m, 2),
            plugin: plugin(&m, 3),
        }));
    }
    if let Some(m) = compiled(&LUA, lua, ICASE).match_at(&t, 0)? {
        return Ok(Some(Form::MwseLua { plugin: plugin(&m, 3) }));
    }
    Ok(None)
}

/// The active plugins a pattern names (`_matching`): with a wildcard or `<VER>`, as
/// `mlox_pattern_to_regex` matches; otherwise a plain name, compared without case, as mlox's
/// `_expand_filename` does (so a bracket in a plain name is a character, not syntax).
fn matching<'a>(pattern: &str, active: &[&'a str]) -> Result<Vec<&'a str>, Unsupported> {
    if !has_meta(pattern) {
        let low = pattern.to_lowercase();
        return Ok(active.iter().copied().filter(|p| p.to_lowercase() == low).collect());
    }
    let pat = Pattern::new(pattern)?;
    Ok(active.iter().copied().filter(|p| pat.is_match(p)).collect())
}

fn eval_form(form: &Form, active: &[&str], plugins: Option<&dyn Plugins>) -> Result<bool, Unsupported> {
    let matched = matching(form.plugin(), active)?;
    match form {
        Form::MwseLua { .. } => Ok(false), // MWSE-Lua content does not exist under OpenMW
        Form::Ver { op, want, .. } => {
            let want = format_version(want)?;
            if matched.is_empty() {
                return Ok(false);
            }
            if !plugins.is_some_and(|x| x.usable()) {
                // mlox with no data folder: the version is not read, `=` is assumed to hold
                return Ok(op == "=");
            }
            for p in &matched {
                let Some(pv) = plugin_version(p, plugins)? else {
                    return Ok(false); // mlox: a plugin with no version ends the test
                };
                if (op == "=" && pv == want) || (op == "<" && pv < want) || (op == ">" && pv > want) {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        Form::Size { bang, want, .. } => {
            let want: u64 = want.parse().map_err(|_| Unsupported)?;
            for p in &matched {
                if !plugins.is_some_and(|x| x.exists(p)) {
                    // no data folder at all: err towards existence; folders readable but this
                    // plugin not in them: cannot be substantiated, try the next
                    if !plugins.is_some_and(|x| x.usable()) {
                        return Ok(true);
                    }
                    continue;
                }
                let Some(actual) = plugins.and_then(|x| x.size(p)) else {
                    return Ok(true); // unreadable file: the same "cannot verify" case
                };
                if (actual == want) != *bang {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        Form::Desc { bang, pat, .. } => {
            for p in &matched {
                if !plugins.is_some_and(|x| x.exists(p)) {
                    if !plugins.is_some_and(|x| x.usable()) {
                        return Ok(true); // no data folder: assume true, as mlox does
                    }
                    continue;
                }
                let desc: Vec<char> = plugins.map(|x| x.description(p)).unwrap_or_default().chars().collect();
                let found = match Regex::new(pat, Flags::default()) {
                    Ok(re) => re.search(&desc, 0)?.is_some(),
                    Err(PatError::Invalid) => false, // Python's re.error: no match
                    Err(PatError::Unsupported) => return Err(Unsupported),
                };
                if found != *bang {
                    return Ok(true);
                }
            }
            Ok(false)
        }
    }
}

// ---------------------------------------------------------------- evaluation

fn is_message(s: &str) -> bool {
    s.starts_with('/') && s.ends_with('/')
}

/// Whether a node holds for the active plugins (`evaluate_node`): a plugin pattern, an
/// `ALL`/`ANY`/`NOT`/`DESC` group, or an atomic form. A group with no operator is an `ANY`.
///
/// `active` is the load order; `plugins` lets the forms read real plugin files (`None`: mlox's
/// conservative behaviour, nothing guessed).
///
/// # Errors
/// [`Unsupported`], see the module.
pub fn evaluate(node: &Node, active: &[&str], plugins: Option<&dyn Plugins>) -> Result<bool, Unsupported> {
    match node {
        Node::Leaf(s) => {
            if s.starts_with('[') {
                return match read_form(s)? {
                    Some(form) => eval_form(&form, active, plugins),
                    None => Ok(false), // a bracketed token not modelled: cannot assert it holds
                };
            }
            if is_message(s) {
                return Ok(true); // /DESC message/ strings carry no truth value
            }
            Ok(!matching(s, active)?.is_empty())
        }
        Node::Group(items) if items.is_empty() => Ok(false),
        Node::Group(items) => {
            let op = match &items[0] {
                Node::Leaf(s) => s.to_uppercase(),
                Node::Group(_) => String::new(),
            };
            match op.as_str() {
                "ALL" => {
                    for arg in &items[1..] {
                        if !evaluate(arg, active, plugins)? {
                            return Ok(false);
                        }
                    }
                    Ok(true)
                }
                "ANY" => any(&items[1..], active, plugins),
                "NOT" => Ok(items.len() >= 2 && !evaluate(&items[1], active, plugins)?),
                "DESC" => evaluate(&items[items.len() - 1], active, plugins),
                _ => any(items, active, plugins), // no leading operator implies ANY
            }
        }
    }
}

fn any(items: &[Node], active: &[&str], plugins: Option<&dyn Plugins>) -> Result<bool, Unsupported> {
    for arg in items {
        if evaluate(arg, active, plugins)? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// The active plugins that actually matched inside a node (`get_triggered_plugins`), so a
/// warning can name them. A `/message/` matches nothing; a group's first item is not read.
///
/// # Errors
/// [`Unsupported`], see the module.
pub fn triggered(node: &Node, active: &[&str]) -> Result<BTreeSet<String>, Unsupported> {
    let mut found = BTreeSet::new();
    match node {
        Node::Leaf(s) => {
            if s.starts_with('[') {
                if let Some(form) = read_form(s)? {
                    found.extend(matching(form.plugin(), active)?.into_iter().map(str::to_string));
                }
            } else if !is_message(s) {
                found.extend(matching(s, active)?.into_iter().map(str::to_string));
            }
        }
        Node::Group(items) => {
            for arg in items.iter().skip(1) {
                found.extend(triggered(arg, active)?);
            }
        }
    }
    Ok(found)
}

// ---------------------------------------------------------------- the rule blocks

/// Python's `str.title()` of a header keyword, for the four it can be one of.
fn keyword_of(raw: &str) -> Option<&'static str> {
    ["Conflict", "Requires", "Note", "Patch"].into_iter().find(|k| raw.eq_ignore_ascii_case(k))
}

/// Every predicate block of a rule text against a sorted load order (`check_predicates`):
/// the warnings that fire, each prefixed with its kind.
///
/// - `rules_text`: the raw text of the rule files, joined.
/// - `final_order`: the sorted plugin names.
/// - `subset_origins`: `{plugin, in lower case: where it came from}`, to say which of the
///   user's own mods a warning is about.
/// - `plugins`: the plugin files (see [`Plugins`]).
///
/// # Errors
/// [`Unsupported`], see the module.
pub fn check_predicates(
    rules_text: &str,
    final_order: &[String],
    subset_origins: &BTreeMap<String, String>,
    plugins: &dyn Plugins,
) -> Result<Vec<String>, Unsupported> {
    static TOP_RE: OnceLock<Regex> = OnceLock::new();
    let top = compiled(&TOP_RE, TOP, Flags { ignore_case: true, multiline: true });
    let mut active: Vec<&str> = Vec::new();
    for name in final_order {
        if !active.contains(&name.as_str()) {
            active.push(name);
        }
    }
    let idx = Some(plugins);
    let annotate = |name: &str| match subset_origins.get(&name.to_lowercase()) {
        Some(origin) if !origin.is_empty() => format!("{name} [{origin}]"),
        _ => name.to_string(),
    };
    let annotate_all =
        |names: &BTreeSet<String>| names.iter().map(|n| annotate(n)).collect::<Vec<_>>().join(", ");

    let chars: Vec<char> = rules_text.chars().collect();
    let heads = top.find_all(&chars)?;
    let mut warnings = Vec::new();
    for (i, m) in heads.iter().enumerate() {
        let Some(keyword) = keyword_of(&group(&chars, m, 1)) else { continue };
        let end = heads.get(i + 1).map_or(chars.len(), Match::start);
        let body: String = chars[m.end()..end].iter().collect();

        // A line that starts with whitespace is message text (mlox's `re_message`), unless a
        // bracket expression is open; everything else is logic. The first line is the rest of
        // the header line: always logic.
        let mut message_lines: Vec<String> = Vec::new();
        let header_arg = group(&chars, m, 2);
        if !strip(&header_arg).is_empty() {
            message_lines.push(strip(&header_arg).to_string());
        }
        let mut logic_text = String::new();
        let mut depth: i64 = 0;
        for (n, raw_line) in splitlines(&body).into_iter().enumerate() {
            let line = strip(strip_comment(raw_line));
            if line.is_empty() {
                continue;
            }
            if n > 0 && depth == 0 && raw_line.starts_with([' ', '\t']) {
                message_lines.push(line.to_string());
            } else {
                logic_text.push(' ');
                logic_text.push_str(line);
                depth += line.matches('[').count() as i64 - line.matches(']').count() as i64;
                depth = depth.max(0);
            }
        }
        let message = strip(&message_lines.join(" ")).to_string();
        let ast = parse(&tokenize(&logic_text)?)?;
        if ast.is_empty() {
            continue;
        }

        match keyword {
            "Conflict" => {
                // a flat list of mutually exclusive items or groups: warn if more than one holds
                let mut true_nodes = Vec::new();
                for n in &ast {
                    if evaluate(n, &active, idx)? {
                        true_nodes.push(n);
                    }
                }
                if true_nodes.len() > 1 {
                    let mut by = BTreeSet::new();
                    for n in true_nodes {
                        by.extend(triggered(n, &active)?);
                    }
                    let mut w = format!("[CONFLICT] {message}");
                    if !by.is_empty() {
                        w.push_str(&format!("\n    Caused by: {}", annotate_all(&by)));
                    }
                    warnings.push(w);
                }
            }
            "Requires" => {
                // the first item is the target, the rest are its dependencies
                if ast.len() >= 2 && evaluate(&ast[0], &active, idx)? {
                    let names = triggered(&ast[0], &active)?;
                    let mut missing = Vec::new();
                    for n in &ast[1..] {
                        if !evaluate(n, &active, idx)? {
                            missing.push(n);
                        }
                    }
                    if !missing.is_empty() {
                        let mut w = format!("[REQUIRES] {message}");
                        if !names.is_empty() {
                            w.push_str(&format!("\n    Needed by: {}", annotate_all(&names)));
                        }
                        let list: Vec<String> = missing.iter().map(|n| describe(n)).collect();
                        w.push_str(&format!("\n    Missing: {}", list.join(", ")));
                        warnings.push(w);
                    }
                }
            }
            "Patch" => {
                // a mutual dependency: the patch without its original, or the original unpatched
                if ast.len() >= 2 {
                    let patch_active = evaluate(&ast[0], &active, idx)?;
                    let originals = &ast[1..];
                    let mut missing = Vec::new();
                    for n in originals {
                        if !evaluate(n, &active, idx)? {
                            missing.push(n);
                        }
                    }
                    if patch_active && !missing.is_empty() {
                        let names = triggered(&ast[0], &active)?;
                        let mut w = format!("[PATCH] {message}");
                        if !names.is_empty() {
                            w.push_str(&format!("\n    Patch present: {}", annotate_all(&names)));
                        }
                        let list: Vec<String> = missing.iter().map(|n| describe(n)).collect();
                        w.push_str(&format!("\n    But not what it patches: {}", list.join(", ")));
                        warnings.push(w);
                    } else if !patch_active && all_hold(originals, &active, idx)? {
                        let mut present = BTreeSet::new();
                        for n in originals {
                            present.extend(triggered(n, &active)?);
                        }
                        let mut w = format!("[PATCH] {message}");
                        if !present.is_empty() {
                            w.push_str(&format!("\n    Unpatched: {}", annotate_all(&present)));
                        }
                        w.push_str(&format!("\n    Missing patch: {}", describe(&ast[0])));
                        warnings.push(w);
                    }
                }
            }
            _ => {
                // a note fires when any of what it lists holds, and is about those that do, as
                // mlox's [Note] prints the true ones
                let mut true_nodes = Vec::new();
                for n in &ast {
                    if evaluate(n, &active, idx)? {
                        true_nodes.push(n);
                    }
                }
                if !true_nodes.is_empty() {
                    let mut by = BTreeSet::new();
                    for n in true_nodes {
                        by.extend(triggered(n, &active)?);
                    }
                    let mut w = format!("[NOTE] {message}");
                    if !by.is_empty() {
                        w.push_str(&format!("\n    About: {}", annotate_all(&by)));
                    }
                    warnings.push(w);
                }
            }
        }
    }
    Ok(warnings)
}

fn all_hold(items: &[Node], active: &[&str], plugins: Option<&dyn Plugins>) -> Result<bool, Unsupported> {
    for n in items {
        if !evaluate(n, active, plugins)? {
            return Ok(false);
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Disk(BTreeMap<String, (u64, String)>);

    impl Plugins for Disk {
        fn usable(&self) -> bool {
            !self.0.is_empty()
        }
        fn exists(&self, name: &str) -> bool {
            self.0.contains_key(&name.to_lowercase())
        }
        fn size(&self, name: &str) -> Option<u64> {
            self.0.get(&name.to_lowercase()).map(|v| v.0)
        }
        fn description(&self, name: &str) -> String {
            self.0.get(&name.to_lowercase()).map(|v| v.1.clone()).unwrap_or_default()
        }
    }

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    fn check(rules: &str, order: &[&str], disk: &Disk) -> Vec<String> {
        check_predicates(rules, &names(order), &BTreeMap::new(), disk).unwrap()
    }

    #[test]
    fn tokens_are_brackets_keywords_messages_and_names() {
        let got = tokenize("[ALL A.esp [NOT B.esp] [VER = 1.0 C.esp]]").unwrap();
        assert_eq!(got, ["[", "ALL", "A.esp", "[", "NOT", "B.esp", "]", "[VER = 1.0 C.esp]", "]"]);
        // a leading message is merged with the name after it, as in Python
        assert_eq!(tokenize(" /msg/ A.esp").unwrap(), ["/msg/ A.esp"]);
    }

    #[test]
    fn a_stray_bracket_ends_the_parse_and_a_missing_one_ends_the_group() {
        let t = |s: &[&str]| parse(&names(s)).unwrap();
        assert_eq!(t(&["A.esp", "]", "B.esp"]), [Node::Leaf("A.esp".into())]);
        assert_eq!(
            t(&["[", "ALL", "A.esp"]),
            [Node::Group(vec![Node::Leaf("ALL".into()), Node::Leaf("A.esp".into())])]
        );
    }

    #[test]
    fn nodes_describe_themselves() {
        let ast = parse(&tokenize("[ALL A.esp [NOT B.esp]] [C.esp D.esp] []").unwrap()).unwrap();
        let d: Vec<String> = ast.iter().map(describe).collect();
        assert_eq!(d, ["ALL(A.esp, NOT(B.esp))", "ANY(C.esp, D.esp)", "?"]);
    }

    #[test]
    fn versions_are_fixed_width_and_keep_three_numbers() {
        assert_eq!(format_version("1.0a").unwrap(), "00001.00000.00000.a");
        assert_eq!(format_version("1.2.3.4").unwrap(), "00001.00002.00003._");
        assert_eq!(format_version("1.2.3.4.5").unwrap(), "");
        assert_eq!(format_version("2").unwrap(), "00002.00000.00000._");
    }

    #[test]
    fn a_requires_names_what_is_missing() {
        let none = Disk(BTreeMap::new());
        let got = check("[Requires needs the base]\nPatch.esp\nBase.esp\n", &["Patch.esp"], &none);
        assert_eq!(got, ["[REQUIRES] needs the base\n    Needed by: Patch.esp\n    Missing: Base.esp"]);
        assert!(check("[Requires]\nPatch.esp\nBase.esp\n", &["Patch.esp", "Base.esp"], &none).is_empty());
    }

    #[test]
    fn a_size_that_cannot_be_checked_errs_towards_existence() {
        let none = Disk(BTreeMap::new());
        let rule = "[Note big]\n[SIZE 5000 Mod.esp]\n";
        assert_eq!(check(rule, &["Mod.esp"], &none).len(), 1);
        let other = Disk([("other.esp".to_string(), (10, String::new()))].into());
        assert!(check(rule, &["Mod.esp"], &other).is_empty());
        let disk = Disk([("mod.esp".to_string(), (5000, String::new()))].into());
        assert_eq!(check(rule, &["Mod.esp"], &disk).len(), 1);
    }

    #[test]
    fn a_description_regex_python_rejects_matches_nothing() {
        let disk = Disk([("mod.esp".to_string(), (500, "hello".to_string()))].into());
        assert!(check("[Note x]\n[DESC /(/ Mod.esp]\n", &["Mod.esp"], &disk).is_empty());
        assert_eq!(check("[Note x]\n[DESC !/(/ Mod.esp]\n", &["Mod.esp"], &disk).len(), 1);
        let got = check_predicates(
            "[Note x]\n[DESC /(?i)h/ Mod.esp]\n",
            &names(&["Mod.esp"]),
            &BTreeMap::new(),
            &disk,
        );
        assert_eq!(got, Err(Unsupported));
    }
}
