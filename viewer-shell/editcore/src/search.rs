//! The Editor's searches: the Object Window's search by any field, the records of a type
//! as the patch leaves them (for the columns), and text found and replaced in every record
//! type at once - `wraithguard/patch/editor.py`'s `search` and `column_overlay`, and
//! `workflow.py`'s `search_all`, `replace_all` and `verify.as_written` (port step 4.1,
//! slice 5a). Cases: `tests/fixtures/search_cases.json`, run here and by
//! `tests/test_search_cases.py`.

use indexmap::IndexMap;
use serde_json::{json, Value};

use crate::columns::{browser_table, settings_from, table};
use crate::edit::{flatten, set_at, value_at, EditError, Editor};
use crate::key::{type_name, type_of, TYPES};
use crate::pyval;
use crate::queue::Choice;
use crate::session::Found;

/// A search's words: spaces part them, except inside quotes (`shlex.split`'s POSIX
/// rules); an unclosed quote takes the words as they are. Empty words are dropped.
pub fn split_query(query: &str) -> Vec<String> {
    fn shlex(s: &str) -> Option<Vec<String>> {
        let mut out = Vec::new();
        let mut word: Option<String> = None;
        let mut chars = s.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                ' ' | '\t' | '\n' | '\r' => {
                    if let Some(w) = word.take() {
                        out.push(w);
                    }
                }
                '\\' => word.get_or_insert_default().push(chars.next()?),
                '\'' => {
                    let w = word.get_or_insert_default();
                    loop {
                        match chars.next()? {
                            '\'' => break,
                            ch => w.push(ch),
                        }
                    }
                }
                '"' => {
                    let w = word.get_or_insert_default();
                    loop {
                        match chars.next()? {
                            '"' => break,
                            '\\' if matches!(chars.peek(), Some('\\' | '"')) => w.push(chars.next()?),
                            ch => w.push(ch),
                        }
                    }
                }
                ch => word.get_or_insert_default().push(ch),
            }
        }
        out.extend(word);
        Some(out)
    }
    let words = shlex(query).unwrap_or_else(|| query.split_whitespace().map(str::to_string).collect());
    words.into_iter().filter(|w| !w.trim().is_empty()).collect()
}

/// One search word: `(path, operator, value)`; a bare word is `("", ":", word)`.
pub fn parse_term(word: &str) -> (String, String, String) {
    static TERM: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = TERM.get_or_init(|| regex::Regex::new(r"^([A-Za-z_][\w.]*)(!=|>=|<=|=|>|<|:)(.*)$").expect("a pattern"));
    match re.captures(word) {
        Some(c) => (c[1].to_lowercase(), c[2].to_string(), c[3].to_string()),
        None => (String::new(), ":".into(), word.to_string()),
    }
}

/// A value as the search reads it: lists and groups by their entries, nothing for null.
fn as_text(v: &Value) -> String {
    match v {
        Value::Array(a) => a.iter().map(as_text).collect::<Vec<_>>().join(" "),
        Value::Object(m) => m.values().map(as_text).collect::<Vec<_>>().join(" "),
        Value::Null => String::new(),
        other => pyval::str_of(other),
    }
}

/// The first field of a flattened record a search word matches, as `path: value`.
fn match_term(term: &(String, String, String), flat: &[(String, Value)]) -> Option<String> {
    let (path, op, want) = term;
    let low = want.to_lowercase();
    for (k, v) in flat {
        if !path.is_empty() && !k.to_lowercase().contains(path.as_str()) {
            continue;
        }
        let text = as_text(v);
        let hit = match op.as_str() {
            ":" => if low.is_empty() { !text.is_empty() } else { text.to_lowercase().contains(&low) },
            "=" | "!=" => (text.to_lowercase() == low) == (op == "="),
            _ => {
                let (Some(a), Some(b)) = (pyval::float_of(&text), pyval::float_of(want)) else { continue };
                match op.as_str() {
                    ">" => a > b,
                    "<" => a < b,
                    ">=" => a >= b,
                    _ => a <= b,
                }
            }
        };
        if hit {
            let shown = if text.chars().count() <= 80 { text } else { text.chars().take(79).collect::<String>() + "..." };
            return Some(format!("{k}: {shown}"));
        }
    }
    None
}

/// Text found in a value (any case), as Python's `re.IGNORECASE` finds it, and only as a
/// whole word when `whole` (no word character either side).
pub struct Finder {
    re: regex::Regex,
    whole: bool,
}

pub(crate) fn word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

impl Finder {
    pub fn new(text: &str, whole: bool) -> Finder {
        Finder { re: regex::Regex::new(&format!("(?i){}", regex::escape(text))).expect("an escaped pattern"), whole }
    }

    /// The next match at or after `from`, as `(start, end)`.
    fn next(&self, hay: &str, mut from: usize) -> Option<(usize, usize)> {
        while from <= hay.len() {
            let m = self.re.find_at(hay, from)?;
            let ok = !self.whole
                || (!hay[..m.start()].chars().next_back().is_some_and(word_char) && !hay[m.end()..].chars().next().is_some_and(word_char));
            if ok {
                return Some((m.start(), m.end()));
            }
            from = m.start() + hay[m.start()..].chars().next().map_or(1, char::len_utf8);
        }
        None
    }

    pub fn found(&self, hay: &str) -> bool {
        self.next(hay, 0).is_some()
    }

    /// How many times it is in `hay`, the matches not overlapping (Python's `findall`).
    pub fn count(&self, hay: &str) -> usize {
        let (mut n, mut at) = (0, 0);
        while let Some((s, e)) = self.next(hay, at) {
            if e == s {
                break;
            }
            n += 1;
            at = e;
        }
        n
    }

    /// `hay` with every match made `by`.
    pub fn replace(&self, hay: &str, by: &str) -> String {
        let (mut out, mut at) = (String::new(), 0);
        while let Some((s, e)) = self.next(hay, at) {
            if e == s {
                break; // nothing to find (the caller refuses empty text)
            }
            out.push_str(&hay[at..s]);
            out.push_str(by);
            at = e;
        }
        out.push_str(&hay[at..]);
        out
    }

    /// A copy of a value with the text replaced in every string in it.
    pub fn replaced(&self, v: &Value, by: &str) -> Value {
        match v {
            Value::String(s) => json!(self.replace(s, by)),
            Value::Array(a) => Value::Array(a.iter().map(|x| self.replaced(x, by)).collect()),
            Value::Object(m) => Value::Object(m.iter().map(|(k, x)| (k.clone(), self.replaced(x, by))).collect()),
            other => other.clone(),
        }
    }
}

/// Every text field of a record (lists' entries included), as `(path, text)`; a record's
/// type, a cell's references and a script's bytecode left out.
fn strings(v: &Value, path: &str, out: &mut Vec<(String, String)>) {
    match v {
        Value::String(s) => out.push((path.to_string(), s.clone())),
        Value::Object(m) => {
            for (k, x) in m {
                if !["type", "references", "bytecode"].contains(&k.as_str()) {
                    strings(x, &if path.is_empty() { k.clone() } else { format!("{path}.{k}") }, out);
                }
            }
        }
        Value::Array(a) => {
            for (i, x) in a.iter().enumerate() {
                strings(x, &if path.is_empty() { i.to_string() } else { format!("{path}.{i}") }, out);
            }
        }
        _ => {}
    }
}

impl Editor {
    /// A type's records as the patch leaves them: every plugin's winning version (a
    /// deleting one too, as the Python's), the queue's typed values applied, and the
    /// records the patch makes; in the order they are first met.
    fn winners(&self, tag: &str, record_type: &str) -> IndexMap<String, Value> {
        let mut out: IndexMap<String, Value> = IndexMap::new();
        for name in self.session.order() {
            for (k, r) in self.session.records(name, tag).iter() {
                if type_name(r) == record_type {
                    out.insert(k.clone(), r.clone());
                }
            }
        }
        for ((t, key), choices) in self.queue.fields().iter() {
            if t != record_type {
                continue;
            }
            if let Some(rec) = out.get_mut(&key.to_lowercase()) {
                for c in choices {
                    if let Choice::Value { path, value } = c {
                        let _ = set_at(rec, path, value.clone());
                    }
                }
            }
        }
        for made in self.queue.new_records() {
            if made.record_type == record_type {
                out.insert(made.key.to_lowercase(), made.record.clone());
            }
        }
        out
    }

    /// The records of a type as the patch would leave them, for the Object Window's
    /// columns: only what the queue touches (a record taken whole, a field taken or
    /// typed, the patch's own records), by lower-case key.
    pub fn column_overlay(&self, tag: &str) -> IndexMap<String, Value> {
        let tag = tag4(tag);
        let Some(record_type) = type_of(&tag) else { return IndexMap::new() };
        let mut winners: IndexMap<String, Value> = IndexMap::new();
        for name in self.session.order() {
            for (k, r) in self.session.records(name, &tag).iter() {
                if type_name(r) == record_type {
                    winners.insert(k.clone(), r.clone());
                }
            }
        }
        let mut out: IndexMap<String, Value> = IndexMap::new();
        for sel in self.queue.selections() {
            if sel.record_type != record_type {
                continue;
            }
            if let Some(rec) = self.session.records(&sel.plugin, &tag).get(&sel.key.to_lowercase()) {
                out.insert(sel.key.to_lowercase(), rec.clone());
            }
        }
        for ((t, key), choices) in self.queue.fields().iter() {
            if t != record_type {
                continue;
            }
            let low = key.to_lowercase();
            let Some(base) = out.get(&low).or_else(|| winners.get(&low)).cloned() else { continue };
            let mut rec = base;
            for c in choices {
                match c {
                    Choice::Value { path, value } => {
                        let _ = set_at(&mut rec, path, value.clone());
                    }
                    Choice::Plugin { path, plugin } => {
                        if let Some(v) = self.session.records(plugin, &tag).get(&low).and_then(|o| value_at(o, path)).cloned() {
                            let _ = set_at(&mut rec, path, v);
                        }
                    }
                }
            }
            out.insert(low, rec);
        }
        for made in self.queue.new_records() {
            if made.record_type == record_type {
                out.insert(made.key.to_lowercase(), made.record.clone());
            }
        }
        out
    }

    /// The Object Window's columns for a type (`editColumns`): every plugin's records,
    /// the patch's overlay last; armor with its weight classes (`armorClass`).
    pub fn columns(&self, tag: &str) -> Value {
        let tag = tag4(tag.trim());
        let mut versions: Vec<IndexMap<String, Value>> = self.session.order().iter().map(|n| (*self.session.records(n, &tag)).clone()).collect();
        versions.push(self.column_overlay(&tag));
        let gmst: Vec<IndexMap<String, Value>> = if tag == "ARMO" { self.session.order().iter().map(|n| (*self.session.records(n, "GMST")).clone()).collect() } else { Vec::new() };
        let settings = settings_from(gmst.iter());
        let mut out = table(&tag, &versions, &settings);
        if tag == "ARMO" {
            out["armorClass"] = browser_table(&settings);
        }
        out
    }

    /// The Object Window's search by any field: the records of a type whose fields match
    /// every word (`text`, `path:text`, `path=value`, `path!=value`, `path>n`...).
    /// `{hits: {id: "path: value"}, count, capped}`.
    pub fn search(&self, tag: &str, query: &str, limit: usize) -> Result<Value, EditError> {
        let tag = tag4(tag);
        let Some(record_type) = type_of(&tag) else {
            return Err(EditError(format!("{tag} is not a record type")));
        };
        let terms: Vec<_> = split_query(query).iter().map(|w| parse_term(w)).collect();
        if terms.is_empty() {
            return Ok(json!({"hits": {}, "count": 0, "capped": false}));
        }
        let mut hits = serde_json::Map::new();
        let mut count = 0usize;
        for (key, rec) in self.winners(&tag, record_type) {
            let flat = flatten(&rec);
            let mut first = String::new();
            let mut all = true;
            for term in &terms {
                match match_term(term, &flat) {
                    Some(got) => {
                        if first.is_empty() {
                            first = got;
                        }
                    }
                    None => {
                        all = false;
                        break;
                    }
                }
            }
            if all {
                count += 1;
                if hits.len() < limit {
                    hits.insert(key, json!(first));
                }
            }
        }
        Ok(json!({"hits": hits, "count": count, "capped": count > limit}))
    }

    /// Text found in every record type at once (OpenMW-CS's global search):
    /// `{hits: [{tag, id, path, value}], count, capped}`.
    pub fn search_all(&self, text: &str, whole: bool, limit: usize) -> Value {
        let t = text.trim();
        if t.is_empty() {
            return json!({"hits": [], "count": 0, "capped": false});
        }
        let finder = Finder::new(t, whole);
        let mut hits = Vec::new();
        let mut count = 0usize;
        let mut types: Vec<(&str, &str)> = TYPES.iter().map(|(n, g)| (*g, *n)).collect();
        types.sort();
        for (tag, rtype) in types {
            if ["TES3", "LAND", "PGRD"].contains(&tag) {
                continue;
            }
            let mut recs: Vec<(String, Value)> = self.winners(tag, rtype).into_iter().collect();
            recs.sort_by(|a, b| a.0.cmp(&b.0));
            for (key, rec) in recs {
                let mut found = Vec::new();
                strings(&rec, "", &mut found);
                for (path, value) in found {
                    if finder.found(&value) {
                        count += 1;
                        if hits.len() < limit {
                            let id = ["id", "name"].iter().filter_map(|f| rec.get(*f)).find(|v| crate::columns::truthy(v)).map(pyval::str_of).unwrap_or_else(|| key.clone());
                            hits.push(json!({"tag": tag, "id": id, "path": path, "value": value}));
                        }
                    }
                }
            }
        }
        json!({"hits": hits, "count": count, "capped": count > limit})
    }

    /// A record as the patch would write it: its base (the whole record queued, else the
    /// winner), every queued field applied.
    pub fn as_written(&self, found: &Found) -> Value {
        let whole = self.queue.selections().iter().find(|s| s.record_type == found.record_type && s.key == found.key).map(|s| s.plugin.clone());
        let version = |p: &str| found.versions.iter().rev().find(|(n, _)| n == p).map(|(_, r)| r);
        let mut record = whole.as_deref().and_then(version).unwrap_or(found.record()).clone();
        for c in self.queue.fields().get(&(found.record_type.clone(), found.key.clone())).into_iter().flatten() {
            let value = match c {
                Choice::Value { value, .. } => value.clone(),
                Choice::Plugin { path, plugin } => match version(plugin).and_then(|r| value_at(r, path)) {
                    Some(v) => v.clone(),
                    None => continue,
                },
            };
            let _ = set_at(&mut record, c.path(), value);
        }
        record
    }

    /// The text replaced in the fields found (`search_all`'s hits, the ones chosen), as
    /// one change: `{changed, failed: [{tag, id, path, error}]}`. A record's id is never
    /// changed here (Search & Replace repoints what names it).
    pub fn replace_all(&mut self, hits: &[Value], text: &str, by: &str, whole: bool) -> Value {
        let finder = Finder::new(text, whole);
        let (mut changed, mut failed) = (0, Vec::new());
        let _ = self.batch(|ed| {
            for h in hits {
                match ed.replace_one(h, &finder, by) {
                    None => changed += 1,
                    Some(error) => {
                        let field = |k: &str| h.get(k).map(pyval::str_of).unwrap_or_default();
                        failed.push(json!({"tag": field("tag"), "id": field("id"), "path": field("path"), "error": error}));
                    }
                }
            }
            Ok(())
        });
        json!({"changed": changed, "failed": failed})
    }

    /// One field's text replaced (through `set_field`); the refusal as text, or `None`.
    fn replace_one(&mut self, hit: &Value, finder: &Finder, by: &str) -> Option<String> {
        let get = |k: &str| hit.get(k).filter(|v| crate::columns::truthy(v)).map(pyval::str_of).unwrap_or_default();
        let (path, tag, id) = (get("path"), get("tag"), get("id"));
        if (path == "id" || path == "name") && tag == "CELL" {
            return Some("a cell's name is its id here".into());
        }
        if path == "id" {
            return Some("an id is changed with Search & Replace (it repoints what names it)".into());
        }
        let Some(found) = self.find(&tag, &id, None) else { return Some("no plugin defines it".into()) };
        // A list's entry: the list is the field.
        let parts: Vec<&str> = path.split('.').collect();
        let top = match parts.iter().position(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit())) {
            Some(i) => parts[..i].join("."),
            None => path.clone(),
        };
        let rec = self.as_written(&found);
        let Some(value) = value_at(&rec, &top).cloned() else { return Some("the field is not there".into()) };
        let new_value = finder.replaced(&value, by);
        if pyval::eq(&new_value, &value) {
            return Some("the text is no longer there".into());
        }
        self.set_field(&found, &top, &new_value).err().map(|e| e.0)
    }
}

/// A tag as four upper-case letters (`npc` -> `NPC_`).
fn tag4(tag: &str) -> String {
    let mut t = tag.to_uppercase();
    while t.len() < 4 {
        t.push('_');
    }
    t.chars().take(4).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_split_and_terms_read_as_python_reads_them() {
        assert_eq!(split_query(r#"a "b c" d'e f' \"x"#), vec!["a", "b c", "de f", "\"x"]);
        assert_eq!(split_query("open \"quote"), vec!["open", "\"quote"]);
        assert_eq!(parse_term("data.weight>=5"), ("data.weight".into(), ">=".into(), "5".into()));
        assert_eq!(parse_term("Name:iron"), ("name".into(), ":".into(), "iron".into()));
        assert_eq!(parse_term("plain"), ("".into(), ":".into(), "plain".into()));
        let f = Finder::new("ore", true);
        assert!(!f.found("more") && f.found("an ore vein"));
        assert_eq!(Finder::new("a a", true).replace("xa a a", "B"), "xa B");
        assert_eq!(Finder::new("Iron", false).replace("IRON iron", "steel"), "steel steel");
    }
}
