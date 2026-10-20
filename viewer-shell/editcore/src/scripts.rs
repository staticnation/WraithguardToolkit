//! Scripts in the Editor: a script's source saved with its bytecode
//! (`EditorSession.set_script_text`, port step 4.1, slice 5b).
//!
//! What compiles is handed in, as [`Compile`]: editcore's own compiler
//! ([`crate::mwscript::ScriptCompiler`], [`Editor::script_compiler`]), or a test's stand-in.
//!
//! And the rest of Script Edit (slice 5d): the window's view (`script_view`: the checks,
//! the compiled listing, the compile), a dialogue result checked (`result_check`), what
//! the editor completes (`script_words`), the scripts naming a word (`script_refs`), a
//! script's flowchart, a leveled list rolled (`leveled_roll`) and a Lua script's charts
//! (`lua_chart`, luacore's).

use indexmap::IndexMap;
use serde_json::{json, Value};

use crate::edit::{crlf, err, value_at, EditError, Editor};
use crate::session::Found;

/// The script record's fields a compile writes, saved beside the text.
pub const COMPILED: [&str; 7] = [
    "header.num_shorts",
    "header.num_longs",
    "header.num_floats",
    "header.bytecode_length",
    "header.variables_length",
    "variables",
    "bytecode",
];

/// What a compiler says of a script.
pub struct Compiled {
    /// It compiles.
    pub ok: bool,
    /// Its errors and warnings: `{line, level, message}` each.
    pub messages: Vec<Value>,
}

/// A script compiler over the load order (and the patch's own records).
pub trait Compile {
    /// Compiles the source.
    fn compile(&self, text: &str) -> Compiled;
    /// The script record the source compiles to, in the form the patch stores (header,
    /// locals, bytecode); `None` when it cannot be made.
    fn compile_record(&self, key: &str, text: &str) -> Option<Value>;
}

/// No compiler: nothing compiles, and only the text is saved (OpenMW compiles the text
/// itself; Morrowind.exe would run the bytecode the script had).
pub struct NoCompiler;

impl Compile for NoCompiler {
    fn compile(&self, _text: &str) -> Compiled {
        Compiled { ok: false, messages: Vec::new() }
    }
    fn compile_record(&self, _key: &str, _text: &str) -> Option<Value> {
        None
    }
}

impl Editor {
    /// Saves a script's source, and its bytecode with it when it compiles, as one change:
    /// `{compiled, messages}`. The text is queued as any field is; when it compiles, the
    /// header, locals and bytecode are queued beside it, so Morrowind.exe runs the new
    /// script too.
    pub fn set_script_text(&mut self, found: &Found, text: &str, compiler: &dyn Compile) -> Result<Value, EditError> {
        if found.record_type != "Script" {
            return err(format!("{} is not a script", found.key));
        }
        let text = crlf(text);
        self.batch(|ed| {
            ed.set_field(found, "text", &json!(text))?;
            let result = compiler.compile(&text);
            let built = if result.ok { compiler.compile_record(&found.key, &text) } else { None };
            let Some(built) = built else {
                return Ok(json!({"compiled": false, "messages": result.messages}));
            };
            for path in COMPILED {
                if let Some(value) = value_at(&built, path)
                    && value_at(found.record(), path).is_some()
                {
                    ed.set_field(found, path, value)?;
                }
            }
            Ok(json!({"compiled": true, "messages": result.messages}))
        })
    }
}

// -- Script Edit and the rest (slice 5d) -------------------------------------------------

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::mwscript::flowchart::splitlines;
use crate::mwscript::{Extra, ScriptCompiler, find_custom_functions, messages_json};
use crate::queue::Choice;
use crate::search::Finder;
use crate::session::truthy;

/// Most lists rolled inside a leveled list.
pub const MAX_DEPTH: usize = 8;

/// The lines of a script that end in a bare LF, not CRLF (from 1).
pub fn bare_lf_lines(text: &str) -> Vec<usize> {
    let parts: Vec<&str> = text.split('\n').collect();
    parts[..parts.len() - 1].iter().enumerate().filter(|(_, l)| !l.ends_with('\r')).map(|(i, _)| i + 1).collect()
}

/// The warning for source a plugin stores with bare LFs, at its first such line.
pub fn bare_lf_finding(text: &str) -> Option<Value> {
    let bare = bare_lf_lines(text);
    let first = *bare.first()?;
    Some(json!({
        "line": first,
        "level": "warning",
        "message": format!("{} line(s) end in LF, not CRLF: a line ending in a comment can swallow the next one in the game. An edit saved here stores CRLF.", bare.len()),
    }))
}

/// Python's `str(v or "")`.
fn text_of(v: Option<&Value>) -> String {
    match v {
        Some(v) if truthy(v) => crate::pyval::str_of(v),
        _ => String::new(),
    }
}

/// Python's `round(x, 6)`.
fn round6(x: f64) -> f64 {
    format!("{x:.6}").parse().unwrap_or(x)
}

/// A flags field as the reader gives it (`A | B`, or a list): its names, upper case.
fn flag_names(v: Option<&Value>) -> std::collections::HashSet<String> {
    match v {
        Some(Value::Array(a)) => a.iter().map(|x| crate::pyval::str_of(x).trim().to_uppercase()).collect(),
        other => text_of(other).split('|').map(|p| p.trim().to_uppercase()).filter(|p| !p.is_empty()).collect(),
    }
}

/// One leveled list rolled once at a level: each entry's chance, `""` the chance of
/// nothing (`workflow.roll_entries`). `chance_none` percent gives nothing; otherwise the
/// entries at or below the level are the candidates (all of them with "calculate from
/// all levels <= PC's level", else only those at the highest of their levels), one taken,
/// each as likely as the others.
pub fn roll_entries(record_type: &str, record: &Value, level: i64) -> IndexMap<String, f64> {
    let items = record_type == "LeveledItem";
    let field = if items { "items" } else { "creatures" };
    let flags = flag_names(record.get(if items { "leveled_item_flags" } else { "leveled_creature_flags" }));
    let entries: Vec<(String, i64)> = record
        .get(field)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|e| {
            let e = e.as_array().filter(|e| e.len() >= 2)?;
            let id = crate::pyval::str_of(&e[0]);
            if id.trim().is_empty() {
                return None;
            }
            Some((id, e[1].as_f64()?.trunc() as i64))
        })
        .collect();
    let chance = record.get("chance_none").filter(|v| truthy(v)).and_then(Value::as_f64).map_or(0, |c| c.trunc() as i64);
    let none = chance.clamp(0, 100) as f64 / 100.0;
    let mut eligible: Vec<(String, i64)> = entries.into_iter().filter(|(_, lv)| *lv <= level).collect();
    let mut out = IndexMap::new();
    if eligible.is_empty() {
        out.insert(String::new(), 1.0);
        return out;
    }
    if !flags.contains("CALCULATE_FROM_ALL_LEVELS") {
        let top = eligible.iter().map(|(_, lv)| *lv).max().unwrap_or(0);
        eligible.retain(|(_, lv)| *lv == top);
    }
    if none != 0.0 {
        out.insert(String::new(), none);
    }
    let each = (1.0 - none) / eligible.len() as f64;
    for (id, _) in eligible {
        *out.entry(id).or_insert(0.0) += each;
    }
    out
}

impl Editor {
    /// Every global variable the load order defines or the patch makes (lower case,
    /// sorted).
    pub fn script_globals(&self) -> Vec<String> {
        let mut names: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for plugin in self.session.order() {
            names.extend(self.session.records(plugin, "GLOB").keys().cloned());
        }
        names.extend(self.queue.new_records().filter(|m| m.record_type == "GlobalVariable").map(|m| m.key.to_lowercase()));
        names.into_iter().collect()
    }

    /// What the patch makes that a script can name (`pool_script_records`).
    pub fn pool_script_records(&self) -> Vec<Extra> {
        const KINDS: [&str; 5] = ["Topic", "Voice", "Greeting", "Persuasion", "Journal"];
        self.queue
            .new_records()
            .map(|made| {
                let rec = &made.record;
                let tag = crate::key::tag_of(&made.record_type).map(str::to_string).unwrap_or_else(|| made.record_type.to_uppercase().chars().take(4).collect());
                let mut dial = -1;
                if made.record_type == "Dialogue" {
                    let kind = rec.get("dialogue_type").filter(|v| truthy(v)).map(crate::pyval::str_of).unwrap_or_else(|| "Topic".into());
                    dial = KINDS.iter().position(|k| *k == kind).unwrap_or(0) as i32;
                }
                let (mut shorts, mut longs, mut floats) = (Vec::new(), Vec::new(), Vec::new());
                if made.record_type == "Script"
                    && let Some(vars) = rec.get("variables").filter(|v| truthy(v))
                {
                    let names = crate::mwscript::listing::decode_variables(vars).unwrap_or_default();
                    let head = rec.get("header");
                    let count = |k: &str| head.and_then(|h| h.get(k)).and_then(Value::as_f64).map_or(0, |n| n.max(0.0) as usize);
                    let (ns, nl) = (count("num_shorts").min(names.len()), count("num_longs"));
                    let nl = (ns + nl).min(names.len());
                    shorts = names[..ns].to_vec();
                    longs = names[ns..nl].to_vec();
                    floats = names[nl..].to_vec();
                }
                (tag, made.key.clone(), text_of(rec.get("script")), dial, shorts, longs, floats)
            })
            .collect()
    }

    /// The load order and the patch's own records, read once, for compiling scripts
    /// (again when what the patch makes changes).
    pub fn script_compiler(&self) -> Arc<ScriptCompiler> {
        let extra = self.pool_script_records();
        let key = format!("{extra:?}");
        let mut cached = self.compiler.lock().unwrap();
        if let Some((have, c)) = cached.as_ref()
            && *have == key
        {
            return c.clone();
        }
        let paths = self.session.plugin_paths();
        let custom = find_custom_functions(&paths);
        let c = Arc::new(ScriptCompiler::new(&paths, &extra, custom.as_deref()));
        *cached = Some((key, c.clone()));
        c
    }

    /// The Script Edit window: `{id, text, findings, listing, compiled, compile}` - the
    /// source (`text`, as typed, else the record's), its checks, the record's compiled
    /// listing, whether the record carries bytecode, and the source compiled now.
    pub fn script_view(&self, found: &Found, text: Option<&str>) -> Result<Value, EditError> {
        if found.record_type != "Script" {
            return err(format!("{} is not a script", found.key));
        }
        let stored = text_of(found.record().get("text"));
        let source = text.map(str::to_string).unwrap_or_else(|| stored.clone());
        let bytecode = found.record().get("bytecode").filter(|b| truthy(b));
        let listing = bytecode.map(|b| crate::mwscript::listing::listing_for_bytecode_field(b, Some(&stored))).unwrap_or_default();
        let mut findings: Vec<Value> = crate::mwscript::check::check_script(&source, &found.key, &self.script_globals())
            .into_iter()
            .map(|(line, level, message)| json!({"line": line, "level": level, "message": message}))
            .collect();
        if text.is_none()
            && let Some(lf) = bare_lf_finding(&source)
        {
            findings.push(lf);
            findings.sort_by_key(|f| f["line"].as_u64().unwrap_or(0));
        }
        let result = self.script_compiler().compile_text(&source);
        Ok(json!({
            "id": found.key,
            "text": source,
            "findings": findings,
            "listing": listing,
            "compiled": bytecode.is_some(),
            "compile": {"ok": result.ok, "bytes": result.data.len(), "messages": messages_json(&result.messages)},
        }))
    }

    /// `editScript`: the window for the text typed, else the text the patch has queued,
    /// else the record's; `queued` whether a changed text waits in the patch.
    pub fn edit_script(&self, found: &Found, text: Option<&str>) -> Result<Value, EditError> {
        let view = self.view(found);
        let waiting = view["fields"].as_array().into_iter().flatten().find(|f| f["path"] == "text").and_then(|f| f.get("queued")).and_then(Value::as_str).map(str::to_string);
        let mut out = self.script_view(found, text.or(waiting.as_deref()))?;
        out["queued"] = json!(waiting.is_some());
        Ok(out)
    }

    /// A dialogue response's result script compiled for its errors: `{messages,
    /// speaker_script}` - `speaker_script` whether the speaker carries a script whose
    /// locals the result reads. `stored`: the text as the plugin stores it (a result with
    /// bare LFs is warned about).
    pub fn result_check(&self, text: &str, speaker: &str, stored: bool) -> Value {
        let compiler = self.script_compiler();
        let mut messages = messages_json(&compiler.check_result(text, speaker));
        if stored && let Some(lf) = bare_lf_finding(text) {
            messages.push(lf);
            messages.sort_by_key(|m| m["line"].as_u64().unwrap_or(0));
        }
        json!({"messages": messages, "speaker_script": !speaker.is_empty() && compiler.script_locals(speaker).is_some()})
    }

    /// `editResult`: the response's result (the text typed, else as the patch leaves it)
    /// checked with its speaker (as the patch leaves it).
    pub fn edit_result(&self, found: &Found, text: Option<&str>) -> Value {
        let view = self.view(found);
        let value = |path: &str| {
            view["fields"].as_array().into_iter().flatten().find(|f| f["path"] == path).map(|f| f.get("queued").unwrap_or(&f["value"]).clone())
        };
        let source = match text {
            Some(t) => t.to_string(),
            None => text_of(value("script_text").as_ref()),
        };
        self.result_check(&source, &text_of(value("speaker_id").as_ref()), text.is_none())
    }

    /// What Script Edit completes: `{functions, keywords, globals}`.
    pub fn script_words(&self) -> Value {
        let latin = |b: &[u8]| b.iter().map(|&c| c as char).collect::<String>();
        let mut functions: Vec<String> = crate::mwscript::data::FUNCTIONS.iter().map(|f| latin(f.0)).chain(crate::mwscript::data::CUSTOM_FUNCTIONS.iter().map(|f| latin(f.0))).collect();
        functions.sort_by_key(|f| f.to_lowercase());
        functions.dedup();
        let mut keywords: Vec<&str> = crate::mwscript::check::KEYWORDS.to_vec();
        keywords.sort_by_key(|k| k.to_lowercase());
        json!({"functions": functions, "keywords": keywords, "globals": self.script_globals()})
    }

    /// Every script's text as the patch leaves it, by id as the record spells it.
    fn script_texts(&self) -> BTreeMap<String, String> {
        let mut out = IndexMap::new();
        for plugin in self.session.order() {
            for rec in self.session.records(plugin, "SCPT").values() {
                out.insert(text_of(rec.get("id")), text_of(rec.get("text")));
            }
        }
        for ((rtype, key), choices) in self.queue.fields().iter() {
            if rtype == "Script" {
                for c in choices {
                    if let Choice::Value { path, value } = c
                        && path == "text"
                    {
                        out.insert(key.clone(), crate::pyval::str_of(value));
                    }
                }
            }
        }
        for made in self.queue.new_records().filter(|m| m.record_type == "Script") {
            out.insert(made.key.clone(), text_of(made.record.get("text")));
        }
        out.into_iter().collect()
    }

    /// Every script naming a word (a variable, an id), as the patch leaves them: matched
    /// whole, any case, in the code and not a comment - `{word, hits: [{script, line,
    /// text}], scripts, capped}`.
    pub fn script_refs(&self, word: &str, limit: usize) -> Value {
        let w = word.trim();
        if w.is_empty() {
            return json!({"word": w, "hits": [], "scripts": 0, "capped": false});
        }
        let finder = Finder::new(w, true);
        let mut hits = Vec::new();
        let mut scripts = std::collections::HashSet::new();
        let mut total = 0;
        for (key, text) in self.script_texts() {
            for (n, line) in splitlines(&text).into_iter().enumerate() {
                let code = line.split(';').next().unwrap_or("");
                if finder.found(code) {
                    total += 1;
                    scripts.insert(key.clone());
                    if hits.len() < limit {
                        hits.push(json!({"script": key, "line": n + 1, "text": line.trim()}));
                    }
                }
            }
        }
        json!({"word": w, "hits": hits, "scripts": scripts.len(), "capped": total > limit})
    }

    /// `editFlowchart`: a script's control flow (the text typed, else the record's) -
    /// `{heading, chart, title}`, the chart as Mermaid text.
    pub fn script_flowchart(&self, found: &Found, text: Option<&str>) -> Value {
        let source = text.map(str::to_string).unwrap_or_else(|| text_of(found.record().get("text")));
        json!({"heading": format!("{} - flowchart", found.key), "chart": crate::mwscript::flowchart::flowchart(&source), "title": "Script flowchart"})
    }

    /// What a leveled list gives at a level, lists inside it rolled too (as the patch
    /// leaves them): `{level, outcomes: [{id, chance}] (most likely first; "" nothing),
    /// nested}`.
    pub fn leveled_roll(&self, found: &Found, level: i64) -> Value {
        let tag = found.tag.clone();
        let mut nested: Vec<String> = Vec::new();
        let mut totals: IndexMap<String, f64> = IndexMap::new();
        self.roll(&self.as_written(found), &found.record_type, &tag, level, 1.0, 0, &mut totals, &mut nested);
        let mut ranked: Vec<(String, f64)> = totals.into_iter().filter(|(_, v)| *v > 0.0).collect();
        ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal).then_with(|| a.0.to_lowercase().cmp(&b.0.to_lowercase())));
        let mut lists: Vec<String> = nested.into_iter().collect::<std::collections::BTreeSet<_>>().into_iter().collect();
        lists.sort_by_key(|n| n.to_lowercase());
        json!({
            "level": level,
            "outcomes": ranked.into_iter().map(|(id, v)| json!({"id": id, "chance": round6(v)})).collect::<Vec<_>>(),
            "nested": lists,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn roll(&self, rec: &Value, rtype: &str, tag: &str, level: i64, weight: f64, depth: usize, out: &mut IndexMap<String, f64>, nested: &mut Vec<String>) {
        for (rid, chance) in roll_entries(rtype, rec, level) {
            if rid.is_empty() {
                *out.entry(String::new()).or_insert(0.0) += weight * chance;
                continue;
            }
            let inner = if depth < MAX_DEPTH { self.find(tag, &rid, None) } else { None };
            match inner.filter(|i| i.record_type == rtype) {
                Some(inner) => {
                    nested.push(inner.key.clone());
                    self.roll(&self.as_written(&inner), rtype, tag, level, weight * chance, depth + 1, out, nested);
                }
                None => *out.entry(rid).or_insert(0.0) += weight * chance,
            }
        }
    }

    /// `editLeveledRoll` `{tag, id, level}`.
    pub fn edit_leveled_roll(&self, tag: &str, id: &str, level: &Value) -> Result<Value, EditError> {
        let Some(level) = level.as_i64().filter(|l| *l >= 1 && !level.is_boolean() && (level.is_i64() || level.is_u64())) else {
            return err("level is a number from 1");
        };
        match self.find(tag, id, None).filter(|f| f.record_type == "LeveledItem" || f.record_type == "LeveledCreature") {
            Some(found) => Ok(self.leveled_roll(&found, level)),
            None => err("That is not a leveled list"),
        }
    }
}

/// `editLuaChart` `{path, text, kind, line?}`: a Lua script's functions (`kind`
/// `functions`: `{functions: [{name, line}]}`), or its flowchart (`flow`: the whole
/// script's, or the function at `line`) or call graph (`calls`) as `{heading, chart,
/// title}`, the chart as Mermaid text (luacore).
pub fn lua_chart(path: &str, text: &str, kind: &str, line: Option<i64>) -> Result<Value, EditError> {
    if !matches!(kind, "functions" | "flow" | "calls") {
        return err("bad request");
    }
    let tree = luacore::parser::parse(text, path.to_lowercase().ends_with(".tl")).map_err(|(msg, line, col)| EditError(format!("line {line}:{col}: {msg}")))?;
    // One per line: a named function's own body node shares its line.
    let mut by_line: BTreeMap<usize, &luacore::parser::Node> = BTreeMap::new();
    let mut stack = vec![&tree];
    while let Some(n) = stack.pop() {
        if matches!(n.kind, "Function" | "LocalFunction" | "FunctionStat") {
            by_line.entry(n.line).or_insert(n);
        }
        stack.extend(n.children.iter().rev());
    }
    let name_of = |n: &luacore::parser::Node| n.value.clone().filter(|v| !v.is_empty()).unwrap_or_else(|| format!("function at line {}", n.line));
    if kind == "functions" {
        return Ok(json!({"functions": by_line.values().map(|n| json!({"name": name_of(n), "line": n.line})).collect::<Vec<_>>()}));
    }
    if kind == "calls" {
        return Ok(json!({"heading": format!("{path} - call graph"), "chart": luacore::mermaid::call_graph_chart(&tree), "title": "Lua call graph"}));
    }
    let target = line.and_then(|l| by_line.values().find(|n| n.line as i64 == l).copied());
    let heading = match target {
        None => path.to_string(),
        Some(n) => format!("{path} - {}", name_of(n)),
    };
    Ok(json!({"heading": heading, "chart": luacore::mermaid::flowchart(target.unwrap_or(&tree)), "title": "Lua flowchart"}))
}
