//! The decisions queued for a patch, the rules for changing them, the journal they are
//! saved to, and the undo history over them: `wraithguard/patch/queue.py` and the
//! journal and history of `wraithguard/patch/editor.py` (`save_queue`, `restore_queue`,
//! `forget_queue`, `_record_step`, `undo`, `redo`), rule for rule (port step 4.1, slice 3).
//! Cases: `tests/fixtures/queue_cases.json`, run here and by `tests/test_queue_cases.py`.
//!
//! **The rules.** *Re-deciding replaces*: choosing a winner for a record already chosen,
//! or a plugin for a field already picked, overwrites the earlier answer. *Whole or
//! merged, never both*: choosing one drops the other. *The base is whatever currently
//! wins*, given by whoever queued a merge for a record the conflict scan does not list.
//!
//! The journal file is the Python app's, unchanged (version 1), so a journal either side
//! writes opens in the other.

use std::collections::HashSet;
use std::path::Path;

use serde_json::{json, Map, Value};

/// The journal's format version.
pub const JOURNAL_VERSION: i64 = 1;

/// The most undo steps kept.
pub const UNDO_LIMIT: usize = 200;

/// A small map that keeps the order keys were first added (as a Python dict does), and
/// compares as a Python dict does: the same keys and values, in any order.
#[derive(Clone, Debug)]
pub struct Ordered<K, V>(Vec<(K, V)>);

impl<K: PartialEq, V: PartialEq> PartialEq for Ordered<K, V> {
    fn eq(&self, other: &Self) -> bool {
        self.0.len() == other.0.len() && self.0.iter().all(|(k, v)| other.get(k) == Some(v))
    }
}

impl<K, V> Default for Ordered<K, V> {
    fn default() -> Self {
        Ordered(Vec::new())
    }
}

impl<K: PartialEq, V> Ordered<K, V> {
    pub fn get(&self, k: &K) -> Option<&V> {
        self.0.iter().find(|(x, _)| x == k).map(|(_, v)| v)
    }
    pub fn get_mut(&mut self, k: &K) -> Option<&mut V> {
        self.0.iter_mut().find(|(x, _)| x == k).map(|(_, v)| v)
    }
    /// Sets a value; a key already there keeps its place.
    pub fn insert(&mut self, k: K, v: V) {
        match self.get_mut(&k) {
            Some(slot) => *slot = v,
            None => self.0.push((k, v)),
        }
    }
    pub fn remove(&mut self, k: &K) -> Option<V> {
        let at = self.0.iter().position(|(x, _)| x == k)?;
        Some(self.0.remove(at).1)
    }
    pub fn contains(&self, k: &K) -> bool {
        self.get(k).is_some()
    }
    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.0.iter().map(|(k, v)| (k, v))
    }
    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.0.iter().map(|(_, v)| v)
    }
    pub fn keys(&self) -> impl Iterator<Item = &K> {
        self.0.iter().map(|(k, _)| k)
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn clear(&mut self) {
        self.0.clear();
    }
    /// Keeps the entries `f` says yes to.
    pub fn retain(&mut self, mut f: impl FnMut(&K, &mut V) -> bool) {
        self.0.retain_mut(|(k, v)| f(k, v));
    }
}

/// A JSON object's fields in their order (`changes`, `fields`, a made record).
pub type Fields = Ordered<String, Value>;

impl Fields {
    /// From a JSON object (its order kept as serde_json gives it).
    pub fn from_object(v: &Value) -> Fields {
        Ordered(v.as_object().map(|m| m.iter().map(|(k, x)| (k.clone(), x.clone())).collect()).unwrap_or_default())
    }
    /// As a JSON object.
    pub fn to_object(&self) -> Value {
        Value::Object(self.0.iter().cloned().collect::<Map<String, Value>>())
    }
}

/// A record to take whole, and the plugin whose version wins.
#[derive(Clone, Debug, PartialEq)]
pub struct Selection {
    pub plugin: String,
    pub record_type: String,
    pub key: String,
}

/// One way to settle a field: take it from a plugin, or a value typed for it.
#[derive(Clone, Debug, PartialEq)]
pub enum Choice {
    Plugin { path: String, plugin: String },
    Value { path: String, value: Value },
}

impl Choice {
    pub fn path(&self) -> &str {
        match self {
            Choice::Plugin { path, .. } | Choice::Value { path, .. } => path,
        }
    }
}

/// Changes to one placed object.
#[derive(Clone, Debug, PartialEq)]
pub struct RefEdit {
    /// The cell's key (an interior's name, an exterior's `(x, y)`).
    pub cell: String,
    /// The plugin that created the reference.
    pub origin: String,
    pub refr_index: i64,
    /// Field -> new value.
    pub changes: Fields,
    /// The plugins with a CELL record for the cell.
    pub plugins: Vec<String>,
}

impl RefEdit {
    /// `(cell, origin, refr_index)`, for de-duplicating.
    pub fn ident(&self) -> (String, String, i64) {
        (self.cell.to_lowercase(), self.origin.to_lowercase(), self.refr_index)
    }
}

/// A reference the patch adds.
#[derive(Clone, Debug, PartialEq)]
pub struct NewRef {
    pub cell: String,
    /// The editor's name for it until it is written.
    pub uid: String,
    pub fields: Fields,
    /// The plugin that defines what it places (last).
    pub base_plugin: String,
    pub plugins: Vec<String>,
    /// What it places is a record of this tag (for the editor, which draws it).
    pub tag: String,
}

impl NewRef {
    /// `(cell, uid)`, for de-duplicating.
    pub fn ident(&self) -> (String, String) {
        (self.cell.to_lowercase(), self.uid.clone())
    }
}

/// A record the patch makes itself ("Make a copy as").
#[derive(Clone, Debug, PartialEq)]
pub struct NewRecord {
    pub record_type: String,
    pub key: String,
    pub record: Value,
    /// The plugin whose record it was copied from (a master of the patch).
    pub source: String,
    /// For a dialogue response, the topic it answers.
    pub topic: String,
}

/// Every decision: what [`PatchQueue::snapshot`] copies.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Decisions {
    pub whole: Vec<Selection>,
    pub fields: Ordered<(String, String), Vec<Choice>>,
    pub bases: Ordered<(String, String), String>,
    pub refs: Ordered<(String, String, i64), RefEdit>,
    pub new: Ordered<(String, String), NewRef>,
    pub made: Ordered<(String, String), NewRecord>,
}

/// What a patch will carry, before it is written.
#[derive(Clone, Debug, Default)]
pub struct PatchQueue {
    d: Decisions,
    /// Counts the changes: the Editor's page polls it (`editRevision`).
    pub revision: u64,
}

impl PatchQueue {
    pub fn new() -> PatchQueue {
        PatchQueue::default()
    }

    /// Records to be taken whole, in the order chosen.
    pub fn selections(&self) -> &[Selection] {
        &self.d.whole
    }

    /// Field decisions, by `(record type, key)`.
    pub fn fields(&self) -> &Ordered<(String, String), Vec<Choice>> {
        &self.d.fields
    }

    /// How many records the patch would carry (a cell with changed references is one).
    pub fn len(&self) -> usize {
        let mut records: HashSet<(String, String)> =
            self.d.whole.iter().map(|s| (s.record_type.clone(), s.key.to_lowercase())).collect();
        records.extend(self.d.fields.keys().map(|(t, k)| (t.clone(), k.to_lowercase())));
        let mut cells: HashSet<(String, String)> =
            self.d.refs.values().map(|e| ("Cell".to_string(), e.cell.to_lowercase())).collect();
        cells.extend(self.d.new.values().map(|n| ("Cell".to_string(), n.cell.to_lowercase())));
        let cells = cells.difference(&records).count();
        self.d.whole.len() + self.d.fields.len() + cells + self.d.made.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Changes to placed objects, one per reference, in the order made.
    pub fn ref_edits(&self) -> impl Iterator<Item = &RefEdit> {
        self.d.refs.values()
    }

    /// Queues changes to a placed object; changes already queued for it are kept, the new
    /// ones replacing any to the same fields.
    pub fn add_ref_edit(&mut self, mut edit: RefEdit) {
        self.revision += 1;
        let id = edit.ident();
        if let Some(old) = self.d.refs.get(&id) {
            let mut changes = old.changes.clone();
            for (k, v) in edit.changes.iter() {
                changes.insert(k.clone(), v.clone());
            }
            let mut plugins = old.plugins.clone();
            for p in &edit.plugins {
                if !plugins.contains(p) {
                    plugins.push(p.clone());
                }
            }
            edit = RefEdit { changes, plugins, ..edit };
        }
        self.d.refs.insert(id, edit);
    }

    /// Records the patch makes itself, in the order made.
    pub fn new_records(&self) -> impl Iterator<Item = &NewRecord> {
        self.d.made.values()
    }

    /// Queues a record the patch makes, or replaces the one queued with its key.
    pub fn add_new_record(&mut self, made: NewRecord) {
        self.revision += 1;
        self.d.made.insert((made.record_type.clone(), made.key.to_lowercase()), made);
    }

    /// Drops a record the patch would make.
    pub fn remove_new_record(&mut self, record_type: &str, key: &str) {
        self.revision += 1;
        self.d.made.remove(&(record_type.to_string(), key.to_lowercase()));
    }

    /// The record the patch makes with this type and id (any case).
    pub fn new_record(&self, record_type: &str, key: &str) -> Option<&NewRecord> {
        self.d.made.get(&(record_type.to_string(), key.to_lowercase()))
    }

    /// References the patch adds, in the order placed.
    pub fn new_refs(&self) -> impl Iterator<Item = &NewRef> {
        self.d.new.values()
    }

    /// Queues a reference to add, or changes one queued (its fields merged).
    pub fn add_new_ref(&mut self, new: NewRef) {
        self.revision += 1;
        let id = new.ident();
        let new = match self.d.new.get(&id) {
            Some(old) => {
                let mut fields = old.fields.clone();
                for (k, v) in new.fields.iter() {
                    fields.insert(k.clone(), v.clone());
                }
                NewRef { fields, ..old.clone() }
            }
            None => new,
        };
        self.d.new.insert(id, new);
    }

    /// Drops a reference queued to be added.
    pub fn remove_new_ref(&mut self, cell: &str, uid: &str) {
        self.revision += 1;
        self.d.new.remove(&(cell.to_lowercase(), uid.to_string()));
    }

    /// Drops the changes to a placed object, or to one of its fields.
    pub fn remove_ref_edit(&mut self, cell: &str, origin: &str, refr_index: i64, path: Option<&str>) {
        self.revision += 1;
        let id = (cell.to_lowercase(), origin.to_lowercase(), refr_index);
        let Some(old) = self.d.refs.get_mut(&id) else { return };
        match path {
            None => {
                self.d.refs.remove(&id);
            }
            Some(p) => {
                old.changes.remove(&p.to_string());
                if old.changes.is_empty() {
                    self.d.refs.remove(&id);
                }
            }
        }
    }

    /// Every decision, copied (undo).
    pub fn snapshot(&self) -> Decisions {
        self.d.clone()
    }

    /// Puts the decisions of a snapshot back, in place of what is there.
    pub fn restore_snapshot(&mut self, snap: &Decisions) {
        self.revision += 1;
        self.d = snap.clone();
    }

    /// Drops only the decisions a snapshot holds, keeping later ones: a build ran on a
    /// worker while the queue stayed editable, and what it wrote is spent.
    pub fn consume(&mut self, snap: &Decisions) {
        self.revision += 1;
        for sel in &snap.whole {
            if let Some(i) = self.d.whole.iter().position(|h| h == sel) {
                self.d.whole.remove(i);
            }
        }
        for (key, chosen) in snap.fields.iter() {
            let Some(live) = self.d.fields.get_mut(key) else { continue };
            for choice in chosen {
                if let Some(i) = live.iter().position(|h| h == choice) {
                    live.remove(i);
                }
            }
            if live.is_empty() {
                self.d.fields.remove(key);
            }
        }
        for (id, edit) in snap.refs.iter() {
            let Some(held) = self.d.refs.get_mut(id) else { continue };
            held.changes.retain(|k, v| edit.changes.get(k) != Some(v));
            if held.changes.is_empty() {
                self.d.refs.remove(id);
            }
        }
        for (id, r) in snap.new.iter() {
            if self.d.new.get(id) == Some(r) {
                self.d.new.remove(id);
            }
        }
        for (id, r) in snap.made.iter() {
            if self.d.made.get(id) == Some(r) {
                self.d.made.remove(id);
            }
        }
        let used: HashSet<(String, String)> = self
            .d
            .whole
            .iter()
            .map(|s| (s.record_type.clone(), s.key.clone()))
            .chain(self.d.fields.keys().cloned())
            .collect();
        self.d.bases.retain(|k, _| used.contains(k));
    }

    /// Drops every decision.
    pub fn clear(&mut self) {
        self.revision += 1;
        self.d = Decisions::default();
    }

    /// Says which plugin wins a record, for one the conflict scan does not list.
    pub fn set_base(&mut self, record_type: &str, key: &str, plugin: &str) {
        self.revision += 1;
        self.d.bases.insert((record_type.to_string(), key.to_string()), plugin.to_string());
    }

    /// The base given for a record, or `""`.
    pub fn base(&self, record_type: &str, key: &str) -> &str {
        self.d.bases.get(&(record_type.to_string(), key.to_string())).map(String::as_str).unwrap_or("")
    }

    /// Queues a record to be taken whole (its field choices go).
    pub fn add_whole(&mut self, sel: Selection) {
        self.revision += 1;
        self.drop_whole(&sel.record_type, &sel.key);
        self.d.fields.remove(&(sel.record_type.clone(), sel.key.clone()));
        self.d.whole.push(sel);
    }

    /// Queues one field of a record; re-deciding the same path replaces the earlier
    /// answer, whichever kind either was (a whole-record choice for it goes).
    pub fn add_field(&mut self, record_type: &str, key: &str, choice: Choice) {
        self.revision += 1;
        let k = (record_type.to_string(), key.to_string());
        if !self.d.fields.contains(&k) {
            self.d.fields.insert(k.clone(), Vec::new());
        }
        let choices = self.d.fields.get_mut(&k).expect("just added");
        choices.retain(|c| c.path() != choice.path());
        choices.push(choice);
        self.drop_whole(record_type, key);
    }

    /// Drops a record entirely, however it was queued.
    pub fn remove_record(&mut self, record_type: &str, key: &str) {
        self.revision += 1;
        self.drop_whole(record_type, key);
        let k = (record_type.to_string(), key.to_string());
        self.d.fields.remove(&k);
        self.d.bases.remove(&k);
    }

    /// Drops one field choice; a record left with none goes too.
    pub fn remove_field(&mut self, record_type: &str, key: &str, path: &str) {
        self.revision += 1;
        let k = (record_type.to_string(), key.to_string());
        let Some(choices) = self.d.fields.get_mut(&k) else { return };
        choices.retain(|c| c.path() != path);
        if choices.is_empty() {
            self.d.fields.remove(&k);
            self.d.bases.remove(&k);
        }
    }

    fn drop_whole(&mut self, record_type: &str, key: &str) {
        self.d.whole.retain(|s| !(s.record_type == record_type && s.key == key));
    }

    // -- the journal ------------------------------------------------------------------

    /// The journal's document for the queue (`save_queue`'s).
    pub fn journal(&self) -> Value {
        let choice = |c: &Choice| match c {
            Choice::Value { path, value } => json!({"path": path, "value": value}),
            Choice::Plugin { path, plugin } => json!({"path": path, "plugin": plugin}),
        };
        json!({
            "version": JOURNAL_VERSION,
            "whole": self.d.whole.iter().map(|s| json!({"type": s.record_type, "key": s.key, "plugin": s.plugin})).collect::<Vec<_>>(),
            "fields": self.d.fields.iter().map(|((t, k), cs)| json!({
                "type": t, "key": k, "base": self.base(t, k), "choices": cs.iter().map(choice).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
            "refs": self.d.refs.values().map(|e| json!({
                "cell": e.cell, "origin": e.origin, "refr": e.refr_index, "changes": e.changes.to_object(), "plugins": e.plugins,
            })).collect::<Vec<_>>(),
            "made": self.d.made.values().map(|m| json!({
                "type": m.record_type, "key": m.key, "record": m.record, "source": m.source, "topic": m.topic,
            })).collect::<Vec<_>>(),
            "new": self.d.new.values().map(|n| json!({
                "cell": n.cell, "uid": n.uid, "fields": n.fields.to_object(), "base": n.base_plugin, "plugins": n.plugins, "tag": n.tag,
            })).collect::<Vec<_>>(),
        })
    }

    /// Writes the whole queue to a journal file (atomically), or removes the file when the
    /// queue is empty. A write that fails is reported, not raised: the journal is a safety
    /// net.
    pub fn save(&self, journal: &Path) -> std::io::Result<()> {
        if self.is_empty() {
            return forget(journal);
        }
        let mut tmp = journal.as_os_str().to_owned();
        tmp.push(".tmp");
        let tmp = std::path::PathBuf::from(tmp);
        std::fs::write(&tmp, serde_json::to_vec(&self.journal()).map_err(std::io::Error::other)?)?;
        std::fs::rename(&tmp, journal)
    }

    /// Puts a journal's decisions back; what the queue holds already wins. How many
    /// records it brought back (0 without a journal, or one that does not read; a damaged
    /// one gives back what read before the damage).
    pub fn restore(&mut self, journal: &Path) -> usize {
        let Ok(text) = std::fs::read(journal) else { return 0 };
        let Ok(doc) = serde_json::from_slice::<Value>(&text) else { return 0 };
        self.restore_doc(&doc)
    }

    /// [`PatchQueue::restore`] from the document.
    pub fn restore_doc(&mut self, doc: &Value) -> usize {
        if doc.get("version").and_then(Value::as_i64) != Some(JOURNAL_VERSION) {
            return 0;
        }
        let mut n = 0;
        let _ = self.restore_parts(doc, &mut n);
        n
    }

    fn restore_parts(&mut self, doc: &Value, n: &mut usize) -> Option<()> {
        let s = |v: &Value, k: &str| -> Option<String> { v.get(k)?.as_str().map(str::to_string) };
        let opt = |v: &Value, k: &str| -> String { v.get(k).and_then(Value::as_str).unwrap_or("").to_string() };
        let list = |v: &Value, k: &str| -> Vec<String> {
            v.get(k).and_then(Value::as_array).map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default()
        };
        let items = |k: &str| doc.get(k).and_then(Value::as_array).cloned().unwrap_or_default();
        let known: HashSet<(String, String)> = self
            .d
            .whole
            .iter()
            .map(|s| (s.record_type.clone(), s.key.clone()))
            .chain(self.d.fields.keys().cloned())
            .collect();
        for w in items("whole") {
            let (t, k, p) = (s(&w, "type")?, s(&w, "key")?, s(&w, "plugin")?);
            if !known.contains(&(t.clone(), k.clone())) {
                self.add_whole(Selection { plugin: p, record_type: t, key: k });
                *n += 1;
            }
        }
        for f in items("fields") {
            let (t, k) = (s(&f, "type")?, s(&f, "key")?);
            if known.contains(&(t.clone(), k.clone())) {
                continue;
            }
            for c in f.get("choices").and_then(Value::as_array).into_iter().flatten() {
                let path = s(c, "path")?;
                let choice = match c.get("value") {
                    Some(v) => Choice::Value { path, value: v.clone() },
                    None => Choice::Plugin { path, plugin: s(c, "plugin")? },
                };
                self.add_field(&t, &k, choice);
            }
            let base = opt(&f, "base");
            if !base.is_empty() {
                self.set_base(&t, &k, &base);
            }
            *n += 1;
        }
        let have: HashSet<_> = self.d.refs.keys().cloned().collect();
        for r in items("refs") {
            let edit = RefEdit {
                cell: s(&r, "cell")?,
                origin: s(&r, "origin")?,
                refr_index: whole(r.get("refr")?)?,
                changes: Fields::from_object(r.get("changes").filter(|c| c.is_object())?),
                plugins: list(&r, "plugins"),
            };
            if !have.contains(&edit.ident()) {
                self.add_ref_edit(edit);
                *n += 1;
            }
        }
        for m in items("made") {
            let (t, k) = (s(&m, "type")?, s(&m, "key")?);
            if self.new_record(&t, &k).is_none() {
                let record = m.get("record").filter(|c| c.is_object())?.clone();
                self.add_new_record(NewRecord { record_type: t, key: k, record, source: opt(&m, "source"), topic: opt(&m, "topic") });
                *n += 1;
            }
        }
        let placed: HashSet<_> = self.d.new.keys().cloned().collect();
        for r in items("new") {
            let uid = match r.get("uid")? {
                Value::String(u) => u.clone(),
                other => other.to_string(),
            };
            let new = NewRef {
                cell: s(&r, "cell")?,
                uid,
                fields: Fields::from_object(r.get("fields").filter(|c| c.is_object())?),
                base_plugin: opt(&r, "base"),
                plugins: list(&r, "plugins"),
                tag: opt(&r, "tag"),
            };
            if !placed.contains(&new.ident()) {
                self.add_new_ref(new);
                *n += 1;
            }
        }
        Some(())
    }
}

/// A whole number as Python's `int()` reads a journal's: a number (a float cut toward
/// zero) or digits.
fn whole(v: &Value) -> Option<i64> {
    match v {
        Value::Number(n) => n.as_i64().or_else(|| n.as_f64().filter(|f| f.is_finite()).map(|f| f.trunc() as i64)),
        Value::String(t) => t.trim().parse().ok(),
        Value::Bool(b) => Some(*b as i64),
        _ => None,
    }
}

/// Removes a journal file (none there is no error).
pub fn forget(journal: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(journal) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// The undo history: the pool before each change, and after each undo.
#[derive(Clone, Debug, Default)]
pub struct History {
    pub undo: Vec<Decisions>,
    pub redo: Vec<Decisions>,
    /// The pool as last seen.
    pub last: Decisions,
}

impl History {
    /// A history that starts from the pool as it is.
    pub fn new(queue: &PatchQueue) -> History {
        History { undo: Vec::new(), redo: Vec::new(), last: queue.snapshot() }
    }

    /// After a change: the pool before it onto the undo list (when it changed), the redo
    /// list emptied. A change made outside the editor since is part of the step.
    pub fn record(&mut self, queue: &PatchQueue) {
        let now = queue.snapshot();
        if now == self.last {
            return;
        }
        self.undo.push(std::mem::replace(&mut self.last, now));
        if self.undo.len() > UNDO_LIMIT {
            let extra = self.undo.len() - UNDO_LIMIT;
            self.undo.drain(..extra);
        }
        self.redo.clear();
    }

    /// One step back (`undo`) or forward (`redo = true`); whether there was one.
    pub fn step(&mut self, queue: &mut PatchQueue, redo: bool) -> bool {
        let (take, give) = if redo { (&mut self.redo, &mut self.undo) } else { (&mut self.undo, &mut self.redo) };
        let Some(snap) = take.pop() else { return false };
        give.push(queue.snapshot());
        queue.restore_snapshot(&snap);
        self.last = queue.snapshot();
        true
    }

    /// `(undo steps, redo steps)`.
    pub fn counts(&self) -> (usize, usize) {
        (self.undo.len(), self.redo.len())
    }
}
