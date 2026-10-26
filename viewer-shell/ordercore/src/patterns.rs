//! mlox name patterns and version strings, as `wraithguard/rules/patterns.py`,
//! `wraithguard/sort/graph.py` and `wraithguard/versions.py` read them.
//!
//! A rule names its plugins by pattern: a plain file name, or one with `*` (any run), `?`
//! (none or one character) and `<VER>` (a version number, in any case). mlox turns a pattern
//! into a regular expression escaping only `( ) + .`, so every other character is passed
//! through to the expression; this crate takes no dependencies, so the few things a pattern
//! can say are matched by hand. What is not read is **declined**, never guessed:
//! [`Pattern::new`] and [`expand`] answer [`Unsupported`] for a pattern that has a wildcard
//! and also a character the expression would read as syntax (`[ ] { } | \ ^ $`), and
//! [`format_version`] for a string that is not ASCII (Python's `int` reads other scripts'
//! digits and whitespace). The caller keeps Python's answer for those. A pattern with no
//! wildcard is compared as plain text, as Python does, so it is never declined.
//!
//! The Python is the reference and `tests/fixtures/pattern_cases.json` pins both. Known and
//! left: letters are matched caselessly by Unicode's simple lower case, where Python's
//! `re.IGNORECASE` also folds a few more (`ſ` onto `s`); a name ending in a newline would
//! match `$` in Python; `\d` in a version is ASCII here.
//!
//! Written from mlox's documented behaviour, not its code.

use std::fmt;

/// What this crate declines to answer (see the module's text); the caller falls back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Unsupported;

impl fmt::Display for Unsupported {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("not read here: the caller keeps the Python answer")
    }
}

impl std::error::Error for Unsupported {}

/// Whether a pattern needs matching at all, rather than comparing as plain text
/// (`pattern_has_meta`): it holds `*`, `?` or `<VER>` in any case.
pub fn has_meta(pattern: &str) -> bool {
    pattern.contains('*') || pattern.contains('?') || pattern.to_lowercase().contains("<ver>")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tok {
    /// A character, matched without regard to case.
    Lit(char),
    /// `*`: `.*`, any run that holds no newline.
    AnyStar,
    /// `?`: `.?`, none or one character.
    AnyOpt,
    /// `<VER>`: `(\d+(?:[_.-]?\d+)*[a-zA-Z]?)`.
    Ver,
}

/// A pattern with a wildcard, ready to be matched against names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pattern {
    toks: Vec<Tok>,
}

impl Pattern {
    /// Read a pattern as `mlox_pattern_to_regex` does.
    ///
    /// # Errors
    /// None now: every character but the wildcards is plain text. (The `Result` stays for the
    /// callers, which declined what this used to.)
    pub fn new(pattern: &str) -> Result<Pattern, Unsupported> {
        let chars: Vec<char> = pattern.chars().collect();
        let mut toks = Vec::new();
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            if c == '<' && ver_at(&chars, i) {
                toks.push(Tok::Ver);
                i += 5;
                continue;
            }
            toks.push(match c {
                '*' => Tok::AnyStar,
                '?' => Tok::AnyOpt,
                other => Tok::Lit(other),
            });
            i += 1;
        }
        Ok(Pattern { toks })
    }

    /// Whether the whole of `name` matches (the expression is anchored at both ends and
    /// caseless, as OpenMW's file system is).
    pub fn is_match(&self, name: &str) -> bool {
        let s: Vec<char> = name.chars().collect();
        let mut failed = vec![false; (self.toks.len() + 1) * (s.len() + 1)];
        go(&self.toks, &s, 0, 0, &mut failed)
    }
}

/// Whether `<VER>`, in any case, starts at `i`.
fn ver_at(chars: &[char], i: usize) -> bool {
    chars.len() >= i + 5
        && chars[i + 1..i + 4].iter().zip("ver".chars()).all(|(c, v)| c.eq_ignore_ascii_case(&v))
        && chars[i + 4] == '>'
}

/// Simple lower case: the one character `c` lowers to, or `c` when it lowers to none or
/// several.
fn lower(c: char) -> char {
    let mut it = c.to_lowercase();
    match (it.next(), it.next()) {
        (Some(l), None) => l,
        _ => c,
    }
}

fn same(a: char, b: char) -> bool {
    a == b || lower(a) == lower(b)
}

/// Where a `<VER>` starting at `si` can end: `\d+(?:[_.-]?\d+)*[a-zA-Z]?`. That is a run of
/// digits and single separators that starts and ends with a digit, then perhaps one letter.
fn ver_ends(s: &[char], si: usize) -> Vec<usize> {
    let n = s.len();
    let mut out = Vec::new();
    if si >= n || !s[si].is_ascii_digit() {
        return out;
    }
    let mut i = si;
    loop {
        i += 1; // s[i - 1] is a digit: the number can end here
        out.push(i);
        if i < n && s[i].is_ascii_alphabetic() {
            out.push(i + 1);
        }
        if i < n && s[i].is_ascii_digit() {
            continue;
        }
        if i + 1 < n && matches!(s[i], '_' | '.' | '-') && s[i + 1].is_ascii_digit() {
            i += 1;
            continue;
        }
        break;
    }
    out
}

/// Whether `toks[ti..]` matches exactly `s[si..]`. `failed` remembers the (token, position)
/// pairs already known not to, so a long name cannot make it slow.
fn go(toks: &[Tok], s: &[char], ti: usize, si: usize, failed: &mut Vec<bool>) -> bool {
    if ti == toks.len() {
        return si == s.len();
    }
    let at = ti * (s.len() + 1) + si;
    if failed[at] {
        return false;
    }
    let n = s.len();
    let ok = match toks[ti] {
        Tok::Lit(c) => si < n && same(c, s[si]) && go(toks, s, ti + 1, si + 1, failed),
        Tok::AnyOpt => {
            go(toks, s, ti + 1, si, failed)
                || (si < n && s[si] != '\n' && go(toks, s, ti + 1, si + 1, failed))
        }
        Tok::AnyStar => {
            let mut j = si;
            loop {
                if go(toks, s, ti + 1, j, failed) {
                    break true;
                }
                if j < n && s[j] != '\n' {
                    j += 1;
                } else {
                    break false;
                }
            }
        }
        Tok::Ver => ver_ends(s, si).into_iter().any(|e| go(toks, s, ti + 1, e, failed)),
    };
    if !ok {
        failed[at] = true;
    }
    ok
}

/// The names in `pool` a rule's pattern stands for (`expand_pattern`), in `pool`'s order:
/// every match for a pattern with a wildcard, the first name equal without regard to case
/// for a plain one.
///
/// # Errors
/// [`Unsupported`] for a pattern with a wildcard and a character the expression would read
/// as syntax.
pub fn expand<'a, S: AsRef<str>>(pattern: &str, pool: &'a [S]) -> Result<Vec<&'a str>, Unsupported> {
    if has_meta(pattern) {
        let pat = Pattern::new(pattern)?;
        let names = pool.iter().map(|n| n.as_ref()).filter(|n: &&str| pat.is_match(n));
        return Ok(names.collect());
    }
    let low = pattern.to_lowercase();
    let first = pool.iter().map(|n| n.as_ref()).find(|n: &&str| n.to_lowercase() == low);
    Ok(first.into_iter().collect())
}

/// What Python's `int()` strips and `\s` means, in ASCII.
fn is_space(c: char) -> bool {
    matches!(c, ' ' | '\t'..='\r' | '\u{1c}'..='\u{1f}')
}

/// Python's `int(part)` for an ASCII string: optional whitespace, an optional sign, digits
/// with single underscores between them. `(negative, digits without leading zeros)`.
fn py_int(part: &str) -> Option<(bool, String)> {
    let t = part.trim_matches(is_space);
    let (neg, rest) = match t.as_bytes().first() {
        Some(b'+') => (false, &t[1..]),
        Some(b'-') => (true, &t[1..]),
        _ => (false, t),
    };
    let b = rest.as_bytes();
    if b.is_empty() || !b[0].is_ascii_digit() || !b[b.len() - 1].is_ascii_digit() {
        return None;
    }
    let mut digits = String::new();
    let mut after_underscore = false;
    for &c in b {
        if c == b'_' {
            if after_underscore {
                return None;
            }
            after_underscore = true;
        } else if c.is_ascii_digit() {
            digits.push(c as char);
            after_underscore = false;
        } else {
            return None;
        }
    }
    let trimmed = digits.trim_start_matches('0');
    let norm = if trimmed.is_empty() { "0".to_string() } else { trimmed.to_string() };
    Some((neg && norm != "0", norm))
}

/// `f"{n:05d}"` for a number held as sign and digits.
fn pad5(n: &(bool, String)) -> String {
    if n.0 {
        format!("-{:0>4}", n.1)
    } else {
        format!("{:0>5}", n.1)
    }
}

/// A version string in mlox's comparable form (`format_version`): `"1.0a"` becomes
/// `"00001.00000.00000.a"`, `"1.0"` `"00001.00000.00000._"`. Only the first three numbers
/// are kept. `""` when the string holds no version: callers read that as "unknowable", not
/// as a low version.
///
/// The string is split at `_`, `.` and `-` at most three times, so a fourth part keeps its
/// own delimiters (`1.2.3.4.5` holds none; `1.2.3.4_5` does).
///
/// # Errors
/// [`Unsupported`] for a string that is not ASCII.
pub fn format_version(version: &str) -> Result<String, Unsupported> {
    if !version.is_ascii() {
        return Err(Unsupported);
    }
    let mut parts: Vec<&str> = Vec::new();
    let mut start = 0;
    for (i, c) in version.char_indices() {
        if parts.len() < 3 && matches!(c, '_' | '.' | '-') {
            parts.push(&version[start..i]);
            start = i + 1;
        }
    }
    parts.push(&version[start..]);
    // The last part's digits, then one letter: `(\d+)([a-zA-Z])` at its start.
    let last = parts.len() - 1;
    let tail = parts[last];
    let k = tail.bytes().take_while(u8::is_ascii_digit).count();
    let mut alpha = '_';
    if k > 0 && k < tail.len() && tail.as_bytes()[k].is_ascii_alphabetic() {
        alpha = tail.as_bytes()[k] as char;
        parts[last] = &tail[..k];
    }
    let mut nums = Vec::new();
    for p in &parts {
        match py_int(p) {
            Some(n) => nums.push(n),
            None => return Ok(String::new()),
        }
    }
    while nums.len() < 3 {
        nums.push((false, "0".to_string()));
    }
    Ok(format!("{}.{}.{}.{}", pad5(&nums[0]), pad5(&nums[1]), pad5(&nums[2]), alpha))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_version_can_end_where_python_lets_it() {
        let s: Vec<char> = "1.2a".chars().collect();
        assert_eq!(ver_ends(&s, 0), [1, 3, 4]);
        // a separator cannot end a version: "1." has no digit after the dot
        let s: Vec<char> = "1.".chars().collect();
        assert_eq!(ver_ends(&s, 0), [1]);
        let s: Vec<char> = "1..2".chars().collect();
        assert_eq!(ver_ends(&s, 0), [1]);
        let s: Vec<char> = "x1".chars().collect();
        assert!(ver_ends(&s, 0).is_empty());
    }

    #[test]
    fn wildcards_match_whole_names_without_regard_to_case() {
        let p = Pattern::new("Better Bodies*.esp").unwrap();
        assert!(p.is_match("better bodies 2.esp"));
        assert!(!p.is_match("Better Bodies 2.esm"));
        let p = Pattern::new("a?.esp").unwrap();
        assert!(p.is_match("a.esp") && p.is_match("ab.esp") && !p.is_match("abc.esp"));
        let p = Pattern::new("x-<ver>.esp").unwrap();
        assert!(p.is_match("X-1.0.ESP") && !p.is_match("x-.esp"));
    }

    #[test]
    fn a_plain_pattern_takes_the_first_equal_name() {
        let pool = ["MOD.ESP".to_string(), "mod.esp".to_string()];
        assert_eq!(expand("Mod.esp", &pool), Ok(vec!["MOD.ESP"]));
        assert_eq!(expand("Mod [x].esp", &pool), Ok(vec![]));
    }

    #[test]
    fn a_bracket_or_bar_in_a_pattern_is_plain_text() {
        let pool = ["a.esp".to_string(), "[a] 1.esp".to_string(), "a|b.esp".to_string()];
        assert_eq!(expand("[a]*", &pool), Ok(vec!["[a] 1.esp"]));
        assert_eq!(expand("a|b?.esp", &pool), Ok(vec!["a|b.esp"]));
        assert_eq!(expand("[a]", &pool), Ok(vec![]));
        let julan = ["Julan Ashlander Companion 2.02 [Rebirth].esp".to_string()];
        assert_eq!(expand("Julan Ashlander Companion <VER> [Rebirth].esp", &julan).map(|v| v.len()), Ok(1));
    }

    #[test]
    fn versions_pad_and_keep_three_numbers() {
        assert_eq!(format_version("1.0a").as_deref(), Ok("00001.00000.00000.a"));
        assert_eq!(format_version("1.2.3.4").as_deref(), Ok("00001.00002.00003._"));
        assert_eq!(format_version("1.2.3.4.5").as_deref(), Ok(""));
        assert_eq!(format_version("1.2.3.4_5").as_deref(), Ok("00001.00002.00003._"));
        assert_eq!(format_version("1.2.3.-5").as_deref(), Ok("00001.00002.00003._"));
        assert_eq!(format_version("٣"), Err(Unsupported));
    }
}
