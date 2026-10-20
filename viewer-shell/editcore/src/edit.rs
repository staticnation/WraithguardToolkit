//! The Editor's records: what the record dialog shows, and the changes made to them -
//! `wraithguard/patch/editor.py`'s `EditorSession.view`, `set_field` (with `coerce`),
//! `set_many`, `batch`, `revert`, `insert`, `duplicate`, `pending`, `save_journal`,
//! `undo` and `redo`, rule for rule and message for message (port step 4.1, slice 4).
//! Cases: `tests/fixtures/edit_cases.json`, run here and by `tests/test_edit_cases.py`.
//!
//! Placed objects, path grids and new cells and land are the rest of slice 4.

use std::path::PathBuf;

use serde_json::{json, Map, Value};

use crate::fields::{enum_options, field_kind, flag_options, flags_name, int_bounds};
use crate::key::{tag_of, type_of};
use crate::pyval::{self, Kind};
use crate::queue::{Choice, History, NewRecord, PatchQueue};
use crate::session::{Found, Session, PATCH};

/// Fields that name a record, not change it.
pub const IDENTITY: [&str; 4] = ["type", "id", "grid", "data.grid"];

/// Fields the editor shows but does not change: a cell's references are one list per
/// cell, and writing it would re-assert every reference that version lists. A changed
/// reference is written alone instead.
pub const READ_ONLY: [&str; 1] = ["references"];

/// Fields that hold script source: a script's text, a response's result.
pub const SCRIPT_TEXT: [(&str, &str); 2] = [("Script", "text"), ("DialogueInfo", "script_text")];

/// Types "Make a copy as" and "New" refuse (a script is the exception: both take care of
/// the `begin` line that names it).
pub const UNCOPYABLE: [&str; 12] = [
    "Header", "Cell", "Landscape", "PathGrid", "Dialogue", "DialogueInfo", "Skill", "MagicEffect", "GameSetting",
    "LandscapeTexture", "Script", "StartScript",
];

/// The longest id the game reads (32 bytes with the terminator).
pub const MAX_ID: usize = 31;

/// A change the Editor refuses, with the reason to show.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditError(pub String);

impl std::fmt::Display for EditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for EditError {}

pub(crate) fn err<T>(m: impl Into<String>) -> Result<T, EditError> {
    Err(EditError(m.into()))
}

fn uncopyable(record_type: &str) -> bool {
    record_type != "Script" && UNCOPYABLE.contains(&record_type)
}

/// Script source with CRLF line ends, as the Construction Set writes it (a bare LF has
/// broken scripts in the game: a line ending in a comment took the next one with it).
pub fn crlf(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n").replace('\n', "\r\n")
}

/// A record's fields as dotted paths, lists kept whole, in the record's order (the rule
/// the conflict viewer and the queue name fields by). An empty group has no fields.
pub fn flatten(record: &Value) -> Vec<(String, Value)> {
    fn walk(v: &Map<String, Value>, parent: &str, out: &mut Vec<(String, Value)>) {
        for (k, x) in v {
            let path = if parent.is_empty() { k.clone() } else { format!("{parent}.{k}") };
            match x {
                Value::Object(m) => walk(m, &path, out),
                other => out.push((path, other.clone())),
            }
        }
    }
    let mut out = Vec::new();
    if let Value::Object(m) = record {
        walk(m, "", &mut out);
    }
    out
}

/// A dotted path's value, or `None` when the path is not there (a null is a value).
pub fn value_at<'a>(record: &'a Value, path: &str) -> Option<&'a Value> {
    let mut cur = record;
    for part in path.split('.') {
        cur = cur.as_object()?.get(part)?;
    }
    Some(cur)
}

/// Writes a dotted path, making the groups on the way. Refused when the path runs
/// through something that is not a group.
pub fn set_at(record: &mut Value, path: &str, value: Value) -> Result<(), EditError> {
    let parts: Vec<&str> = path.split('.').collect();
    let mut cur = record;
    for part in &parts[..parts.len() - 1] {
        let map = cur.as_object_mut().ok_or_else(|| EditError(format!("cannot set {path}: {part} is not a group of fields in this record.")))?;
        let next = map.entry(part.to_string()).or_insert(Value::Null);
        if next.is_null() {
            *next = Value::Object(Map::new());
        }
        if !next.is_object() {
            return err(format!("cannot set {path}: {part} is not a group of fields in this record."));
        }
        cur = next;
    }
    cur.as_object_mut()
        .ok_or_else(|| EditError(format!("cannot set {path}: it is not a group of fields in this record.")))?
        .insert(parts[parts.len() - 1].to_string(), value);
    Ok(())
}

/// The existing entry a new one is checked against: one with the same fields, for a list
/// whose entries differ in kind (AI packages), else the first.
fn template_for<'a>(current: &'a [Value], entry: &Value) -> &'a Value {
    if let Value::Object(e) = entry {
        for have in current {
            if let Value::Object(h) = have
                && h.len() == e.len()
                && h.keys().all(|k| e.contains_key(k))
            {
                return have;
            }
        }
    }
    &current[0]
}

/// One entry of a list, checked by the shape of its template.
fn coerce_entry(template: &Value, entry: &Value) -> Result<Value, EditError> {
    if let Value::Array(t) = template {
        let Some(e) = entry.as_array().filter(|e| e.len() == t.len()) else {
            return err(format!("an entry here is {} values, not {}", t.len(), pyval::repr(entry)));
        };
        return Ok(Value::Array(t.iter().zip(e).map(|(a, b)| coerce(a, b, None)).collect::<Result<_, _>>()?));
    }
    coerce(template, entry, None)
}

/// A value the viewer sent, as the type the field holds: read by the field's current
/// value (its type is the field's), an int checked against the kind's range. A list is
/// checked entry by entry against the shape of its entries, a group against its fields.
pub fn coerce(current: &Value, raw: &Value, kind: Option<&str>) -> Result<Value, EditError> {
    let text = || pyval::str_of(raw);
    match pyval::kind(current) {
        Kind::Bool => {
            if let Value::Bool(b) = raw {
                return Ok(Value::Bool(*b));
            }
            match text().trim().to_lowercase().as_str() {
                "true" | "1" | "yes" => Ok(Value::Bool(true)),
                "false" | "0" | "no" => Ok(Value::Bool(false)),
                _ => err(format!("{} is not true or false", pyval::repr(raw))),
            }
        }
        Kind::Int => {
            let Some(n) = pyval::float_of(&text()).filter(|n| n.is_finite() && n.fract() == 0.0) else {
                return err(format!("{} is not a whole number", pyval::repr(raw)));
            };
            let value = n as i128;
            if let Some((lo, hi)) = kind.and_then(int_bounds)
                && !(lo..=hi).contains(&value)
            {
                return err(format!("{value} is outside {lo}..{hi}"));
            }
            Ok(pyval::int_value(value))
        }
        Kind::Float => match pyval::float_of(&text()) {
            Some(x) => Ok(pyval::float_value(x)),
            None => err(format!("{} is not a number", pyval::repr(raw))),
        },
        Kind::List => {
            let Value::Array(r) = raw else {
                return err(format!("this field holds a list, not {}", pyval::repr(raw)));
            };
            let cur = current.as_array().expect("a list");
            if cur.is_empty() {
                return Ok(raw.clone()); // nothing to say what an entry is
            }
            Ok(Value::Array(r.iter().map(|e| coerce_entry(template_for(cur, e), e)).collect::<Result<_, _>>()?))
        }
        Kind::Dict => {
            let Value::Object(r) = raw else {
                return err(format!("this field holds a group, not {}", pyval::repr(raw)));
            };
            let cur = current.as_object().expect("a group");
            let mut unknown: Vec<&String> = r.keys().filter(|k| !cur.contains_key(*k)).collect();
            if !unknown.is_empty() {
                unknown.sort();
                return err(format!("{} is not part of this entry", unknown.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")));
            }
            let mut out = Map::new();
            for (k, c) in cur {
                out.insert(k.clone(), coerce(c, r.get(k).unwrap_or(c), None)?);
            }
            Ok(Value::Object(out))
        }
        Kind::Str => Ok(match raw {
            Value::String(_) => raw.clone(),
            other => Value::String(pyval::str_of(other)),
        }),
        Kind::None => Ok(raw.clone()),
    }
}

/// The Editor's session: the load order read, the patch queue, its undo history and its
/// journal.
pub struct Editor {
    pub session: Session,
    pub queue: PatchQueue,
    pub history: History,
    /// Where the queue is saved after every change (`None`: nowhere).
    pub journal: Option<PathBuf>,
    pub(crate) batching: bool,
    /// The dialogue's views, read once for the patch as it is (`crate::dialogue_graph`).
    pub(crate) dial_index: std::sync::Mutex<Option<(String, std::sync::Arc<crate::dialogue_graph::DialogueIndex>)>>,
    /// The script compiler, read once for the patch's own records as they are.
    pub(crate) compiler: std::sync::Mutex<Option<(String, std::sync::Arc<crate::mwscript::ScriptCompiler>)>>,
    /// The patch last written (`crate::build`), where the Build patch panel suggests.
    pub last_patch: Option<PathBuf>,
}

impl Editor {
    pub fn new(session: Session, queue: PatchQueue, journal: Option<PathBuf>) -> Editor {
        let history = History::new(&queue);
        Editor { session, queue, history, journal, batching: false, dial_index: std::sync::Mutex::default(), compiler: std::sync::Mutex::default(), last_patch: None }
    }

    /// A record, from every plugin that defines it, or the patch's own.
    pub fn find(&self, tag: &str, rid: &str, plugins: Option<&[String]>) -> Option<Found> {
        self.session.find_in(tag, rid, plugins, Some(&self.queue))
    }

    /// What the record dialog shows: `{tag, type, id, winner, plugins, whole, new,
    /// fields}`, each field `{path, value, editable, kind, options}` with `count` for a
    /// list and `queued` and `source` (a plugin, or "typed") when the patch changes it.
    pub fn view(&self, found: &Found) -> Value {
        let choices = self.queue.fields().get(&(found.record_type.clone(), found.key.clone()));
        let by_plugin: Vec<(&str, Vec<(String, Value)>)> = found.versions.iter().map(|(p, r)| (p.as_str(), flatten(r))).collect();
        let mut fields = Vec::new();
        for (path, value) in flatten(found.record()) {
            let kind = field_kind(&found.record_type, &path);
            let mut options: Vec<String> = enum_options(&path).to_vec();
            if let Some(name) = kind.and_then(flags_name) {
                options = flag_options(name).to_vec();
            }
            let editable = !IDENTITY.contains(&path.as_str()) && !READ_ONLY.contains(&path.as_str()) && !path.starts_with("references");
            let mut item = json!({
                "path": path,
                "value": value,
                "editable": editable,
                "kind": kind.map(str::to_string).unwrap_or_else(|| pyval::type_name(&value).to_string()),
                "options": options,
            });
            if let Value::Array(a) = &value {
                item["count"] = json!(a.len());
            }
            match choices.and_then(|cs| cs.iter().find(|c| c.path() == path)) {
                Some(Choice::Value { value, .. }) => {
                    item["queued"] = value.clone();
                    item["source"] = json!("typed");
                }
                Some(Choice::Plugin { plugin, .. }) => {
                    let v = by_plugin.iter().find(|(p, _)| p == plugin).and_then(|(_, f)| f.iter().find(|(k, _)| *k == path)).map(|(_, v)| v.clone());
                    item["queued"] = v.unwrap_or(Value::Null);
                    item["source"] = json!(plugin);
                }
                None => {}
            }
            fields.push(item);
        }
        let whole = self.queue.selections().iter().find(|s| s.record_type == found.record_type && s.key == found.key).map(|s| s.plugin.clone());
        json!({
            "tag": found.tag,
            "type": found.record_type,
            "id": found.key,
            "winner": found.winner(),
            "plugins": found.versions.iter().map(|(p, _)| p.clone()).collect::<Vec<_>>(),
            "whole": whole,
            "new": found.new,
            "fields": fields,
        })
    }

    /// Queues a typed value for one field (and saves the journal). A value equal to what
    /// the load order has drops the change instead.
    pub fn set_field(&mut self, found: &Found, path: &str, raw: &Value) -> Result<(), EditError> {
        if IDENTITY.contains(&path) || READ_ONLY.contains(&path) || path.starts_with("references") {
            return err(format!("{path} cannot be changed here"));
        }
        let mut raw = raw.clone();
        if SCRIPT_TEXT.contains(&(found.record_type.as_str(), path))
            && let Value::String(s) = &raw
        {
            raw = Value::String(crlf(s));
        }
        let Some(current) = value_at(found.record(), path) else {
            return err(format!("{} has no field {path}", found.key));
        };
        let value = coerce(current, &raw, field_kind(&found.record_type, path))?;
        if found.new {
            // The patch's own record: the change is the record.
            let Some(made) = self.queue.new_record(&found.record_type, &found.key).cloned() else {
                return err(format!("{} is no longer in the patch", found.key));
            };
            let mut record = made.record.clone();
            set_at(&mut record, path, value)?;
            self.queue.add_new_record(NewRecord { record, ..made });
            self.save_journal();
            return Ok(());
        }
        if pyval::eq(&value, current) {
            self.queue.remove_field(&found.record_type, &found.key, path);
        } else {
            self.queue.add_field(&found.record_type, &found.key, Choice::Value { path: path.to_string(), value });
            self.queue.set_base(&found.record_type, &found.key, found.winner());
        }
        self.save_journal();
        Ok(())
    }

    /// Changes made inside are one change: one journal save, one undo step. All or
    /// nothing: when a step fails, the outermost batch puts the queue back as it was.
    pub fn batch<T>(&mut self, f: impl FnOnce(&mut Editor) -> Result<T, EditError>) -> Result<T, EditError> {
        let outer = self.batching;
        let before = (!outer).then(|| self.queue.snapshot());
        self.batching = true;
        let out = f(self);
        self.batching = outer;
        match &out {
            Err(_) => {
                if let Some(b) = before {
                    self.queue.restore_snapshot(&b);
                }
            }
            Ok(_) if !outer => self.save_journal(),
            Ok(_) => {}
        }
        out
    }

    /// One field set to one value on many records: one change, one undo step.
    /// `{changed, failed: [{id, error}]}` - a record that refuses is left as it was.
    pub fn set_many(&mut self, founds: &[Found], path: &str, raw: &Value) -> Value {
        let (mut changed, mut failed) = (0, Vec::new());
        let _ = self.batch(|ed| {
            for f in founds {
                match ed.set_field(f, path, raw) {
                    Ok(()) => changed += 1,
                    Err(e) => failed.push(json!({"id": f.key, "error": e.0})),
                }
            }
            Ok(())
        });
        json!({"changed": changed, "failed": failed})
    }

    /// Drops the queued change to one field, or to the whole record.
    pub fn revert(&mut self, found: &Found, path: Option<&str>) -> Result<(), EditError> {
        if found.new {
            if path.is_some() {
                return err("a record the patch makes has nothing to go back to: set the value, or remove the record");
            }
            self.queue.remove_new_record(&found.record_type, &found.key);
        } else {
            match path {
                None => self.queue.remove_record(&found.record_type, &found.key),
                Some(p) => self.queue.remove_field(&found.record_type, &found.key, p),
            }
        }
        self.save_journal();
        Ok(())
    }

    /// An id for a record the patch makes, checked: not empty, not too long, and not one
    /// the load order (or the patch) has - for an object, as any object type.
    fn new_id(&self, tag: &str, record_type: &str, new_id: &str) -> Result<String, EditError> {
        let rid = new_id.trim();
        if rid.is_empty() {
            return err("the record needs an id");
        }
        if rid.len() > MAX_ID {
            return err(format!("an id is at most {MAX_ID} bytes"));
        }
        if self.find(tag, rid, None).is_some() {
            return err(format!("{rid} is already a {record_type} id in this load order"));
        }
        let t: [u8; 4] = tag.as_bytes().try_into().unwrap_or([0; 4]);
        if ordercore::records::is_object_type(&t) {
            // Objects share one id space: the Construction Set refuses two types with one
            // id, and Morrowind.exe will not load them.
            for other in ordercore::records::OBJECT_TYPES.iter().filter(|o| ***o != t) {
                let other = std::str::from_utf8(*other).expect("a tag");
                if let Some(clash) = self.find(other, rid, None) {
                    return err(format!(
                        "{rid} is already a {} id in this load order: two object types with one id break the game",
                        clash.record_type
                    ));
                }
            }
        }
        Ok(rid.to_string())
    }

    /// Makes a blank record of a type under a new id, in the patch (and journals it):
    /// every field at the type's default, a script a bare `begin`/`end`.
    pub fn insert(&mut self, tag: &str, new_id: &str) -> Result<Found, EditError> {
        let mut t = tag.to_uppercase();
        while t.len() < 4 {
            t.push('_');
        }
        let (Some(record_type), Some(mut record)) = (type_of(&t), type_of(&t).and_then(crate::blank::blank)) else {
            return err(format!("no record type has the tag {t}"));
        };
        if uncopyable(record_type) {
            return err(format!("a {record_type} record cannot be made this way"));
        }
        let rid = self.new_id(&t, record_type, new_id)?;
        record["id"] = json!(rid);
        if record_type == "StartScript" {
            record["script"] = json!(rid);
        }
        if record_type == "Script" {
            record["text"] = json!(format!("begin {rid}\n\nend\n"));
        }
        self.queue.add_new_record(NewRecord { record_type: record_type.to_string(), key: rid.clone(), record: record.clone(), source: String::new(), topic: String::new() });
        self.save_journal();
        Ok(Found { tag: t, record_type: record_type.to_string(), key: rid, versions: vec![(PATCH.into(), record)], new: true })
    }

    /// Makes a copy of a record under a new id, in the patch (and saves the journal): the
    /// Construction Set's way of making a record. What the copy names stays the
    /// original's, so the plugin it came from becomes a master.
    pub fn duplicate(&mut self, found: &Found, new_id: &str) -> Result<Found, EditError> {
        if uncopyable(&found.record_type) {
            return err(format!("a {} record cannot be copied under a new id", found.record_type));
        }
        let rid = self.new_id(&found.tag, &found.record_type, new_id)?;
        let mut record = found.record().clone();
        record["id"] = json!(rid);
        if found.record_type == "StartScript" {
            record["script"] = json!(rid);
        }
        if found.record_type == "Script" {
            // The text names the script: its `begin` line takes the new id.
            let text = record.get("text").map(pyval::str_of).filter(|s| s != "None").unwrap_or_default();
            let re = regex::Regex::new(r#"(?im)^(\s*begin\s+)("[^"]*"|\S+)"#).expect("a pattern");
            let renamed = re.replacen(&text, 1, |c: &regex::Captures| format!("{}{rid}", &c[1])).into_owned();
            record["text"] = json!(renamed);
        }
        let flags = record.get("flags").map(pyval::str_of).filter(|s| !s.is_empty() && s != "None").unwrap_or_default();
        if flags.contains("DELETED") {
            let kept: Vec<&str> = flags.split(" | ").filter(|f| f.trim() != "DELETED").collect();
            record["flags"] = json!(kept.join(" | "));
        }
        let source = if found.new { String::new() } else { found.winner().to_string() };
        self.queue.add_new_record(NewRecord { record_type: found.record_type.clone(), key: rid.clone(), record: record.clone(), source, topic: String::new() });
        self.save_journal();
        Ok(Found { tag: found.tag.clone(), record_type: found.record_type.clone(), key: rid, versions: vec![(PATCH.into(), record)], new: true })
    }

    /// Everything the patch would carry: one `{tag, type, id, whole, changes}` per record;
    /// a changed placed object is `Reference` with `ref`, a new one `NewReference` with
    /// `new`, a record the patch makes has `made`.
    pub fn pending(&self) -> Vec<Value> {
        let tag = |t: &str| tag_of(t).unwrap_or("").to_string();
        let mut out: Vec<Value> = self
            .queue
            .selections()
            .iter()
            .map(|s| json!({"tag": tag(&s.record_type), "type": s.record_type, "id": s.key, "whole": s.plugin, "changes": []}))
            .collect();
        for ((t, k), choices) in self.queue.fields().iter() {
            let changes: Vec<Value> = choices
                .iter()
                .map(|c| match c {
                    Choice::Value { path, value } => json!({"path": path, "value": value}),
                    Choice::Plugin { path, plugin } => json!({"path": path, "plugin": plugin}),
                })
                .collect();
            out.push(json!({"tag": tag(t), "type": t, "id": k, "whole": null, "changes": changes}));
        }
        for e in self.queue.ref_edits() {
            out.push(json!({
                "tag": "", "type": "Reference", "id": format!("{}:{} in {}", e.origin, e.refr_index, e.cell),
                "ref": {"cell": e.cell, "origin": e.origin, "refr": e.refr_index, "plugins": e.plugins},
                "whole": null,
                "changes": e.changes.iter().map(|(k, v)| json!({"path": k, "value": v})).collect::<Vec<_>>(),
            }));
        }
        for m in self.queue.new_records() {
            let get = |f: &str| m.record.get(f).cloned().unwrap_or(json!(""));
            out.push(json!({
                "tag": tag(&m.record_type), "type": m.record_type, "id": m.key, "whole": null,
                "made": {"source": m.source, "name": get("name"), "mesh": get("mesh")},
                "changes": [],
            }));
        }
        for n in self.queue.new_refs() {
            let id = n.fields.get(&"id".to_string()).cloned().unwrap_or(json!(""));
            out.push(json!({
                "tag": "", "type": "NewReference", "id": format!("{} (new) in {}", pyval::str_of(&id), n.cell),
                "new": {"cell": n.cell, "uid": n.uid, "id": id, "tag": n.tag, "plugins": n.plugins},
                "whole": null,
                "changes": n.fields.iter().map(|(k, v)| json!({"path": k, "value": v})).collect::<Vec<_>>(),
            }));
        }
        out
    }

    /// After every change: the pool before it as an undo step, and the journal saved
    /// (inside a batch, once, when it ends).
    pub fn save_journal(&mut self) {
        if self.batching {
            return;
        }
        self.history.record(&self.queue);
        self.write_journal();
    }

    fn write_journal(&self) {
        if let Some(j) = &self.journal
            && let Err(e) = self.queue.save(j)
        {
            eprintln!("patch journal: cannot write {}: {e}", j.display());
        }
    }

    /// How many steps there are to undo and to redo: `{undo, redo}`.
    pub fn can_undo(&self) -> Value {
        let (u, r) = self.history.counts();
        json!({"undo": u, "redo": r})
    }

    /// Puts the pool back as it was before the last change: `{done, what, undo, redo}`,
    /// `what` the records and references it touched.
    pub fn undo(&mut self) -> Value {
        self.step(false)
    }

    /// Makes again what [`Editor::undo`] took back.
    pub fn redo(&mut self) -> Value {
        self.step(true)
    }

    fn step(&mut self, redo: bool) -> Value {
        let (u, r) = self.history.counts();
        if (if redo { r } else { u }) == 0 {
            return json!({"done": false, "what": [], "undo": u, "redo": r});
        }
        let before = self.pending();
        self.history.step(&mut self.queue, redo);
        self.write_journal();
        let after = self.pending();
        let (u, r) = self.history.counts();
        // In the Python's order (`{"done", "what", **can_undo()}`).
        json!({"done": true, "what": pending_diff(&before, &after), "undo": u, "redo": r})
    }
}

/// What differs between two pending lists, in words: the names of the records and
/// references whose entry changed, came or went, sorted.
pub fn pending_diff(before: &[Value], after: &[Value]) -> Vec<String> {
    fn name(p: &Value) -> String {
        let s = |v: &Value| pyval::str_of(v);
        if let Some(r) = p.get("ref").filter(|r| !r.is_null()) {
            let g = |k: &str| r.get(k).map(s).unwrap_or_default();
            return format!("{} ({}:{})", p.get("id").map(s).unwrap_or_default(), g("origin"), g("refr"));
        }
        if let Some(n) = p.get("new").filter(|n| !n.is_null()) {
            let id = n.get("id").or_else(|| p.get("id")).map(s).unwrap_or_default();
            return format!("new {id}");
        }
        let tag = p.get("tag").map(s).filter(|t| !t.is_empty()).or_else(|| p.get("type").map(s)).unwrap_or_default();
        format!("{tag} {}", p.get("id").map(s).unwrap_or_default())
    }
    let keyed = |items: &[Value]| -> std::collections::BTreeMap<String, Value> { items.iter().map(|p| (name(p), p.clone())).collect() };
    let (a, b) = (keyed(before), keyed(after));
    let mut names: Vec<String> = a.keys().chain(b.keys()).filter(|k| a.get(*k) != b.get(*k)).cloned().collect();
    names.sort();
    names.dedup();
    names
}
