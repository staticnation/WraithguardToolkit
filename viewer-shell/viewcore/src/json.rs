//! Minimal JSON for the Tauri bridge, both ways.
//!
//! The UI receives structured summaries — never bulk geometry — and sends back the
//! config it is editing, so a hand-rolled pair is enough and keeps the crate
//! dependency-free.
//!
//! The reader exists so the page can stop owning a second TOML implementation. The
//! page holds the config being edited, which is input and belongs there; what it must
//! not hold is a second definition of the file format. Sending the model as JSON and
//! letting the engine write every byte that reaches disk is what removes it.

pub struct J(pub String);

impl J {
    pub fn obj() -> J {
        J(String::from("{"))
    }
    fn comma(&mut self) {
        if !self.0.ends_with('{') && !self.0.ends_with('[') {
            self.0.push(',');
        }
    }
    pub fn key(&mut self, k: &str) -> &mut Self {
        self.comma();
        self.0.push('"');
        escape_into(k, &mut self.0);
        self.0.push_str("\":");
        self
    }
    pub fn num(&mut self, k: &str, v: f64) -> &mut Self {
        self.key(k);
        if v.is_finite() {
            self.0.push_str(&format!("{}", v));
        } else {
            self.0.push_str("null");
        }
        self
    }
    /// Unsigned. A negative number written through here wraps into something
    /// astronomical — half of Morrowind's grid is negative, so reach for `num` when
    /// the value is a coordinate.
    pub fn int(&mut self, k: &str, v: u64) -> &mut Self {
        self.key(k);
        self.0.push_str(&v.to_string());
        self
    }
    pub fn str(&mut self, k: &str, v: &str) -> &mut Self {
        self.key(k);
        self.0.push('"');
        escape_into(v, &mut self.0);
        self.0.push('"');
        self
    }
    pub fn bool(&mut self, k: &str, v: bool) -> &mut Self {
        self.key(k);
        self.0.push_str(if v { "true" } else { "false" });
        self
    }
    pub fn raw(&mut self, k: &str, v: &str) -> &mut Self {
        self.key(k);
        self.0.push_str(v);
        self
    }
    pub fn done(&mut self) -> String {
        self.0.push('}');
        std::mem::take(&mut self.0)
    }
}

pub fn escape_into(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
}

/// An array of things that are already JSON — objects a caller assembled itself.
pub fn raw_array<S: AsRef<str>>(items: &[S]) -> String {
    let mut s = String::from("[");
    for (i, it) in items.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str(it.as_ref());
    }
    s.push(']');
    s
}

pub fn string_array<S: AsRef<str>>(items: &[S]) -> String {
    let mut s = String::from("[");
    for (i, it) in items.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push('"');
        escape_into(it.as_ref(), &mut s);
        s.push('"');
    }
    s.push(']');
    s
}


/* =====================================================================================
   Reading.

   Enough JSON to read what the page sends: objects, arrays, strings, numbers, the
   three literals. No streaming, no borrowing — the payloads are a config, measured in
   kilobytes, and clarity is worth more here than an allocation saved.
   ===================================================================================== */

#[derive(Debug, Clone, PartialEq)]
pub enum Val {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Val>),
    Obj(Vec<(String, Val)>),
}

impl Val {
    /// A member of an object, or None for anything else. Linear, because a config has
    /// tens of keys and a map would cost more to build than the scans it saves.
    pub fn get(&self, key: &str) -> Option<&Val> {
        match self {
            Val::Obj(kv) => kv.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Val::Str(s) => Some(s.as_str()),
            _ => None,
        }
    }
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Val::Num(n) => Some(*n),
            _ => None,
        }
    }
    pub fn as_f32(&self) -> Option<f32> {
        self.as_f64().map(|v| v as f32)
    }
    pub fn as_i64(&self) -> Option<i64> {
        self.as_f64().map(|v| v as i64)
    }
    /// Numbers count as truthy the way JavaScript means it, because the other side of
    /// this boundary is JavaScript and sends `1` for a checkbox often enough.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Val::Bool(b) => Some(*b),
            Val::Num(n) => Some(*n != 0.0),
            _ => None,
        }
    }
    pub fn is_null(&self) -> bool {
        matches!(self, Val::Null)
    }
    pub fn as_arr(&self) -> Option<&[Val]> {
        match self {
            Val::Arr(v) => Some(v.as_slice()),
            _ => None,
        }
    }
    /// Convenience for the common shape: a field that should be a string, defaulted.
    pub fn str_or(&self, key: &str, fallback: &str) -> String {
        self.get(key).and_then(|v| v.as_str()).unwrap_or(fallback).to_string()
    }
    pub fn f32_or(&self, key: &str, fallback: f32) -> f32 {
        self.get(key).and_then(|v| v.as_f32()).unwrap_or(fallback)
    }
    pub fn bool_or(&self, key: &str, fallback: bool) -> bool {
        self.get(key).and_then(|v| v.as_bool()).unwrap_or(fallback)
    }
}

pub fn parse(text: &str) -> Result<Val, String> {
    let b = text.as_bytes();
    let mut p = Parser { b, i: 0 };
    p.ws();
    let v = p.value()?;
    p.ws();
    if p.i != b.len() {
        return Err(format!("trailing text at byte {}", p.i));
    }
    Ok(v)
}

struct Parser<'a> {
    b: &'a [u8],
    i: usize,
}

fn bad(at: usize) -> impl Fn() -> String {
    move || format!("unexpected token at byte {at}")
}

impl<'a> Parser<'a> {
    fn ws(&mut self) {
        while self.i < self.b.len() && matches!(self.b[self.i], b' ' | b'\t' | b'\n' | b'\r') {
            self.i += 1;
        }
    }
    fn eat(&mut self, c: u8) -> Result<(), String> {
        if self.i < self.b.len() && self.b[self.i] == c {
            self.i += 1;
            Ok(())
        } else {
            Err(format!("expected {:?} at byte {}", c as char, self.i))
        }
    }
    fn lit(&mut self, word: &str) -> bool {
        if self.b[self.i..].starts_with(word.as_bytes()) {
            self.i += word.len();
            true
        } else {
            false
        }
    }

    fn value(&mut self) -> Result<Val, String> {
        if self.i >= self.b.len() {
            return Err("unexpected end".into());
        }
        match self.b[self.i] {
            b'{' => self.object(),
            b'[' => self.array(),
            b'"' => Ok(Val::Str(self.string()?)),
            b't' => self.lit("true").then_some(Val::Bool(true)).ok_or_else(bad(self.i)),
            b'f' => self.lit("false").then_some(Val::Bool(false)).ok_or_else(bad(self.i)),
            b'n' => self.lit("null").then_some(Val::Null).ok_or_else(bad(self.i)),
            _ => self.number(),
        }
    }

    fn object(&mut self) -> Result<Val, String> {
        self.eat(b'{')?;
        let mut out = Vec::new();
        self.ws();
        if self.i < self.b.len() && self.b[self.i] == b'}' {
            self.i += 1;
            return Ok(Val::Obj(out));
        }
        loop {
            self.ws();
            let k = self.string()?;
            self.ws();
            self.eat(b':')?;
            self.ws();
            out.push((k, self.value()?));
            self.ws();
            match self.b.get(self.i) {
                Some(b',') => self.i += 1,
                Some(b'}') => {
                    self.i += 1;
                    return Ok(Val::Obj(out));
                }
                _ => return Err(format!("expected , or }} at byte {}", self.i)),
            }
        }
    }

    fn array(&mut self) -> Result<Val, String> {
        self.eat(b'[')?;
        let mut out = Vec::new();
        self.ws();
        if self.i < self.b.len() && self.b[self.i] == b']' {
            self.i += 1;
            return Ok(Val::Arr(out));
        }
        loop {
            self.ws();
            out.push(self.value()?);
            self.ws();
            match self.b.get(self.i) {
                Some(b',') => self.i += 1,
                Some(b']') => {
                    self.i += 1;
                    return Ok(Val::Arr(out));
                }
                _ => return Err(format!("expected , or ] at byte {}", self.i)),
            }
        }
    }

    fn string(&mut self) -> Result<String, String> {
        self.eat(b'"')?;
        // Bytes, decoded once at the end: pushing `c as char` per byte double-encodes
        // every multi-byte character, which is how a mod name with an accent in it came
        // back mangled and got worse on each save.
        let mut out: Vec<u8> = Vec::new();
        while self.i < self.b.len() {
            match self.b[self.i] {
                b'"' => {
                    self.i += 1;
                    return Ok(String::from_utf8_lossy(&out).into_owned());
                }
                b'\\' => {
                    self.i += 1;
                    let e = *self.b.get(self.i).ok_or("unterminated escape")?;
                    self.i += 1;
                    match e {
                        b'"' => out.push(b'"'),
                        b'\\' => out.push(b'\\'),
                        b'/' => out.push(b'/'),
                        b'b' => out.push(0x08),
                        b'f' => out.push(0x0c),
                        b'n' => out.push(b'\n'),
                        b'r' => out.push(b'\r'),
                        b't' => out.push(b'\t'),
                        b'u' => {
                            let h = self
                                .b
                                .get(self.i..self.i + 4)
                                .ok_or("truncated \\u escape")?;
                            let n = u32::from_str_radix(
                                std::str::from_utf8(h).map_err(|_| "bad \\u escape")?,
                                16,
                            )
                            .map_err(|_| "bad \\u escape")?;
                            self.i += 4;
                            // Surrogate pairs: JavaScript emits them for anything above
                            // the BMP, and JSON.stringify does so by default.
                            let ch = if (0xD800..0xDC00).contains(&n)
                                && self.b.get(self.i) == Some(&b'\\')
                                && self.b.get(self.i + 1) == Some(&b'u')
                            {
                                let h2 = self
                                    .b
                                    .get(self.i + 2..self.i + 6)
                                    .ok_or("truncated surrogate")?;
                                let lo = u32::from_str_radix(
                                    std::str::from_utf8(h2).map_err(|_| "bad surrogate")?,
                                    16,
                                )
                                .map_err(|_| "bad surrogate")?;
                                if (0xDC00..0xE000).contains(&lo) {
                                    self.i += 6;
                                    0x10000 + ((n - 0xD800) << 10) + (lo - 0xDC00)
                                } else {
                                    n
                                }
                            } else {
                                n
                            };
                            let mut buf = [0u8; 4];
                            out.extend_from_slice(
                                char::from_u32(ch).unwrap_or('\u{fffd}').encode_utf8(&mut buf).as_bytes(),
                            );
                        }
                        other => return Err(format!("unknown escape \\{}", other as char)),
                    }
                }
                c => {
                    out.push(c);
                    self.i += 1;
                }
            }
        }
        Err("unterminated string".into())
    }

    fn number(&mut self) -> Result<Val, String> {
        let start = self.i;
        if self.b.get(self.i) == Some(&b'-') || self.b.get(self.i) == Some(&b'+') {
            self.i += 1;
        }
        while self.i < self.b.len()
            && (self.b[self.i].is_ascii_digit()
                || matches!(self.b[self.i], b'.' | b'e' | b'E' | b'+' | b'-'))
        {
            self.i += 1;
        }
        std::str::from_utf8(&self.b[start..self.i])
            .ok()
            .and_then(|s| s.parse::<f64>().ok())
            .map(Val::Num)
            .ok_or_else(|| format!("not a number at byte {start}"))
    }
}

/// Wraithguard: standard base64 (with or without padding; whitespace and a `data:...,`
/// prefix ignored) to bytes, or `None` on a character outside the alphabet.
pub fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let s = match s.find(',') {
        Some(i) if s.starts_with("data:") => &s[i + 1..],
        _ => s,
    };
    let val = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => (c - b'A') as u32,
            b'a'..=b'z' => (c - b'a' + 26) as u32,
            b'0'..=b'9' => (c - b'0' + 52) as u32,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => return None,
        })
    };
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for &c in s.as_bytes() {
        if c == b'=' || c.is_ascii_whitespace() {
            continue;
        }
        acc = (acc << 6) | val(c)?;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

#[cfg(test)]
mod b64_tests {
    #[test]
    fn decodes() {
        assert_eq!(super::base64_decode("aGVsbG8=").unwrap(), b"hello");
        assert_eq!(super::base64_decode("data:image/png;base64,aGk").unwrap(), b"hi");
        assert!(super::base64_decode("a$").is_none());
    }
}
