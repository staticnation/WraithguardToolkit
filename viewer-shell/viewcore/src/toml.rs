//! A small TOML reader and writer.
//!
//! Covers the subset the config format uses: tables, arrays of tables, inline
//! tables, arrays, strings, numbers and booleans. Written by hand for the same
//! reason as everything else here — the engine stays dependency-free, so it
//! builds offline and cross-compiles without a toolchain hunt.
//!
//! Unsupported TOML (dates, multi-line strings, dotted keys outside headers) is
//! reported as an error rather than silently mis-parsed.

use std::collections::BTreeMap;
use std::fmt::Write as _;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Str(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    Array(Vec<Value>),
    Table(Table),
}

pub type Table = BTreeMap<String, Value>;

impl Value {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }
    pub fn as_f32(&self) -> Option<f32> {
        match self {
            Value::Int(i) => Some(*i as f32),
            Value::Float(f) => Some(*f as f32),
            _ => None,
        }
    }
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Int(i) => Some(*i),
            Value::Float(f) => Some(*f as i64),
            _ => None,
        }
    }
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }
    pub fn as_array(&self) -> Option<&[Value]> {
        match self {
            Value::Array(a) => Some(a),
            _ => None,
        }
    }
    pub fn as_table(&self) -> Option<&Table> {
        match self {
            Value::Table(t) => Some(t),
            _ => None,
        }
    }
    /// Convenience for `[[array-of-tables]]` that may have been written as a
    /// single table when there is only one entry.
    pub fn as_tables(&self) -> Vec<&Table> {
        match self {
            Value::Array(a) => a.iter().filter_map(|v| v.as_table()).collect(),
            Value::Table(t) => vec![t],
            _ => Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

struct P<'a> {
    b: &'a [u8],
    i: usize,
    line: usize,
}

type R<T> = Result<T, String>;

impl<'a> P<'a> {
    fn err<T>(&self, msg: &str) -> R<T> {
        Err(format!("line {}: {}", self.line, msg))
    }
    #[inline]
    fn peek(&self) -> u8 {
        if self.i < self.b.len() {
            self.b[self.i]
        } else {
            0
        }
    }
    #[inline]
    fn bump(&mut self) -> u8 {
        let c = self.peek();
        self.i += 1;
        if c == b'\n' {
            self.line += 1;
        }
        c
    }
    /// Skips spaces, and optionally newlines, plus any comments in between.
    fn ws(&mut self, newlines: bool) {
        loop {
            match self.peek() {
                b' ' | b'\t' | b'\r' => {
                    self.bump();
                }
                b'\n' if newlines => {
                    self.bump();
                }
                b'#' => {
                    while self.i < self.b.len() && self.peek() != b'\n' {
                        self.bump();
                    }
                }
                _ => return,
            }
        }
    }
    fn eol(&mut self) -> R<()> {
        self.ws(false);
        match self.peek() {
            0 => Ok(()),
            b'\n' => {
                self.bump();
                Ok(())
            }
            c => self.err(&format!("unexpected {:?} after value", c as char)),
        }
    }

    fn key(&mut self) -> R<String> {
        self.ws(false);
        match self.peek() {
            b'"' | b'\'' => self.string(),
            _ => {
                let start = self.i;
                while matches!(self.peek(), b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'_' | b'-') {
                    self.bump();
                }
                if start == self.i {
                    return self.err("expected a key");
                }
                Ok(String::from_utf8_lossy(&self.b[start..self.i]).into_owned())
            }
        }
    }

    /// A dotted key path, as used in table headers.
    fn key_path(&mut self) -> R<Vec<String>> {
        let mut out = vec![self.key()?];
        loop {
            self.ws(false);
            if self.peek() != b'.' {
                return Ok(out);
            }
            self.bump();
            out.push(self.key()?);
        }
    }

    /// Reads a quoted string.
    ///
    /// Bytes are collected and decoded as UTF-8 **once, at the end**. Pushing them
    /// one at a time as `c as char` is a latin1 decode wearing a Rust cast: every
    /// multi-byte character comes back mojibake, so an em dash in a comment or an
    /// accent in an author's name was corrupted a little more on every save.
    fn string(&mut self) -> R<String> {
        let q = self.bump();
        /* Round 17m: the multi-line forms, `'''literal'''` and `"""basic"""`. Real TOML
           and not a curiosity — G7's MGE XE fork writes a Windows path with an apostrophe
           in it that way (`data_dirs = ['''C:\...\Robin's grass''']`), and without these
           the whole file failed to parse and every fog value silently fell back to a
           default. A newline straight after the opening delimiter is dropped, as the
           format says. */
        if self.i + 1 < self.b.len() && self.b[self.i] == q && self.b[self.i + 1] == q {
            self.bump();
            self.bump();
            if self.peek() == b'\r' {
                self.bump();
            }
            if self.peek() == b'\n' {
                self.bump();
            }
            let mut s: Vec<u8> = Vec::new();
            loop {
                if self.i >= self.b.len() {
                    return self.err("unterminated multi-line string");
                }
                if self.b[self.i] == q
                    && self.i + 2 < self.b.len()
                    && self.b[self.i + 1] == q
                    && self.b[self.i + 2] == q
                {
                    self.bump();
                    self.bump();
                    self.bump();
                    return Ok(String::from_utf8_lossy(&s).into_owned());
                }
                let c = self.bump();
                /* A literal (single-quoted) multi-line string has no escapes at all; a
                   basic one keeps the line-ending backslash, which swallows the newline
                   and the whitespace after it. */
                if q == b'"' && c == b'\\' {
                    let n = self.peek();
                    if n == b'\n' || n == b'\r' {
                        while matches!(self.peek(), b' ' | b'\t' | b'\n' | b'\r') {
                            self.bump();
                        }
                        continue;
                    }
                    let one = self.escape()?;
                    s.extend_from_slice(one.as_bytes());
                    continue;
                }
                s.push(c);
            }
        }
        let mut s: Vec<u8> = Vec::new();
        loop {
            if self.i >= self.b.len() {
                return self.err("unterminated string");
            }
            let c = self.bump();
            if c == q {
                return Ok(String::from_utf8_lossy(&s).into_owned());
            }
            if c == b'\n' {
                return self.err("unterminated string");
            }
            if q == b'\'' {
                s.push(c);
                continue;
            }
            if c != b'\\' {
                s.push(c);
                continue;
            }
            let one = self.escape()?;
            s.extend_from_slice(one.as_bytes());
        }
    }

    /// One backslash escape, the backslash already eaten. Shared by the single-line and
    /// multi-line basic strings so the two cannot drift (round 17m).
    fn escape(&mut self) -> R<String> {
        let e = self.bump();
        Ok(match e {
            b'n' => "\n".to_string(),
            b't' => "\t".to_string(),
            b'r' => "\r".to_string(),
            b'"' => "\"".to_string(),
            b'\\' => "\\".to_string(),
            b'0' => "\0".to_string(),
            b'u' | b'U' => {
                let n = if e == b'u' { 4 } else { 8 };
                let mut v: u32 = 0;
                for _ in 0..n {
                    let d = self.bump() as char;
                    v = v * 16 + d.to_digit(16).unwrap_or(0);
                }
                char::from_u32(v).unwrap_or('?').to_string()
            }
            _ => return self.err("unknown string escape"),
        })
    }

    fn value(&mut self) -> R<Value> {
        self.ws(false);
        match self.peek() {
            b'"' | b'\'' => Ok(Value::Str(self.string()?)),
            b'[' => {
                self.bump();
                let mut a = Vec::new();
                loop {
                    self.ws(true);
                    if self.peek() == b']' {
                        self.bump();
                        return Ok(Value::Array(a));
                    }
                    a.push(self.value()?);
                    self.ws(true);
                    match self.peek() {
                        b',' => {
                            self.bump();
                        }
                        b']' => {
                            self.bump();
                            return Ok(Value::Array(a));
                        }
                        _ => return self.err("expected , or ] in array"),
                    }
                }
            }
            b'{' => {
                self.bump();
                let mut t = Table::new();
                loop {
                    self.ws(false);
                    if self.peek() == b'}' {
                        self.bump();
                        return Ok(Value::Table(t));
                    }
                    let k = self.key()?;
                    self.ws(false);
                    if self.bump() != b'=' {
                        return self.err("expected = in inline table");
                    }
                    let v = self.value()?;
                    t.insert(k, v);
                    self.ws(false);
                    match self.peek() {
                        b',' => {
                            self.bump();
                        }
                        b'}' => {
                            self.bump();
                            return Ok(Value::Table(t));
                        }
                        _ => return self.err("expected , or } in inline table"),
                    }
                }
            }
            _ => {
                let start = self.i;
                while !matches!(self.peek(), 0 | b',' | b']' | b'}' | b'\n' | b'#') {
                    self.bump();
                }
                let raw = String::from_utf8_lossy(&self.b[start..self.i]).trim().to_string();
                if raw == "true" {
                    return Ok(Value::Bool(true));
                }
                if raw == "false" {
                    return Ok(Value::Bool(false));
                }
                let clean = raw.replace('_', "");
                if !clean.contains(['.', 'e', 'E', 'n', 'i']) {
                    if let Ok(i) = clean.parse::<i64>() {
                        return Ok(Value::Int(i));
                    }
                }
                if let Ok(f) = clean.parse::<f64>() {
                    return Ok(Value::Float(f));
                }
                self.err(&format!("cannot read {:?} as a value", raw))
            }
        }
    }
}

/// Walks (creating as needed) to the table named by `path`.
fn descend<'t>(root: &'t mut Table, path: &[String], array_last: bool) -> R<&'t mut Table> {
    let mut cur = root;
    for (i, k) in path.iter().enumerate() {
        let last = i == path.len() - 1;
        if last && array_last {
            let e = cur.entry(k.clone()).or_insert_with(|| Value::Array(Vec::new()));
            let Value::Array(a) = e else {
                return Err(format!("{} is not an array of tables", k));
            };
            a.push(Value::Table(Table::new()));
            let Some(Value::Table(t)) = a.last_mut() else { unreachable!() };
            return Ok(t);
        }
        let e = cur.entry(k.clone()).or_insert_with(|| Value::Table(Table::new()));
        cur = match e {
            Value::Table(t) => t,
            // A header may address the newest element of an array of tables.
            Value::Array(a) => match a.last_mut() {
                Some(Value::Table(t)) => t,
                _ => return Err(format!("{} is not a table", k)),
            },
            _ => return Err(format!("{} is not a table", k)),
        };
    }
    Ok(cur)
}

pub fn parse(text: &str) -> R<Table> {
    /* Round 18bc (D5): a UTF-8 byte-order mark is not a key. Notepad and several Windows
       editors write one by default, and the two files people are explicitly invited to
       hand-edit are exactly these: a `.rules.toml` somebody sent them, and
       `gardenfell.<code>.toml`, which a translator opens in whatever editor they have.
       Nothing stripped it, so the file failed with "line 1: expected a key" and the pack
       showed in the picker with an error pointing at a line that looks perfectly fine. */
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut p = P { b: text.as_bytes(), i: 0, line: 1 };
    let mut root = Table::new();
    let mut path: Vec<String> = Vec::new();

    loop {
        p.ws(true);
        if p.i >= p.b.len() {
            return Ok(root);
        }
        if p.peek() == b'[' {
            p.bump();
            let array = p.peek() == b'[';
            if array {
                p.bump();
            }
            let kp = p.key_path()?;
            p.ws(false);
            if p.bump() != b']' {
                return p.err("expected ] in table header");
            }
            if array && p.bump() != b']' {
                return p.err("expected ]] in array-of-tables header");
            }
            descend(&mut root, &kp, array)?;
            path = kp;
            p.eol()?;
            continue;
        }
        let k = p.key()?;
        p.ws(false);
        if p.bump() != b'=' {
            return p.err("expected = after key");
        }
        let v = p.value()?;
        let t = descend(&mut root, &path, false)?;
        t.insert(k, v);
        p.eol()?;
    }
}

// ---------------------------------------------------------------------------
// Writing
// ---------------------------------------------------------------------------

pub fn escape(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    o.push('"');
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\t' => o.push_str("\\t"),
            '\r' => o.push_str("\\r"),
            c if (c as u32) < 0x20 => {
                let _ = write!(o, "\\u{:04x}", c as u32);
            }
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

/// Formats a float without a trailing `.0` avalanche but always as a float, so
/// re-reading gives the same type back.
///
/// Round 18bd (D6): **including the three that are not numbers.** `inf` and `nan` are
/// what TOML spells them and what this file's own reader takes; `format!("{}", f32::INFINITY)`
/// gives `inf`, and the `.0` this used to bolt on made `inf.0`, which reads back as
/// nothing at all. So a rules file containing `spacing = inf` (or `1e999`, which parses
/// to it) loaded fine, was written back unreadable, and `ruleset::write` then refused
/// every save of that set from then on — D3's silent failure reached from another
/// direction. `scale = [nan, 2.0]` did the same, and slips past `validate` besides,
/// because `NaN > NaN` is false.
pub fn num(v: f32) -> String {
    if v.is_nan() {
        return "nan".into();
    }
    if v.is_infinite() {
        return if v > 0.0 { "inf".into() } else { "-inf".into() };
    }
    if v == v.trunc() && v.abs() < 1e15 {
        format!("{:.1}", v)
    } else {
        let s = format!("{}", v);
        if s.contains(['.', 'e', 'E']) {
            s
        } else {
            format!("{}.0", s)
        }
    }
}

