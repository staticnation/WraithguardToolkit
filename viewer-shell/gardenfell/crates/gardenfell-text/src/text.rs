//! The engine's own words: the catalogue of every key the page can say (embedded at build
//! time), and `msg!`, which words a message from it so the page can match it back. Round
//! 18ge: moved out of `lang.rs` so that the install reader, the validator and the store
//! can word a message without depending on the language-pack files, which sit above them.

use std::collections::{BTreeMap, BTreeSet};

use crate::json;

/// The page's catalogue, as `catalog.js` writes it. The build embeds it, so the engine
/// and the page it ships with always agree on what a key is.
pub(crate) const CATALOG_JSON: &str = include_str!("../../../page-source/catalog.json");
/// The translator's template, the same file `catalog.js` commits. Embedded so that
/// `new_pack()` can start a pack from it anywhere the program runs, and so that
/// `update()` can refresh a pack against it line by line — every comment kept, every key
/// in its place.
pub const TEMPLATE: &str = include_str!("../../../languages/gardenfell.en.toml");


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
    /// Whether a pack key names anything at all — an entry, or a form of one.
    pub fn knows(&self, key: &str) -> bool {
        self.reference(key).is_some()
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
        $crate::text::word($code, &[$((stringify!($k), ($v).to_string())),*])
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
pub fn tags_of(s: &str) -> BTreeSet<String> {
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

