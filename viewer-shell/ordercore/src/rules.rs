//! mlox rule files: the ordering blocks in their text (`[Order]`, `[NearStart]` and
//! `[NearEnd]`), read as `wraithguard/rules/parser.py` reads them.
//!
//! Only the ordering keywords are extracted; the predicate keywords (`[Requires]`,
//! `[Conflict]`, `[Note]`) end a block and are read elsewhere. Two things carry weight, and
//! both were bugs before they were rules:
//!
//! - an `[Order]` body is a chain ("A before B before C"); the `[NearStart]` and
//!   `[NearEnd]` bodies are hints ("as near the start as the constraints allow") and not
//!   chains;
//! - blocks are kept whole, so that an uninstalled plugin in the middle of a chain can be
//!   bridged over later instead of losing the constraint.
//!
//! The Python reader is the reference and `tests/fixtures/rule_cases.json` pins both: what
//! is here is its regular expressions written out by hand (this crate takes no
//! dependencies), with Python's idea of a line break and of whitespace, which are not
//! Rust's.
//!
//! One difference is known and left: the keyword and the extensions are matched
//! case-insensitively as ASCII, where Python's `re.IGNORECASE` also folds a few non-ASCII
//! letters onto them (`ſ` for `s`). No rule file spells a keyword that way.
//!
//! Written from mlox's and plox's documented behaviour, not their code.

/// Which ordering keyword a block was under.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// `[Order]`: a chain.
    Order,
    /// `[NearStart]`: hints.
    NearStart,
    /// `[NearEnd]`: hints.
    NearEnd,
}

impl Kind {
    /// The keyword in lower case, as the Python reader and the case files spell it.
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Order => "order",
            Kind::NearStart => "nearstart",
            Kind::NearEnd => "nearend",
        }
    }
}

/// One ordering block: its keyword and the plugin names (patterns) in its body, in file
/// order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    /// The keyword.
    pub kind: Kind,
    /// The names, as written (case, `*`, `?` and `<VER>` untouched).
    pub names: Vec<String>,
}

/// What reading a rule file's text gave.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Parsed {
    /// The ordering blocks with at least one name, in file order.
    pub blocks: Vec<Block>,
    /// How many body lines named no plugin (or were a bracketed qualifier): bridged over
    /// as phantom names, as mlox does, and counted so the caller can say so.
    pub skipped: usize,
}

/// Every keyword a rule header can carry, in the order the Python expression lists them.
const KEYWORDS: [&str; 8] =
    ["Order", "NearStart", "NearEnd", "Requires", "Conflict", "Patch", "Note", "Version"];

/// The plugin extensions a name must end in.
const EXTENSIONS: [&str; 5] = ["esp", "esm", "omwaddon", "omwgame", "omwscripts"];

/// Python's `str.isspace` (and so `\s` and `str.strip` in a `str` pattern). It is Rust's
/// `char::is_whitespace` plus the four information separators U+001C to U+001F.
pub(crate) fn is_space(c: char) -> bool {
    c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
}

/// Python's `\w`: a letter or digit of any script, or an underscore.
pub(crate) fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Python's `str.splitlines()`: on `\n`, `\r`, `\r\n`, and the vertical tab, form feed,
/// U+001C to U+001E, NEL (U+0085) and the two Unicode separators; no empty last piece.
pub(crate) fn splitlines(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut chars = s.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        let breaks = matches!(
            c,
            '\n' | '\r'
                | '\u{b}'
                | '\u{c}'
                | '\u{1c}'
                | '\u{1d}'
                | '\u{1e}'
                | '\u{85}'
                | '\u{2028}'
                | '\u{2029}'
        );
        if !breaks {
            continue;
        }
        out.push(&s[start..i]);
        let mut next = i + c.len_utf8();
        if c == '\r' {
            if let Some(&(j, '\n')) = chars.peek() {
                chars.next();
                next = j + 1;
            }
        }
        start = next;
    }
    if start < s.len() {
        out.push(&s[start..]);
    }
    out
}

/// `strip_comment`: the line up to the first `;`.
pub(crate) fn strip_comment(line: &str) -> &str {
    line.find(';').map_or(line, |i| &line[..i])
}

/// A rule header at the start of `s` (`^\[(Order|...)\b([^\]\n]*)\]`, any case): which
/// keyword, and how many bytes the header takes, through its `]`.
fn header(s: &str) -> Option<(usize, usize)> {
    let b = s.as_bytes();
    if b.first() != Some(&b'[') {
        return None;
    }
    for (k, kw) in KEYWORDS.iter().enumerate() {
        let end = 1 + kw.len();
        if b.len() < end || !b[1..end].eq_ignore_ascii_case(kw.as_bytes()) {
            continue;
        }
        // `\b`: the keyword's last letter is a word character, so the next must not be.
        if s[end..].chars().next().is_some_and(is_word) {
            continue;
        }
        for (i, c) in s[end..].char_indices() {
            match c {
                '\n' => return None,
                ']' => return Some((k, end + i + 1)),
                _ => {}
            }
        }
        return None;
    }
    None
}

/// The plugin names on one line: `\S[^\n]*?\.(?:esp|esm|omwaddon|omwgame|omwscripts)\*?(?=\s|$)`
/// in any case, found left to right without overlap. The name is as short as it can be
/// while the lookahead holds, so `A.esp.esp` is one name and `B.esp x.esp` two.
fn names_in(line: &str) -> Vec<&str> {
    let chars: Vec<(usize, char)> = line.char_indices().collect();
    let n = chars.len();
    let byte_at = |i: usize| if i < n { chars[i].0 } else { line.len() };
    let boundary = |i: usize| i >= n || is_space(chars[i].1);
    let ext_at = |i: usize, ext: &str| {
        i + ext.len() <= n
            && ext
                .bytes()
                .zip(&chars[i..])
                .all(|(e, &(_, c))| c.is_ascii() && (c as u8).eq_ignore_ascii_case(&e))
    };
    let mut out = Vec::new();
    let mut p = 0;
    'scan: while p < n {
        if is_space(chars[p].1) {
            p += 1;
            continue;
        }
        for d in p + 1..n {
            if chars[d].1 != '.' {
                continue;
            }
            for ext in EXTENSIONS {
                if !ext_at(d + 1, ext) {
                    continue;
                }
                let e = d + 1 + ext.len();
                // `\*?` is greedy: with the star first, then without.
                let end = if e < n && chars[e].1 == '*' && boundary(e + 1) {
                    Some(e + 1)
                } else if boundary(e) {
                    Some(e)
                } else {
                    None
                };
                if let Some(end) = end {
                    out.push(&line[chars[p].0..byte_at(end)]);
                    p = end;
                    continue 'scan;
                }
            }
        }
        p += 1;
    }
    out
}

/// Read the ordering blocks out of a rule file's text (its byte-order mark already gone).
///
/// `wraithguard.rules.parser.parse_mlox_text` is the reference.
pub fn parse_text(raw: &str) -> Parsed {
    let text = splitlines(raw).into_iter().map(strip_comment).collect::<Vec<_>>().join("\n");
    // Where each header starts and ends: only a line's first byte can start one.
    let mut found: Vec<(usize, usize, usize)> = Vec::new();
    let mut at = 0;
    for line in text.split('\n') {
        if let Some((k, len)) = header(&text[at..]) {
            found.push((k, at, at + len));
        }
        at += line.len() + 1;
    }
    let mut blocks = Vec::new();
    let mut skipped = 0;
    for (i, &(k, _start, end)) in found.iter().enumerate() {
        let kind = match k {
            0 => Kind::Order,
            1 => Kind::NearStart,
            2 => Kind::NearEnd,
            _ => continue,
        };
        let stop = found.get(i + 1).map_or(text.len(), |next| next.1);
        let mut names: Vec<String> = Vec::new();
        for raw_line in splitlines(&text[end..stop]) {
            let line = raw_line.trim_matches(is_space);
            if line.is_empty() {
                continue;
            }
            if line.starts_with('[') {
                skipped += 1;
                continue;
            }
            let got = names_in(line);
            if got.is_empty() {
                skipped += 1;
            } else {
                names.extend(got.into_iter().map(str::to_string));
            }
        }
        if !names.is_empty() {
            blocks.push(Block { kind, names });
        }
    }
    Parsed { blocks, skipped }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn python_splits_lines_where_it_does() {
        assert_eq!(splitlines("a\r\nb\rc\nd"), ["a", "b", "c", "d"]);
        assert_eq!(splitlines("a\n"), ["a"]);
        assert_eq!(splitlines("a\n\nb"), ["a", "", "b"]);
        assert_eq!(splitlines(""), Vec::<&str>::new());
        assert_eq!(splitlines("a\u{85}b\u{2028}c\u{b}d"), ["a", "b", "c", "d"]);
        // The unit separator U+001F is whitespace, not a break.
        assert_eq!(splitlines("a\u{1f}b"), ["a\u{1f}b"]);
    }

    #[test]
    fn a_header_needs_a_closed_bracket_on_its_own_line() {
        assert_eq!(header("[Order]\nA"), Some((0, 7)));
        assert_eq!(header("[order x y]"), Some((0, 11)));
        assert_eq!(header("[Orderly]"), None);
        assert_eq!(header("[Order\n]"), None);
        assert_eq!(header(" [Order]"), None);
    }

    #[test]
    fn names_are_the_shortest_that_end_at_a_boundary() {
        assert_eq!(names_in("A.esp.esp"), ["A.esp.esp"]);
        assert_eq!(names_in("B.esp x.esp"), ["B.esp", "x.esp"]);
        assert_eq!(names_in("A.esp*"), ["A.esp*"]);
        assert_eq!(names_in("A.esp.bak B.espx"), Vec::<&str>::new());
        assert_eq!(names_in("A.ESP\u{a0}B.Esm"), ["A.ESP", "B.Esm"]);
    }
}
