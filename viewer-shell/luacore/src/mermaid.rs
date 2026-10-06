//! Control-flow flowcharts and call graphs of Lua code, as Mermaid (was
//! `wraithguard/lua/flowchart.py` and `callgraph.py`, which stay as the fallback and the
//! reference this is held to, node ids and labels included). Their docstrings say how
//! the charts are built.

use std::collections::{HashMap, HashSet};

use super::analysis::dotted;
use super::lexer::py_repr;
use super::omwscripts::py_split;
use super::parser::Node;

const LABEL_MAX: usize = 60;
const CONTROL: [&str; 10] = ["If", "While", "Repeat", "ForNum", "ForIn", "Do", "Return", "Break", "Goto", "Label"];

/// A node's value as an f-string shows it (`None` when it has none).
fn shown(n: &Node) -> &str {
    n.value.as_deref().unwrap_or("None")
}

/// `value or ""`.
fn or_empty(n: &Node) -> &str {
    n.value.as_deref().unwrap_or("")
}

/// Text trimmed to `n` characters (whitespace runs made one space), the cut marked.
pub fn cut(text: &str, n: usize) -> String {
    let text = py_split(text).collect::<Vec<_>>().join(" ");
    if text.chars().count() <= n {
        return text;
    }
    let mut s: String = text.chars().take(n - 3).collect();
    s.push_str("...");
    s
}

fn args(nodes: &[Node]) -> String {
    nodes.iter().map(expr_text).collect::<Vec<_>>().join(", ")
}

/// An expression (or a simple statement) as compact Lua-ish text.
pub fn expr_text(node: &Node) -> String {
    let (k, kids) = (node.kind, &node.children);
    match k {
        "Nil" => "nil".into(),
        "True" => "true".into(),
        "False" => "false".into(),
        "Vararg" => "...".into(),
        "Name" | "Number" => or_empty(node).into(),
        "String" => py_repr(&cut(or_empty(node), 24)),
        "Index" if node.value.as_deref() == Some(".") => format!("{}.{}", expr_text(&kids[0]), shown(&kids[1])),
        "Index" => format!("{}[{}]", expr_text(&kids[0]), expr_text(&kids[1])),
        "Call" => format!("{}({})", expr_text(&kids[0]), args(&kids[1..])),
        "Method" => format!("{}:{}({})", expr_text(&kids[0]), shown(node), args(&kids[1..])),
        "Binop" => format!("{} {} {}", expr_text(&kids[0]), shown(node), expr_text(&kids[1])),
        "Unop" => {
            let sep = if node.value.as_deref() == Some("not") { " " } else { "" };
            format!("{}{sep}{}", shown(node), expr_text(&kids[0]))
        }
        "Cast" => format!("{} {} ...", expr_text(&kids[0]), shown(node)),
        "TypeDecl" => node.value.clone().filter(|v| !v.is_empty()).unwrap_or_else(|| k.to_string()),
        "Paren" => format!("({})", expr_text(&kids[0])),
        "Function" => format!("function({}) ... end", args(&kids[0].children)),
        "Table" => (if kids.is_empty() { "{}" } else { "{...}" }).into(),
        "Local" => {
            let names: Vec<&str> = kids[0].children.iter().map(or_empty).collect();
            let vals = args(&kids[1].children);
            if vals.is_empty() {
                format!("local {}", names.join(", "))
            } else {
                format!("local {} = {vals}", names.join(", "))
            }
        }
        "Assign" => format!("{} = {}", args(&kids[0].children), args(&kids[1].children)),
        "CallStat" => expr_text(&kids[0]),
        "LocalFunction" => format!("local function {}({})", shown(node), args(&kids[0].children[0].children)),
        "FunctionStat" => format!("function {}({})", shown(node), args(&kids[0].children[0].children)),
        _ => k.into(),
    }
}

/// Text for a quoted Mermaid label (Python's `html.escape(quote=False)`, then quotes and
/// line breaks).
fn esc(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "#quot;")
        .replace('\n', "<br/>")
}

/// A graph as a Mermaid flowchart: nodes `(id, shape, label)` in order, edges `(from,
/// to, label)`.
pub fn render(nodes: &[(String, String, String)], edges: &[(String, String, Option<String>)]) -> String {
    let mut lines = vec!["flowchart TD".to_string()];
    for (nid, shape, label) in nodes {
        let t = esc(label);
        lines.push(match shape.as_str() {
            "diamond" => format!("  {nid}{{\"{t}\"}}"),
            "stadium" => format!("  {nid}([\"{t}\"])"),
            _ => format!("  {nid}[\"{t}\"]"),
        });
    }
    for (src, dst, edge) in edges {
        lines.push(match edge.as_deref().filter(|e| !e.is_empty()) {
            Some(e) => format!("  {src} -->|{}| {dst}", esc(e)),
            None => format!("  {src} --> {dst}"),
        });
    }
    lines.join("\n")
}

type Edges = Vec<(String, Option<String>)>;

/// Builds one control-flow graph.
struct Builder {
    nodes: Vec<(String, String, String)>,
    edges: Vec<(String, String, Option<String>)>,
    loops: Vec<Edges>,
    labels: HashMap<String, String>,
    gotos: Vec<(String, String)>,
    start: String,
    end: String,
}

fn e(id: &str, label: Option<&str>) -> (String, Option<String>) {
    (id.to_string(), label.map(String::from))
}

impl Builder {
    fn new() -> Self {
        let mut b = Builder {
            nodes: Vec::new(),
            edges: Vec::new(),
            loops: Vec::new(),
            labels: HashMap::new(),
            gotos: Vec::new(),
            start: String::new(),
            end: String::new(),
        };
        b.start = b.add("stadium", "Start".into());
        b.end = b.add("stadium", "End".into());
        b
    }

    fn add(&mut self, shape: &str, label: String) -> String {
        let nid = format!("n{}", self.nodes.len() + 1);
        self.nodes.push((nid.clone(), shape.to_string(), label));
        nid
    }

    fn link(&mut self, cur: &Edges, to: &str) {
        self.edges.extend(cur.iter().map(|(src, label)| (src.clone(), to.to_string(), label.clone())));
    }

    fn lp(&mut self, test: String, body: &Node, cur: &Edges, yes: &str, no: &str) -> Edges {
        let cond = self.add("diamond", test);
        self.link(cur, &cond);
        self.loops.push(Vec::new());
        let exits = self.block(body, vec![e(&cond, Some(yes))]);
        self.link(&exits, &cond);
        let breaks = self.loops.pop().unwrap_or_default();
        let mut out = vec![e(&cond, Some(no))];
        out.extend(breaks);
        out
    }

    fn flush(&mut self, buf: &mut Vec<String>, cur: &mut Edges) {
        if !buf.is_empty() {
            let nid = self.add("rect", buf.join("\n"));
            self.link(cur, &nid);
            *cur = vec![e(&nid, None)];
            buf.clear();
        }
    }

    fn block(&mut self, block: &Node, mut cur: Edges) -> Edges {
        let mut buf: Vec<String> = Vec::new();
        for st in &block.children {
            if cur.is_empty() && st.kind != "Label" {
                break; // unreachable
            }
            if !CONTROL.contains(&st.kind) {
                buf.push(cut(&expr_text(st), LABEL_MAX));
                continue;
            }
            self.flush(&mut buf, &mut cur);
            cur = self.statement(st, cur);
        }
        self.flush(&mut buf, &mut cur);
        cur
    }

    fn statement(&mut self, st: &Node, mut cur: Edges) -> Edges {
        let kids = &st.children;
        match st.kind {
            "If" => {
                let mut out: Edges = Vec::new();
                for part in kids {
                    if part.kind == "Else" {
                        out.extend(self.block(&part.children[0], cur));
                        cur = Vec::new();
                        break;
                    }
                    let cond = self.add("diamond", format!("if {}", cut(&expr_text(&part.children[0]), LABEL_MAX)));
                    self.link(&cur, &cond);
                    out.extend(self.block(&part.children[1], vec![e(&cond, Some("yes"))]));
                    cur = vec![e(&cond, Some("no"))];
                }
                out.extend(cur);
                out
            }
            "While" => self.lp(format!("while {}", cut(&expr_text(&kids[0]), LABEL_MAX)), &kids[1], &cur, "true", "false"),
            "ForNum" => {
                let rng: Vec<String> = kids[1..kids.len() - 1].iter().map(expr_text).collect();
                let test = format!("for {} = {}", shown(&kids[0]), cut(&rng.join(", "), LABEL_MAX));
                self.lp(test, &kids[kids.len() - 1], &cur, "next", "done")
            }
            "ForIn" => {
                let names: Vec<&str> = kids[0].children.iter().map(or_empty).collect();
                let its = cut(&args(&kids[1].children), LABEL_MAX);
                self.lp(format!("for {} in {its}", names.join(", ")), &kids[2], &cur, "next", "done")
            }
            "Repeat" => {
                let top = self.add("rect", "repeat".into());
                self.link(&cur, &top);
                self.loops.push(Vec::new());
                let body = self.block(&kids[0], vec![e(&top, None)]);
                let cond = self.add("diamond", format!("until {}", cut(&expr_text(&kids[1]), LABEL_MAX)));
                self.link(&body, &cond);
                self.edges.push((cond.clone(), top, Some("false".into())));
                let mut out = vec![e(&cond, Some("true"))];
                out.extend(self.loops.pop().unwrap_or_default());
                out
            }
            "Do" => self.block(&kids[0], cur),
            "Return" => {
                let vals = args(&kids[0].children);
                let text = if vals.is_empty() { "return".to_string() } else { format!("return {vals}") };
                let nid = self.add("rect", cut(&text, LABEL_MAX));
                self.link(&cur, &nid);
                self.edges.push((nid, self.end.clone(), None));
                Vec::new()
            }
            "Break" => {
                let nid = self.add("rect", "break".into());
                self.link(&cur, &nid);
                if let Some(top) = self.loops.last_mut() {
                    top.push(e(&nid, None));
                }
                Vec::new()
            }
            "Goto" => {
                let nid = self.add("rect", format!("goto {}", shown(st)));
                self.link(&cur, &nid);
                self.gotos.push((nid, or_empty(st).to_string()));
                Vec::new()
            }
            _ => {
                // Label
                let nid = self.add("rect", format!("::{}::", shown(st)));
                self.link(&cur, &nid);
                self.labels.insert(or_empty(st).to_string(), nid.clone());
                vec![e(&nid, None)]
            }
        }
    }

    fn build(mut self, body: &Node) -> String {
        let start = vec![e(&self.start.clone(), None)];
        let exits = self.block(body, start);
        let end = self.end.clone();
        self.link(&exits, &end);
        for (src, label) in std::mem::take(&mut self.gotos) {
            if let Some(dst) = self.labels.get(&label) {
                self.edges.push((src, dst.clone(), None));
            }
        }
        render(&self.nodes, &self.edges)
    }
}

/// The control flow of a function (`Function`, `LocalFunction`, `FunctionStat`), or of a
/// whole chunk (`Block`), as a Mermaid flowchart.
pub fn flowchart(node: &Node) -> String {
    let node = if matches!(node.kind, "LocalFunction" | "FunctionStat") { &node.children[0] } else { node };
    let body = if node.kind == "Function" { &node.children[1] } else { node };
    Builder::new().build(body)
}

/// The node for code at the top level of the chunk.
pub const MAIN: &str = "(main chunk)";

fn key(name: &str) -> String {
    name.replace(':', ".")
}

fn id_of(n: &Node) -> usize {
    n as *const Node as usize
}

fn dotted0(n: &Node) -> Option<String> {
    dotted(n, &HashMap::new()).filter(|s| !s.is_empty())
}

/// Finds the named functions and the entry points.
#[derive(Default)]
struct Collector<'a> {
    functions: Vec<(String, &'a Node)>,
    names: HashMap<usize, String>,
    entries: Vec<String>,
    aliases: Vec<(String, &'a Node)>,
}

impl<'a> Collector<'a> {
    fn name(&mut self, f: &'a Node, name: &str) {
        self.names.insert(id_of(f), name.to_string());
        let k = key(name);
        if !self.functions.iter().any(|(x, _)| *x == k) {
            self.functions.push((k, f));
        }
    }

    fn chunk(&mut self, chunk: &'a Node) {
        for stmt in &chunk.children {
            if stmt.kind == "Return"
                && let Some(exported) = stmt.children.first().and_then(|r| r.children.first())
                && exported.kind == "Table"
            {
                self.table(exported, Some(""), true);
                continue;
            }
            self.visit(stmt);
        }
    }

    fn visit(&mut self, node: &'a Node) {
        let kind = node.kind;
        if matches!(kind, "LocalFunction" | "FunctionStat")
            && let Some(v) = node.value.as_deref().filter(|v| !v.is_empty())
        {
            self.name(&node.children[0], v);
        } else if matches!(kind, "Local" | "Assign") && node.children.len() == 2 {
            let (targets, exprs) = (&node.children[0], &node.children[1]);
            let mut tables: HashSet<usize> = HashSet::new();
            for (target, expr) in targets.children.iter().zip(&exprs.children) {
                let name = if kind == "Local" { target.value.clone() } else { dotted0(target) };
                let Some(name) = name.filter(|n| !n.is_empty()) else { continue };
                if expr.kind == "Function" {
                    self.name(expr, &name);
                } else if expr.kind == "Table" {
                    self.table(expr, Some(&name), false);
                    tables.insert(id_of(expr));
                }
            }
            for child in targets.children.iter().chain(&exprs.children) {
                if !tables.contains(&id_of(child)) {
                    self.visit(child); // table() has been through those
                }
            }
            return;
        }
        if kind == "Table" {
            self.table(node, None, false);
            return;
        }
        for child in &node.children {
            self.visit(child);
        }
    }

    fn table(&mut self, table: &'a Node, path: Option<&str>, exported: bool) {
        for item in &table.children {
            let Some(value) = item.children.last() else { continue };
            let k = if item.kind == "Field" { item.children.first() } else { None };
            let Some(kv) = k.filter(|k| k.kind == "String").and_then(|k| k.value.as_deref()).filter(|v| !v.is_empty())
            else {
                self.visit(value);
                continue;
            };
            let name = match path {
                Some(p) if !p.is_empty() => format!("{p}.{kv}"),
                _ => kv.to_string(),
            };
            if value.kind == "Function" {
                if !self.names.contains_key(&id_of(value)) {
                    self.name(value, &name);
                }
                if exported {
                    self.entries.push(name);
                }
                self.visit(value);
            } else if value.kind == "Table" {
                let sub = if path.is_some() { Some(name.as_str()) } else { None };
                self.table(value, sub, exported);
            } else {
                if exported && dotted0(value).is_some() {
                    self.aliases.push((name, value));
                }
                self.visit(value);
            }
        }
    }
}

/// A script's functions (key -> line), its entry points and the calls between them.
pub struct CallGraph {
    pub functions: Vec<(String, usize)>,
    pub entries: Vec<String>,
    pub calls: Vec<(String, String)>,
}

struct Calls<'c, 'a> {
    c: &'c Collector<'a>,
    calls: Vec<(String, String)>,
    seen: HashSet<(String, String)>,
}

impl Calls<'_, '_> {
    fn add(&mut self, a: String, b: String) {
        if self.seen.insert((a.clone(), b.clone())) {
            self.calls.push((a, b));
        }
    }

    fn has(&self, k: &str) -> bool {
        self.c.functions.iter().any(|(x, _)| x == k)
    }

    fn resolve(&self, node: &Node) -> Option<String> {
        if node.kind == "Call" {
            let k = key(&dotted0(&node.children[0])?);
            return self.has(&k).then_some(k);
        }
        let base = dotted0(&node.children[0]);
        let method = or_empty(node);
        if let Some(b) = base.filter(|b| b != "self") {
            let k = format!("{b}.{method}");
            return self.has(&k).then_some(k);
        }
        let matches: Vec<&String> = self
            .c
            .functions
            .iter()
            .map(|(k, _)| k)
            .filter(|k| k.contains('.') && k.rsplit('.').next() == Some(method))
            .collect();
        if matches.len() == 1 { Some(matches[0].clone()) } else { None }
    }

    fn walk(&mut self, caller: &str, node: &Node) {
        let mut stack: Vec<&Node> = node.children.iter().rev().collect();
        while let Some(n) = stack.pop() {
            if n.kind == "Function" && self.c.names.contains_key(&id_of(n)) {
                continue;
            }
            if matches!(n.kind, "Call" | "Method")
                && let Some(callee) = self.resolve(n)
            {
                self.add(caller.to_string(), callee);
            }
            stack.extend(n.children.iter().rev());
        }
    }
}

/// The functions a script defines and the calls between them.
pub fn call_graph(chunk: &Node) -> CallGraph {
    let mut collected = Collector::default();
    collected.chunk(chunk);
    let mut calls = Calls { c: &collected, calls: Vec::new(), seen: HashSet::new() };
    calls.walk(MAIN, chunk);
    for (k, f) in &collected.functions {
        calls.walk(k, f);
    }
    let mut entries: Vec<String> = Vec::new();
    for name in &collected.entries {
        if !entries.contains(name) {
            entries.push(name.clone());
        }
    }
    for (name, value) in &collected.aliases {
        if let Some(target) = dotted0(value).map(|t| key(&t))
            && calls.has(&target)
        {
            entries.push(name.clone());
            calls.add(name.clone(), target);
        }
    }
    CallGraph {
        functions: collected.functions.iter().map(|(k, f)| (k.clone(), f.line)).collect(),
        entries,
        calls: calls.calls,
    }
}

/// A script's call graph as a Mermaid flowchart (left to right).
pub fn call_graph_chart(chunk: &Node) -> String {
    let graph = call_graph(chunk);
    let mut ids: HashMap<String, String> = HashMap::new();
    let mut nodes: Vec<(String, String, String)> = Vec::new();
    let mut node_id = |name: &str| -> String {
        if let Some(id) = ids.get(name) {
            return id.clone();
        }
        let id = format!("n{}", ids.len());
        ids.insert(name.to_string(), id.clone());
        let label = match graph.functions.iter().find(|(k, _)| k == name) {
            Some((_, line)) => format!("{name}\nline {line}"),
            None => name.to_string(),
        };
        let shape = if name == MAIN || graph.entries.iter().any(|e| e == name) { "stadium" } else { "rect" };
        nodes.push((id.clone(), shape.into(), label));
        id
    };
    if graph.calls.iter().any(|(a, _)| a == MAIN) {
        node_id(MAIN);
    }
    for name in &graph.entries {
        node_id(name);
    }
    for (name, _) in &graph.functions {
        node_id(name);
    }
    let edges: Vec<(String, String, Option<String>)> =
        graph.calls.iter().map(|(a, b)| (node_id(a), node_id(b), None)).collect();
    render(&nodes, &edges).replacen("flowchart TD", "flowchart LR", 1)
}

#[cfg(test)]
mod tests {
    use super::super::parser::parse;
    use super::*;

    #[test]
    fn flowchart_branches_and_loops() {
        let src = "local x = 1\nif a then return 1 elseif b then x = 2 else x = 3 end\nwhile x do break end\nreturn x";
        let chart = flowchart(&parse(src, false).unwrap());
        assert!(chart.starts_with("flowchart TD\n  n1([\"Start\"])\n  n2([\"End\"])\n  n3[\"local x = 1\"]"));
        assert!(chart.contains("n4{\"if a\"}"));
        assert!(chart.contains("-->|yes|"));
        assert!(chart.contains("{\"while x\"}"));
    }

    #[test]
    fn call_graph_of_a_script() {
        let src = "local function helper() end\nlocal function tick() helper() end\nlocal M = {}\nfunction M.go() \
                   tick() end\nreturn { engineHandlers = { onUpdate = tick, onInit = function() M.go() end } }";
        let tree = parse(src, false).unwrap();
        let g = call_graph(&tree);
        let keys: Vec<&str> = g.functions.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, ["helper", "tick", "M.go", "engineHandlers.onInit"]);
        assert!(g.calls.contains(&("tick".into(), "helper".into())));
        assert!(g.calls.contains(&("engineHandlers.onUpdate".into(), "tick".into())));
        assert!(call_graph_chart(&tree).starts_with("flowchart LR"));
    }
}
