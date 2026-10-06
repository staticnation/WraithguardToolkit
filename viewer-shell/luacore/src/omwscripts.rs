//! The `.omwscripts` format (was `wraithguard/lua/omwscripts.py`, which stays as the
//! fallback and the reference this is held to - problems and their wording included).
//!
//! One script a line, `<flags>: <path in the virtual file system>`; `#` starts a comment
//! line; blank lines are skipped. Flags are comma- (or space-) separated.

use super::lexer::py_repr;

/// Flags OpenMW 0.51 documents. Anything else is reported, not dropped.
pub const KNOWN_FLAGS: [&str; 22] = [
    "GLOBAL", "MENU", "CUSTOM", "PLAYER", "LOAD", "ACTIVATOR", "APPARATUS", "ARMOR", "BOOK", "CLOTHING",
    "CONTAINER", "CREATURE", "DOOR", "INGREDIENT", "LIGHT", "LOCKPICK", "MISC_ITEM", "NPC", "POTION", "PROBE",
    "REPAIR", "WEAPON",
];

/// One registration: the script's path as written, its flags (upper case, in the order
/// written), the file it came from and its 1-based line (0 for a LUAL record).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub path: String,
    pub flags: Vec<String>,
    pub source: String,
    pub line: usize,
}

/// A parsed `.omwscripts` file (or a content file's LUAL records).
#[derive(Clone, Debug, Default)]
pub struct OmwScripts {
    pub name: String,
    pub entries: Vec<Entry>,
    pub problems: Vec<(usize, String)>,
}

/// Whitespace as Python's `str.isspace` sees it (Rust's, plus the four separators).
pub fn py_space(c: char) -> bool {
    c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
}

/// Python's `str.strip()`.
pub fn py_strip(s: &str) -> &str {
    s.trim_matches(py_space)
}

/// Python's `str.split()` (no argument).
pub fn py_split(s: &str) -> impl Iterator<Item = &str> {
    s.split(py_space).filter(|p| !p.is_empty())
}

/// Python's `str.splitlines()`: every line boundary Python knows, no empty last line.
pub fn py_splitlines(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut it = s.char_indices().peekable();
    while let Some((i, c)) = it.next() {
        let boundary = matches!(
            c,
            '\n' | '\r' | '\u{0b}' | '\u{0c}' | '\u{1c}' | '\u{1d}' | '\u{1e}' | '\u{85}' | '\u{2028}' | '\u{2029}'
        );
        if !boundary {
            continue;
        }
        out.push(&s[start..i]);
        let mut next = i + c.len_utf8();
        if c == '\r'
            && let Some(&(j, '\n')) = it.peek()
        {
            it.next();
            next = j + 1;
        }
        start = next;
    }
    if start < s.len() {
        out.push(&s[start..]);
    }
    out
}

/// A VFS path as OpenMW compares it: lower case, `/`, no leading slash.
pub fn normalize_vfs_path(path: &str) -> String {
    py_strip(path).replace('\\', "/").trim_start_matches('/').to_lowercase()
}

/// Parse the text of an `.omwscripts` file. A line whose flags include an unknown one is
/// kept (OpenMW versions add flags) and reported as a problem.
pub fn parse_omwscripts(text: &str, name: &str) -> OmwScripts {
    let mut out = OmwScripts { name: name.to_string(), ..Default::default() };
    for (i, raw) in py_splitlines(text).into_iter().enumerate() {
        let number = i + 1;
        let line = py_strip(raw).trim_start_matches('\u{feff}');
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (head, path) = match line.split_once(':') {
            Some((h, t)) => (h, py_strip(t)),
            None => (line, ""),
        };
        if !line.contains(':') || path.is_empty() {
            out.problems.push((number, format!("not 'FLAGS: path': {}", py_repr(line))));
            continue;
        }
        let head = head.replace(',', " ");
        let flags: Vec<String> = py_split(&head).map(|f| f.to_uppercase()).collect();
        if flags.is_empty() {
            out.problems.push((number, format!("no flags before the path: {}", py_repr(line))));
            continue;
        }
        let unknown: Vec<&str> = flags.iter().map(String::as_str).filter(|f| !KNOWN_FLAGS.contains(f)).collect();
        if !unknown.is_empty() {
            out.problems.push((number, format!("unknown flag(s): {}", unknown.join(", "))));
        }
        out.entries.push(Entry { path: path.to_string(), flags, source: name.to_string(), line: number });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_flags_and_problems() {
        let text = "\u{feff}# c\r\nGLOBAL: scripts/a.lua\r\n\r\nplayer npc: Scripts\\B.lua\nnope\nWEIRD, NPC: c.lua\n: d.lua\n";
        let got = parse_omwscripts(text, "m.omwscripts");
        let paths: Vec<&str> = got.entries.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(paths, ["scripts/a.lua", "Scripts\\B.lua", "c.lua"]);
        assert_eq!(got.entries[1].flags, ["PLAYER", "NPC"]);
        assert_eq!(got.entries[1].line, 4);
        assert_eq!(
            got.problems,
            [
                (5, "not 'FLAGS: path': 'nope'".to_string()),
                (6, "unknown flag(s): WEIRD".to_string()),
                (7, "no flags before the path: ': d.lua'".to_string()),
            ]
        );
        assert_eq!(normalize_vfs_path(" /Scripts\\B.lua "), "scripts/b.lua");
    }

    #[test]
    fn splitlines_as_python() {
        assert_eq!(py_splitlines("a\r\nb\rc\n\nd"), ["a", "b", "c", "", "d"]);
        assert_eq!(py_splitlines("a\n"), ["a"]);
        assert!(py_splitlines("").is_empty());
    }
}
