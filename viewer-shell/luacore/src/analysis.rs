//! What one OpenMW Lua script declares, and what it does every frame (was
//! `wraithguard/lua/analysis.py`, which stays as the fallback and the reference this is
//! held to - findings, their order and their wording included). The checks are the
//! Python module's; its docstring says what each is for.

use std::collections::{HashMap, HashSet};

use super::parser::{Node, parse};

/// The API rules one OpenMW release has (the Python `ApiVersion`'s): engine handler ->
/// the contexts that may define it, the per-frame handlers, package -> contexts.
#[derive(Clone, Default)]
pub struct Rules {
    pub handlers: HashMap<String, Vec<String>>,
    pub per_frame: HashSet<String>,
    pub packages: HashMap<String, Vec<String>>,
}

/// One finding: severity, code, line, message.
pub type Finding = (&'static str, &'static str, usize, String);

/// An insertion-ordered map, as a Python dict keeps keys.
#[derive(Clone)]
pub struct Ordered<V> {
    pub items: Vec<(String, V)>,
}

impl<V> Default for Ordered<V> {
    fn default() -> Self {
        Ordered { items: Vec::new() }
    }
}

impl<V: Clone> Ordered<V> {
    fn get(&self, k: &str) -> Option<&V> {
        self.items.iter().find(|(key, _)| key == k).map(|(_, v)| v)
    }
    /// `d[k] = v`: in place when there, at the end when not.
    fn set(&mut self, k: &str, v: V) {
        match self.items.iter_mut().find(|(key, _)| key == k) {
            Some(slot) => slot.1 = v,
            None => self.items.push((k.to_string(), v)),
        }
    }
    /// `d.setdefault(k, v)`.
    fn set_default(&mut self, k: &str, v: V) {
        if self.get(k).is_none() {
            self.items.push((k.to_string(), v));
        }
    }
    fn update(&mut self, other: &Ordered<V>) {
        for (k, v) in &other.items {
            self.set(k, v.clone());
        }
    }
}

/// What `analyze` learned about one script (the Python `ScriptInfo`, the tree aside).
#[derive(Default)]
pub struct Info {
    pub interface_name: Option<String>,
    pub interface_line: usize,
    pub interface_members: Vec<String>,
    pub engine_handlers: Ordered<usize>,
    pub event_handlers: Ordered<usize>,
    pub requires: Ordered<usize>,
    pub sent_events: Ordered<usize>,
    pub findings: Vec<Finding>,
}

const BIG_COLLECTIONS: [&str; 7] = [
    "nearby.actors",
    "nearby.items",
    "nearby.activators",
    "nearby.containers",
    "nearby.doors",
    "world.activeActors",
    "world.cells",
];
fn costly(name: &str) -> Option<&'static str> {
    match name {
        "findPath" => Some("asks the navigator for a path"),
        "castRenderingRay" => Some("casts a ray against rendered geometry"),
        "updateAll" => Some("relayouts every UI element"),
        _ => None,
    }
}
const LOOPS: [&str; 4] = ["ForIn", "ForNum", "While", "Repeat"];
fn not_in_sandbox(name: &str) -> Option<&'static str> {
    Some(match name {
        "collectgarbage" => {
            "collectgarbage is not in OpenMW's sandbox: the call fails. OpenMW runs the collector itself (memory limits: [Lua] in settings.cfg); make less garbage instead"
        }
        "load" => "load is not in OpenMW's sandbox: scripts cannot compile code at run time",
        "loadstring" => "loadstring is not in OpenMW's sandbox: scripts cannot compile code at run time",
        "loadfile" => "loadfile is not in OpenMW's sandbox: read files with openmw.vfs",
        "dofile" => "dofile is not in OpenMW's sandbox: use require",
        "setfenv" => "setfenv is not in OpenMW's sandbox",
        "getfenv" => "getfenv is not in OpenMW's sandbox",
        "gcinfo" => "gcinfo is not in OpenMW's sandbox",
        "newproxy" => "newproxy is not in OpenMW's sandbox",
        "module" => "module is not in OpenMW's sandbox: return a table from the script instead",
        "io" => "io is not in OpenMW's sandbox: read files with openmw.vfs, keep data with openmw.storage",
        "debug" => "debug is not in OpenMW's sandbox (openmw.debug is a different thing)",
        "package" => "package is not in OpenMW's sandbox",
        _ => return None,
    })
}
const OS_ALLOWED: [&str; 3] = ["date", "difftime", "time"];
const READ_ONLY_PACKAGES: [&str; 6] = ["coroutine", "math", "string", "table", "utf8", "os"];
const USERDATA_MAKERS: [&str; 9] = ["vector2", "vector3", "vector4", "rgb", "rgba", "hex", "move", "rotate", "scale"];
const SEND_EVENT_FUNCS: [&str; 2] = ["sendGlobalEvent", "sendMenuEvent"];

fn val(n: &Node) -> Option<&str> {
    n.value.as_deref()
}

/// Preorder, as the Python `Node.walk`.
fn walk_all<'a>(n: &'a Node, out: &mut Vec<&'a Node>) {
    out.push(n);
    for c in &n.children {
        walk_all(c, out);
    }
}

fn required_module(expr: &Node) -> Option<String> {
    let mut e = expr;
    while e.kind == "Paren" {
        e = &e.children[0];
    }
    if e.kind != "Call" || e.children.len() < 2 {
        return None;
    }
    let (f, arg) = (&e.children[0], &e.children[1]);
    if f.kind == "Name" && val(f) == Some("require") && arg.kind == "String" {
        return arg.value.clone();
    }
    None
}

pub fn dotted(node: &Node, aliases: &HashMap<String, String>) -> Option<String> {
    match node.kind {
        "Name" => {
            let v = val(node).filter(|v| !v.is_empty())?;
            Some(aliases.get(v).cloned().unwrap_or_else(|| v.to_string()))
        }
        "Index" if val(node) == Some(".") => {
            let base = dotted(&node.children[0], aliases)?;
            let key = val(&node.children[1]).filter(|k| !k.is_empty())?;
            if base.is_empty() { None } else { Some(format!("{base}.{key}")) }
        }
        "Method" => {
            let m = val(node).unwrap_or("None");
            match dotted(&node.children[0], aliases).filter(|b| !b.is_empty()) {
                Some(base) => Some(format!("{base}:{m}")),
                None => Some(format!("?:{m}")),
            }
        }
        "Call" => dotted(&node.children[0], aliases).filter(|b| !b.is_empty()).map(|i| format!("{i}()")),
        _ => None,
    }
}

fn loop_collection(lp: &Node, aliases: &HashMap<String, String>) -> Option<String> {
    let exprs = &lp.children[1].children;
    let mut it = exprs.first()?;
    if it.kind == "Call" && it.children.len() >= 2 {
        let f = dotted(&it.children[0], aliases);
        if matches!(f.as_deref(), Some("ipairs" | "pairs")) {
            it = &it.children[1];
        }
    }
    let text = dotted(it, aliases).filter(|t| !t.is_empty())?;
    if BIG_COLLECTIONS.contains(&text.as_str()) || text.ends_with(".records") {
        return Some(text);
    }
    if text.contains(":getAll") && !text.to_lowercase().contains("inventory") {
        return Some(text);
    }
    None
}

fn returns(node: &Node) -> bool {
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        if n.kind == "Return" {
            return true;
        }
        stack.extend(n.children.iter().filter(|c| c.kind != "Function"));
    }
    false
}

struct Symbols<'a> {
    funcs: HashMap<String, &'a Node>,
    tables: HashMap<String, &'a Node>,
    fields: HashMap<String, Ordered<&'a Node>>,
    aliases: HashMap<String, String>,
}

impl<'a> Symbols<'a> {
    fn new(chunk: &'a Node) -> Self {
        let mut s = Symbols { funcs: HashMap::new(), tables: HashMap::new(), fields: HashMap::new(), aliases: HashMap::new() };
        for stmt in &chunk.children {
            match stmt.kind {
                "LocalFunction" | "FunctionStat" if val(stmt).is_some_and(|v| !v.is_empty()) => {
                    s.funcs.insert(val(stmt).unwrap_or("").replace(':', "."), &stmt.children[0]);
                }
                "Local" => {
                    let (names, exprs) = (&stmt.children[0], &stmt.children[1]);
                    for (name, expr) in names.children.iter().zip(&exprs.children) {
                        s.bind(val(name).unwrap_or(""), expr);
                    }
                }
                "Assign" => {
                    let (targets, exprs) = (&stmt.children[0], &stmt.children[1]);
                    for (target, expr) in targets.children.iter().zip(&exprs.children) {
                        s.assign(target, expr);
                    }
                }
                _ => {}
            }
        }
        s
    }

    fn bind(&mut self, name: &str, expr: &'a Node) {
        if expr.kind == "Function" {
            self.funcs.insert(name.to_string(), expr);
        } else if expr.kind == "Table" {
            self.tables.insert(name.to_string(), expr);
        } else if let Some(module) = required_module(expr).filter(|m| !m.is_empty()) {
            let short = module.rsplit('.').next().unwrap_or(&module).to_string();
            self.aliases.insert(name.to_string(), short);
        }
    }

    fn assign(&mut self, target: &'a Node, expr: &'a Node) {
        if target.kind == "Name" && val(target).is_some_and(|v| !v.is_empty()) {
            self.bind(val(target).unwrap_or(""), expr);
            return;
        }
        if target.kind != "Index" || val(target) != Some(".") {
            return;
        }
        let (obj, key) = (&target.children[0], &target.children[1]);
        if obj.kind == "Name"
            && let (Some(o), Some(k)) = (val(obj).filter(|v| !v.is_empty()), val(key).filter(|v| !v.is_empty()))
        {
            self.fields.entry(o.to_string()).or_default().set(k, expr);
            if expr.kind == "Function" {
                self.funcs.insert(format!("{o}.{k}"), expr);
            }
        }
    }

    fn table_fields(&self, node: Option<&'a Node>) -> Option<Ordered<&'a Node>> {
        let node = node?;
        if node.kind == "Table" {
            let mut found = Ordered::default();
            for c in &node.children {
                let key = &c.children[0];
                if c.kind == "Field" && key.kind == "String" && val(key).is_some_and(|v| !v.is_empty()) {
                    found.set(val(key).unwrap_or(""), &c.children[1]);
                }
            }
            return Some(found);
        }
        if node.kind == "Name" && val(node).is_some_and(|v| !v.is_empty()) {
            let name = val(node).unwrap_or("");
            let base = self.tables.get(name).copied();
            let extra = self.fields.get(name);
            if base.is_none() && extra.is_none_or(|e| e.items.is_empty()) {
                return None;
            }
            let mut out = self.table_fields(base).filter(|t| !t.items.is_empty()).unwrap_or_default();
            if let Some(extra) = extra {
                out.update(extra);
            }
            return Some(out);
        }
        None
    }

    fn function_of(&self, node: &'a Node) -> Option<&'a Node> {
        if node.kind == "Function" {
            return Some(node);
        }
        let name = dotted(node, &HashMap::new()).filter(|n| !n.is_empty())?;
        self.funcs.get(&name).copied()
    }
}

struct FrameWalker<'a, 'i> {
    info: &'i mut Info,
    symbols: &'i Symbols<'a>,
    handler: String,
    seen: HashSet<usize>,
    reported: HashSet<(&'static str, usize)>,
}

impl<'a> FrameWalker<'a, '_> {
    fn function(&mut self, f: &'a Node, guarded: bool, loops: &[String], in_loop: bool) {
        let id = f as *const Node as usize;
        if !self.seen.insert(id) {
            return;
        }
        self.walk(&f.children[1], &[], guarded, in_loop || !loops.is_empty());
    }

    fn report(&mut self, severity: &'static str, code: &'static str, line: usize, message: String) {
        if self.reported.insert((code, line)) {
            self.info.findings.push((severity, code, line, message));
        }
    }

    fn walk(&mut self, node: &'a Node, loops: &[String], guarded: bool, in_loop: bool) {
        let mut guarded = guarded;
        if node.kind == "Function" {
            self.garbage(node, "GC_CLOSURE", "makes a new function (closure)", loops, guarded, in_loop);
            return;
        }
        if node.kind == "Block" {
            for child in &node.children {
                self.walk(child, loops, guarded, in_loop);
                if child.kind == "If" && returns(child) {
                    guarded = true;
                }
            }
            return;
        }
        if node.kind == "If" {
            for child in &node.children {
                self.walk(child, loops, true, in_loop);
            }
            return;
        }
        let mut inner: Vec<String> = loops.to_vec();
        if node.kind == "ForIn" {
            if let Some(coll) = loop_collection(node, &self.symbols.aliases) {
                self.big_loop(node, &coll, loops, guarded);
                inner.push(coll);
            }
        } else if node.kind == "Call" || node.kind == "Method" {
            self.call(node, loops, guarded, in_loop);
            if in_loop && self.makes_userdata(node) {
                self.garbage(node, "GC_USERDATA", "makes a new vector/colour/transform", loops, guarded, true);
            }
        } else if node.kind == "Table" {
            self.garbage(node, "GC_TABLE", "makes a new table", loops, guarded, in_loop);
        } else if node.kind == "Binop" && val(node) == Some("..") && in_loop {
            self.garbage(node, "GC_STRING", "builds a new string", loops, guarded, true);
        }
        let looping = in_loop || LOOPS.contains(&node.kind);
        for child in &node.children {
            if matches!(node.kind, "Table" | "Field" | "Item") && child.kind == "Table" {
                for grand in &child.children {
                    self.walk(grand, &inner, guarded, looping);
                }
                continue;
            }
            self.walk(child, &inner, guarded, looping);
        }
    }

    fn makes_userdata(&self, node: &Node) -> bool {
        if node.kind != "Call" {
            return false;
        }
        let name = dotted(&node.children[0], &self.symbols.aliases).unwrap_or_default();
        let parts: Vec<&str> = name.split('.').collect();
        parts.len() >= 2 && parts[0] == "util" && USERDATA_MAKERS.contains(parts.last().unwrap_or(&""))
    }

    fn garbage(&mut self, node: &Node, code: &'static str, what: &str, loops: &[String], guarded: bool, in_loop: bool) {
        if guarded {
            return;
        }
        let h = &self.handler;
        let msg = if let Some(last) = loops.last() {
            format!("{h} {what} for every one of {last}, every frame")
        } else if in_loop {
            format!("{h} {what} on every pass of a loop, every frame")
        } else {
            format!("{h} {what} every frame: hoist it out, or reuse one")
        };
        self.report(if in_loop { "warn" } else { "info" }, code, node.line, msg + " (garbage for the collector)");
    }

    fn big_loop(&mut self, node: &Node, coll: &str, loops: &[String], guarded: bool) {
        let severity = if guarded { "info" } else { "warn" };
        let when = if guarded { "when its condition holds" } else { "every frame" };
        let h = self.handler.clone();
        if let Some(last) = loops.last() {
            let msg = format!(
                "{h} loops over {coll} inside a loop over {last} {when}: the work grows with the square of what is nearby"
            );
            self.report(severity, "PERF_NESTED", node.line, msg);
        } else {
            let tip = if guarded { "" } else { "; consider a timer or the engine's events" };
            self.report(severity, "PERF_LOOP", node.line, format!("{h} loops over {coll} {when}{tip}"));
        }
    }

    fn call(&mut self, node: &'a Node, loops: &[String], guarded: bool, in_loop: bool) {
        let name: String = if node.kind == "Method" {
            val(node).unwrap_or("").to_string()
        } else {
            let f = &node.children[0];
            let n = if f.kind == "Index" { val(&f.children[1]) } else { val(f) };
            let name = n.unwrap_or("").to_string();
            if let Some(target) = self.symbols.function_of(f) {
                self.function(target, guarded, loops, in_loop);
            }
            name
        };
        if let Some(why) = costly(&name) {
            let wh = loops.last().map(|l| format!(" inside a loop over {l}")).unwrap_or_default();
            let when = if guarded { "when its condition holds" } else { "every frame" };
            let sev = if !loops.is_empty() && !guarded { "warn" } else { "info" };
            let msg = format!("{} calls {name}(){wh} {when}: it {why}", self.handler);
            self.report(sev, "PERF_CALL", node.line, msg);
        }
        if name == "print" && node.kind == "Call" && !guarded {
            let msg = format!("{} prints every frame", self.handler);
            self.report("info", "PERF_PRINT", node.line, msg);
        }
    }
}

/// Every name a script binds anywhere (the Python `bound_names`).
pub fn bound_names(tree: &Node) -> HashSet<String> {
    let mut nodes = Vec::new();
    walk_all(tree, &mut nodes);
    let mut out = HashSet::new();
    let mut add = |v: Option<&str>| {
        if let Some(v) = v.filter(|v| !v.is_empty()) {
            out.insert(v.to_string());
        }
    };
    for n in nodes {
        match n.kind {
            "Local" | "ForIn" => n.children[0].children.iter().for_each(|c| add(val(c))),
            "ForNum" if !n.children.is_empty() => add(val(&n.children[0])),
            "LocalFunction" => add(val(n)),
            "Params" => n.children.iter().filter(|c| c.kind == "Name").for_each(|c| add(val(c))),
            "Assign" => n.children[0].children.iter().filter(|t| t.kind == "Name").for_each(|t| add(val(t))),
            "FunctionStat" if val(n).is_some_and(|v| !v.contains('.') && !v.contains(':')) => add(val(n)),
            _ => {}
        }
    }
    out
}

fn sandbox_checks(info: &mut Info, chunk: &Node) {
    let own = bound_names(chunk);
    let mut seen: HashSet<(String, usize)> = HashSet::new();
    let mut nodes = Vec::new();
    walk_all(chunk, &mut nodes);
    let mut flag = |info: &mut Info, line: usize, what: String, msg: String| {
        if seen.insert((what, line)) {
            info.findings.push(("error", "SANDBOX", line, msg));
        }
    };
    for n in nodes {
        if n.kind == "Name" {
            if let Some(v) = val(n)
                && let Some(msg) = not_in_sandbox(v)
                && !own.contains(v)
            {
                flag(info, n.line, v.to_string(), msg.to_string());
            }
        } else if n.kind == "Index" && val(n) == Some(".") {
            let (obj, key) = (&n.children[0], &n.children[1]);
            let k = val(key).filter(|k| !k.is_empty());
            let unsandboxed = k.is_some_and(|k| !OS_ALLOWED.contains(&k));
            if obj.kind == "Name" && val(obj) == Some("os") && !own.contains("os") && unsandboxed {
                let k = k.unwrap_or("");
                flag(
                    info,
                    n.line,
                    format!("os.{k}"),
                    format!("os.{k} is not in OpenMW's sandbox (only os.date, os.difftime and os.time are)"),
                );
            }
        } else if n.kind == "Assign" {
            for t in &n.children[0].children {
                if t.kind == "Index" && t.children[0].kind == "Name" {
                    let pkg = val(&t.children[0]).unwrap_or("");
                    if READ_ONLY_PACKAGES.contains(&pkg) && !own.contains(pkg) {
                        flag(info, t.line, format!("{pkg}="), format!("{pkg} is read-only in OpenMW: this assignment fails"));
                    }
                }
            }
        }
    }
}

fn collect_calls(info: &mut Info, chunk: &Node) {
    let mut nodes = Vec::new();
    walk_all(chunk, &mut nodes);
    for node in nodes {
        if let Some(module) = required_module(node).filter(|m| !m.is_empty()) {
            info.requires.set_default(&module, node.line);
            continue;
        }
        if node.kind == "Method" && val(node) == Some("sendEvent") && node.children.len() >= 2 {
            let arg = &node.children[1];
            if arg.kind == "String"
                && let Some(v) = val(arg).filter(|v| !v.is_empty())
            {
                info.sent_events.set_default(v, node.line);
            }
        } else if node.kind == "Call" && node.children.len() >= 2 {
            let (f, arg) = (&node.children[0], &node.children[1]);
            let name = if f.kind == "Index" { val(&f.children[1]) } else { val(f) };
            if name.is_some_and(|n| SEND_EVENT_FUNCS.contains(&n))
                && arg.kind == "String"
                && let Some(v) = val(arg).filter(|v| !v.is_empty())
            {
                info.sent_events.set_default(v, node.line);
            }
        }
    }
}

fn sorted_join(items: &[String]) -> String {
    let mut v: Vec<&str> = items.iter().map(String::as_str).collect();
    v.sort_unstable();
    v.dedup();
    v.join("/")
}

fn check_contexts(info: &mut Info, contexts: &[String], rules: &Rules) {
    if contexts.is_empty() {
        return;
    }
    let wh = sorted_join(contexts);
    let meets = |allowed: &[String]| allowed.iter().any(|a| contexts.contains(a));
    let handlers = info.engine_handlers.items.clone();
    for (name, line) in handlers {
        match rules.handlers.get(&name) {
            None => info.findings.push(("warn", "UNKNOWN_HANDLER", line, format!("{name} is not an engine handler"))),
            Some(allowed) if !meets(allowed) => info.findings.push((
                "warn",
                "HANDLER_CONTEXT",
                line,
                format!("{name} is never called in a {wh} script (only {})", sorted_join(allowed)),
            )),
            _ => {}
        }
    }
    let requires = info.requires.items.clone();
    for (module, line) in requires {
        if let Some(allowed) = rules.packages.get(&module)
            && !meets(allowed)
        {
            info.findings.push((
                "error",
                "PACKAGE_CONTEXT",
                line,
                format!("{module} is not available to a {wh} script (only {}); require fails there", sorted_join(allowed)),
            ));
        }
    }
}

fn finish(info: &mut Info, contexts: &[String], rules: &Rules) {
    check_contexts(info, contexts, rules);
    info.findings.sort_by(|a, b| (a.2, a.1).cmp(&(b.2, b.1)));
}

/// Reads one script: its declarations and findings, and its tree (None when it does
/// not parse).
pub fn analyze(src: &str, contexts: &[String], rules: &Rules) -> (Info, Option<Node>) {
    let mut info = Info::default();
    let chunk = match parse(src, false) {
        Ok(c) => c,
        Err((msg, line, col)) => {
            info.findings.push(("error", "SYNTAX", line, format!("line {line}:{col}: {msg}")));
            return (info, None);
        }
    };
    {
        let symbols = Symbols::new(&chunk);
        collect_calls(&mut info, &chunk);
        sandbox_checks(&mut info, &chunk);
        let last = chunk.children.last();
        let returned = match last {
            Some(l) if l.kind == "Return" && !l.children[0].children.is_empty() => {
                symbols.table_fields(Some(&l.children[0].children[0]))
            }
            _ => None,
        };
        if let Some(returned) = returned {
            if let Some(n) = returned.get("interfaceName").filter(|n| n.kind == "String") {
                info.interface_name = n.value.clone();
                info.interface_line = n.line;
            }
            if let Some(members) = symbols.table_fields(returned.get("interface").copied()).filter(|m| !m.items.is_empty()) {
                let mut names: Vec<String> = members.items.into_iter().map(|(k, _)| k).collect();
                names.sort();
                info.interface_members = names;
            }
            let engine = symbols.table_fields(returned.get("engineHandlers").copied()).unwrap_or_default();
            let events = symbols.table_fields(returned.get("eventHandlers").copied()).unwrap_or_default();
            info.engine_handlers = Ordered { items: engine.items.iter().map(|(k, v)| (k.clone(), v.line)).collect() };
            info.event_handlers = Ordered { items: events.items.iter().map(|(k, v)| (k.clone(), v.line)).collect() };
            let mut names: Vec<&String> = engine.items.iter().map(|(k, _)| k).collect();
            names.sort();
            for name in names {
                if rules.per_frame.contains(name)
                    && let Some(f) = engine.get(name).copied().and_then(|n| symbols.function_of(n))
                {
                    let mut walker = FrameWalker {
                        info: &mut info,
                        symbols: &symbols,
                        handler: name.clone(),
                        seen: HashSet::new(),
                        reported: HashSet::new(),
                    };
                    walker.function(f, false, &[], false);
                }
            }
        }
    }
    finish(&mut info, contexts, rules);
    (info, Some(chunk))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules() -> Rules {
        let mut handlers = HashMap::new();
        handlers.insert("onUpdate".to_string(), vec!["global".into(), "local".into(), "player".into()]);
        handlers.insert("onKeyPress".to_string(), vec!["menu".into(), "player".into()]);
        let mut packages = HashMap::new();
        packages.insert("openmw.nearby".to_string(), vec!["local".into(), "player".into()]);
        Rules { handlers, per_frame: ["onUpdate".to_string()].into_iter().collect(), packages }
    }

    #[test]
    fn handlers_loops_and_contexts() {
        let src = "local nearby = require('openmw.nearby')\nlocal function tick(dt)\n  for _, a in ipairs(nearby.actors) do local t = {} end\nend\nreturn { engineHandlers = { onUpdate = tick, onKeyPress = function() end } }\n";
        let (info, tree) = analyze(src, &["global".to_string()], &rules());
        assert!(tree.is_some());
        let codes: Vec<&str> = info.findings.iter().map(|f| f.1).collect();
        assert!(codes.contains(&"PERF_LOOP"), "{codes:?}");
        assert!(codes.contains(&"GC_TABLE"), "{codes:?}");
        assert!(codes.contains(&"HANDLER_CONTEXT"), "{codes:?}");
        assert!(codes.contains(&"PACKAGE_CONTEXT"), "{codes:?}");
        assert_eq!(info.engine_handlers.items.len(), 2);
    }

    #[test]
    fn sandbox_and_syntax() {
        let (info, _) = analyze("collectgarbage()\nlocal x = os.clock()\nstring.foo = 1\n", &[], &rules());
        assert_eq!(info.findings.iter().filter(|f| f.1 == "SANDBOX").count(), 3);
        let (info, tree) = analyze("x = = 1", &[], &rules());
        assert!(tree.is_none());
        assert_eq!(info.findings[0].1, "SYNTAX");
        assert!(info.findings[0].3.starts_with("line 1:5: "));
    }
}
