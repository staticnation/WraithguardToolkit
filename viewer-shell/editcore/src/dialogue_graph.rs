//! The load order's dialogue as something to look at: a topic's flow, the map around it,
//! the flags the dialogue writes and tests, and a rehearsal of the game's dialogue window
//! (`wraithguard/patch/dialogue_graph.py`, with the conditions in words of
//! `wraithguard/tes3fields/dialogue.py` - the toolkit's adaptations of MWDE's - and
//! `workflow.who_can_say`). Port step 4.1, slice 5c. Cases:
//! `tests/fixtures/dialogue_cases.json`, run here and by `tests/test_dialogue_cases.py`.
//!
//! - **Flow** ([`DialogueIndex::flow`]): one topic's responses in engine order, each with
//!   its conditions in words, its text cut into words and topic links, and what its result
//!   script does (the `Choice` options, topics added, quest stages, variables set).
//! - **Map** ([`DialogueIndex::neighbourhood`]): topics as a graph around one topic.
//! - **Flags** ([`DialogueIndex::flags`]): every global, local, journal index and item the
//!   dialogue writes or tests, and who does.
//! - **Rehearsal** ([`DialogueIndex::play`]): an NPC's greeting, the topics they answer,
//!   the response the engine would pick for each.

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, OnceLock};

use indexmap::{IndexMap, IndexSet};
use regex::Regex;
use serde_json::{json, Value};

use crate::dialogue::{orphans, responses_by_topic, topic_order, Response};
use crate::edit::{err, set_at, EditError, Editor};
use crate::pyval::{float_repr, str_of};
use crate::queue::Choice;
use crate::session::{truthy, PATCH};

/// The topic kinds the game lists, hyperlinks and asks.
pub const TOPIC: &str = "Topic";
pub const GREETING: &str = "Greeting";

/// How many nodes a map answers at most.
pub const MAP_CAP: usize = 80;

macro_rules! re {
    ($name:ident, $pat:expr) => {
        fn $name() -> &'static Regex {
            static R: OnceLock<Regex> = OnceLock::new();
            R.get_or_init(|| Regex::new($pat).expect("a valid pattern"))
        }
    };
}

re!(choice_arg, r#""([^"]*)"\s*,?\s*(-?\d+)"#);
re!(line_re, r"[^\r\n]+");
re!(set_re, r#"(?i)^\s*set\s+(?:"?([\w.\- ]+?)"?\s*->\s*)?"?([\w.\-]+)"?\s+to\b"#);
re!(call_re, r#"(?i)^\s*(?:"?([\w.\- ]+?)"?\s*->\s*)?([a-z_]\w*)\b\s*,?\s*(.*)$"#);
re!(first_arg, r#"^\s*(?:"([^"]*)"|([^\s,]+))\s*,?\s*(.*)$"#);
re!(macro_re, r"%[A-Za-z]+");
re!(word_re, r"[\w']+");
re!(lead_num, r"^\s*(-?\d+)");
re!(camel, r"[A-Z][a-z0-9]*");

/// Python's `str(v or "")`.
fn text(v: Option<&Value>) -> String {
    match v {
        Some(v) if truthy(v) => str_of(v),
        _ => String::new(),
    }
}

/// Python's `str(v or "").strip().lower()`.
fn low(v: Option<&Value>) -> String {
    text(v).trim().to_lowercase()
}

// -- the conditions in words (tes3fields/dialogue.py) -----------------------------------

const COMPARISONS: [(&str, &str); 6] = [("Equal", "="), ("NotEqual", "!="), ("Greater", ">"), ("GreaterEqual", ">="), ("Less", "<"), ("LessEqual", "<=")];

const FUNCTION_NAMES: [(&str, &str); 7] = [
    ("PcCorprus", "PC Corprus"),
    ("FactionRankDifference", "PC Rank Minus NPC Rank"),
    ("SameSex", "Same Gender As PC"),
    ("SameRace", "Same Race As PC"),
    ("SameFaction", "Same Faction As PC"),
    ("TalkedToPc", "Talked To PC"),
    ("CreatureTarget", "Is Targeting A Creature"),
];

/// A function's name in words: `PcAxe` -> `PC Axe`.
fn prettify_function(name: &str) -> String {
    if let Some((_, n)) = FUNCTION_NAMES.iter().find(|(k, _)| *k == name) {
        return n.to_string();
    }
    let words: Vec<&str> = camel().find_iter(name).map(|m| m.as_str()).collect();
    if words.is_empty() {
        return name.to_string();
    }
    words.iter().map(|w| if *w == "Pc" { "PC" } else { w }).collect::<Vec<_>>().join(" ")
}

/// A filter's number: the `data` of its tagged value (or a bare number).
fn filter_value(flt: &Value) -> Value {
    let v = flt.get("value");
    let data = match v {
        Some(Value::Object(m)) => m.get("data").cloned().unwrap_or(json!(0)),
        Some(n @ Value::Number(_)) => n.clone(),
        _ => json!(0),
    };
    if data.is_number() { data } else { json!(0) }
}

/// A number as the conditions write it: a whole float without its `.0`.
fn number(v: &Value) -> String {
    match v.as_i64() {
        Some(i) => i.to_string(),
        None => {
            let x = v.as_f64().unwrap_or(0.0);
            if x.fract() == 0.0 && x.abs() < 1e18 { (x as i64).to_string() } else { float_repr(x) }
        }
    }
}

/// One condition of a response as an English clause; `None` for an unused one.
pub fn describe_filter(flt: &Value) -> Option<String> {
    let ftype = match flt.get("filter_type") {
        None | Some(Value::Null) => return None,
        Some(v) => str_of(v),
    };
    if ftype == "None" {
        return None;
    }
    let comparison = flt.get("comparison").map(str_of).unwrap_or_else(|| "None".into());
    let comp = COMPARISONS.iter().find(|(k, _)| *k == comparison).map_or("?", |(_, s)| s);
    let value = filter_value(flt);
    let x = value.as_f64().unwrap_or(0.0);
    let num = number(&value);
    let variable = text(flt.get("id"));
    match ftype.as_str() {
        "Function" => {
            let func = text(flt.get("function"));
            let name = prettify_function(&func);
            if func == "PcSex" {
                return Some(format!("{name} {comp} {num} ({})", if x != 0.0 { "female" } else { "male" }));
            }
            return Some(format!("{name} {comp} {num}"));
        }
        "Global" => return Some(format!("global {variable} {comp} {num}")),
        "Local" => return Some(format!("local {variable} {comp} {num}")),
        "Journal" => return Some(format!("quest {variable} {comp} {num}")),
        "Item" => return Some(format!("player inventory {variable} {comp} {num}")),
        "Dead" => {
            if (matches!(comp, "=" | "<=") && x == 0.0) || (comp == "<" && x == 1.0) {
                return Some(format!("NPC {variable} is not dead"));
            }
            return Some(format!("NPC death count for {variable} {comp} {num}"));
        }
        _ => {}
    }
    let template = match ftype.as_str() {
        "NotId" => Some("NPC is {neg}{variable}"),
        "NotFaction" => Some("NPC is {neg}a member of faction {variable}"),
        "NotClass" => Some("NPC is {neg}a member of class {variable}"),
        "NotRace" => Some("NPC is {neg}a member of race {variable}"),
        "NotCell" => Some("NPC is {neg}located in cell {variable}"),
        _ => None,
    };
    if let Some(t) = template {
        let fill = |neg: &str| t.replace("{neg}", neg).replace("{variable}", &variable);
        if (matches!(comp, "=" | "<=") && x == 0.0) || (matches!(comp, "!=" | "<") && x == 1.0) {
            return Some(fill("not "));
        }
        if (matches!(comp, "=" | ">=") && x == 1.0) || (matches!(comp, "!=" | ">") && x == 0.0) {
            return Some(fill(""));
        }
        return Some(format!("{ftype} {variable} {comp} {num}"));
    }
    if ftype == "NotLocal" {
        return Some(format!("not local {variable} {comp} {num}"));
    }
    Some(format!("{ftype} {variable} {comp} {num}").replace("  ", " ").trim().to_string())
}

/// A whole number of a JSON value (not a float, as Python's `isinstance(v, int)`).
fn int_of(v: Option<&Value>) -> Option<i64> {
    v.filter(|v| v.is_i64() || v.is_u64()).and_then(Value::as_i64)
}

/// A response's conditions as "If ..." lines: its data block's, then its filters'.
pub fn condition_lines(rec: &Value) -> Vec<String> {
    let mut out = Vec::new();
    let data = rec.get("data").filter(|d| d.is_object()).cloned().unwrap_or(json!({}));
    let kind = data.get("dialogue_type").and_then(Value::as_str);
    let faction = rec.get("player_faction").filter(|v| truthy(v)).map(str_of);
    let prank = data.get("player_rank");
    if let Some(f) = &faction {
        if prank.and_then(Value::as_f64) == Some(0.0) {
            out.push(format!("player is not a member of faction {f}"));
        } else if let Some(r) = int_of(prank).filter(|r| *r > 0) {
            out.push(format!("player rank in faction {f} is at least {r}"));
        } else {
            out.push(format!("player is a member of faction {f}"));
        }
    }
    let disposition = int_of(data.get("disposition"));
    if kind == Some("Journal") && let Some(d) = disposition {
        let state = text(rec.get("quest_state"));
        let suffix = if state.is_empty() { String::new() } else { format!(" ({state})") };
        out.push(format!("quest index is {d}{suffix}"));
    } else if let Some(d) = disposition.filter(|d| *d > 0) {
        out.push(format!("disposition is at least {d}"));
    }
    match data.get("speaker_sex").and_then(Value::as_str) {
        Some("Male") => out.push("NPC gender is male".into()),
        Some("Female") => out.push("NPC gender is female".into()),
        _ => {}
    }
    if let Some(r) = int_of(data.get("speaker_rank")).filter(|r| *r > 0) {
        out.push(format!("NPC rank is at least {r}"));
    }
    for flt in rec.get("filters").and_then(Value::as_array).into_iter().flatten() {
        if flt.is_object()
            && let Some(clause) = describe_filter(flt).filter(|c| !c.is_empty())
        {
            out.push(clause);
        }
    }
    out
}

// -- result scripts ----------------------------------------------------------------------

/// What a result script does, as far as reading it tells.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Effects {
    /// `(label, number)` per `Choice` option.
    pub choices: Vec<(String, i64)>,
    pub addtopics: Vec<String>,
    /// `(quest, index)` per `Journal`/`SetJournalIndex`.
    pub journal: Vec<(String, i64)>,
    /// `(variable, target)` per `set X to` (target: the reference before `->`).
    pub sets: Vec<(String, String)>,
    /// `(item, "add"/"remove")`.
    pub items: Vec<(String, String)>,
    pub goodbye: bool,
    /// Every other call, once each.
    pub other: Vec<String>,
}

/// A script line without its `;` comment (a `;` inside quotes is text).
fn strip_comment(line: &str) -> &str {
    let mut quoted = false;
    for (i, ch) in line.char_indices() {
        if ch == '"' {
            quoted = !quoted;
        } else if ch == ';' && !quoted {
            return &line[..i];
        }
    }
    line
}

/// A call's first argument (quoted or bare) and what follows it.
fn arg(rest: &str) -> (String, String) {
    match first_arg().captures(rest) {
        None => (String::new(), String::new()),
        Some(m) => {
            let a = m.get(1).or(m.get(2)).map_or("", |x| x.as_str()).to_string();
            (a, m.get(3).map_or("", |x| x.as_str()).to_string())
        }
    }
}

const NOT_CALLS: [&str; 10] = ["if", "elseif", "else", "endif", "while", "endwhile", "short", "long", "float", "return"];

/// Reads a result script for what it does (a line scan, not a compiler).
pub fn script_effects(source: &str) -> Effects {
    let mut out = Effects::default();
    for raw in line_re().find_iter(source) {
        let line = strip_comment(raw.as_str()).trim();
        if line.is_empty() {
            continue;
        }
        if let Some(m) = set_re().captures(line) {
            out.sets.push((m[2].to_string(), m.get(1).map_or("", |x| x.as_str()).trim().to_string()));
            continue;
        }
        let Some(m) = call_re().captures(line) else { continue };
        let target = m.get(1).map_or("", |x| x.as_str()).trim().to_string();
        let name = m[2].to_lowercase();
        let rest = m.get(3).map_or("", |x| x.as_str());
        if NOT_CALLS.contains(&name.as_str()) {
            continue;
        }
        match name.as_str() {
            "choice" => {
                for c in choice_arg().captures_iter(rest) {
                    if let Ok(n) = c[2].parse() {
                        out.choices.push((c[1].to_string(), n));
                    }
                }
            }
            "addtopic" => {
                let (topic, _) = arg(rest);
                if !topic.is_empty() {
                    out.addtopics.push(topic);
                }
            }
            "journal" | "setjournalindex" => {
                let (quest, more) = arg(rest);
                if let Some(n) = lead_num().captures(&more).and_then(|c| c[1].parse().ok())
                    && !quest.is_empty()
                {
                    out.journal.push((quest, n));
                }
            }
            "additem" | "removeitem" => {
                let (item, _) = arg(rest);
                if !item.is_empty() {
                    out.items.push((item, if name == "additem" { "add" } else { "remove" }.into()));
                }
            }
            "goodbye" => out.goodbye = true,
            _ => {
                let call = if target.is_empty() { m[2].to_string() } else { format!("{target}->{}", &m[2]) };
                if !out.other.contains(&call) {
                    out.other.push(call);
                }
            }
        }
    }
    out
}

/// The `Choice` a response answers (its `Function Choice = n` condition), if any.
pub fn choice_gate(rec: &Value) -> Option<i64> {
    for flt in rec.get("filters").and_then(Value::as_array).into_iter().flatten() {
        if flt.get("filter_type").and_then(Value::as_str) == Some("Function") && flt.get("function").and_then(Value::as_str) == Some("Choice") {
            let v = flt.get("value");
            let data = match v {
                Some(Value::Object(m)) => m.get("data"),
                other => other,
            };
            return data.and_then(Value::as_f64).map(|x| x.trunc() as i64);
        }
    }
    None
}

/// The named things a response's conditions test: `(kind, name, clause)`.
pub fn reads_of(rec: &Value) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    for flt in rec.get("filters").and_then(Value::as_array).into_iter().flatten() {
        if !flt.is_object() {
            continue;
        }
        let kind = match flt.get("filter_type").map(str_of).as_deref() {
            Some("Global") => "global",
            Some("Local" | "NotLocal") => "local",
            Some("Journal") => "journal",
            Some("Item") => "item",
            Some("Dead") => "dead",
            _ => continue,
        };
        let name = text(flt.get("id"));
        if !name.is_empty() {
            out.push((kind.to_string(), name, describe_filter(flt).unwrap_or_default()));
        }
    }
    out
}

/// Who says it: `Actor: x & Race: y`, or `anyone`.
pub fn speaker_line(rec: &Value) -> String {
    let parts: Vec<String> = [("speaker_id", "Actor"), ("speaker_race", "Race"), ("speaker_class", "Class"), ("speaker_faction", "Faction"), ("speaker_cell", "Cell")]
        .iter()
        .filter(|(k, _)| rec.get(*k).is_some_and(truthy))
        .map(|(k, label)| format!("{label}: {}", str_of(&rec[*k])))
        .collect();
    if parts.is_empty() { "anyone".into() } else { parts.join(" & ") }
}

/// An NPC (or creature) record as a rehearsal reads the speaker.
pub fn npc_of(record: &Value) -> Value {
    let flags = text(record.get("npc_flags")).to_uppercase();
    let name = if record.get("name").is_some_and(truthy) { text(record.get("name")) } else { text(record.get("id")) };
    json!({
        "id": text(record.get("id")),
        "name": name,
        "race": text(record.get("race")),
        "class": text(record.get("class")),
        "faction": text(record.get("faction")),
        "sex": if flags.contains("FEMALE") { "Female" } else { "Male" },
    })
}

// -- the index ---------------------------------------------------------------------------

/// One response, as every view reads it.
pub struct Line {
    /// The topic, as spelled.
    pub topic: String,
    pub key: String,
    /// The winning record, the pool's typed changes applied.
    pub rec: Value,
    pub plugins: Vec<String>,
    /// Its predecessor is not in the topic (the engine puts it last).
    pub orphan: bool,
    effects: OnceLock<Effects>,
}

impl Line {
    pub fn text(&self) -> String {
        text(self.rec.get("text"))
    }
    pub fn script(&self) -> String {
        text(self.rec.get("script_text"))
    }
    /// What its result script does (read once).
    pub fn effects(&self) -> &Effects {
        self.effects.get_or_init(|| script_effects(&self.script()))
    }
}

/// `(kind, target, via)` links out of a topic.
type Out = Vec<(String, String, String)>;

/// Every topic and response of a load order, read once, for the views.
pub struct DialogueIndex {
    kinds: IndexMap<String, String>,
    spelled: IndexMap<String, String>,
    /// Topic (lower) -> its responses in engine order.
    pub lines: IndexMap<String, Vec<Line>>,
    phrases: OnceLock<(HashMap<String, String>, HashMap<String, Vec<usize>>)>,
    out_cache: Mutex<HashMap<String, Arc<Out>>>,
    incoming: OnceLock<HashMap<String, Vec<(String, String, String)>>>,
    flag_index: OnceLock<IndexMap<(String, String), Value>>,
}

/// A greeting's name as the greetings are sorted: digits as numbers ("Greeting 10" after
/// "Greeting 9").
fn natural_key(s: &str) -> Vec<(bool, String)> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut digits = false;
    for c in s.chars() {
        let d = c.is_ascii_digit();
        if d != digits {
            out.push((digits, std::mem::take(&mut cur)));
            digits = d;
        }
        cur.push(c);
    }
    out.push((digits, cur));
    if out.first().is_some_and(|(d, _)| *d) {
        out.insert(0, (false, String::new()));
    }
    out.into_iter().map(|(d, t)| if d { (true, t.trim_start_matches('0').to_string()) } else { (false, t.to_lowercase()) }).collect()
}

fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (ka, kb) = (natural_key(a), natural_key(b));
    for (x, y) in ka.iter().zip(&kb) {
        let o = if x.0 && y.0 { (x.1.len(), &x.1).cmp(&(y.1.len(), &y.1)) } else { x.1.cmp(&y.1) };
        if o != Ordering::Equal {
            return o;
        }
    }
    ka.len().cmp(&kb.len())
}

impl DialogueIndex {
    /// Reads every topic: its responses in engine order, with what the patch makes
    /// (`(record type, topic, key, record)`) and its typed changes to responses
    /// (response id -> `{path: value}`) laid over.
    pub fn new(plugins: &[(String, Arc<Vec<Value>>)], made: &[(String, String, String, Value)], edits: &IndexMap<String, IndexMap<String, Value>>) -> DialogueIndex {
        let mut defs: IndexMap<String, Vec<Response>> = IndexMap::new();
        let mut latest: IndexMap<(String, String), Value> = IndexMap::new();
        let mut kinds: IndexMap<String, String> = IndexMap::new();
        let mut spelled: IndexMap<String, String> = IndexMap::new();
        for (name, records) in plugins {
            for (tid, found) in responses_by_topic(records, name) {
                let low = tid.to_lowercase();
                defs.entry(low.clone()).or_default().extend(found);
                spelled.entry(low).or_insert(tid);
            }
            let mut current = String::new();
            for rec in records.iter() {
                match rec.get("type").and_then(Value::as_str) {
                    Some("Dialogue") => {
                        let tid = text(rec.get("id"));
                        current = tid.to_lowercase();
                        spelled.entry(current.clone()).or_insert(tid);
                        let k = text(rec.get("dialogue_type"));
                        let k = if k.is_empty() { kinds.get(&current).cloned().unwrap_or_default() } else { k };
                        kinds.insert(current.clone(), k);
                    }
                    Some("DialogueInfo") if !current.is_empty() => {
                        latest.insert((current.clone(), text(rec.get("id"))), rec.clone());
                    }
                    _ => {}
                }
            }
        }
        for (record_type, topic, key, rec) in made {
            let low = if record_type == "DialogueInfo" { topic.to_lowercase() } else { key.to_lowercase() };
            if record_type == "Dialogue" {
                spelled.entry(low.clone()).or_insert(key.clone());
                let k = text(rec.get("dialogue_type"));
                let k = if k.is_empty() { kinds.get(&low).cloned().unwrap_or_default() } else { k };
                kinds.insert(low, k);
            } else if record_type == "DialogueInfo" {
                spelled.entry(low.clone()).or_insert(topic.clone());
                defs.entry(low.clone()).or_default().push(Response { key: key.clone(), prev: text(rec.get("prev_id")), plugin: PATCH.into(), deleted: false });
                latest.insert((low, key.clone()), rec.clone());
            }
        }
        let edits: IndexMap<String, &IndexMap<String, Value>> = edits.iter().map(|(k, v)| (k.to_lowercase(), v)).collect();
        // A response the pool moves (its prev_id queued) is the patch's version, read last,
        // as the dialogue window reads it (`Editor::topic`).
        let mut placed_in: HashMap<String, Vec<(String, String)>> = HashMap::new();
        for (low, key) in latest.keys() {
            placed_in.entry(key.to_lowercase()).or_default().push((low.clone(), key.clone()));
        }
        for (moved, changes) in &edits {
            if let Some(prev) = changes.get("prev_id") {
                for (low, key) in placed_in.get(moved).map(Vec::as_slice).unwrap_or(&[]) {
                    defs.entry(low.clone()).or_default().push(Response { key: key.clone(), prev: text(Some(prev)), plugin: PATCH.into(), deleted: false });
                }
            }
        }
        let mut lines: IndexMap<String, Vec<Line>> = IndexMap::new();
        for (low, found) in &defs {
            let order = topic_order(found);
            let lost = orphans(&order);
            let out = order
                .into_iter()
                .map(|placed| {
                    let mut rec = latest.get(&(low.clone(), placed.key.clone())).cloned().unwrap_or(json!({}));
                    for (path, value) in edits.get(&placed.key.to_lowercase()).map(|e| e.iter()).into_iter().flatten() {
                        let _ = set_at(&mut rec, path, value.clone());
                    }
                    Line {
                        topic: spelled.get(low).cloned().unwrap_or_else(|| low.clone()),
                        orphan: lost.contains(&placed.key),
                        key: placed.key,
                        rec,
                        plugins: placed.plugins,
                        effects: OnceLock::new(),
                    }
                })
                .collect();
            lines.insert(low.clone(), out);
        }
        for low in spelled.keys() {
            lines.entry(low.clone()).or_default();
        }
        DialogueIndex { kinds, spelled, lines, phrases: OnceLock::new(), out_cache: Mutex::default(), incoming: OnceLock::new(), flag_index: OnceLock::new() }
    }

    /// A topic's kind (`Topic`, `Greeting`, `Journal`, ...).
    pub fn kind(&self, topic: &str) -> String {
        self.kinds.get(&topic.to_lowercase()).cloned().unwrap_or_default()
    }

    fn spelled_of(&self, low: &str) -> String {
        self.spelled.get(low).cloned().unwrap_or_else(|| low.to_string())
    }

    /// Every topic of a kind, as spelled, by name.
    pub fn topic_names(&self, kind: &str) -> Vec<String> {
        let mut v: Vec<String> = self.kinds.iter().filter(|(_, k)| *k == kind).map(|(low, _)| self.spelled_of(low)).collect();
        v.sort_by_key(|s| s.to_lowercase());
        v
    }

    /// Every `Topic` by its words (lower), and per first word how many words they have.
    fn phrases(&self) -> &(HashMap<String, String>, HashMap<String, Vec<usize>>) {
        self.phrases.get_or_init(|| {
            let mut phrases = HashMap::new();
            let mut starts: HashMap<String, HashSet<usize>> = HashMap::new();
            for name in self.topic_names(TOPIC) {
                let words: Vec<String> = word_re().find_iter(&name.to_lowercase()).map(|m| m.as_str().to_string()).collect();
                if let Some(first) = words.first() {
                    starts.entry(first.clone()).or_default().insert(words.len());
                    phrases.insert(words.join(" "), name.clone());
                }
            }
            let starts = starts
                .into_iter()
                .map(|(w, ns)| {
                    let mut v: Vec<usize> = ns.into_iter().collect();
                    v.sort_by(|a, b| b.cmp(a));
                    (w, v)
                })
                .collect();
            (phrases, starts)
        })
    }

    /// Where a text names topics, as the game hyperlinks them: `(start, end, topic)`, the
    /// longest name at each place, never overlapping.
    fn matches(&self, text: &str) -> Vec<(usize, usize, String)> {
        let (phrases, starts) = self.phrases();
        if phrases.is_empty() {
            return Vec::new();
        }
        let spans: Vec<(usize, usize)> = word_re().find_iter(text).map(|m| (m.start(), m.end())).collect();
        let words: Vec<String> = spans.iter().map(|(a, b)| text[*a..*b].to_lowercase()).collect();
        let mut out = Vec::new();
        let mut i = 0;
        while i < words.len() {
            let mut hit = None;
            for &n in starts.get(&words[i]).map(Vec::as_slice).unwrap_or(&[]) {
                if i + n > words.len() {
                    continue;
                }
                let key = if n == 1 { words[i].clone() } else { words[i..i + n].join(" ") };
                if let Some(name) = phrases.get(&key) {
                    hit = Some((spans[i].0, spans[i + n - 1].1, name.clone(), n));
                    break;
                }
            }
            match hit {
                Some((a, b, name, n)) => {
                    out.push((a, b, name));
                    i += n;
                }
                None => i += 1,
            }
        }
        out
    }

    /// Plain text with its `%Macro` names marked.
    fn macros(text: &str) -> Vec<Value> {
        let mut out = Vec::new();
        let mut pos = 0;
        for m in macro_re().find_iter(text) {
            if m.start() > pos {
                out.push(json!({"t": &text[pos..m.start()]}));
            }
            out.push(json!({"t": m.as_str(), "macro": &m.as_str()[1..]}));
            pos = m.end();
        }
        if pos < text.len() {
            out.push(json!({"t": &text[pos..]}));
        }
        out
    }

    /// A line's text cut into plain words, topic links and macros, as the game shows it;
    /// only the `allowed` topics (lower) are links when given.
    pub fn segments(&self, text: &str, allowed: Option<&HashSet<String>>) -> Vec<Value> {
        let mut out = Vec::new();
        let mut pos = 0;
        for (start, end, name) in self.matches(text) {
            if allowed.is_some_and(|a| !a.contains(&name.to_lowercase())) {
                continue;
            }
            if start > pos {
                out.extend(DialogueIndex::macros(&text[pos..start]));
            }
            out.push(json!({"t": &text[start..end], "topic": name}));
            pos = end;
        }
        if pos < text.len() {
            out.extend(DialogueIndex::macros(&text[pos..]));
        }
        out
    }

    /// The topics a response's text hyperlinks, once each.
    pub fn links_of(&self, line: &Line) -> Vec<String> {
        let set: IndexSet<String> = self.matches(&line.text()).into_iter().map(|(_, _, n)| n).collect();
        set.into_iter().collect()
    }

    fn line_view(&self, line: &Line, index: usize) -> Value {
        let eff = line.effects();
        let rec = &line.rec;
        let text = line.text();
        let disposition = rec.get("data").filter(|d| d.is_object()).map(|d| d.get("disposition").cloned().unwrap_or(Value::Null)).unwrap_or(Value::Null);
        json!({
            "id": line.key,
            "n": index + 1,
            "speaker": speaker_line(rec),
            "conditions": condition_lines(rec),
            "gate": choice_gate(rec),
            "text": text,
            "segments": self.segments(&text, None),
            "script": line.script(),
            "choices": eff.choices.iter().map(|(l, n)| json!({"label": l, "n": n})).collect::<Vec<_>>(),
            "addtopics": eff.addtopics,
            "journal": eff.journal.iter().map(|(q, i)| json!({"quest": q, "index": i})).collect::<Vec<_>>(),
            "sets": eff.sets.iter().map(|(n, t)| json!({"name": n, "target": t})).collect::<Vec<_>>(),
            "items": eff.items.iter().map(|(i, v)| json!({"item": i, "verb": v})).collect::<Vec<_>>(),
            "goodbye": eff.goodbye,
            "other": eff.other,
            "links": self.links_of(line),
            "plugins": line.plugins,
            "orphan": line.orphan,
            "quest": crate::dialogue_graph::text(rec.get("quest_state")),
            "disposition": disposition,
        })
    }

    /// One topic as a flow: `{id, type, responses, outof, into}` - its responses in engine
    /// order (each `Choice` option and the responses gated on its number make the tree
    /// inside the topic), and the topics it leads to and that lead to it.
    pub fn flow(&self, topic: &str) -> Result<Value, EditError> {
        let low = topic.trim().to_lowercase();
        let Some(lines) = self.lines.get(&low) else {
            return err(format!("no plugin of this load order has the topic {topic}"));
        };
        let rows: Vec<Value> = lines.iter().enumerate().map(|(i, l)| self.line_view(l, i)).collect();
        let (outs, ins) = self.edges_of(&low);
        Ok(json!({"id": self.spelled.get(&low).cloned().unwrap_or_else(|| topic.to_string()), "type": self.kind(&low), "responses": rows, "outof": outs, "into": ins}))
    }

    /// Where a topic leads: `(kind, target, via)` - `addtopic`/`link` to a topic (lower),
    /// `quest` to a journal, `sets` to a variable; `via` the response.
    fn out(&self, low: &str) -> Out {
        let mut seen: HashSet<(String, String)> = HashSet::new();
        let mut out = Vec::new();
        let mut add = |kind: &str, target: String, via: &str| {
            if seen.insert((kind.to_string(), target.clone())) {
                out.push((kind.to_string(), target, via.to_string()));
            }
        };
        for line in self.lines.get(low).map(Vec::as_slice).unwrap_or(&[]) {
            let eff = line.effects();
            for t in &eff.addtopics {
                add("addtopic", t.to_lowercase(), &line.key);
            }
            for t in self.links_of(line) {
                if t.to_lowercase() != low {
                    add("link", t.to_lowercase(), &line.key);
                }
            }
            for (q, _) in &eff.journal {
                add("quest", q.to_lowercase(), &line.key);
            }
            for (name, target) in &eff.sets {
                if target.is_empty() {
                    add("sets", name.to_lowercase(), &line.key);
                }
            }
        }
        out
    }

    fn out_cached(&self, low: &str) -> Arc<Out> {
        if let Some(hit) = self.out_cache.lock().unwrap().get(low) {
            return hit.clone();
        }
        let got = Arc::new(self.out(low));
        self.out_cache.lock().unwrap().insert(low.to_string(), got.clone());
        got
    }

    /// Every `addtopic`/`link` into each topic: topic (lower) -> `(from, kind, via)`.
    fn incoming(&self) -> &HashMap<String, Vec<(String, String, String)>> {
        self.incoming.get_or_init(|| {
            let mut inc: HashMap<String, Vec<(String, String, String)>> = HashMap::new();
            for other in self.lines.keys() {
                for (k, t, v) in self.out_cached(other).iter() {
                    if (k == "addtopic" || k == "link") && t != other {
                        inc.entry(t.clone()).or_default().push((other.clone(), k.clone(), v.clone()));
                    }
                }
            }
            inc
        })
    }

    fn edges_of(&self, low: &str) -> (Vec<Value>, Vec<Value>) {
        let outs = self
            .out(low)
            .into_iter()
            .filter(|(k, _, _)| k == "addtopic" || k == "link")
            .map(|(k, t, v)| json!({"kind": k, "to": self.spelled_of(&t), "via": v}))
            .collect();
        let ins = self
            .incoming()
            .get(low)
            .map(|v| v.iter().map(|(other, k, via)| json!({"kind": k, "from": self.spelled_of(other), "via": via})).collect())
            .unwrap_or_default();
        (outs, ins)
    }

    /// A topic and the topics, quests and variables a step or two from it: `{centre,
    /// nodes: [{id, kind, label, depth, responses}], edges: [{from, to, kind}], capped}`.
    pub fn neighbourhood(&self, topic: &str, depth: i64) -> Result<Value, EditError> {
        let centre = topic.trim().to_lowercase();
        if !self.lines.contains_key(&centre) {
            return err(format!("no plugin of this load order has the topic {topic}"));
        }
        let depth = if depth >= 2 { 2 } else { 1 };
        let mut map = Map { nodes: IndexMap::new(), edges: Vec::new(), seen: HashSet::new(), capped: false };
        let topic_kind = |low: &str| {
            let k = self.kind(low);
            if k.is_empty() { TOPIC.to_string() } else { k }
        };
        let responses = |nid: &str| nid.strip_prefix("t:").map_or(0, |low| self.lines.get(low).map_or(0, Vec::len));
        map.node(&format!("t:{centre}"), &topic_kind(&centre), &self.spelled_of(&centre), 0, responses(&format!("t:{centre}")));
        let mut frontier = vec![centre.clone()];
        for d in 1..=depth {
            let mut next = Vec::new();
            for low in &frontier {
                let me = format!("t:{low}");
                for (kind, target, _) in self.out_cached(low).iter() {
                    match kind.as_str() {
                        "addtopic" | "link" => {
                            let nid = format!("t:{target}");
                            if map.node(&nid, &topic_kind(target), &self.spelled_of(target), d, responses(&nid)) {
                                map.edge(&me, &nid, kind);
                                next.push(target.clone());
                            }
                        }
                        "quest" => {
                            let nid = format!("q:{target}");
                            if map.node(&nid, "quest", &self.spelled_of(target), d, 0) {
                                map.edge(&me, &nid, "quest");
                            }
                        }
                        "sets" => {
                            let nid = format!("v:{target}");
                            if map.node(&nid, "var", target, d, 0) {
                                map.edge(&me, &nid, "sets");
                            }
                        }
                        _ => {}
                    }
                }
                for (other, kind, _) in self.incoming().get(low).map(Vec::as_slice).unwrap_or(&[]) {
                    let nid = format!("t:{other}");
                    if map.node(&nid, &topic_kind(other), &self.spelled_of(other), d, responses(&nid)) {
                        map.edge(&nid, &me, kind);
                        next.push(other.clone());
                    }
                }
            }
            frontier = next;
        }
        // Variables set here and tested elsewhere: the flag links between trees.
        let flags = self.flag_index();
        let vars: Vec<String> = map.nodes.keys().filter(|n| n.starts_with("v:")).cloned().collect();
        for nid in vars {
            for ((_, name), entry) in flags {
                if *name != nid[2..] {
                    continue;
                }
                for r in entry["readers"].as_array().into_iter().flatten() {
                    let tid = format!("t:{}", r["topic"].as_str().unwrap_or("").to_lowercase());
                    if map.nodes.contains_key(&tid) {
                        map.edge(&nid, &tid, "reads");
                    }
                }
            }
        }
        Ok(json!({"centre": format!("t:{centre}"), "nodes": map.nodes.into_values().collect::<Vec<_>>(), "edges": map.edges, "capped": map.capped}))
    }

    /// `(kind, name lower) -> {name, writers, readers}` over every response (once).
    fn flag_index(&self) -> &IndexMap<(String, String), Value> {
        self.flag_index.get_or_init(|| {
            let mut flags: IndexMap<(String, String), Value> = IndexMap::new();
            let mut put = |kind: &str, name: &str, side: &str, line: &Line, how: String| {
                let e = flags.entry((kind.to_string(), name.to_lowercase())).or_insert_with(|| json!({"name": name, "writers": [], "readers": []}));
                let short: String = line.text().chars().take(140).collect();
                e[side].as_array_mut().unwrap().push(json!({"topic": line.topic, "id": line.key, "text": short, "how": how}));
            };
            for lines in self.lines.values() {
                for line in lines {
                    let eff = line.effects();
                    for (name, target) in &eff.sets {
                        let how = format!("set {}{name}", if target.is_empty() { String::new() } else { format!("{target}->") });
                        put(if target.is_empty() { "variable" } else { "local" }, name, "writers", line, how);
                    }
                    for (quest, index) in &eff.journal {
                        put("journal", quest, "writers", line, format!("journal {quest} {index}"));
                    }
                    for (item, verb) in &eff.items {
                        put("item", item, "writers", line, format!("{verb} {item}"));
                    }
                    for (kind, name, clause) in reads_of(&line.rec) {
                        let kind = if kind == "global" || kind == "local" { "variable".to_string() } else { kind };
                        put(&kind, &name, "readers", line, clause);
                    }
                }
            }
            flags
        })
    }

    /// The blackboard: `{flags: [{kind, name, writers, readers}]}` (counts), or one flag's
    /// `{kind, name, writers, readers}` (the lists) when `name` is given (`kind` choosing
    /// among flags of that name).
    pub fn flags(&self, name: &str, kind: &str) -> Result<Value, EditError> {
        let index = self.flag_index();
        if name.is_empty() {
            let mut rows: Vec<(String, String, Value)> = index
                .iter()
                .map(|((k, _), v)| {
                    let n = v["name"].as_str().unwrap_or("").to_string();
                    let row = json!({"kind": k, "name": n, "writers": v["writers"].as_array().map_or(0, Vec::len), "readers": v["readers"].as_array().map_or(0, Vec::len)});
                    (k.clone(), n.to_lowercase(), row)
                })
                .collect();
            rows.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
            return Ok(json!({"flags": rows.into_iter().map(|r| r.2).collect::<Vec<_>>()}));
        }
        let want = name.trim().to_lowercase();
        let Some(((k, _), v)) = index.iter().find(|((k, n), _)| *n == want && (kind.is_empty() || k == kind)) else {
            return err(format!("nothing in the dialogue writes or tests {name}"));
        };
        Ok(json!({"kind": k, "name": v["name"], "writers": v["writers"], "readers": v["readers"]}))
    }

    /// Whether a response could be said by this NPC, and what could not be checked here.
    fn speaker_ok(rec: &Value, npc: &Value, cell: &str, disposition: i64, choice: i64) -> (bool, Vec<String>) {
        for (k, mine) in [("speaker_id", "id"), ("speaker_race", "race"), ("speaker_class", "class"), ("speaker_faction", "faction")] {
            if rec.get(k).is_some_and(truthy) && low(rec.get(k)) != low(npc.get(mine)) {
                return (false, Vec::new());
            }
        }
        let mut unchecked = Vec::new();
        if rec.get("speaker_cell").is_some_and(truthy) {
            if !cell.is_empty() {
                if !cell.trim().to_lowercase().starts_with(&low(rec.get("speaker_cell"))) {
                    return (false, Vec::new());
                }
            } else {
                unchecked.push(format!("NPC is in cell {}", str_of(&rec["speaker_cell"])));
            }
        }
        let data = rec.get("data").filter(|d| d.is_object());
        let sex = data.and_then(|d| d.get("speaker_sex")).and_then(Value::as_str);
        if let Some(s) = sex.filter(|s| *s == "Male" || *s == "Female")
            && npc.get("sex").is_some_and(truthy)
            && Some(s) != npc.get("sex").and_then(Value::as_str)
        {
            return (false, Vec::new());
        }
        if int_of(data.and_then(|d| d.get("disposition"))).is_some_and(|d| d > disposition) {
            return (false, Vec::new());
        }
        if rec.get("player_faction").is_some_and(truthy) {
            unchecked.push(format!("player faction {}", str_of(&rec["player_faction"])));
        }
        if choice_gate(rec).is_some_and(|g| g != choice) {
            return (false, Vec::new());
        }
        for flt in rec.get("filters").and_then(Value::as_array).into_iter().flatten() {
            if !flt.is_object() {
                continue;
            }
            let ftype = flt.get("filter_type").map(str_of);
            let Some(clause) = describe_filter(flt).filter(|c| !c.is_empty()) else { continue };
            if matches!(ftype.as_deref(), None | Some("None")) {
                continue;
            }
            if ftype.as_deref() == Some("Function") && flt.get("function").and_then(Value::as_str) == Some("Choice") {
                continue;
            }
            let ident = low(flt.get("id"));
            let ours = match ftype.as_deref() {
                Some("NotId") => Some(low(npc.get("id"))),
                Some("NotFaction") => Some(low(npc.get("faction"))),
                Some("NotClass") => Some(low(npc.get("class"))),
                Some("NotRace") => Some(low(npc.get("race"))),
                _ => None,
            };
            if let Some(mine) = ours {
                let is_it = mine == ident;
                if clause.starts_with("NPC is not") && is_it {
                    return (false, Vec::new());
                }
                if clause.starts_with("NPC is ") && !clause.starts_with("NPC is not") && !is_it {
                    return (false, Vec::new());
                }
                continue;
            }
            unchecked.push(clause);
        }
        (true, unchecked)
    }

    /// The response the engine would give for a topic (lower): the first whose conditions
    /// hold - `(line, place, what could not be checked)`.
    pub fn pick(&self, topic: &str, npc: &Value, cell: &str, disposition: i64, choice: i64, strict: bool) -> Option<(&Line, usize, Vec<String>)> {
        let lines = self.lines.get(topic)?;
        let mut order: Vec<(usize, &Line)> = lines.iter().enumerate().collect();
        if choice != 0 {
            // A choice just made: the responses answering it first.
            let (mut a, b): (Vec<_>, Vec<_>) = order.into_iter().partition(|(_, l)| choice_gate(&l.rec) == Some(choice));
            a.extend(b);
            order = a;
        }
        for (i, line) in order {
            if text(line.rec.get("flags")).to_uppercase().contains("DELETED") {
                continue;
            }
            let (ok, unchecked) = DialogueIndex::speaker_ok(&line.rec, npc, cell, disposition, choice);
            if ok && !(strict && !unchecked.is_empty()) {
                return Some((line, i, unchecked));
            }
        }
        None
    }

    /// The game's dialogue window, rehearsed for an NPC (`npc_of`): `{topics, line}` - the
    /// topics they answer, and the line said for `topic` (empty: the greeting), or null.
    pub fn play(&self, npc: &Value, topic: &str, choice: i64, cell: &str, disposition: i64, strict: bool) -> Value {
        let mut answers = Vec::new();
        let mut allowed = HashSet::new();
        for name in self.topic_names(TOPIC) {
            let low = name.to_lowercase();
            if self.pick(&low, npc, cell, disposition, 0, strict).is_some() {
                answers.push(name);
                allowed.insert(low);
            }
        }
        let asked = topic.trim().to_lowercase();
        let got = if !asked.is_empty() {
            self.pick(&asked, npc, cell, disposition, choice, strict)
        } else {
            let mut greetings = self.topic_names(GREETING);
            greetings.sort_by(|a, b| natural_cmp(a, b));
            greetings.iter().find_map(|name| self.pick(&name.to_lowercase(), npc, cell, disposition, 0, strict))
        };
        let line = got.map(|(line, pos, unchecked)| {
            let eff = line.effects();
            json!({
                "topic": line.topic,
                "id": line.key,
                "n": pos + 1,
                "segments": self.segments(&line.text(), Some(&allowed)),
                "choices": eff.choices.iter().map(|(l, n)| json!({"label": l, "n": n})).collect::<Vec<_>>(),
                "goodbye": eff.goodbye,
                "conditions": condition_lines(&line.rec),
                "unchecked": unchecked,
                "addtopics": eff.addtopics,
                "journal": eff.journal.iter().map(|(q, i)| json!({"quest": q, "index": i})).collect::<Vec<_>>(),
            })
        });
        json!({"topics": answers, "line": line})
    }
}

/// A map being drawn.
struct Map {
    nodes: IndexMap<String, Value>,
    edges: Vec<Value>,
    seen: HashSet<(String, String, String)>,
    capped: bool,
}

impl Map {
    /// Adds a node unless the map is full; whether it is there.
    fn node(&mut self, nid: &str, kind: &str, label: &str, depth: i64, responses: usize) -> bool {
        if self.nodes.contains_key(nid) {
            return true;
        }
        if self.nodes.len() >= MAP_CAP {
            self.capped = true;
            return false;
        }
        self.nodes.insert(nid.to_string(), json!({"id": nid, "kind": kind, "label": label, "depth": depth, "responses": responses}));
        true
    }

    /// Adds an edge once, between nodes on the map.
    fn edge(&mut self, a: &str, b: &str, kind: &str) {
        if self.nodes.contains_key(a) && self.nodes.contains_key(b) && self.seen.insert((a.to_string(), b.to_string(), kind.to_string())) {
            self.edges.push(json!({"from": a, "to": b, "kind": kind}));
        }
    }
}

impl Editor {
    /// The whole load order's dialogue with the patch's topics, responses and typed
    /// changes to responses laid over (built again when those change).
    pub fn dialogue_index(&self) -> Arc<DialogueIndex> {
        let made: Vec<(String, String, String, Value)> = self
            .queue
            .new_records()
            .filter(|m| m.record_type == "Dialogue" || m.record_type == "DialogueInfo")
            .map(|m| (m.record_type.clone(), m.topic.clone(), m.key.clone(), m.record.clone()))
            .collect();
        let mut edits: IndexMap<String, IndexMap<String, Value>> = IndexMap::new();
        for ((rtype, key), choices) in self.queue.fields().iter() {
            if rtype == "DialogueInfo" {
                for c in choices {
                    if let Choice::Value { path, value } = c {
                        edits.entry(key.clone()).or_default().insert(path.clone(), value.clone());
                    }
                }
            }
        }
        let sig = json!([made.iter().map(|(t, tp, k, r)| json!([t, tp, k, r])).collect::<Vec<_>>(), edits.iter().map(|(k, v)| json!([k, v.iter().map(|(p, x)| json!([p, x])).collect::<Vec<_>>()])).collect::<Vec<_>>()]).to_string();
        let mut cached = self.dial_index.lock().unwrap();
        if let Some((have, index)) = cached.as_ref()
            && *have == sig
        {
            return index.clone();
        }
        let plugins: Vec<(String, Arc<Vec<Value>>)> = self.session.order().iter().map(|n| (n.clone(), self.session.dialogue(n))).collect();
        let index = Arc::new(DialogueIndex::new(&plugins, &made, &edits));
        *cached = Some((sig, index.clone()));
        index
    }

    /// `editDialoguePlay`: the rehearsal for the NPC (or creature) `speaker`, with
    /// `speaker` (the NPC as read) in the answer.
    pub fn dialogue_play(&self, speaker: &str, topic: &str, choice: i64, cell: &str, disposition: i64, strict: bool) -> Result<Value, EditError> {
        let who = speaker.trim();
        if who.is_empty() {
            return err("Choose who to talk to (an NPC's id)");
        }
        let Some(found) = self.find("NPC_", who, None).or_else(|| self.find("CREA", who, None)) else {
            return err(format!("{speaker} is not an NPC or creature of this load order"));
        };
        let npc = npc_of(found.record());
        let mut out = self.dialogue_index().play(&npc, topic, choice, cell, disposition, strict);
        out["speaker"] = npc;
        Ok(out)
    }

    /// `editWhoCanSay`: the NPCs a response's speaker conditions (as the patch leaves the
    /// response) let say it - `{npcs: [{id, name}], count, capped, unchecked}`. Checked:
    /// the speaker's id, race, class, faction (and rank), sex, and the conditions naming
    /// ids; the rest depends on the game being played and is listed as not checked.
    pub fn who_can_say(&self, info_id: &str, limit: usize) -> Result<Value, EditError> {
        let Some(found) = self.find("INFO", info_id, None) else {
            return err("That response is not in this load order");
        };
        let info = self.as_written(&found);
        let want = |k: &str| low(info.get(k));
        let data = info.get("data").filter(|d| d.is_object()).cloned().unwrap_or(json!({}));
        let rank = data.get("speaker_rank").map_or(Some(-1.0), Value::as_f64).map_or(-1, |r| r.trunc() as i64);
        let sex = if data.get("speaker_sex").is_some_and(truthy) { text(data.get("speaker_sex")) } else { "Any".into() };
        let mut nots: HashMap<&str, HashSet<String>> = ["NotId", "NotFaction", "NotClass", "NotRace"].iter().map(|k| (*k, HashSet::new())).collect();
        let mut unchecked = Vec::new();
        if !text(info.get("speaker_cell")).trim().is_empty() {
            unchecked.push(format!("the cell ({})", str_of(&info["speaker_cell"])));
        }
        for f in info.get("filters").and_then(Value::as_array).into_iter().flatten() {
            let kind = text(f.get("filter_type"));
            if let Some(set) = nots.get_mut(kind.as_str()) {
                set.insert(text(f.get("id")).to_lowercase());
            } else if !kind.is_empty() && kind != "None" {
                let label = if f.get("id").is_some_and(truthy) { text(f.get("id")) } else { text(f.get("function")) };
                unchecked.push(format!("{kind} {label}").trim().to_string());
            }
        }
        let mut winners: std::collections::BTreeMap<String, Value> = std::collections::BTreeMap::new();
        for plugin in self.session.order() {
            for (k, r) in self.session.records(plugin, "NPC_").iter() {
                if r.get("type").and_then(Value::as_str) == Some("Npc") {
                    winners.insert(k.clone(), r.clone());
                }
            }
        }
        let mut npcs = Vec::new();
        let mut count = 0;
        for (key, npc) in &winners {
            if !want("speaker_id").is_empty() && *key != want("speaker_id") {
                continue;
            }
            let field = |k: &str| text(npc.get(k)).to_lowercase();
            if !want("speaker_race").is_empty() && field("race") != want("speaker_race") {
                continue;
            }
            if !want("speaker_class").is_empty() && field("class") != want("speaker_class") {
                continue;
            }
            if !want("speaker_faction").is_empty() {
                let npc_rank = npc.get("data").and_then(|d| d.get("rank")).and_then(Value::as_f64).map_or(0, |r| r.trunc() as i64);
                if field("faction") != want("speaker_faction") || (rank >= 0 && npc_rank < rank) {
                    continue;
                }
            }
            let flags: HashSet<String> = match npc.get("npc_flags") {
                Some(Value::Array(a)) => a.iter().map(|v| str_of(v).trim().to_uppercase()).collect(),
                v => text(v).split('|').map(|p| p.trim().to_uppercase()).filter(|p| !p.is_empty()).collect(),
            };
            let female = flags.contains("FEMALE");
            if (sex == "Male" && female) || (sex == "Female" && !female) {
                continue;
            }
            if nots["NotId"].contains(key) || nots["NotFaction"].contains(&field("faction")) {
                continue;
            }
            if nots["NotClass"].contains(&field("class")) || nots["NotRace"].contains(&field("race")) {
                continue;
            }
            count += 1;
            if npcs.len() < limit {
                let id = if npc.get("id").is_some_and(truthy) { text(npc.get("id")) } else { key.clone() };
                npcs.push(json!({"id": id, "name": text(npc.get("name"))}));
            }
        }
        Ok(json!({"npcs": npcs, "count": count, "capped": count > limit, "unchecked": unchecked}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripts_read_for_what_they_do() {
        let e = script_effects("Choice \"Yes\" 1 \"No\" 2 ; ask\nAddTopic \"little secret\"\nJournal MS_Quest 10\nset foo to 1\nset guard->bar to 2\nplayer->AddItem gold_001 5\nGoodbye\nif ( x == 1 )\nModPCFacRep 5 \"Hlaalu\"\nendif");
        assert_eq!(e.choices, vec![("Yes".into(), 1), ("No".into(), 2)]);
        assert_eq!(e.addtopics, ["little secret"]);
        assert_eq!(e.journal, vec![("MS_Quest".into(), 10)]);
        assert_eq!(e.sets, vec![("foo".into(), "".into()), ("bar".into(), "guard".into())]);
        assert_eq!(e.items, vec![("gold_001".into(), "add".into())]);
        assert!(e.goodbye);
        assert_eq!(e.other, ["ModPCFacRep"]);
        assert_eq!(prettify_function("PcSex"), "PC Sex");
        assert_eq!(natural_cmp("Greeting 9", "Greeting 10"), Ordering::Less);
    }
}
