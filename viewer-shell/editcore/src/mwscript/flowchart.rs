//! A Morrowind script's control flow as a Mermaid flowchart
//! (`wraithguard/mwscript/flowchart.py`, port step 4.1, slice 5d), drawn as the Lua tools
//! draw a function's (`luacore::mermaid::render`): runs of plain statements as boxes,
//! `if`/`elseif`/`else` and `while` as decisions, `return` ending the frame. A script runs
//! from `begin` to `end` every frame, so the chart's end is "the next frame".

use std::sync::OnceLock;

use regex::Regex;

/// Statements a box holds before the next one starts.
const RUN_MAX: usize = 6;
/// Characters of a line kept in a label.
const LABEL_MAX: usize = 60;

fn head() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"^\s*([A-Za-z_]+)\b(.*)$").expect("a valid pattern"))
}

/// Python's `str.splitlines()`.
pub fn splitlines(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut it = text.char_indices().peekable();
    while let Some((i, c)) = it.next() {
        if matches!(c, '\n' | '\r' | '\x0b' | '\x0c' | '\x1c' | '\x1d' | '\x1e' | '\u{85}' | '\u{2028}' | '\u{2029}') {
            out.push(&text[start..i]);
            let mut next = i + c.len_utf8();
            if c == '\r' && it.peek().is_some_and(|(_, d)| *d == '\n') {
                it.next();
                next += 1;
            }
            start = next;
        }
    }
    if start < text.len() {
        out.push(&text[start..]);
    }
    out
}

/// A line without its comment (`;` outside quotes), stripped.
fn code(line: &str) -> &str {
    let mut quoted = false;
    for (i, ch) in line.char_indices() {
        if ch == '"' {
            quoted = !quoted;
        } else if ch == ';' && !quoted {
            return line[..i].trim();
        }
    }
    line.trim()
}

/// An `if`/`while` line's condition, without its outer brackets.
fn cond(rest: &str) -> String {
    let text = rest.trim();
    if text.starts_with('(') && text.ends_with(')') && text.len() >= 2 {
        return text[1..text.len() - 1].trim().to_string();
    }
    text.to_string()
}

/// A label cut to the chart's width, with an ellipsis.
fn cut(text: &str) -> String {
    if text.chars().count() <= LABEL_MAX { text.to_string() } else { text.chars().take(LABEL_MAX - 1).collect::<String>() + "…" }
}

enum ItemKind {
    Stmt(String),
    If(Vec<(String, Block)>, Option<Block>),
    While(String, Block),
    Return,
}

#[derive(Default)]
struct Block {
    items: Vec<ItemKind>,
}

/// Where the parser is: a path of item indices from the root to the open block.
enum Open {
    /// The arm (index) of the if at this item, or its else (`None`).
    If(usize, Option<usize>),
    While(usize),
}

fn block_at<'a>(root: &'a mut Block, path: &[Open]) -> &'a mut Block {
    let mut cur = root;
    for p in path {
        cur = match p {
            Open::If(at, arm) => match &mut cur.items[*at] {
                ItemKind::If(arms, otherwise) => match arm {
                    Some(a) => &mut arms[*a].1,
                    None => otherwise.as_mut().expect("an open else"),
                },
                _ => unreachable!("an if is open here"),
            },
            Open::While(at) => match &mut cur.items[*at] {
                ItemKind::While(_, body) => body,
                _ => unreachable!("a while is open here"),
            },
        };
    }
    cur
}

/// The script's name and its body as nested blocks; a block never closed is closed at
/// the end.
fn parse(text: &str) -> (String, Block) {
    let mut name = String::new();
    let mut root = Block::default();
    let mut path: Vec<Open> = Vec::new();
    for raw in splitlines(text) {
        let line = code(raw);
        if line.is_empty() {
            continue;
        }
        let m = head().captures(line);
        let word = m.as_ref().map_or(String::new(), |m| m[1].to_lowercase());
        let rest = m.as_ref().map_or("", |m| m.get(2).map_or("", |x| x.as_str()));
        let in_if = matches!(path.last(), Some(Open::If(..)));
        let in_while = matches!(path.last(), Some(Open::While(_)));
        match word.as_str() {
            "begin" if name.is_empty() => name = rest.trim().to_string(),
            "end" => break,
            "if" => {
                let cur = block_at(&mut root, &path);
                cur.items.push(ItemKind::If(vec![(cond(rest), Block::default())], None));
                path.push(Open::If(cur.items.len() - 1, Some(0)));
            }
            "elseif" | "else" if in_if => {
                let Some(Open::If(at, _)) = path.pop() else { unreachable!() };
                let parent = block_at(&mut root, &path);
                let ItemKind::If(arms, otherwise) = &mut parent.items[at] else { unreachable!() };
                if word == "elseif" {
                    arms.push((cond(rest), Block::default()));
                    path.push(Open::If(at, Some(arms.len() - 1)));
                } else {
                    *otherwise = Some(Block::default());
                    path.push(Open::If(at, None));
                }
            }
            "endif" if in_if => {
                path.pop();
            }
            "while" => {
                let cur = block_at(&mut root, &path);
                cur.items.push(ItemKind::While(cond(rest), Block::default()));
                path.push(Open::While(cur.items.len() - 1));
            }
            "endwhile" if in_while => {
                path.pop();
            }
            "return" => block_at(&mut root, &path).items.push(ItemKind::Return),
            "short" | "long" | "float" => {}
            _ => block_at(&mut root, &path).items.push(ItemKind::Stmt(line.to_string())),
        }
    }
    (name, root)
}

type Exits = Vec<(String, Option<String>)>;

#[derive(Default)]
struct Graph {
    nodes: Vec<(String, String, String)>,
    edges: Vec<(String, String, Option<String>)>,
}

impl Graph {
    fn node(&mut self, shape: &str, label: &str) -> String {
        let id = format!("n{}", self.nodes.len() + 1);
        self.nodes.push((id.clone(), shape.into(), label.into()));
        id
    }

    fn link(&mut self, exits: &Exits, dst: &str) {
        for (src, label) in exits {
            self.edges.push((src.clone(), dst.to_string(), label.clone()));
        }
    }

    /// Draws a block after `exits`: the block's own exits (none when it always returns).
    fn block(&mut self, blk: &Block, mut exits: Exits, end: &str) -> Exits {
        let mut run: Vec<&str> = Vec::new();
        let flush = |g: &mut Graph, run: &mut Vec<&str>, exits: &mut Exits| {
            for chunk in run.chunks(RUN_MAX) {
                let label = chunk.iter().map(|s| cut(s)).collect::<Vec<_>>().join("\n");
                let id = g.node("box", &label);
                g.link(exits, &id);
                *exits = vec![(id, None)];
            }
            run.clear();
        };
        for item in &blk.items {
            match item {
                ItemKind::Stmt(s) => run.push(s),
                ItemKind::Return => {
                    flush(self, &mut run, &mut exits);
                    let id = self.node("stadium", "return");
                    self.link(&exits, &id);
                    self.edges.push((id, end.to_string(), None));
                    return Vec::new();
                }
                ItemKind::While(c, body) => {
                    flush(self, &mut run, &mut exits);
                    let test = self.node("diamond", &format!("while {}", cut(c)));
                    self.link(&exits, &test);
                    let back = self.block(body, vec![(test.clone(), Some("yes".into()))], end);
                    self.link(&back, &test);
                    exits = vec![(test, Some("no".into()))];
                }
                ItemKind::If(arms, otherwise) => {
                    flush(self, &mut run, &mut exits);
                    let mut out: Exits = Vec::new();
                    let mut come = exits.clone();
                    for (i, (c, body)) in arms.iter().enumerate() {
                        let test = self.node("diamond", &format!("{}{}", if i == 0 { "if " } else { "elseif " }, cut(c)));
                        self.link(&come, &test);
                        out.extend(self.block(body, vec![(test.clone(), Some("yes".into()))], end));
                        come = vec![(test, Some("no".into()))];
                    }
                    match otherwise {
                        Some(b) => out.extend(self.block(b, come, end)),
                        None => out.extend(come),
                    }
                    exits = out;
                }
            }
        }
        flush(self, &mut run, &mut exits);
        exits
    }
}

/// The Mermaid flowchart of a Morrowind script's source.
pub fn flowchart(text: &str) -> String {
    let (name, root) = parse(text);
    let mut g = Graph::default();
    let start = g.node("stadium", format!("begin {name}").trim());
    let end = g.node("stadium", "end (next frame)");
    let exits = g.block(&root, vec![(start, None)], &end);
    g.link(&exits, &end);
    luacore::mermaid::render(&g.nodes, &g.edges)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_split_as_python_splits_them() {
        assert_eq!(splitlines("a\r\nb\rc\nd\n"), ["a", "b", "c", "d"]);
        assert_eq!(splitlines(""), Vec::<&str>::new());
        assert_eq!(splitlines("x\n\ny"), ["x", "", "y"]);
    }

    #[test]
    fn a_script_charts() {
        let chart = flowchart("Begin t\nshort x\nif ( x == 1 )\n  set x to 2\nelseif ( x == 2 )\n  return\nelse\n  set x to 3\nendif\nwhile ( x < 5 )\n set x to x + 1\nendwhile\nEnd t");
        assert!(chart.contains("begin t"), "{chart}");
        assert!(chart.contains("if x == 1"), "{chart}");
    }
}
