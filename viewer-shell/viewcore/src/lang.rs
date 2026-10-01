//! Language packs — round 18aq, phase 2 of `wording-plan.md`.
//!
//! English lives in the page, where each thing is. This module owns the *files*: the
//! catalogue of every key the page can say (`page-source/catalog.json`, embedded at build
//! time, so a pack can be checked with no page running), the packs in `languages\` under
//! the store — beside the program, or in the folder Settings chose, so they travel with
//! the rest of a person's files (round 18as) — named `gardenfell.<code>.toml`, and what
//! can be said about a pack — whether it parses, which of its lines cannot be used and
//! why, how much of the catalogue it covers.
//!
//! A pack is a translation keyed by ID and nothing else: it never carries English, so it
//! can never override it, and a reworded English never touches a pack. A key the catalogue
//! no longer has is an *orphan*, reported and kept; a value whose placeholders differ from
//! the English's is refused with its line number; a tag the English does not use is
//! refused the same way. Untranslated keys show English — the page's business.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::json::{self, J};
use crate::toml;

/// The page's catalogue, as `catalog.js` writes it. The build embeds it, so the engine
/// and the page it ships with always agree on what a key is.
const CATALOG_JSON: &str = include_str!("../data/catalog.json");
/// The translator's template, the same file `catalog.js` commits. Embedded so that
/// `new_pack()` can start a pack from it anywhere the program runs, and so that
/// `update()` can refresh a pack against it line by line — every comment kept, every key
/// in its place.
const TEMPLATE: &str = include_str!("../data/lang.en.toml");

/// The folder the packs live in, under the store.
pub const LANG_DIR: &str = "languages";

/// One catalogue entry: the English and the placeholders it carries.
#[derive(Debug, Clone)]
pub struct Entry {
    pub en: String,
    pub placeholders: Vec<String>,
}

pub struct Catalog {
    pub entries: BTreeMap<String, Entry>,
}

impl Catalog {
    /// The entry a pack line is checked against: the key itself, or — for a plural form
    /// the English does not have (`.few` where English has `.one`/`.other`) — the
    /// base's `.other`, then the bare base.
    pub fn reference(&self, key: &str) -> Option<&Entry> {
        if let Some(e) = self.entries.get(key) {
            return Some(e);
        }
        let base = plural_base(key)?;
        self.entries
            .get(&format!("{}.other", base))
            .or_else(|| self.entries.get(base))
    }
}

/// `"paint.n_cells.one"` → `Some("paint.n_cells")`; a key with no plural suffix → None.
pub fn plural_base(key: &str) -> Option<&str> {
    for form in [".one", ".few", ".many", ".other"] {
        if let Some(b) = key.strip_suffix(form) {
            return Some(b);
        }
    }
    None
}

/// The embedded catalogue, parsed once.
pub fn catalog() -> &'static Catalog {
    static C: std::sync::OnceLock<Catalog> = std::sync::OnceLock::new();
    C.get_or_init(|| parse_catalog(CATALOG_JSON).expect("catalog.json is what catalog.js writes"))
}

pub fn parse_catalog(text: &str) -> Result<Catalog, String> {
    let v = json::parse(text)?;
    let strings = v.get("strings").ok_or("catalog.json: no \"strings\"")?;
    let mut entries = BTreeMap::new();
    if let json::Val::Obj(kv) = strings {
        for (k, e) in kv {
            let en = e.get("en").and_then(|x| x.as_str()).ok_or_else(|| format!("catalog.json: {} has no en", k))?;
            entries.insert(
                k.clone(),
                Entry { en: en.to_string(), placeholders: placeholders_of(en) },
            );
        }
    }
    Ok(Catalog { entries })
}

/// An engine message, worded in English from the catalogue — phase 3 of the plan.
///
/// The engine says `msg!("eng.no_interior", name = n)` and the English comes from the
/// same catalogue the page and the packs use, so a message has one wording, kept in
/// 05_text.js like every other string, and the page can match the English it receives
/// back to its key and translate it (`Engine.word`). A code the catalogue lacks renders
/// as the code and its parameters, so the miss is seen rather than swallowed.
pub fn word(code: &str, params: &[(&str, String)]) -> String {
    match catalog().entries.get(code) {
        Some(e) => fill(&e.en, params),
        None => {
            let mut s = code.to_string();
            for (k, v) in params {
                s.push_str(&format!(" {}={}", k, v));
            }
            s
        }
    }
}

/// `{name}` → its value; a placeholder with no value stays as it is.
pub fn fill(en: &str, params: &[(&str, String)]) -> String {
    let mut out = String::with_capacity(en.len() + 32);
    let b = en.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'{' {
            let start = i + 1;
            let mut j = start;
            while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == b'_') {
                j += 1;
            }
            if j > start && j < b.len() && b[j] == b'}' {
                let name = &en[start..j];
                match params.iter().find(|(k, _)| *k == name) {
                    Some((_, v)) => out.push_str(v),
                    None => out.push_str(&en[i..=j]),
                }
                i = j + 1;
                continue;
            }
        }
        // Copy one UTF-8 character.
        let ch = en[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// `msg!("eng.code", name = value, n = count)` — [`word`] with the values `to_string`ed.
#[macro_export]
macro_rules! msg {
    ($code:expr $(, $k:ident = $v:expr)* $(,)?) => {
        $crate::lang::word($code, &[$((stringify!($k), ($v).to_string())),*])
    };
}

/// The `{name}` placeholders in a string, each once, sorted — what a translation must
/// keep, in whatever order suits the language.
pub fn placeholders_of(s: &str) -> Vec<String> {
    let mut out = BTreeSet::new();
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'{' {
            let start = i + 1;
            let mut j = start;
            while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == b'_') {
                j += 1;
            }
            if j > start && j < b.len() && b[j] == b'}' {
                out.insert(s[start..j].to_string());
                i = j + 1;
                continue;
            }
        }
        i += 1;
    }
    out.into_iter().collect()
}

/// The tag names a string uses (`<b>`, `</b>` and `<br>` all count as `b` / `br`), so
/// a translation can be held to the same set.
fn tags_of(s: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'<' {
            let mut j = i + 1;
            if j < b.len() && b[j] == b'/' {
                j += 1;
            }
            let start = j;
            while j < b.len() && b[j].is_ascii_alphanumeric() {
                j += 1;
            }
            let name = s[start..j].to_ascii_lowercase();
            // Up to the closing angle bracket, whatever attributes there are.
            let mut k = j;
            while k < b.len() && b[k] != b'>' {
                k += 1;
            }
            out.insert(if name.is_empty() { "<".to_string() } else { name });
            i = k + 1;
            continue;
        }
        i += 1;
    }
    out
}

/// A line of a pack that cannot be used, and why. The line number is the one a person
/// opens the file at.
#[derive(Debug, Clone, PartialEq)]
pub struct Problem {
    pub line: usize,
    pub key: String,
    pub what: String,
}

/// A parsed pack: its header, the values that passed, and everything that did not.
#[derive(Debug, Clone, Default)]
pub struct Pack {
    pub code: String,
    pub name: String,
    pub author: String,
    pub made_for: String,
    pub notes: String,
    /// The usable translations, by key.
    pub text: BTreeMap<String, String>,
    /// Keys the catalogue does not have (kept in the file, not in `text`).
    pub orphans: Vec<String>,
    /// Lines refused, with their reasons.
    pub problems: Vec<Problem>,
    /// Catalogue keys with a non-empty translation.
    pub translated: usize,
    /// Catalogue keys in all.
    pub total: usize,
}

/// Parses a pack against the catalogue. `Err` is a file that cannot be read at all — not
/// TOML, or no `[language]` with a `code`; a file that reads gets a `Pack`, with every
/// line it could not use in `problems`.
pub fn parse_pack(text: &str) -> Result<Pack, String> {
    parse_pack_with(text, catalog())
}

pub fn parse_pack_with(text: &str, cat: &Catalog) -> Result<Pack, String> {
    let root = toml::parse(text)?;
    let lang = root
        .get("language")
        .and_then(|v| v.as_table())
        .ok_or_else(|| crate::msg!("eng.pack_no_language"))?;
    let s = |k: &str| lang.get(k).and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    let mut p = Pack { code: s("code"), name: s("name"), author: s("author"), made_for: s("made_for"), notes: s("notes"), ..Default::default() };
    check_code(&p.code)?;
    if p.name.is_empty() {
        p.name = p.code.clone();
    }
    p.total = cat.entries.len();
    let lines = key_lines(text);
    if let Some(t) = root.get("text").and_then(|v| v.as_table()) {
        for (key, val) in t {
            let line = lines.get(key).copied().unwrap_or(0);
            let v = match val.as_str() {
                Some(v) => v,
                None => {
                    p.problems.push(Problem { line, key: key.clone(), what: crate::msg!("eng.pack_not_string") });
                    continue;
                }
            };
            if v.is_empty() {
                continue; // untranslated: English shows
            }
            let reference = match cat.reference(key) {
                Some(r) => r,
                None => {
                    p.orphans.push(key.clone());
                    continue;
                }
            };
            let want = &reference.placeholders;
            let have = placeholders_of(v);
            if &have != want {
                let fmt = |x: &[String]| {
                    if x.is_empty() { "none".to_string() } else { x.iter().map(|s| format!("{{{}}}", s)).collect::<Vec<_>>().join(" ") }
                };
                p.problems.push(Problem {
                    line,
                    key: key.clone(),
                    what: crate::msg!("eng.pack_placeholders", english = fmt(want), pack = fmt(&have)),
                });
                continue;
            }
            let allowed = tags_of(&reference.en);
            let used = tags_of(v);
            if let Some(bad) = used.difference(&allowed).next() {
                p.problems.push(Problem {
                    line,
                    key: key.clone(),
                    what: if bad == "<" {
                        crate::msg!("eng.pack_stray_lt")
                    } else {
                        crate::msg!("eng.pack_tag", tag = bad)
                    },
                });
                continue;
            }
            if cat.entries.contains_key(key) {
                p.translated += 1;
            }
            p.text.insert(key.clone(), v.to_string());
        }
    }
    // In file order, which is the order a person fixes them in.
    p.problems.sort_by_key(|q| q.line);
    Ok(p)
}

/// The line each quoted key is defined on: `"a.b" = …` at the start of a line.
fn key_lines(text: &str) -> BTreeMap<String, usize> {
    let mut out = BTreeMap::new();
    for (i, line) in text.lines().enumerate() {
        let t = line.trim_start();
        if let Some(rest) = t.strip_prefix('"') {
            if let Some(end) = rest.find('"') {
                let key = &rest[..end];
                if rest[end + 1..].trim_start().starts_with('=') && !out.contains_key(key) {
                    out.insert(key.to_string(), i + 1);
                }
            }
        }
    }
    out
}

/// Whether a code can name a pack: something, and only letters, digits and hyphens —
/// the shape of a language tag, loosely.
pub fn check_code(code: &str) -> Result<(), String> {
    if code.is_empty() {
        return Err(crate::msg!("eng.pack_no_code"));
    }
    if !code.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err(crate::msg!("eng.pack_bad_code", code = format!("{:?}", code)));
    }
    Ok(())
}

/// What a pack file is called for a code: `gardenfell.sv.toml`.
pub fn file_for(code: &str) -> String {
    format!("gardenfell.{}.toml", code)
}

/// Whether a file in `languages\` is one of this program's: a pack, or the template.
/// What the store move carries along (round 18as); a `.bak` or a stray file is not.
pub fn is_pack_file(name: &str) -> bool {
    name.strip_prefix("gardenfell.").and_then(|r| r.strip_suffix(".toml")).is_some_and(|r| !r.is_empty() && !r.contains('.'))
}


/// A new pack for `code`, called `name`: the template with its `[language]` filled in and
/// every value empty — what "Create new language file" writes (round 18as). `en` is the
/// built-in wording itself, so it is refused; `en-GB` is a pack like any other.
pub fn new_pack(code: &str, name: &str) -> Result<String, String> {
    let code = code.trim();
    check_code(code)?;
    if code.eq_ignore_ascii_case("en") {
        return Err(crate::msg!("eng.pack_code_en"));
    }
    let name = name.trim();
    let name = if name.is_empty() { code } else { name };
    let header = format!("[language]\ncode = {}\nname = {}\n\n[text]\n", toml::escape(code), toml::escape(name));
    update_with(&header, TEMPLATE).map(|(text, _, _)| text)
}

/// The code a pack file name carries, or None for any other file.
pub fn code_of_file(name: &str) -> Option<String> {
    let rest = name.strip_prefix("gardenfell.")?.strip_suffix(".toml")?;
    if rest.is_empty() || rest == "en" || rest.contains('.') {
        return None; // the template is English; a `.bak` is not a pack
    }
    Some(rest.to_string())
}

/// One pack as `lang_list` reports it: the header, the coverage, and either the problems
/// or the reason the file could not be read.
pub struct Listed {
    pub file: PathBuf,
    pub code: String,
    pub pack: Result<Pack, String>,
}

/// Every pack in the folders, by the code its file name carries. A code in two folders is
/// the first folder's — the one beside the program.
pub fn list(dirs: &[PathBuf]) -> Vec<Listed> {
    let mut out: Vec<Listed> = Vec::new();
    for d in dirs {
        let rd = match std::fs::read_dir(d) {
            Ok(rd) => rd,
            Err(_) => continue,
        };
        let mut names: Vec<(String, PathBuf)> =
            rd.filter_map(|e| e.ok()).map(|e| (e.file_name().to_string_lossy().to_string(), e.path())).collect();
        names.sort();
        for (name, path) in names {
            let code = match code_of_file(&name) {
                Some(c) => c,
                None => continue,
            };
            if out.iter().any(|l| l.code == code) {
                continue;
            }
            let pack = std::fs::read_to_string(&path)
                .map_err(|e| format!("{}: {}", path.display(), e))
                .and_then(|t| parse_pack(&t));
            out.push(Listed { file: path, code, pack });
        }
    }
    out
}

/// The pack for a code, read and parsed, or why not.
pub fn get(dirs: &[PathBuf], code: &str) -> Result<(PathBuf, Pack), String> {
    let name = file_for(code);
    for d in dirs {
        let path = d.join(&name);
        if path.is_file() {
            let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {}", path.display(), e))?;
            return parse_pack(&text).map(|p| (path, p)).map_err(|e| format!("{}: {}", name, e));
        }
    }
    Err(crate::msg!("eng.pack_not_found", file = name, dirs = dirs.iter().map(|d| d.display().to_string()).collect::<Vec<_>>().join(" or ")))
}


/// A pack refreshed against the current catalogue: the template's every line, with the
/// pack's values filled in where it has them, its `[language]` kept, and the keys the
/// catalogue no longer has listed at the end — kept, not dropped, since a key can come
/// back. What is returned is the new text and the count of keys that were new to it.
pub fn update(pack_text: &str) -> Result<(String, usize, Vec<String>), String> {
    update_with(pack_text, TEMPLATE)
}

pub fn update_with(pack_text: &str, template: &str) -> Result<(String, usize, Vec<String>), String> {
    let root = toml::parse(pack_text)?;
    let lang = root.get("language").and_then(|v| v.as_table()).ok_or_else(|| crate::msg!("eng.pack_no_language"))?;
    let values: BTreeMap<String, String> = root
        .get("text")
        .and_then(|v| v.as_table())
        .map(|t| t.iter().filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string()))).collect())
        .unwrap_or_default();
    let mut out = String::with_capacity(template.len() + 4096);
    let mut used = BTreeSet::new();
    let mut added = 0usize;
    let mut in_language = false;
    for line in template.lines() {
        let t = line.trim_start();
        if t == "[language]" {
            in_language = true;
            out.push_str(line);
            out.push('\n');
            continue;
        }
        if t.starts_with('[') {
            in_language = false;
        }
        if in_language {
            // `code     = "en"        # …` → the pack's own value, the comment kept.
            if let Some((k, _)) = t.split_once('=') {
                let k = k.trim();
                if let Some(v) = lang.get(k).and_then(|v| v.as_str()) {
                    let comment = line.find('#').map(|i| &line[i..]).unwrap_or("");
                    let pad = if comment.is_empty() { String::new() } else { "     ".to_string() };
                    out.push_str(&format!("{:<8} = {}{}{}\n", k, toml::escape(v), pad, comment));
                    continue;
                }
            }
            out.push_str(line);
            out.push('\n');
            continue;
        }
        if let Some(rest) = t.strip_prefix('"') {
            if let Some(end) = rest.find('"') {
                let key = &rest[..end];
                if rest[end + 1..].trim_start().starts_with("= \"\"") {
                    match values.get(key) {
                        Some(v) => {
                            out.push_str(&format!("\"{}\" = {}\n", key, toml::escape(v)));
                            used.insert(key.to_string());
                        }
                        None => {
                            added += 1;
                            out.push_str(line);
                            out.push('\n');
                        }
                    }
                    /* A form the English has not got — `.few`, `.many` for a language
                       with three — sits under its base's last form rather than among
                       the orphans: it is a translation of a key that exists. */
                    if let Some(base) = key.strip_suffix(".other") {
                        for form in [".one", ".few", ".many"] {
                            let extra = format!("{}{}", base, form);
                            if !used.contains(&extra) && values.contains_key(&extra) && !template_has(template, &extra) {
                                out.push_str(&format!("\"{}\" = {}\n", extra, toml::escape(&values[&extra])));
                                used.insert(extra);
                            }
                        }
                    }
                    continue;
                }
            }
        }
        out.push_str(line);
        out.push('\n');
    }
    let orphans: Vec<String> = values.keys().filter(|k| !used.contains(*k)).cloned().collect();
    if !orphans.is_empty() {
        out.push_str("\n# ── Keys this version of Gardenfell no longer has ─────────────────────────────\n");
        out.push_str("# Kept in case they come back; nothing reads them.\n\n");
        for k in &orphans {
            out.push_str(&format!("\"{}\" = {}\n", k, toml::escape(&values[k])));
        }
    }
    Ok((out, added, orphans))
}

/// Whether the template defines `key` on a line of its own.
fn template_has(template: &str, key: &str) -> bool {
    let needle = format!("\"{}\" = ", key);
    template.lines().any(|l| l.trim_start().starts_with(&needle))
}

/* ---- the wire ------------------------------------------------------------------- */

fn problems_json(ps: &[Problem]) -> String {
    let items: Vec<String> = ps
        .iter()
        .map(|p| {
            let mut o = J::obj();
            o.int("line", p.line as u64).str("key", &p.key).str("what", &p.what);
            o.done()
        })
        .collect();
    format!("[{}]", items.join(","))
}

/// A listed pack, for the picker: header, coverage, problems, or the read error.
pub fn listed_json(l: &Listed) -> String {
    let mut o = J::obj();
    o.str("file", &l.file.display().to_string()).str("code", &l.code);
    match &l.pack {
        Ok(p) => {
            o.str("name", &p.name)
                .str("author", &p.author)
                .str("madeFor", &p.made_for)
                .str("notes", &p.notes)
                .int("translated", p.translated as u64)
                .int("total", p.total as u64)
                .int("orphans", p.orphans.len() as u64)
                .raw("problems", &problems_json(&p.problems));
        }
        Err(e) => {
            o.str("name", &l.code).str("error", e);
        }
    }
    o.done()
}

/// The whole pack, for the page to apply: the header, every usable value, the problems.
pub fn pack_json(path: &Path, p: &Pack) -> String {
    let mut text = J::obj();
    for (k, v) in &p.text {
        text.str(k, v);
    }
    let mut o = J::obj();
    o.str("file", &path.display().to_string())
        .str("code", &p.code)
        .str("name", &p.name)
        .str("author", &p.author)
        .str("madeFor", &p.made_for)
        .str("notes", &p.notes)
        .int("translated", p.translated as u64)
        .int("total", p.total as u64)
        .raw("orphans", &json::string_array(&p.orphans))
        .raw("problems", &problems_json(&p.problems))
        .raw("text", &text.done());
    o.done()
}
