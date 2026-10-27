//! The program's own Markdown documents as HTML, for the viewer's Help page
//! (`wraithguard/viz/docs.py`, port step 4.4).
//!
//! A deliberately small Markdown subset: exactly what this project's own documents use - ATX
//! headings, fenced code, pipe tables, nested lists, block quotes, rules, links, images, bold,
//! italic, strikethrough and inline code. Not CommonMark and not trying to be: what it does
//! not recognise comes out as escaped text, so an unsupported construct is visibly plain
//! rather than silently mangled. Everything is escaped; raw HTML is not passed through, and a
//! link with a scheme other than http, https, mailto or `#` is rendered as its text.
//!
//! The Python's regular expressions are written out by hand here, each beside the pattern it
//! stands for, with the same leftmost-first, lazy behaviour.

use std::path::{Path, PathBuf};

/// The documents the Help offers, in menu order (`HELP_DOCUMENTS`), with their titles.
pub const DOCUMENTS: [(&str, &str); 3] =
    [("QUICKSTART.md", "Quick start"), ("README.md", "Read me"), ("MLOX_RULES.md", "Writing mlox rules")];

/// One rendered document: the body HTML and its headings as `(level, text, id)`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Rendered {
    pub body: String,
    pub headings: Vec<(usize, String, String)>,
}

/// `html.escape(text, quote=True)`.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#x27;"),
            c => out.push(c),
        }
    }
    out
}

/// A unique anchor id for a heading (`_slug`): lower case, runs of anything but `a-z0-9`
/// one `-`, `section` when nothing is left, `-2`, `-3` ... when it repeats.
fn slug(text: &str, used: &mut Vec<String>) -> String {
    let mut base = String::new();
    let mut dash = false;
    for c in text.to_lowercase().chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            base.push(c);
            dash = false;
        } else if !dash {
            base.push('-');
            dash = true;
        }
    }
    let base = base.trim_matches('-');
    let base = if base.is_empty() { "section" } else { base };
    let mut s = base.to_string();
    let mut n = 2;
    while used.contains(&s) {
        s = format!("{base}-{n}");
        n += 1;
    }
    used.push(s.clone());
    s
}

/// A link target vetted (`_safe_href`): the escaped href, or `None` for a scheme that is
/// not allowed.
fn safe_href(url: &str) -> Option<String> {
    let url = url.trim();
    let low = url.to_lowercase();
    if ["http://", "https://", "mailto:", "#"].iter().any(|s| low.starts_with(s)) {
        return Some(escape(url));
    }
    if low.split('/').next().unwrap_or("").contains(':') {
        return None;
    }
    Some(escape(url))
}

fn s(chars: &[char]) -> String {
    chars.iter().collect()
}

fn starts(chars: &[char], at: usize, pat: &str) -> bool {
    (at..).zip(pat.chars()).all(|(i, p)| chars.get(i) == Some(&p))
}

/// The first `pat` at or after `from`.
fn find(chars: &[char], from: usize, pat: &str) -> Option<usize> {
    (from..chars.len()).find(|&i| starts(chars, i, pat))
}

fn word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// One inline construct at `at`: its end and its HTML.
fn construct(t: &[char], at: usize) -> Option<(usize, String)> {
    let c = t[at];
    // (?P<code>`+)(?P<codetext>.+?)(?P=code): the longest run first, then shorter.
    if c == '`' {
        let mut run = 0;
        while t.get(at + run) == Some(&'`') {
            run += 1;
        }
        for n in (1..=run).rev() {
            let ticks: String = "`".repeat(n);
            if let Some(e) = find(t, at + n + 1, &ticks) {
                return Some((e + n, format!("<code>{}</code>", escape(&s(&t[at + n..e])))));
            }
        }
    }
    // !\[(?P<alt>[^\]]*)\]\((?P<src>[^)\s]+)\)  and  \[(?P<label>[^\]]+)\]\((?P<href>[^)\s]+)\)
    let image = c == '!' && t.get(at + 1) == Some(&'[');
    if image || c == '[' {
        let open = if image { at + 1 } else { at };
        let close = (open + 1..t.len()).find(|&i| t[i] == ']');
        if let Some(close) = close
            && (image || close > open + 1)
            && t.get(close + 1) == Some(&'(')
        {
            let mut e = close + 2;
            while e < t.len() && t[e] != ')' && !t[e].is_whitespace() {
                e += 1;
            }
            if e > close + 2 && t.get(e) == Some(&')') {
                let inner = s(&t[open + 1..close]);
                let href = safe_href(&s(&t[close + 2..e]));
                let html = if image {
                    let alt = escape(&inner);
                    match href {
                        Some(h) => format!("<img src=\"{h}\" alt=\"{alt}\">"),
                        None => alt,
                    }
                } else {
                    let label = inline(&inner);
                    match href {
                        Some(h) => format!("<a href=\"{h}\">{label}</a>"),
                        None => label,
                    }
                };
                return Some((e + 1, html));
            }
        }
    }
    // (?P<strike>~~)(?P<striketext>.+?)~~
    if starts(t, at, "~~")
        && let Some(e) = find(t, at + 3, "~~")
    {
        return Some((e + 2, format!("<del>{}</del>", inline(&s(&t[at + 2..e])))));
    }
    // (?P<bold>\*\*|__)(?P<boldtext>.+?)(?P=bold)
    for d in ["**", "__"] {
        if starts(t, at, d)
            && let Some(e) = find(t, at + 3, d)
        {
            return Some((e + 2, format!("<strong>{}</strong>", inline(&s(&t[at + 2..e])))));
        }
    }
    // (?<!\*)\*(?!\*)(?P<emstar>[^\s*](?:.*?[^\s*])?)\*(?!\*)
    // (?<![\w_])_(?!_)(?P<emund>[^\s_](?:.*?[^\s_])?)_(?!\w)
    let em = |mark: char, before_ok: bool, after_ok: &dyn Fn(Option<&char>) -> bool| -> Option<(usize, String)> {
        if c != mark || !before_ok {
            return None;
        }
        let first = *t.get(at + 1)?;
        if first.is_whitespace() || first == mark {
            return None;
        }
        for e in at + 2..t.len() {
            let last = t[e - 1];
            if t[e] == mark && !last.is_whitespace() && last != mark && after_ok(t.get(e + 1)) {
                return Some((e + 1, format!("<em>{}</em>", inline(&s(&t[at + 1..e])))));
            }
        }
        None
    };
    let before = if at == 0 { None } else { Some(t[at - 1]) };
    if let Some(found) = em('*', before != Some('*'), &|n| n != Some(&'*')) {
        return Some(found);
    }
    em('_', !before.is_some_and(word), &|n| !n.is_some_and(|c| word(*c)))
}

/// One line's inline markup (`inline`); everything outside a construct escaped.
pub fn inline(text: &str) -> String {
    let t: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut plain = 0;
    let mut i = 0;
    while i < t.len() {
        match construct(&t, i) {
            Some((end, html)) => {
                out.push_str(&escape(&s(&t[plain..i])));
                out.push_str(&html);
                i = end;
                plain = end;
            }
            None => i += 1,
        }
    }
    out.push_str(&escape(&s(&t[plain..])));
    out
}

/// `^(?P<level>#{1,6})\s+(?P<text>.*?)\s*#*\s*$`: the level and the text.
fn heading(line: &str) -> Option<(usize, String)> {
    let level = line.chars().take_while(|&c| c == '#').count();
    if !(1..=6).contains(&level) {
        return None;
    }
    let rest = &line[level..];
    if !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let text = rest.trim_end().trim_end_matches('#').trim_end().trim_start();
    Some((level, text.to_string()))
}

/// `^\s*(?P<ticks>`{3,}|~{3,})\s*(?P<lang>[\w+-]*)\s*$`: the fence.
fn fence(line: &str) -> Option<String> {
    let t = line.trim();
    let mark = t.chars().next().filter(|c| *c == '`' || *c == '~')?;
    let n = t.chars().take_while(|&c| c == mark).count();
    if n < 3 {
        return None;
    }
    let lang = t[n..].trim();
    if lang.chars().all(|c| word(c) || c == '+' || c == '-') {
        Some(t[..n].to_string())
    } else {
        None
    }
}

/// `^\s{0,3}([-*_])(?:\s*\1){2,}\s*$`.
fn rule(line: &str) -> bool {
    let lead = line.chars().take_while(|c| c.is_whitespace()).count();
    let rest: Vec<char> = line.chars().skip(lead).collect();
    let Some(&mark) = rest.first() else { return false };
    lead <= 3
        && matches!(mark, '-' | '*' | '_')
        && rest.iter().all(|&c| c == mark || c.is_whitespace())
        && rest.iter().filter(|&&c| c == mark).count() >= 3
}

/// `^(?P<indent>\s*)[-*+]\s+(?P<text>.*)$` or `^(?P<indent>\s*)\d+[.)]\s+(?P<text>.*)$`:
/// the indent and the text.
fn marker(line: &str, ordered: bool) -> Option<(usize, String)> {
    let indent = line.chars().take_while(|c| c.is_whitespace()).count();
    let rest: String = line.chars().skip(indent).collect();
    let after = if ordered {
        let digits = rest.chars().take_while(char::is_ascii_digit).count();
        if digits == 0 || !matches!(rest[digits..].chars().next(), Some('.' | ')')) {
            return None;
        }
        &rest[digits + 1..]
    } else {
        if !matches!(rest.chars().next(), Some('-' | '*' | '+')) {
            return None;
        }
        &rest[1..]
    };
    if !after.starts_with(char::is_whitespace) {
        return None;
    }
    Some((indent, after.trim_start().to_string()))
}

fn bullet(line: &str) -> Option<(usize, String)> {
    marker(line, false)
}

fn number(line: &str) -> Option<(usize, String)> {
    marker(line, true)
}

/// `^\s*\|.*\|\s*$`.
fn table_row(line: &str) -> bool {
    let t = line.trim();
    t.len() >= 2 && t.starts_with('|') && t.ends_with('|')
}

/// `^\s*\|(?:\s*:?-{2,}:?\s*\|)+\s*$`.
fn table_sep(line: &str) -> bool {
    let t = line.trim();
    if !table_row(line) {
        return false;
    }
    let cells: Vec<&str> = t[1..t.len() - 1].split('|').collect();
    cells.iter().all(|c| {
        let c = c.trim();
        let c = c.strip_prefix(':').unwrap_or(c);
        let c = c.strip_suffix(':').unwrap_or(c);
        c.len() >= 2 && c.chars().all(|x| x == '-')
    })
}

/// A pipe-table row's cells (`_split_row`); `\|` stays in a cell.
fn split_row(line: &str) -> Vec<String> {
    let t = line.trim().trim_matches('|');
    let mut cells = Vec::new();
    let mut cur = String::new();
    let mut prev = '\0';
    for c in t.chars() {
        if c == '|' && prev != '\\' {
            cells.push(std::mem::take(&mut cur));
        } else {
            cur.push(c);
        }
        prev = c;
    }
    cells.push(cur);
    cells.into_iter().map(|c| c.trim().replace("\\|", "|")).collect()
}

fn quoted(line: &str) -> bool {
    line.trim_start().starts_with('>')
}

/// One list item from its lines and already-rendered nested lists (`_item`).
fn item(parts: &[String]) -> String {
    let text: Vec<&str> = parts.iter().filter(|p| !p.starts_with('<')).map(String::as_str).collect();
    let nested: String = parts.iter().filter(|p| p.starts_with('<')).map(String::as_str).collect();
    format!("<li>{}{nested}</li>", inline(&text.join(" ")))
}

struct Renderer<'a> {
    lines: &'a [String],
    i: usize,
    slugs: Vec<String>,
    headings: Vec<(usize, String, String)>,
}

impl Renderer<'_> {
    fn render(&mut self) -> String {
        let mut out = String::new();
        while self.i < self.lines.len() {
            let line = &self.lines[self.i];
            if line.trim().is_empty() {
                self.i += 1;
                continue;
            }
            let html = if let Some(ticks) = fence(line) {
                self.code(&ticks)
            } else if rule(line) {
                self.i += 1;
                "<hr>".to_string()
            } else if let Some((level, text)) = heading(line) {
                self.i += 1;
                let plain: String = text.chars().filter(|c| !matches!(c, '`' | '*' | '_' | '~')).collect();
                let id = slug(&plain, &mut self.slugs);
                self.headings.push((level, plain, id.clone()));
                format!("<h{level} id=\"{id}\">{}</h{level}>", inline(&text))
            } else if quoted(line) {
                self.quote()
            } else if table_row(line) && self.table_start() {
                self.table()
            } else if bullet(line).is_some() || number(line).is_some() {
                self.list(0)
            } else {
                self.paragraph()
            };
            out.push_str(&html);
        }
        out
    }

    fn code(&mut self, ticks: &str) -> String {
        self.i += 1;
        let mut body = Vec::new();
        while self.i < self.lines.len() {
            let line = &self.lines[self.i];
            self.i += 1;
            if fence(line).is_some_and(|t| t.starts_with(ticks)) {
                break;
            }
            body.push(line.as_str());
        }
        format!("<pre><code>{}</code></pre>", escape(&body.join("\n")))
    }

    fn quote(&mut self) -> String {
        let mut body = Vec::new();
        while self.i < self.lines.len() && quoted(&self.lines[self.i]) {
            body.push(self.lines[self.i].trim_start()[1..].trim_start().to_string());
            self.i += 1;
        }
        let mut inner = Renderer { lines: &body, i: 0, slugs: Vec::new(), headings: Vec::new() };
        format!("<blockquote>{}</blockquote>", inner.render())
    }

    fn table_start(&self) -> bool {
        self.lines.get(self.i + 1).is_some_and(|l| table_sep(l))
    }

    fn table(&mut self) -> String {
        let header = split_row(&self.lines[self.i]);
        self.i += 2;
        let mut rows = Vec::new();
        while self.i < self.lines.len() && table_row(&self.lines[self.i]) {
            rows.push(split_row(&self.lines[self.i]));
            self.i += 1;
        }
        let head: String = header.iter().map(|c| format!("<th>{}</th>", inline(c))).collect();
        let body: String =
            rows.iter().map(|r| format!("<tr>{}</tr>", r.iter().map(|c| format!("<td>{}</td>", inline(c))).collect::<String>())).collect();
        format!("<table><thead><tr>{head}</tr></thead><tbody>{body}</tbody></table>")
    }

    fn continues_list(&self, indent: usize) -> bool {
        let line = &self.lines[self.i];
        bullet(line).or_else(|| number(line)).is_some_and(|(w, _)| w >= indent)
    }

    fn list(&mut self, indent: usize) -> String {
        let ordered = number(&self.lines[self.i]).is_some();
        let mut items = Vec::new();
        let mut current: Vec<String> = Vec::new();
        while self.i < self.lines.len() {
            let line = self.lines[self.i].clone();
            if line.trim().is_empty() {
                self.i += 1;
                if self.i < self.lines.len() && !self.continues_list(indent) {
                    break;
                }
                continue;
            }
            let (own, other) = if ordered { (number(&line), bullet(&line)) } else { (bullet(&line), number(&line)) };
            let Some((width, text)) = own.clone().or(other) else {
                let lead = line.chars().take_while(|c| c.is_whitespace()).count();
                if lead > indent && !current.is_empty() {
                    current.push(line.trim().to_string());
                    self.i += 1;
                    continue;
                }
                break;
            };
            if width > indent {
                let nested = self.list(width);
                if current.is_empty() {
                    items.push(nested);
                } else {
                    current.push(nested);
                }
                continue;
            }
            if width < indent || own.is_none() {
                break;
            }
            if !current.is_empty() {
                items.push(item(&current));
            }
            current = vec![text];
            self.i += 1;
        }
        if !current.is_empty() {
            items.push(item(&current));
        }
        let tag = if ordered { "ol" } else { "ul" };
        format!("<{tag}>{}</{tag}>", items.concat())
    }

    fn paragraph(&mut self) -> String {
        let mut body = Vec::new();
        while self.i < self.lines.len() {
            let line = &self.lines[self.i];
            if line.trim().is_empty() || heading(line).is_some() || rule(line) || fence(line).is_some() || quoted(line) {
                break;
            }
            if bullet(line).is_some() || number(line).is_some() || (table_row(line) && self.table_start()) {
                break;
            }
            body.push(line.trim());
            self.i += 1;
        }
        format!("<p>{}</p>", inline(&body.join(" ")))
    }
}

/// Markdown to HTML (`render_markdown`): the body and the headings found.
pub fn render(text: &str) -> Rendered {
    let lines: Vec<String> = text.replace("\r\n", "\n").replace('\r', "\n").split('\n').map(str::to_string).collect();
    let mut r = Renderer { lines: &lines, i: 0, slugs: Vec::new(), headings: Vec::new() };
    let body = r.render();
    Rendered { body, headings: r.headings }
}

/// Where a document is (`doc_path`): the first of `dirs` holding it.
pub fn locate(name: &str, dirs: &[PathBuf]) -> Option<PathBuf> {
    dirs.iter().map(|d| d.join(name)).find(|p| p.is_file())
}

/// The folders the documents may be in, for the viewer: `given` first (what Wraithguard
/// passes), then the program's own folder and the three above it (a build tree's
/// `target/release` sits three below the checkout).
pub fn search_dirs(given: &[String], exe: Option<&Path>) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = given.iter().filter(|g| !g.is_empty()).map(PathBuf::from).collect();
    if let Some(dir) = exe.and_then(Path::parent) {
        out.extend(dir.ancestors().take(4).map(Path::to_path_buf));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_markup() {
        assert_eq!(inline("a `**x**` b"), "a <code>**x**</code> b");
        assert_eq!(inline("**bold** and *it*"), "<strong>bold</strong> and <em>it</em>");
        assert_eq!(
            inline("*Export writes nothing while **Dry run** is checked*"),
            "<em>Export writes nothing while <strong>Dry run</strong> is checked</em>"
        );
        assert_eq!(inline("snake_case_name stays"), "snake_case_name stays");
        assert_eq!(inline("_under_ ~~gone~~"), "<em>under</em> <del>gone</del>");
        assert_eq!(inline("[site](https://x.org/a) [bad](javascript:alert(1))"), "<a href=\"https://x.org/a\">site</a> bad)");
        assert_eq!(inline("![pic](img/a.png) <b>"), "<img src=\"img/a.png\" alt=\"pic\"> &lt;b&gt;");
        assert_eq!(inline("a * b * c"), "a * b * c");
        assert_eq!(inline("``a`"), "<code>`a</code>");
        assert_eq!(inline("x**y**z [](x) ![](x)"), "x<strong>y</strong>z [](x) <img src=\"x\" alt=\"\">");
    }

    #[test]
    fn blocks() {
        let md = "# Title #\n\nSome *text*\nwrapped.\n\n## Notes\n\n- one\n- two\n  - nested\n- three\n\n1. a\n2. b\n\n> quoted **bold**\n\n---\n\n| A | B |\n|---|:--:|\n| 1 | x \\| y |\n\n```sh\n<run> me\n```\n\n## Notes\n";
        let r = render(md);
        assert_eq!(
            r.body,
            "<h1 id=\"title\">Title</h1><p>Some <em>text</em> wrapped.</p><h2 id=\"notes\">Notes</h2>\
             <ul><li>one</li><li>two<ul><li>nested</li></ul></li><li>three</li></ul><ol><li>a</li><li>b</li></ol>\
             <blockquote><p>quoted <strong>bold</strong></p></blockquote><hr>\
             <table><thead><tr><th>A</th><th>B</th></tr></thead><tbody><tr><td>1</td><td>x | y</td></tr></tbody></table>\
             <pre><code>&lt;run&gt; me</code></pre><h2 id=\"notes-2\">Notes</h2>"
        );
        assert_eq!(r.headings[2], (2, "Notes".to_string(), "notes-2".to_string()));
    }

    /// The documents rendered as the Python renders them: `WG_DOCS_REF` a folder of
    /// `<name>.html` from `render_markdown`, `WG_DOCS_SRC` the folder with the `.md`.
    #[test]
    #[ignore]
    fn matches_the_python_renderer() {
        let (Ok(refs), Ok(src)) = (std::env::var("WG_DOCS_REF"), std::env::var("WG_DOCS_SRC")) else { return };
        for entry in std::fs::read_dir(&refs).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_stem().unwrap().to_string_lossy().into_owned();
            let want = std::fs::read_to_string(&path).unwrap();
            let got = render(&std::fs::read_to_string(Path::new(&src).join(&name)).unwrap()).body;
            if got != want {
                let at = got.chars().zip(want.chars()).take_while(|(a, b)| a == b).count();
                let ctx = |t: &str| t.chars().skip(at.saturating_sub(80)).take(200).collect::<String>();
                panic!("{name} differs at {at}:\n got: {}\nwant: {}", ctx(&got), ctx(&want));
            }
        }
    }

    #[test]
    fn unterminated_fence_runs_to_the_end() {
        assert_eq!(render("```\na\nb").body, "<pre><code>a\nb</code></pre>");
    }
}
