//! A Lua 5.1 tokenizer, with LuaJIT's and OpenMW's additions (was the heart of
//! `wraithguard/lua/lexer.py`, which stays as the fallback and the reference this is held
//! to - token for token, offsets and errors included).
//!
//! Offsets and columns count characters (code points), as the Python module's do, so a
//! token's `start` indexes the same `str` on both sides.

/// One token: kind, exact text, start offset, 1-based line and column, value (a string's
/// decoded contents, a number without its LuaJIT suffix, otherwise the text).
pub type Token = (&'static str, String, usize, usize, usize, String);

/// Source that is not Lua: the message, and its 1-based line and column.
pub type LexError = (String, usize, usize);

const KEYWORDS: [&str; 22] = [
    "and", "break", "do", "else", "elseif", "end", "false", "for", "function", "goto", "if", "in", "local", "nil",
    "not", "or", "repeat", "return", "then", "true", "until", "while",
];

/// Longest first, so `...` wins over `..` and `..` over `.`.
const OPERATORS: [&str; 33] = [
    "...", "..", "==", "~=", "<=", ">=", "::", "//", "<<", ">>", "+", "-", "*", "/", "%", "^", "#", "&", "~", "|", "<",
    ">", "=", "(", ")", "{", "}", "[", "]", ";", ":", ",", ".",
];

struct Cursor<'a> {
    src: &'a [char],
    pos: usize,
    line: usize,
    line_start: usize,
}

impl Cursor<'_> {
    fn col(&self, pos: usize) -> usize {
        pos - self.line_start + 1
    }

    fn advance_to(&mut self, end: usize) {
        for i in self.pos..end {
            if self.src[i] == '\n' {
                self.line += 1;
                self.line_start = i + 1;
            }
        }
        self.pos = end;
    }

    fn error(&self, message: impl Into<String>) -> LexError {
        (message.into(), self.line, self.col(self.pos))
    }

    fn starts_with(&self, at: usize, s: &str) -> bool {
        (at..).zip(s.chars()).all(|(i, c)| self.src.get(i) == Some(&c))
    }

    fn find(&self, from: usize, s: &[char]) -> Option<usize> {
        if s.is_empty() || from > self.src.len() {
            return None;
        }
        (from..=self.src.len().saturating_sub(s.len())).find(|&i| self.src[i..i + s.len()] == *s)
    }
}

fn is_name_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn is_name(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Python's `str.isdigit` for one character, near enough: ASCII digits, and the other
/// Unicode digits (which the number rule then refuses, as the Python module does).
fn is_digit(c: char) -> bool {
    c.is_ascii_digit() || (!c.is_ascii() && c.is_numeric())
}

/// `[==[`: the end of the long bracket opening at `start`, and the opening's length.
fn long_bracket_end(cur: &Cursor, start: usize) -> Result<Option<(usize, usize)>, LexError> {
    if cur.src.get(start) != Some(&'[') {
        return Ok(None);
    }
    let mut i = start + 1;
    while cur.src.get(i) == Some(&'=') {
        i += 1;
    }
    if cur.src.get(i) != Some(&'[') {
        return Ok(None);
    }
    let level = i - start - 1;
    let open_end = i + 1;
    let mut close: Vec<char> = vec![']'];
    close.extend(std::iter::repeat_n('=', level));
    close.push(']');
    match cur.find(open_end, &close) {
        Some(end) => Ok(Some((end + close.len(), open_end - start))),
        None => Err(cur.error("unfinished long string or comment")),
    }
}

fn long_value(text: &[char], open_len: usize) -> String {
    let body = &text[open_len..text.len() - open_len];
    let body = if body.starts_with(&['\r', '\n']) {
        &body[2..]
    } else if body.first().is_some_and(|c| *c == '\n' || *c == '\r') {
        &body[1..]
    } else {
        body
    };
    body.iter().collect()
}

fn read_string(cur: &Cursor) -> Result<(usize, String), LexError> {
    let src = cur.src;
    let quote = src[cur.pos];
    let mut i = cur.pos + 1;
    let mut out = String::new();
    loop {
        let Some(&ch) = src.get(i) else { return Err(cur.error("unfinished string")) };
        if ch == quote {
            return Ok((i + 1, out));
        }
        if ch == '\n' {
            return Err(cur.error("unfinished string (newline before the closing quote)"));
        }
        if ch != '\\' {
            out.push(ch);
            i += 1;
            continue;
        }
        i += 1;
        let Some(&esc) = src.get(i) else { return Err(cur.error("unfinished string")) };
        match esc {
            'n' => out.push('\n'),
            't' => out.push('\t'),
            'r' => out.push('\r'),
            'a' => out.push('\x07'),
            'b' => out.push('\x08'),
            'f' => out.push('\x0c'),
            'v' => out.push('\x0b'),
            '\\' | '"' | '\'' => out.push(esc),
            '\n' => out.push('\n'),
            _ => {}
        }
        if matches!(esc, 'n' | 't' | 'r' | 'a' | 'b' | 'f' | 'v' | '\\' | '"' | '\'' | '\n') {
            i += 1;
        } else if esc == '\r' {
            out.push('\n');
            i += if src.get(i + 1) == Some(&'\n') { 2 } else { 1 };
        } else if is_digit(esc) {
            let mut j = i;
            while j < src.len() && j - i < 3 && src[j].is_ascii_digit() {
                j += 1;
            }
            let digits: String = if j > i { src[i..j].iter().collect() } else { esc.to_string() };
            let Ok(code) = digits.parse::<u32>() else { return Err(cur.error(format!("invalid escape \\{esc}"))) };
            if code > 255 {
                return Err(cur.error(format!("escape \\{digits} is too large")));
            }
            out.push(char::from_u32(code).unwrap_or('\u{fffd}'));
            i += digits.chars().count();
        } else if esc == 'x' {
            let hex: Option<String> = src.get(i + 1..i + 3).filter(|h| h.iter().all(char::is_ascii_hexdigit)).map(|h| h.iter().collect());
            let Some(hex) = hex else { return Err(cur.error("\\x needs two hexadecimal digits")) };
            out.push(char::from_u32(u32::from_str_radix(&hex, 16).unwrap_or(0)).unwrap_or('\u{fffd}'));
            i += 3;
        } else if esc == 'z' {
            i += 1;
            while i < src.len() && matches!(src[i], ' ' | '\t' | '\r' | '\n' | '\x0c' | '\x0b') {
                i += 1;
            }
        } else if esc == 'u' {
            let open = i + 1;
            let mut j = open + 1;
            while j < src.len() && src[j].is_ascii_hexdigit() {
                j += 1;
            }
            if src.get(open) != Some(&'{') || j == open + 1 || src.get(j) != Some(&'}') {
                return Err(cur.error("\\u needs {hex digits}"));
            }
            let hex: String = src[open + 1..j].iter().collect();
            let code = u32::from_str_radix(&hex, 16).unwrap_or(u32::MAX).min(0x10FFFF);
            out.push(char::from_u32(code).unwrap_or('\u{fffd}'));
            i = j + 1;
        } else {
            return Err(cur.error(format!("invalid escape \\{esc}")));
        }
    }
}

/// The number's end (before a LuaJIT suffix) at `pos`, Python's `_NUMBER` rule.
fn number_end(src: &[char], pos: usize) -> Option<usize> {
    let hex = |c: char| c.is_ascii_hexdigit();
    let dig = |c: char| c.is_ascii_digit();
    let n = src.len();
    let run = |mut i: usize, f: &dyn Fn(char) -> bool| {
        while i < n && f(src[i]) {
            i += 1;
        }
        i
    };
    if src[pos] == '0' && pos + 1 < n && matches!(src[pos + 1], 'x' | 'X') {
        // `0x` then `[hex]* .? [hex]+ | [hex]+ .?`: as `re` backtracks, the first form
        // takes every digit (and a fraction when one follows the dot); a dot with no digit
        // after it is not taken. No digit at all: the decimal rule below reads the "0".
        let s = pos + 2;
        let a = run(s, &hex);
        let mut end = None;
        if a < n && src[a] == '.' {
            let b = run(a + 1, &hex);
            if b > a + 1 {
                end = Some(b);
            }
        }
        if end.is_none() && a > s {
            end = Some(a);
        }
        if let Some(mut e) = end {
            if e < n && matches!(src[e], 'p' | 'P') {
                let mut k = e + 1;
                if k < n && matches!(src[k], '+' | '-') {
                    k += 1;
                }
                let d = run(k, &dig);
                if d > k {
                    e = d;
                }
            }
            return Some(e);
        }
    }
    let mut e;
    if dig(src[pos]) {
        e = run(pos, &dig);
        if e < n && src[e] == '.' {
            e = run(e + 1, &dig);
        }
    } else if src[pos] == '.' && pos + 1 < n && dig(src[pos + 1]) {
        e = run(pos + 1, &dig);
    } else {
        return None;
    }
    if e < n && matches!(src[e], 'e' | 'E') {
        let mut k = e + 1;
        if k < n && matches!(src[k], '+' | '-') {
            k += 1;
        }
        let d = run(k, &dig);
        if d > k {
            e = d;
        }
    }
    Some(e)
}

/// LuaJIT's suffixes (`1LL`, `2ULL`, `3i`): the end past one, or `end`.
fn suffix_end(src: &[char], end: usize) -> usize {
    let at = |i: usize, set: &[char]| src.get(i).is_some_and(|c| set.contains(c));
    if at(end, &['i', 'I']) {
        return end + 1;
    }
    let mut i = end;
    if at(i, &['u', 'U']) {
        i += 1;
    }
    if at(i, &['l', 'L']) && at(i + 1, &['l', 'L']) {
        return i + 2;
    }
    end
}

/// Splits Lua source into tokens, ending with one `eof`. `comments` keeps comment tokens
/// (for highlighting); `teal` makes `?` a token too (Teal's optional parameters).
pub fn tokenize(text: &str, comments: bool, teal: bool) -> Result<Vec<Token>, LexError> {
    let src: Vec<char> = text.chars().collect();
    let n = src.len();
    let mut cur = Cursor { src: &src, pos: 0, line: 1, line_start: 0 };
    let mut tokens: Vec<Token> = Vec::new();
    if src.first() == Some(&'#') {
        let nl = cur.find(0, &['\n']).unwrap_or(n);
        cur.advance_to(nl);
    }
    let slice = |a: usize, b: usize| -> String { src[a..b].iter().collect() };
    macro_rules! emit {
        ($kind:expr, $end:expr, $value:expr) => {{
            let end = $end;
            let text = slice(cur.pos, end);
            let value: Option<String> = $value;
            let col = cur.col(cur.pos);
            tokens.push(($kind, text.clone(), cur.pos, cur.line, col, value.unwrap_or(text)));
            cur.advance_to(end);
        }};
    }
    while cur.pos < n {
        let ch = src[cur.pos];
        if ch == '\n' {
            cur.advance_to(cur.pos + 1);
            continue;
        }
        if matches!(ch, ' ' | '\t' | '\r' | '\x0c' | '\x0b') {
            let mut e = cur.pos;
            while e < n && matches!(src[e], ' ' | '\t' | '\r' | '\x0c' | '\x0b') {
                e += 1;
            }
            cur.advance_to(e);
            continue;
        }
        if cur.starts_with(cur.pos, "--") {
            let end = match long_bracket_end(&cur, cur.pos + 2)? {
                Some((end, _)) => end,
                None => cur.find(cur.pos, &['\n']).unwrap_or(n),
            };
            if comments {
                emit!("comment", end, None);
            } else {
                cur.advance_to(end);
            }
            continue;
        }
        if is_name_start(ch) {
            let mut e = cur.pos;
            while e < n && is_name(src[e]) {
                e += 1;
            }
            let word = slice(cur.pos, e);
            let kind = if KEYWORDS.contains(&word.as_str()) { "keyword" } else { "name" };
            emit!(kind, e, None);
            continue;
        }
        if is_digit(ch) || (ch == '.' && cur.pos + 1 < n && is_digit(src[cur.pos + 1])) {
            let Some(num_end) = number_end(&src, cur.pos) else { return Err(cur.error("malformed number")) };
            let end = suffix_end(&src, num_end);
            if end < n && (src[end].is_alphanumeric() || src[end] == '_') {
                let near: String = src[cur.pos..=end].iter().collect();
                return Err(cur.error(format!("malformed number near {}", py_repr(&near))));
            }
            let value = slice(cur.pos, num_end);
            emit!("number", end, Some(value));
            continue;
        }
        if ch == '"' || ch == '\'' {
            let (end, value) = read_string(&cur)?;
            emit!("string", end, Some(value));
            continue;
        }
        if ch == '['
            && let Some((end, open_len)) = long_bracket_end(&cur, cur.pos)?
        {
            let value = long_value(&src[cur.pos..end], open_len);
            emit!("string", end, Some(value));
            continue;
        }
        if teal && ch == '?' {
            emit!("op", cur.pos + 1, None);
            continue;
        }
        match OPERATORS.iter().find(|op| cur.starts_with(cur.pos, op)) {
            Some(op) => {
                let end = cur.pos + op.chars().count();
                emit!("op", end, None);
            }
            None => return Err(cur.error(format!("unexpected character {}", py_repr(&ch.to_string())))),
        }
    }
    let col = cur.col(cur.pos);
    tokens.push(("eof", String::new(), n, cur.line, col, String::new()));
    Ok(tokens)
}

/// Python's `repr` of a short string, for messages that match the Python module's.
pub fn py_repr(s: &str) -> String {
    let quote = if s.contains('\'') && !s.contains('"') { '"' } else { '\'' };
    let mut out = String::new();
    out.push(quote);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            c if (c as u32) < 0x20 || c as u32 == 0x7f => out.push_str(&format!("\\x{:02x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push(quote);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_spans_and_values() {
        let t = tokenize("local x = 0x1p4 .. \"a\\n\" --[[ c ]] [==[\nlong]==] 3LL", true, false).unwrap();
        let kinds: Vec<&str> = t.iter().map(|t| t.0).collect();
        assert_eq!(kinds, ["keyword", "name", "op", "number", "op", "string", "comment", "string", "number", "eof"]);
        assert_eq!(t[3].5, "0x1p4");
        assert_eq!(t[5].5, "a\n");
        assert_eq!(t[7].5, "long");
        assert_eq!(t[8].1, "3LL");
        assert_eq!(t[8].5, "3");
    }

    #[test]
    fn errors_say_where() {
        let e = tokenize("x = 'a\nb'", false, false).unwrap_err();
        assert_eq!((e.1, e.2), (1, 5));
        assert!(tokenize("x = 1a", false, false).is_err());
        assert!(tokenize("x ? y", false, true).is_ok());
        assert!(tokenize("x ? y", false, false).is_err());
    }
}
