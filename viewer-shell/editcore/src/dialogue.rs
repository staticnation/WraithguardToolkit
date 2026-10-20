//! Dialogue in the Editor: the order the engine reads a topic in
//! (`wraithguard/patch/dialogue.py`), and the dialogue window - every topic, one topic's
//! responses, a response moved or added, a topic copied (`EditorSession.topics`, `topic`,
//! `move_response`, `new_response`, `copy_topic`). Port step 4.1, slice 5c. Cases:
//! `tests/fixtures/dialogue_cases.json`, run here and by `tests/test_dialogue_cases.py`.
//!
//! **The rule.** Responses are read plugin by plugin in load order. Each names, in
//! `prev_id`, the response it follows: seen for the first time, it goes straight after
//! that one (an empty `prev_id` is the top; one naming a response not yet seen is the
//! end - the engine has nowhere else to put it); seen before, a changed `prev_id` moves
//! it; a deletion takes it out (a later plugin adding the id again puts it in afresh).
//! The rule is yampt's (`scanner/dial_info_align.cpp`, MIT, Rafał Wierzchoś).

use indexmap::IndexMap;
use serde_json::{json, Value};

use crate::blank::blank;
use crate::edit::{err, EditError, Editor};
use crate::pyval::str_of;
use crate::queue::{Choice, NewRecord};
use crate::session::{Found, PATCH};

/// One definition of a response, by one plugin.
#[derive(Clone, Debug, PartialEq)]
pub struct Response {
    /// Its id, unique within its topic.
    pub key: String,
    /// What it says it follows (empty: the top).
    pub prev: String,
    /// The file the definition came from.
    pub plugin: String,
    /// The definition deletes it.
    pub deleted: bool,
}

/// A response in its resolved place.
#[derive(Clone, Debug, PartialEq)]
pub struct Placed {
    pub key: String,
    /// The `prev_id` of the definition that last moved it.
    pub prev: String,
    /// Every file defining it, in the order read; the last is the one the game uses.
    pub plugins: Vec<String>,
}

/// Where a response claiming to follow `prev` goes.
fn slot_for(prev: &str, order: &[Placed]) -> usize {
    if prev.is_empty() {
        return 0;
    }
    match order.iter().position(|p| p.key == prev) {
        Some(at) => at + 1,
        // The chain is broken: it lands at the end, filtered last.
        None => order.len(),
    }
}

/// A topic's responses (every definition, in load order) in the order the engine reads
/// them, each naming every plugin that defined it.
pub fn topic_order(responses: &[Response]) -> Vec<Placed> {
    let mut order: Vec<Placed> = Vec::new();
    for r in responses {
        let existing = order.iter().position(|p| p.key == r.key);
        if r.deleted {
            if let Some(at) = existing {
                order.remove(at);
            }
            continue;
        }
        if let Some(at) = existing {
            if !r.plugin.is_empty() {
                order[at].plugins.push(r.plugin.clone());
            }
            if order[at].prev == r.prev {
                continue;
            }
            let mut moved = order.remove(at);
            moved.prev = r.prev.clone();
            let slot = slot_for(&r.prev, &order);
            order.insert(slot, moved);
            continue;
        }
        let placed = Placed { key: r.key.clone(), prev: r.prev.clone(), plugins: if r.plugin.is_empty() { Vec::new() } else { vec![r.plugin.clone()] } };
        let slot = slot_for(&r.prev, &order);
        order.insert(slot, placed);
    }
    order
}

/// The responses whose declared predecessor is nowhere in the topic: the ones the engine
/// pushes to the end.
pub fn orphans(order: &[Placed]) -> Vec<String> {
    order.iter().filter(|p| !p.prev.is_empty() && !order.iter().any(|q| q.key == p.prev)).map(|p| p.key.clone()).collect()
}

/// Python's `str(v or "")`.
fn text(v: Option<&Value>) -> String {
    match v {
        None | Some(Value::Null) => String::new(),
        Some(v) if !crate::session::truthy(v) => String::new(),
        Some(v) => str_of(v),
    }
}

/// One plugin's responses under the topics they answer (a response belongs to the last
/// topic read before it; one before any topic is dropped), in file order.
pub fn responses_by_topic(records: &[Value], plugin: &str) -> IndexMap<String, Vec<Response>> {
    let mut found: IndexMap<String, Vec<Response>> = IndexMap::new();
    let mut topic = String::new();
    for rec in records {
        match rec.get("type").and_then(Value::as_str) {
            Some("Dialogue") => topic = text(rec.get("id")),
            Some("DialogueInfo") if !topic.is_empty() => found.entry(topic.clone()).or_default().push(Response {
                key: text(rec.get("id")),
                prev: text(rec.get("prev_id")),
                plugin: plugin.to_string(),
                deleted: text(rec.get("flags")).to_uppercase().contains("DELETED"),
            }),
            _ => {}
        }
    }
    found
}

/// A new response's id: 19 digits, as the Python Editor makes them.
fn new_info_id() -> String {
    use std::hash::{BuildHasher, Hasher};
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u64(NEXT.fetch_add(1, Ordering::Relaxed));
    h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0));
    (h.finish() % 9_000_000_000_000_000_000 + 1_000_000_000_000_000_000).to_string()
}

impl Editor {
    /// Every topic of the load order and the patch: `[{id, type, plugins}]`, by kind and
    /// then id.
    pub fn topics(&self) -> Vec<Value> {
        let mut out: IndexMap<String, (String, String, Vec<String>)> = IndexMap::new();
        for plugin in self.session.order().to_vec() {
            for rec in self.session.dialogue(&plugin).iter() {
                if rec.get("type").and_then(Value::as_str) != Some("Dialogue") {
                    continue;
                }
                let tid = text(rec.get("id"));
                let e = out.entry(tid.to_lowercase()).or_insert_with(|| (tid.clone(), String::new(), Vec::new()));
                let kind = text(rec.get("dialogue_type"));
                if !kind.is_empty() {
                    e.1 = kind;
                }
                e.2.push(plugin.clone());
            }
        }
        for made in self.queue.new_records().filter(|m| m.record_type == "Dialogue") {
            let e = out.entry(made.key.to_lowercase()).or_insert_with(|| (made.key.clone(), String::new(), Vec::new()));
            let kind = text(made.record.get("dialogue_type"));
            if !kind.is_empty() {
                e.1 = kind;
            }
            e.2.push(PATCH.to_string());
        }
        let mut v: Vec<(String, String, Vec<String>)> = out.into_values().collect();
        v.sort_by(|a, b| (&a.1, a.0.to_lowercase()).cmp(&(&b.1, b.0.to_lowercase())));
        v.into_iter().map(|(id, kind, plugins)| json!({"id": id, "type": kind, "plugins": plugins})).collect()
    }

    /// Each response of a topic as the load order has it last: id -> (plugin, record).
    fn latest_responses(&self, want: &str) -> IndexMap<String, (String, Value)> {
        let mut latest = IndexMap::new();
        for plugin in self.session.order().to_vec() {
            let mut current = String::new();
            for rec in self.session.dialogue(&plugin).iter() {
                match rec.get("type").and_then(Value::as_str) {
                    Some("Dialogue") => current = text(rec.get("id")).to_lowercase(),
                    Some("DialogueInfo") if current == want => {
                        latest.insert(text(rec.get("id")), (plugin.clone(), rec.clone()));
                    }
                    _ => {}
                }
            }
        }
        latest
    }

    /// A topic's responses in the order the engine reads them: `{id, type, responses:
    /// [{id, text, speaker, disposition, plugins, winner, orphan, quest}]}` - `speaker`
    /// the response's conditions in a few words, `orphan` one whose predecessor is not in
    /// the topic (it goes last), `quest` a journal entry's state.
    pub fn topic(&self, topic_id: &str) -> Result<Value, EditError> {
        let want = topic_id.trim().to_lowercase();
        let mut defs: Vec<Response> = Vec::new();
        let (mut spelled, mut kind) = (String::new(), String::new());
        for plugin in self.session.order().to_vec() {
            let records = self.session.dialogue(&plugin);
            for (tid, found) in responses_by_topic(&records[..], &plugin) {
                if tid.to_lowercase() == want {
                    if spelled.is_empty() {
                        spelled = tid;
                    }
                    defs.extend(found);
                }
            }
            for rec in records.iter() {
                let k = text(rec.get("dialogue_type"));
                if rec.get("type").and_then(Value::as_str) == Some("Dialogue") && text(rec.get("id")).to_lowercase() == want && !k.is_empty() {
                    kind = k;
                }
            }
        }
        let mut latest = self.latest_responses(&want);
        for made in self.queue.new_records() {
            if made.record_type == "Dialogue" && made.key.to_lowercase() == want {
                if spelled.is_empty() {
                    spelled = made.key.clone();
                }
                let k = text(made.record.get("dialogue_type"));
                if !k.is_empty() {
                    kind = k;
                }
            }
            if made.record_type == "DialogueInfo" && made.topic.to_lowercase() == want {
                defs.push(Response { key: made.key.clone(), prev: text(made.record.get("prev_id")), plugin: PATCH.into(), deleted: false });
                latest.insert(made.key.clone(), (PATCH.into(), made.record.clone()));
            }
        }
        if spelled.is_empty() && kind.is_empty() {
            return err(format!("no plugin of this load order has the topic {topic_id}"));
        }
        // A response the pool moves (its prev_id queued): the patch's version, read last.
        for ((rtype, key), choices) in self.queue.fields().iter() {
            if rtype != "DialogueInfo" || !latest.contains_key(key) {
                continue;
            }
            for c in choices {
                if let Choice::Value { path, value } = c
                    && path == "prev_id"
                {
                    defs.push(Response { key: key.clone(), prev: text(Some(value)), plugin: PATCH.into(), deleted: false });
                    if let Some((_, rec)) = latest.get_mut(key)
                        && let Some(m) = rec.as_object_mut()
                    {
                        m.insert("prev_id".into(), value.clone());
                    }
                }
            }
        }
        let order = topic_order(&defs);
        let lost = orphans(&order);
        let rows: Vec<Value> = order
            .iter()
            .map(|placed| {
                let (winner, rec) = latest.get(&placed.key).cloned().unwrap_or((String::new(), json!({})));
                let mut bits: Vec<String> = [
                    ("speaker_id", "who:"),
                    ("speaker_race", "race:"),
                    ("speaker_class", "class:"),
                    ("speaker_faction", "faction:"),
                    ("speaker_cell", "cell:"),
                    ("player_faction", "player faction:"),
                ]
                .iter()
                .filter(|(k, _)| rec.get(*k).is_some_and(crate::session::truthy))
                .map(|(k, label)| format!("{label} {}", str_of(&rec[*k])))
                .collect();
                if let Some(f) = rec.get("filters").filter(|f| crate::session::truthy(f)) {
                    bits.push(format!("{} condition(s)", f.as_array().map_or(0, Vec::len)));
                }
                let disposition = rec.get("data").filter(|d| d.is_object()).and_then(|d| d.get("disposition")).cloned().unwrap_or(Value::Null);
                json!({
                    "id": placed.key,
                    "text": text(rec.get("text")),
                    "speaker": bits.join(", "),
                    "disposition": disposition,
                    "plugins": placed.plugins,
                    "winner": winner,
                    "orphan": lost.contains(&placed.key),
                    "quest": text(rec.get("quest_state")),
                })
            })
            .collect();
        Ok(json!({"id": if spelled.is_empty() { topic_id.to_string() } else { spelled }, "type": kind, "responses": rows}))
    }

    fn response_ids(view: &Value) -> Vec<String> {
        view["responses"].as_array().into_iter().flatten().map(|r| r["id"].as_str().unwrap_or("").to_string()).collect()
    }

    /// Moves a response to after another (or to the top, `after` empty) - drag to
    /// reorder: its `prev_id` (and `next_id`, naming the one that will follow it, as the
    /// Construction Set writes it), queued in the pool. The topic after the move.
    pub fn move_response(&mut self, topic_id: &str, info_id: &str, after: &str) -> Result<Value, EditError> {
        let view = self.topic(topic_id)?;
        let ids = Editor::response_ids(&view);
        let name = view["id"].as_str().unwrap_or("").to_string();
        if !ids.iter().any(|i| i == info_id) {
            return err(format!("{info_id} is not a response of {name}"));
        }
        if !after.is_empty() && !ids.iter().any(|i| i == after) {
            return err(format!("{after} is not a response of {name}"));
        }
        if after == info_id {
            return err("a response cannot follow itself");
        }
        let rest: Vec<String> = ids.iter().filter(|i| *i != info_id).cloned().collect();
        let at = if after.is_empty() { 0 } else { rest.iter().position(|i| i == after).unwrap() + 1 };
        let following = rest.get(at).cloned().unwrap_or_default();
        let mut placed = rest.clone();
        placed.insert(at, info_id.to_string());
        if placed == ids {
            return Ok(view); // already there
        }
        if let Some(made) = self.queue.new_record("DialogueInfo", info_id).cloned() {
            let mut record = made.record.clone();
            if let Some(m) = record.as_object_mut() {
                m.insert("prev_id".into(), json!(after));
                m.insert("next_id".into(), json!(following));
            }
            self.queue.add_new_record(NewRecord { record, ..made });
        } else {
            let Some(found) = self.find("INFO", info_id, None) else {
                return err(format!("{info_id} is not a response of this load order"));
            };
            for (path, value) in [("prev_id", after), ("next_id", following.as_str())] {
                self.queue.add_field(&found.record_type, &found.key, Choice::Value { path: path.into(), value: json!(value) });
            }
            self.queue.set_base(&found.record_type, &found.key, found.winner());
        }
        self.save_journal();
        self.topic(topic_id)
    }

    /// Adds a response to a topic after another (or at the top), in the patch: placed by
    /// its `prev_id` (the patch loads last), its `next_id` naming the one that followed.
    /// The response, as `find` finds it.
    pub fn new_response(&mut self, topic_id: &str, after: &str) -> Result<Found, EditError> {
        let view = self.topic(topic_id)?;
        let ids = Editor::response_ids(&view);
        let name = view["id"].as_str().unwrap_or("").to_string();
        if !after.is_empty() && !ids.iter().any(|i| i == after) {
            return err(format!("{after} is not a response of {name}"));
        }
        let want = topic_id.trim().to_lowercase();
        let source = self
            .topics()
            .into_iter()
            .find(|t| t["id"].as_str().unwrap_or("").to_lowercase() == want)
            .and_then(|t| t["plugins"].as_array().and_then(|p| p.last()).and_then(Value::as_str).map(str::to_string))
            .unwrap_or_default();
        let following = if after.is_empty() {
            ids.first().cloned().unwrap_or_default()
        } else {
            let at = ids.iter().position(|i| i == after).unwrap() + 1;
            ids.get(at).cloned().unwrap_or_default()
        };
        let mut rid = new_info_id();
        while ids.contains(&rid) {
            rid = new_info_id();
        }
        let mut record = blank("DialogueInfo").expect("the crate has a blank response");
        record["id"] = json!(rid);
        record["prev_id"] = json!(after);
        record["next_id"] = json!(following);
        let kind = view["type"].as_str().unwrap_or("");
        if !kind.is_empty()
            && let Some(data) = record.get_mut("data").and_then(Value::as_object_mut)
        {
            data.insert("dialogue_type".into(), json!(kind));
        }
        self.queue.add_new_record(NewRecord { record_type: "DialogueInfo".into(), key: rid.clone(), record: record.clone(), source, topic: name });
        self.save_journal();
        Ok(Found { tag: "INFO".into(), record_type: "DialogueInfo".into(), key: rid, versions: vec![(PATCH.into(), record)], new: true })
    }

    /// Copies a topic under a new name with its responses, in the patch: the topic's
    /// record the patch's own (its kind kept), each response the version the load order
    /// uses, under a new id, linked to the copies before and after it. The new topic.
    pub fn copy_topic(&mut self, topic_id: &str, new_id: &str) -> Result<Value, EditError> {
        let view = self.topic(topic_id)?;
        let name = new_id.trim().to_string();
        if name.is_empty() {
            return err("the topic needs a name");
        }
        if self.topics().iter().any(|t| t["id"].as_str().unwrap_or("").to_lowercase() == name.to_lowercase()) {
            return err(format!("{name} is already a topic in this load order"));
        }
        let from = view["id"].as_str().unwrap_or("").to_lowercase();
        let mut latest = self.latest_responses(&from);
        for made in self.queue.new_records() {
            if made.record_type == "DialogueInfo" && made.topic.to_lowercase() == from {
                latest.insert(made.key.clone(), (made.source.clone(), made.record.clone()));
            }
        }
        let mut topic = blank("Dialogue").expect("the crate has a blank topic");
        topic["id"] = json!(name);
        let kind = view["type"].as_str().unwrap_or("");
        if !kind.is_empty() {
            topic["dialogue_type"] = json!(kind);
        }
        self.queue.add_new_record(NewRecord { record_type: "Dialogue".into(), key: name.clone(), record: topic, source: String::new(), topic: String::new() });
        let rows = Editor::response_ids(&view);
        let ids: Vec<String> = rows.iter().map(|_| new_info_id()).collect();
        for (i, row) in rows.iter().enumerate() {
            let (source, found) = latest.get(row).cloned().unwrap_or((String::new(), json!({})));
            let mut rec = if found.is_object() { found } else { json!({}) };
            let m = rec.as_object_mut().unwrap();
            m.insert("type".into(), json!("DialogueInfo"));
            m.insert("id".into(), json!(ids[i]));
            m.insert("prev_id".into(), json!(if i > 0 { ids[i - 1].as_str() } else { "" }));
            m.insert("next_id".into(), json!(ids.get(i + 1).map_or("", String::as_str)));
            let flags = text(m.get("flags"));
            let kept: Vec<&str> = flags.split(" | ").filter(|f| !f.trim().is_empty() && f.trim() != "DELETED").collect();
            m.insert("flags".into(), json!(kept.join(" | ")));
            self.queue.add_new_record(NewRecord { record_type: "DialogueInfo".into(), key: ids[i].clone(), record: rec, source, topic: name.clone() });
        }
        self.save_journal();
        self.topic(&name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(key: &str, prev: &str, plugin: &str) -> Response {
        Response { key: key.into(), prev: prev.into(), plugin: plugin.into(), deleted: false }
    }

    #[test]
    fn the_engine_order() {
        let keys = |o: &[Placed]| o.iter().map(|p| p.key.clone()).collect::<Vec<_>>();
        let o = topic_order(&[r("a", "", "M"), r("b", "a", "M"), r("c", "b", "M"), r("x", "a", "P"), r("y", "zz", "P")]);
        assert_eq!(keys(&o), ["a", "x", "b", "c", "y"]);
        assert_eq!(orphans(&o), ["y"]);
        let o = topic_order(&[r("a", "", "M"), r("b", "a", "M"), r("a", "b", "P")]);
        assert_eq!(keys(&o), ["b", "a"]);
        assert_eq!(o[1].plugins, ["M", "P"]);
        let mut del = r("a", "", "P");
        del.deleted = true;
        assert_eq!(keys(&topic_order(&[r("a", "", "M"), r("b", "a", "M"), del])), ["b"]);
        let id = new_info_id();
        assert_eq!(id.len(), 19);
    }
}
