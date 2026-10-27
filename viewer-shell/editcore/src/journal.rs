//! Every quest's journal stages across the load order, and what sets each one (port step 4.4,
//! slice 6): `wraithguard/patch/journal.py` and `journal_scripts.py`, whole, for the Journal
//! view.
//!
//! A `Dialogue` of kind `Journal` is a quest and each `DialogueInfo` under it a stage. A stage's
//! identity is its INFO id (a later plugin reusing it overrides it, last wins; a deletion removes
//! it); its index is the INFO's disposition field, and the stages are shown sorted by it.
//!
//! What sets a stage: every `Journal <quest>, N` / `SetJournalIndex` call (the quest quoted or
//! not) in the surviving scripts - a stage's own result script, a standalone `Script` (its
//! compiled bytecode when the source was stripped), or an ordinary dialogue response's result
//! script, where most quest progression happens. Each call carries its enclosing
//! `if`/`elseif`/`while` conditions as written, outermost first, never evaluated. The other
//! statements of the same scripts (items given, disposition changed, scripts started) come
//! with them as effects, one statement a line, function and target split from the arguments.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde_json::{Value, json};

// ---------------------------------------------------------------- the script lexer

/// `script_tokens`: mwscript as `(kind, text)` spans - `keyword`, `string`, `number`,
/// `operator`, `comment`, `text` - that put back together give the input (MWDE's
/// `lexscan_list`).
pub fn script_tokens(text: &str) -> Vec<(&'static str, String)> {
    const KEYWORDS: [&str; 17] =
        ["elseif", "ifx", "if", "else", "endif", "while", "endwhile", "return", "begin", "end", "startscript", "stopscript", "float", "short", "long", "set", "to"];
    const OPERATORS: [&str; 11] = ["+", "*", "-", "/", "==", "<=", "<", ">=", ">", "!=", "->"];
    let chars: Vec<char> = text.chars().collect();
    let word = |c: char| c.is_alphanumeric() || c == '_';
    let mut out = Vec::new();
    let mut pos = 0usize;
    let take = |a: usize, b: usize| chars[a..b].iter().collect::<String>();
    while pos < chars.len() {
        let c = chars[pos];
        // string: "([^\\"]|\\.)*"
        if c == '"' {
            let mut i = pos + 1;
            let mut closed = None;
            while i < chars.len() {
                match chars[i] {
                    '\\' if i + 1 < chars.len() => i += 2,
                    '"' => {
                        closed = Some(i + 1);
                        break;
                    }
                    _ => i += 1,
                }
            }
            if let Some(end) = closed {
                out.push(("string", take(pos, end)));
                pos = end;
                continue;
            }
        }
        // number: [+-]?[0-9]+(\.[0-9]+)?
        {
            let mut i = pos;
            if matches!(chars[i], '+' | '-') {
                i += 1;
            }
            let d0 = i;
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
            }
            if i > d0 {
                if i + 1 < chars.len() && chars[i] == '.' && chars[i + 1].is_ascii_digit() {
                    i += 1;
                    while i < chars.len() && chars[i].is_ascii_digit() {
                        i += 1;
                    }
                }
                out.push(("number", take(pos, i)));
                pos = i;
                continue;
            }
        }
        // operator, in the pattern's order ("-" before "->", so an arrow is two tokens)
        if let Some(op) = OPERATORS.iter().find(|op| {
            let oc: Vec<char> = op.chars().collect();
            chars[pos..].starts_with(&oc)
        }) {
            out.push(("operator", (*op).to_string()));
            pos += op.chars().count();
            continue;
        }
        // keyword: (?i)\b(...)\b
        let boundary_before = pos == 0 || !word(chars[pos - 1]);
        if boundary_before && word(c) {
            let hit = KEYWORDS.iter().find(|k| {
                let n = k.chars().count();
                pos + n <= chars.len()
                    && take(pos, pos + n).to_lowercase() == **k
                    && (pos + n == chars.len() || !word(chars[pos + n]))
            });
            if let Some(k) = hit {
                let n = k.chars().count();
                out.push(("keyword", take(pos, pos + n)));
                pos += n;
                continue;
            }
        }
        // comment: ;.*
        if c == ';' {
            let end = chars[pos..].iter().position(|&x| x == '\n').map_or(chars.len(), |i| pos + i);
            out.push(("comment", take(pos, end)));
            pos = end;
            continue;
        }
        // whitespace
        if c.is_whitespace() {
            let end = chars[pos..].iter().position(|x| !x.is_whitespace()).map_or(chars.len(), |i| pos + i);
            out.push(("text", take(pos, end)));
            pos = end;
            continue;
        }
        // identifier: [_a-zA-Z][_a-zA-Z0-9]*
        if c == '_' || c.is_ascii_alphabetic() {
            let end = chars[pos..].iter().position(|x| !(*x == '_' || x.is_ascii_alphanumeric())).map_or(chars.len(), |i| pos + i);
            out.push(("text", take(pos, end)));
            pos = end;
            continue;
        }
        out.push(("text", c.to_string()));
        pos += 1;
    }
    out
}

fn is_space(s: &str) -> bool {
    !s.is_empty() && s.chars().all(char::is_whitespace)
}

fn is_journal_fn(w: &str) -> bool {
    matches!(w.to_lowercase().as_str(), "journal" | "setjournalindex")
}

fn canonical(name: &str) -> &'static str {
    if name.eq_ignore_ascii_case("journal") { "Journal" } else { "SetJournalIndex" }
}

/// `int(float(text))` for a `number` token.
fn number(text: &str) -> Option<i64> {
    text.parse::<f64>().ok().filter(|f| f.is_finite()).map(|f| f.trunc() as i64)
}

/// `(token index, function, quest, index)` for every `Journal <id>, N` in a token stream.
fn scan_calls(tokens: &[(&str, String)]) -> Vec<(usize, &'static str, String, i64)> {
    let mut calls = Vec::new();
    let n = tokens.len();
    let mut i = 0;
    while i < n {
        let (kind, w) = (&tokens[i].0, &tokens[i].1);
        if *kind != "text" || !is_journal_fn(w) {
            i += 1;
            continue;
        }
        let start = i;
        let mut j = i + 1;
        while j < n && tokens[j].0 == "text" && is_space(&tokens[j].1) {
            j += 1;
        }
        if j >= n {
            i += 1;
            continue;
        }
        let quest = match (tokens[j].0, &tokens[j].1) {
            ("string", s) => s.trim_matches('"').to_string(),
            ("text", s) if s.chars().next().is_some_and(|c| c.is_alphabetic() || c == '_') => s.clone(),
            _ => {
                i += 1;
                continue;
            }
        };
        j += 1;
        while j < n && tokens[j].0 == "text" && (is_space(&tokens[j].1) || tokens[j].1 == ",") {
            j += 1;
        }
        if j >= n || tokens[j].0 != "number" {
            i += 1;
            continue;
        }
        let Some(index) = number(&tokens[j].1) else {
            i = j;
            continue;
        };
        calls.push((start, canonical(w), quest, index));
        i = j + 1;
    }
    calls
}

/// The `(` at `start`'s matching `)`, one past it (the end when unbalanced).
fn matching_paren(tokens: &[(&str, String)], start: usize) -> usize {
    let mut depth = 0i64;
    for (i, t) in tokens.iter().enumerate().skip(start) {
        if t.1 == "(" {
            depth += 1;
        } else if t.1 == ")" {
            depth -= 1;
            if depth == 0 {
                return i + 1;
            }
        }
    }
    tokens.len()
}

/// The enclosing conditions before each token, outermost first.
fn condition_stack(tokens: &[(&str, String)]) -> Vec<Vec<String>> {
    let n = tokens.len();
    let mut at: Vec<Vec<String>> = vec![Vec::new(); n];
    let mut stack: Vec<String> = Vec::new();
    let mut i = 0;
    while i < n {
        at[i] = stack.clone();
        let (kind, w) = (tokens[i].0, tokens[i].1.to_lowercase());
        let opener = matches!(w.as_str(), "if" | "elseif" | "while");
        let closer = matches!(w.as_str(), "endif" | "endwhile");
        if kind != "keyword" || !(opener || closer || w == "else") {
            i += 1;
            continue;
        }
        if opener {
            let mut j = i + 1;
            while j < n && tokens[j].0 == "text" && is_space(&tokens[j].1) {
                j += 1;
            }
            if j >= n || tokens[j].1 != "(" {
                i += 1;
                continue;
            }
            let k = matching_paren(tokens, j);
            for slot in at.iter_mut().take(k).skip(i + 1) {
                *slot = stack.clone();
            }
            let condition: String = tokens[j..k].iter().map(|t| t.1.as_str()).collect::<String>().trim().to_string();
            let label = format!("{w} {condition}");
            if w == "elseif" && !stack.is_empty() {
                *stack.last_mut().expect("not empty") = label;
            } else {
                stack.push(label);
            }
            i = k;
            continue;
        }
        if w == "else" && !stack.is_empty() {
            *stack.last_mut().expect("not empty") = "else".into();
        } else if closer {
            stack.pop();
        }
        i += 1;
    }
    at
}

/// `calls_in_text_with_context`: `(function, quest, index, conditions)` in source order.
pub fn calls_in_text(text: &str) -> Vec<(&'static str, String, i64, Vec<String>)> {
    let tokens = script_tokens(text);
    let at = condition_stack(&tokens);
    scan_calls(&tokens).into_iter().map(|(p, f, q, i)| (f, q, i, at[p].clone())).collect()
}

/// `statements_in_text_with_context`: every other statement, `(function, target, arguments,
/// raw line, conditions)`.
pub fn statements_in_text(text: &str) -> Vec<(String, String, String, String, Vec<String>)> {
    const SKIP: [&str; 11] = ["if", "elseif", "else", "endif", "while", "endwhile", "float", "short", "long", "begin", "end"];
    let tokens = script_tokens(text);
    let at = condition_stack(&tokens);
    // Lines: a whitespace token holding a newline ends one.
    let mut lines: Vec<(usize, Vec<(&str, String)>)> = Vec::new();
    let mut current: Vec<(&str, String)> = Vec::new();
    let mut start = 0;
    for (index, (kind, t)) in tokens.iter().enumerate() {
        if *kind == "text" && is_space(t) && t.contains('\n') {
            if !current.is_empty() {
                lines.push((start, std::mem::take(&mut current)));
            }
            start = index + 1;
            continue;
        }
        current.push((kind, t.clone()));
    }
    if !current.is_empty() {
        lines.push((start, current));
    }
    let mut out = Vec::new();
    for (start, lt) in lines {
        let n = lt.len();
        let mut i = 0;
        while i < n && lt[i].0 == "text" && is_space(&lt[i].1) {
            i += 1;
        }
        if i >= n || lt[i].0 == "comment" {
            continue;
        }
        if lt[i].0 == "keyword" && SKIP.contains(&lt[i].1.to_lowercase().as_str()) {
            continue;
        }
        let joined = |s: &[(&str, String)]| s.iter().map(|t| t.1.as_str()).collect::<String>().trim().to_string();
        let raw = joined(&lt);
        if raw.is_empty() {
            continue;
        }
        let arrow = (i..n.saturating_sub(1)).find(|&k| lt[k].1 == "-" && lt[k + 1].1 == ">");
        let (function, target, arguments) = if let Some(a) = arrow {
            let target = joined(&lt[i..a]);
            let rest = &lt[a + 2..];
            let mut j = 0;
            while j < rest.len() && rest[j].0 == "text" && is_space(&rest[j].1) {
                j += 1;
            }
            if j >= rest.len() {
                continue;
            }
            (rest[j].1.clone(), target, joined(&rest[j + 1..]))
        } else {
            (lt[i].1.clone(), String::new(), joined(&lt[i + 1..]))
        };
        if is_journal_fn(&function) {
            continue;
        }
        let conditions = at.get(start).cloned().unwrap_or_default();
        out.push((function, target, arguments, raw, conditions));
    }
    out
}

// ---------------------------------------------------------------- stages

/// One journal stage as the engine shows it (`Resolved`).
#[derive(Clone, Debug, PartialEq)]
pub struct Stage {
    pub info_id: String,
    pub index: i64,
    pub text: String,
    pub quest_state: String,
    pub script_text: String,
    /// Every plugin that defined it, in load order; the last one wins.
    pub plugins: Vec<String>,
    /// The winning definition, whole.
    pub raw: Value,
}

impl Stage {
    pub fn finished(&self) -> bool {
        self.quest_state == "Finished"
    }
}

fn s(v: &Value, k: &str) -> String {
    match v.get(k) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Null) | None => String::new(),
        Some(other) => other.to_string(),
    }
}

fn topic_kind(v: &Value) -> String {
    v.get("dialogue_type")
        .or_else(|| v.get("data").and_then(|d| d.get("dialogue_type")))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn deleted(v: &Value) -> bool {
    match v.get("flags") {
        Some(Value::String(f)) => f.to_uppercase().contains("DELETED"),
        Some(Value::Null) | None => false,
        Some(other) => other.to_string().to_uppercase().contains("DELETED"),
    }
}

/// `resolve_journals`: each quest's surviving stages by journal index, the sources being each
/// plugin's records in file order, read in `order`. A quest left with no stage is not listed.
pub fn resolve(sources: &HashMap<String, Vec<Value>>, order: &[String]) -> BTreeMap<String, Vec<Stage>> {
    // (quest, info id) in the order first defined; deleted ones dropped.
    let mut quests: BTreeMap<String, (HashMap<String, Stage>, Vec<String>)> = BTreeMap::new();
    for plugin in order {
        let Some(records) = sources.get(plugin) else { continue };
        let mut quest = String::new();
        let mut journal = false;
        for r in records {
            match r.get("type").and_then(Value::as_str) {
                Some("Dialogue") => {
                    quest = s(r, "id");
                    journal = topic_kind(r) == "Journal";
                }
                Some("DialogueInfo") if journal && !quest.is_empty() => {
                    let id = s(r, "id");
                    let (at, ids) = quests.entry(quest.clone()).or_default();
                    if deleted(r) {
                        if at.remove(&id).is_some() {
                            ids.retain(|x| *x != id);
                        }
                        continue;
                    }
                    let index = r.get("data").and_then(|d| d.get("disposition")).and_then(Value::as_i64).unwrap_or(0);
                    let fresh = Stage {
                        info_id: id.clone(),
                        index,
                        text: s(r, "text"),
                        quest_state: s(r, "quest_state"),
                        script_text: s(r, "script_text"),
                        plugins: vec![plugin.clone()],
                        raw: r.clone(),
                    };
                    match at.get_mut(&id) {
                        Some(e) => {
                            let mut plugins = std::mem::take(&mut e.plugins);
                            plugins.push(plugin.clone());
                            *e = Stage { plugins, ..fresh };
                        }
                        None => {
                            at.insert(id.clone(), fresh);
                            ids.push(id);
                        }
                    }
                }
                _ => {}
            }
        }
    }
    quests
        .into_iter()
        .filter_map(|(q, (mut at, ids))| {
            let mut stages: Vec<Stage> = ids.into_iter().filter_map(|id| at.remove(&id)).collect();
            stages.sort_by_key(|s| s.index);
            (!stages.is_empty()).then_some((q, stages))
        })
        .collect()
}

/// One place a script sets a journal stage.
#[derive(Clone, Debug, PartialEq)]
pub struct Call {
    pub function: &'static str,
    pub quest: String,
    pub index: i64,
    pub owner_type: &'static str,
    pub owner_id: String,
    pub plugin: String,
    pub from_bytecode: bool,
    pub conditions: Vec<String>,
}

/// One other statement a quest's scripts run.
#[derive(Clone, Debug, PartialEq)]
pub struct Effect {
    pub function: String,
    pub target: String,
    pub arguments: String,
    pub raw: String,
    pub conditions: Vec<String>,
    pub owner_type: &'static str,
    pub owner_id: String,
    pub plugin: String,
}

/// The surviving record of each id of `kind`, by id, with its plugin.
fn winners<'a>(sources: &'a HashMap<String, Vec<Value>>, order: &[String], kind: &str) -> BTreeMap<String, (&'a Value, String)> {
    let mut out: BTreeMap<String, (&Value, String)> = BTreeMap::new();
    for plugin in order {
        let Some(records) = sources.get(plugin) else { continue };
        for r in records {
            if r.get("type").and_then(Value::as_str) != Some(kind) {
                continue;
            }
            let id = s(r, "id");
            if id.is_empty() {
                continue;
            }
            if deleted(r) {
                out.remove(&id);
            } else {
                out.insert(id, (r, plugin.clone()));
            }
        }
    }
    out
}

/// `_calls_in_bytecode`: the calls a stripped script's compiled form makes.
fn calls_in_bytecode(bytecode: Option<&Value>, source: &str) -> Vec<(&'static str, String, i64)> {
    use crate::mwscript::listing::{Item, Operand, decode_field, disassemble};
    let Some(v) = bytecode.filter(|v| !v.is_null()) else { return Vec::new() };
    let Ok(data) = decode_field(v) else { return Vec::new() };
    if data.is_empty() {
        return Vec::new();
    }
    let listing = disassemble(&data, true, (!source.is_empty()).then_some(source));
    listing
        .items
        .iter()
        .filter_map(|it| match it {
            Item::Instruction { name, operands, .. } if is_journal_fn(name) && operands.len() == 2 => match (&operands[0], &operands[1]) {
                (Operand::Text(q), Operand::Int(i)) => Some((canonical(name), q.clone(), *i)),
                (Operand::Text(q), Operand::Float(f)) => Some((canonical(name), q.clone(), f.trunc() as i64)),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

/// Every call and effect of the load order's scripts (`calls_from_*`, `effects_from_*`): the
/// stages' own scripts, the standalone scripts, the other dialogue responses.
pub fn scan(sources: &HashMap<String, Vec<Value>>, order: &[String], resolved: &BTreeMap<String, Vec<Stage>>) -> (Vec<Call>, Vec<Effect>) {
    let mut calls = Vec::new();
    let mut effects = Vec::new();
    let push = |calls: &mut Vec<Call>, effects: &mut Vec<Effect>, text: &str, owner_type: &'static str, owner: &str, plugin: &str| {
        for (f, q, i, c) in calls_in_text(text) {
            calls.push(Call { function: f, quest: q, index: i, owner_type, owner_id: owner.into(), plugin: plugin.into(), from_bytecode: false, conditions: c });
        }
        for (function, target, arguments, raw, conditions) in statements_in_text(text) {
            effects.push(Effect { function, target, arguments, raw, conditions, owner_type, owner_id: owner.into(), plugin: plugin.into() });
        }
    };
    // The stages' own scripts.
    for stages in resolved.values() {
        for st in stages {
            if !st.script_text.is_empty() {
                push(&mut calls, &mut effects, &st.script_text, "DialogueInfo", &st.info_id, st.plugins.last().map_or("", String::as_str));
            }
        }
    }
    // The standalone scripts, their compiled form when the source is gone.
    for (id, (r, plugin)) in winners(sources, order, "Script") {
        let text = s(r, "text");
        if text.trim().is_empty() {
            for (f, q, i) in calls_in_bytecode(r.get("bytecode"), &text) {
                calls.push(Call { function: f, quest: q, index: i, owner_type: "Script", owner_id: id.clone(), plugin: plugin.clone(), from_bytecode: true, conditions: Vec::new() });
            }
        } else {
            push(&mut calls, &mut effects, &text, "Script", &id, &plugin);
        }
    }
    // The other dialogue responses (a journal stage's own are already read).
    let journal_ids: HashSet<&str> = resolved.values().flatten().map(|s| s.info_id.as_str()).collect();
    for (id, (r, plugin)) in winners(sources, order, "DialogueInfo") {
        if journal_ids.contains(id.as_str()) {
            continue;
        }
        let text = s(r, "script_text");
        if !text.trim().is_empty() {
            push(&mut calls, &mut effects, &text, "DialogueInfo", &id, &plugin);
        }
    }
    (calls, effects)
}

fn call_json(c: &Call, target: Option<(&str, &Stage)>) -> Value {
    json!({"function": c.function, "quest": c.quest, "index": c.index, "ownerType": c.owner_type, "ownerId": c.owner_id,
           "plugin": c.plugin, "fromBytecode": c.from_bytecode, "conditions": c.conditions,
           "target": target.map(|(q, s)| json!({"quest": q, "infoId": s.info_id, "text": s.text}))})
}

fn effect_json(e: &Effect) -> Value {
    json!({"function": e.function, "target": e.target, "arguments": e.arguments, "raw": e.raw, "conditions": e.conditions})
}

/// The Journal view (`journalview.py`): every quest, its stages by index - text, state, the
/// plugins defining it, who must be speaking and what must hold (`condition_lines`), its own
/// script's effects - each with the calls that set it (where, under what conditions, what else
/// that place does). Answers `{quests: [{id, stages: [...]}], counts: {quests, stages, calls,
/// unattached}}`.
pub fn view(sources: &HashMap<String, Vec<Value>>, order: &[String]) -> Value {
    let resolved = resolve(sources, order);
    let (calls, effects) = scan(sources, order, &resolved);
    let mut by_index: HashMap<(String, i64), (&str, &Stage)> = HashMap::new();
    for (q, stages) in &resolved {
        for st in stages {
            by_index.insert((q.to_lowercase(), st.index), (q.as_str(), st));
        }
    }
    let target = |c: &Call| by_index.get(&(c.quest.to_lowercase(), c.index)).copied();
    let mut effects_of: HashMap<(&str, &str), Vec<&Effect>> = HashMap::new();
    for e in &effects {
        effects_of.entry((e.owner_type, e.owner_id.as_str())).or_default().push(e);
    }
    let mut incoming: HashMap<(&str, &str), Vec<&Call>> = HashMap::new();
    let mut outgoing: HashMap<&str, Vec<&Call>> = HashMap::new();
    let mut unattached = 0usize;
    for c in &calls {
        match target(c) {
            Some((q, st)) => incoming.entry((q, st.info_id.as_str())).or_default().push(c),
            None => unattached += 1,
        }
        if c.owner_type == "DialogueInfo" {
            outgoing.entry(c.owner_id.as_str()).or_default().push(c);
        }
    }
    // The owners' records, for a trigger's own dialogue conditions.
    let infos = winners(sources, order, "DialogueInfo");
    let quests: Vec<Value> = resolved
        .iter()
        .map(|(q, stages)| {
            let stages: Vec<Value> = stages
                .iter()
                .map(|st| {
                    let set_by: Vec<Value> = incoming
                        .get(&(q.as_str(), st.info_id.as_str()))
                        .into_iter()
                        .flatten()
                        .map(|c| {
                            let mut v = call_json(c, target(c));
                            let owner = (c.owner_type == "DialogueInfo").then(|| infos.get(&c.owner_id)).flatten();
                            v["speaker"] = json!(owner.map(|(r, _)| crate::dialogue_graph::condition_lines(r)).unwrap_or_default());
                            v["effects"] = json!(effects_of.get(&(c.owner_type, c.owner_id.as_str())).into_iter().flatten().map(|e| effect_json(e)).collect::<Vec<_>>());
                            v
                        })
                        .collect();
                    let also: Vec<Value> = outgoing.get(st.info_id.as_str()).into_iter().flatten().map(|c| call_json(c, target(c))).collect();
                    json!({
                        "infoId": st.info_id, "index": st.index, "text": st.text, "state": st.quest_state, "finished": st.finished(),
                        "plugins": st.plugins, "script": st.script_text, "conditions": crate::dialogue_graph::condition_lines(&st.raw),
                        "effects": effects_of.get(&("DialogueInfo", st.info_id.as_str())).into_iter().flatten().map(|e| effect_json(e)).collect::<Vec<_>>(),
                        "setBy": set_by, "alsoSets": also,
                    })
                })
                .collect();
            json!({"id": q, "stages": stages})
        })
        .collect();
    json!({
        "quests": quests,
        "counts": {"quests": resolved.len(), "stages": resolved.values().map(Vec::len).sum::<usize>(), "calls": calls.len(), "unattached": unattached},
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_put_back_together_give_the_script() {
        let src = "if ( GetJournalIndex \"Q\" >= 10 ) ; note\n  player->AddItem gold_001, 100\nendif";
        let t = script_tokens(src);
        assert_eq!(t.iter().map(|t| t.1.as_str()).collect::<String>(), src);
        assert!(t.contains(&("keyword", "if".into())));
        assert!(t.contains(&("comment", "; note".into())));
        // "->" is two tokens, as the Python lexer gives it.
        assert!(t.windows(2).any(|w| w[0].1 == "-" && w[1].1 == ">"));
    }

    #[test]
    fn calls_carry_their_conditions_and_effects_their_lines() {
        let src = "if ( GetDisposition >= 50 )\n  Journal \"MS_Quest\", 10\n  player->AddItem gold_001 100\nelseif ( x == 1 )\n  SetJournalIndex MS_Quest 20\nelse\n  journal MS_Other 5\nendif\nJournal 3 4\n";
        let calls = calls_in_text(src);
        assert_eq!(
            calls,
            vec![
                ("Journal", "MS_Quest".into(), 10, vec!["if ( GetDisposition >= 50 )".to_string()]),
                ("SetJournalIndex", "MS_Quest".into(), 20, vec!["elseif ( x == 1 )".to_string()]),
                ("Journal", "MS_Other".into(), 5, vec!["else".to_string()]),
            ]
        );
        let st = statements_in_text(src);
        assert_eq!(st.len(), 1);
        assert_eq!((st[0].0.as_str(), st[0].1.as_str(), st[0].2.as_str()), ("AddItem", "player", "gold_001 100"));
        assert_eq!(st[0].4, ["if ( GetDisposition >= 50 )"]);
    }

    #[test]
    fn stages_override_by_id_and_sort_by_index() {
        let dial = json!({"type": "Dialogue", "id": "Q", "dialogue_type": "Journal"});
        let info = |id: &str, i: i64, text: &str, flags: &str| json!({"type": "DialogueInfo", "id": id, "data": {"disposition": i}, "text": text, "flags": flags, "script_text": ""});
        let mut sources = HashMap::new();
        sources.insert("A.esp".to_string(), vec![dial.clone(), info("1", 20, "b", ""), info("2", 10, "a", ""), info("3", 30, "c", "")]);
        sources.insert("B.esp".to_string(), vec![dial, info("2", 10, "a2", ""), info("3", 30, "", "DELETED")]);
        let r = resolve(&sources, &["A.esp".into(), "B.esp".into()]);
        let q = &r["Q"];
        assert_eq!(q.iter().map(|s| (s.index, s.text.as_str())).collect::<Vec<_>>(), [(10, "a2"), (20, "b")]);
        assert_eq!(q[0].plugins, ["A.esp", "B.esp"]);
        let v = view(&sources, &["A.esp".into(), "B.esp".into()]);
        assert_eq!(v["counts"]["stages"], 2);
    }
}
