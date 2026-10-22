//! The part of Python's `re` that the rule code uses, matched by hand: this crate takes no
//! dependencies, and the regular expressions of `wraithguard/rules/expressions.py` and
//! `predicates.py` (and the `/regex/` a rule's `[DESC]` carries) are Python's.
//!
//! Supported: literals, `.`, classes (`[a-z]`, `[^...]`, ranges, `\d \w \s`), `^ $ \b \B`,
//! groups (capturing and `(?:...)`), alternation, and the quantifiers `* + ? {m} {m,} {,n}
//! {m,n}` with their lazy forms. Matching backtracks in the order Python's does, so which
//! match a pattern finds (not only whether it finds one) is Python's.
//!
//! Two answers other than a match:
//!
//! - [`PatError::Invalid`]: Python would raise `re.error` for this pattern (an unbalanced
//!   parenthesis, "nothing to repeat", a bad escape, an unterminated class, a reversed range).
//! - [`PatError::Unsupported`] (and [`Unsupported`] from a match): not read here, never guessed;
//!   the caller keeps Python's answer. Lookarounds, flags, named groups, back-references,
//!   possessive quantifiers, `\A \Z \x \u \N`, octal escapes, an odd `{`, a repeat count over
//!   1000, a pattern that backtracks past a step budget, a non-ASCII letter written in a
//!   pattern that ignores case, and a repeated group that can match nothing.
//!
//! Known and left: `\w`, `\W`, `\b` and `\B` take a non-ASCII character as a word character
//! when Rust's `is_alphanumeric` does, which differs from Python's `str.isalnum` for a few
//! combining marks and ideographic numerals; `\s` is Python's `str.isspace`; `\d` is Python's
//! decimal digits as of Unicode 15.0 (a table here, for Python 3.12); and `\B` on an
//! empty string is declined (the answer changed between Python versions).
//!
//! Ignoring case is Python's simple case folding, which for ASCII letters adds only `i` with
//! `İ` and `ı`, `k` with the Kelvin sign and `s` with `ſ`.
//!
//! Written from Python's documented `re` behaviour, not its code.

use crate::patterns::Unsupported;

/// Why a pattern was not compiled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PatError {
    /// Python raises `re.error` for it.
    Invalid,
    /// Not read here.
    Unsupported,
}

impl From<Unsupported> for PatError {
    fn from(_: Unsupported) -> Self {
        PatError::Unsupported
    }
}

/// The flags this reader takes (Python's `re.IGNORECASE` and `re.MULTILINE`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Flags {
    /// `re.IGNORECASE`.
    pub ignore_case: bool,
    /// `re.MULTILINE`: `^` and `$` at every line.
    pub multiline: bool,
}

#[derive(Clone, Debug)]
enum Item {
    Ch(char),
    Range(char, char),
    Digit,
    NotDigit,
    Word,
    NotWord,
    Space,
    NotSpace,
}

#[derive(Clone, Debug)]
struct Class {
    neg: bool,
    items: Vec<Item>,
}

#[derive(Clone, Debug)]
enum Node {
    Lit(char),
    Any,
    Class(Class),
    Bol,
    Eol,
    WordB,
    NotWordB,
    Group(usize, Box<Node>),
    Cat(Vec<Node>),
    Alt(Vec<Node>),
    Rep { node: Box<Node>, min: usize, max: usize, lazy: bool },
}

const INF: usize = usize::MAX;
/// The largest finite repeat count read.
const MAX_COUNT: usize = 1000;
/// How many steps a match may take before it is declined.
const STEP_BUDGET: usize = 2_000_000;

/// A compiled pattern.
#[derive(Clone, Debug)]
pub struct Regex {
    node: Node,
    groups: usize,
    flags: Flags,
}

/// A match: where it is, and its groups (0 is the whole), as char positions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Match {
    caps: Vec<Option<(usize, usize)>>,
}

impl Match {
    /// Where the whole match starts.
    pub fn start(&self) -> usize {
        self.caps[0].map_or(0, |c| c.0)
    }

    /// Where the whole match ends.
    pub fn end(&self) -> usize {
        self.caps[0].map_or(0, |c| c.1)
    }

    /// Where group `n` matched, if it did.
    pub fn group(&self, n: usize) -> Option<(usize, usize)> {
        self.caps.get(n).copied().flatten()
    }
}

// ---------------------------------------------------------------- the pattern's syntax

struct Parser<'a> {
    c: &'a [char],
    i: usize,
    groups: usize,
    flags: Flags,
}

impl Parser<'_> {
    fn peek(&self) -> Option<char> {
        self.c.get(self.i).copied()
    }

    fn alt(&mut self) -> Result<Node, PatError> {
        let mut branches = vec![self.cat()?];
        while self.peek() == Some('|') {
            self.i += 1;
            branches.push(self.cat()?);
        }
        Ok(if branches.len() == 1 { branches.remove(0) } else { Node::Alt(branches) })
    }

    /// `{` at `self.i` read as a quantifier: `(min, max, length)`, or `None` when it is a
    /// literal brace (Python's rule: digits, an optional comma and digits, then `}`).
    fn brace(&self) -> Result<Option<(usize, usize, usize)>, PatError> {
        let mut j = self.i + 1;
        let digits = |j: &mut usize| {
            let from = *j;
            while self.c.get(*j).is_some_and(char::is_ascii_digit) {
                *j += 1;
            }
            self.c[from..*j].iter().collect::<String>()
        };
        let lo = digits(&mut j);
        let (hi, comma) = if self.c.get(j) == Some(&',') {
            j += 1;
            (digits(&mut j), true)
        } else {
            (lo.clone(), false)
        };
        if self.c.get(j) != Some(&'}') {
            return Ok(None);
        }
        if lo.is_empty() && hi.is_empty() {
            return Err(PatError::Unsupported); // `{}` and `{,}`
        }
        let num = |s: &str, default: usize| -> Result<usize, PatError> {
            if s.is_empty() {
                return Ok(default);
            }
            match s.parse::<usize>() {
                Ok(n) if n <= MAX_COUNT => Ok(n),
                _ => Err(PatError::Unsupported),
            }
        };
        let min = num(&lo, 0)?;
        let max = if comma { num(&hi, INF)? } else { min };
        if min > max {
            return Err(PatError::Invalid);
        }
        Ok(Some((min, max, j + 1 - self.i)))
    }

    fn quantifier(&self) -> Result<Option<(usize, usize, usize)>, PatError> {
        match self.peek() {
            Some('*') => Ok(Some((0, INF, 1))),
            Some('+') => Ok(Some((1, INF, 1))),
            Some('?') => Ok(Some((0, 1, 1))),
            Some('{') => self.brace(),
            _ => Ok(None),
        }
    }

    fn cat(&mut self) -> Result<Node, PatError> {
        let mut items: Vec<Node> = Vec::new();
        while let Some(ch) = self.peek() {
            if ch == '|' || ch == ')' {
                break;
            }
            let atom = self.atom(ch)?;
            let repeatable = !matches!(atom, Node::Bol | Node::Eol | Node::WordB | Node::NotWordB);
            let mut node = atom;
            if let Some((min, max, len)) = self.quantifier()? {
                if !repeatable {
                    return Err(PatError::Invalid); // nothing to repeat
                }
                self.i += len;
                let lazy = self.peek() == Some('?');
                if lazy {
                    self.i += 1;
                }
                if !is_single(&node) && nullable(&node) {
                    return Err(PatError::Unsupported); // Python's captures for an empty round
                }
                node = Node::Rep { node: Box::new(node), min, max, lazy };
                // another quantifier right after: possessive (declined) or a repeat of a repeat
                match self.peek() {
                    Some('+') if !lazy => return Err(PatError::Unsupported),
                    Some('*' | '+' | '?') => return Err(PatError::Invalid),
                    Some('{') if self.brace()?.is_some() => return Err(PatError::Invalid),
                    _ => {}
                }
            }
            items.push(node);
        }
        Ok(if items.len() == 1 { items.remove(0) } else { Node::Cat(items) })
    }

    fn atom(&mut self, ch: char) -> Result<Node, PatError> {
        self.i += 1;
        match ch {
            '(' => {
                let index = if self.peek() == Some('?') {
                    if self.c.get(self.i + 1) != Some(&':') {
                        return Err(PatError::Unsupported);
                    }
                    self.i += 2;
                    None
                } else {
                    self.groups += 1;
                    Some(self.groups)
                };
                let inner = self.alt()?;
                if self.peek() != Some(')') {
                    return Err(PatError::Invalid); // missing )
                }
                self.i += 1;
                Ok(match index {
                    Some(n) => Node::Group(n, Box::new(inner)),
                    // a group, so that what is inside is repeatable even if it is an anchor
                    None => Node::Cat(vec![inner]),
                })
            }
            '[' => self.class(),
            '.' => Ok(Node::Any),
            '^' => Ok(Node::Bol),
            '$' => Ok(Node::Eol),
            '*' | '+' | '?' => Err(PatError::Invalid), // nothing to repeat
            '{' => {
                self.i -= 1;
                if self.brace()?.is_some() {
                    return Err(PatError::Invalid);
                }
                self.i += 1;
                self.lit('{')
            }
            '\\' => self.escape(),
            other => self.lit(other),
        }
    }

    fn lit(&self, c: char) -> Result<Node, PatError> {
        if self.flags.ignore_case && !c.is_ascii() {
            return Err(PatError::Unsupported);
        }
        Ok(Node::Lit(c))
    }

    /// What a backslash and one more character stand for, outside or inside a class:
    /// a class item, or a character.
    fn escape_item(&mut self, in_class: bool) -> Result<Result<Item, char>, PatError> {
        let Some(c) = self.peek() else { return Err(PatError::Invalid) };
        self.i += 1;
        Ok(match c {
            'd' => Ok(Item::Digit),
            'D' => Ok(Item::NotDigit),
            'w' => Ok(Item::Word),
            'W' => Ok(Item::NotWord),
            's' => Ok(Item::Space),
            'S' => Ok(Item::NotSpace),
            'n' => Err('\n'),
            't' => Err('\t'),
            'r' => Err('\r'),
            'f' => Err('\u{c}'),
            'v' => Err('\u{b}'),
            'a' => Err('\u{7}'),
            'b' if in_class => Err('\u{8}'),
            'A' | 'Z' | 'x' | 'u' | 'U' | 'N' | '0'..='9' => return Err(PatError::Unsupported),
            'b' | 'B' => return Err(PatError::Unsupported), // handled by the caller outside a class
            c if c.is_ascii_alphabetic() => return Err(PatError::Invalid), // bad escape
            c => Err(c),
        })
    }

    fn escape(&mut self) -> Result<Node, PatError> {
        match self.peek() {
            Some('b') => {
                self.i += 1;
                return Ok(Node::WordB);
            }
            Some('B') => {
                self.i += 1;
                return Ok(Node::NotWordB);
            }
            _ => {}
        }
        Ok(match self.escape_item(false)? {
            Ok(item) => Node::Class(Class { neg: false, items: vec![item] }),
            Err(c) => self.lit(c)?,
        })
    }

    fn class(&mut self) -> Result<Node, PatError> {
        let neg = self.peek() == Some('^');
        if neg {
            self.i += 1;
        }
        let mut items: Vec<Item> = Vec::new();
        let mut first = true;
        loop {
            let Some(ch) = self.peek() else { return Err(PatError::Invalid) }; // unterminated
            self.i += 1;
            if ch == ']' && !first {
                break;
            }
            first = false;
            let lo = if ch == '\\' { self.escape_item(true)? } else { Err(ch) };
            let Err(lo_char) = lo else {
                // a class escape: `-` after it is a range with a class, an error
                if self.peek() == Some('-') && self.c.get(self.i + 1).is_some_and(|&n| n != ']') {
                    return Err(PatError::Invalid);
                }
                items.extend(lo.ok());
                continue;
            };
            if self.flags.ignore_case && !lo_char.is_ascii() {
                return Err(PatError::Unsupported);
            }
            if self.peek() == Some('-') && self.c.get(self.i + 1).is_some_and(|&n| n != ']') {
                self.i += 1;
                let Some(h) = self.peek() else { return Err(PatError::Invalid) };
                self.i += 1;
                let hi = if h == '\\' { self.escape_item(true)? } else { Err(h) };
                let Err(hi_char) = hi else { return Err(PatError::Invalid) };
                if self.flags.ignore_case && !hi_char.is_ascii() {
                    return Err(PatError::Unsupported);
                }
                if hi_char < lo_char {
                    return Err(PatError::Invalid); // bad character range
                }
                items.push(Item::Range(lo_char, hi_char));
            } else {
                items.push(Item::Ch(lo_char));
            }
        }
        Ok(Node::Class(Class { neg, items }))
    }
}

impl Regex {
    /// Compile a pattern as Python's `re.compile(pattern, flags)` reads it.
    ///
    /// # Errors
    /// [`PatError::Invalid`] where Python raises `re.error`; [`PatError::Unsupported`] where
    /// this does not read the pattern.
    pub fn new(pattern: &str, flags: Flags) -> Result<Regex, PatError> {
        let chars: Vec<char> = pattern.chars().collect();
        let mut p = Parser { c: &chars, i: 0, groups: 0, flags };
        let node = p.alt()?;
        if p.i < chars.len() {
            return Err(PatError::Invalid); // unbalanced parenthesis
        }
        Ok(Regex { node, groups: p.groups, flags })
    }

    /// `pattern.match(text, start)`: a match that begins at `start`.
    ///
    /// # Errors
    /// [`Unsupported`] when the match met something it does not read.
    pub fn match_at(&self, s: &[char], start: usize) -> Result<Option<Match>, Unsupported> {
        let mut ctx = Ctx { s, flags: self.flags, caps: vec![None; self.groups + 1], bad: false, steps: 0 };
        let mut end = None;
        let ok = m(&mut ctx, &self.node, start, &mut |_, j| {
            end = Some(j);
            true
        });
        if ctx.bad {
            return Err(Unsupported);
        }
        if !ok {
            return Ok(None);
        }
        let mut caps = ctx.caps;
        caps[0] = Some((start, end.unwrap_or(start)));
        Ok(Some(Match { caps }))
    }

    /// `pattern.search(text, from)`: the leftmost match from `from`.
    ///
    /// # Errors
    /// [`Unsupported`], as [`Regex::match_at`].
    pub fn search(&self, s: &[char], from: usize) -> Result<Option<Match>, Unsupported> {
        for start in from..=s.len() {
            if let Some(found) = self.match_at(s, start)? {
                return Ok(Some(found));
            }
        }
        Ok(None)
    }

    /// `pattern.finditer(text)`: the matches left to right without overlap.
    ///
    /// # Errors
    /// [`Unsupported`], as [`Regex::match_at`].
    pub fn find_all(&self, s: &[char]) -> Result<Vec<Match>, Unsupported> {
        let mut out = Vec::new();
        let mut at = 0;
        while at <= s.len() {
            let Some(found) = self.search(s, at)? else { break };
            at = if found.end() == found.start() { found.end() + 1 } else { found.end() };
            out.push(found);
        }
        Ok(out)
    }
}

// ---------------------------------------------------------------- matching

struct Ctx<'a> {
    s: &'a [char],
    flags: Flags,
    caps: Vec<Option<(usize, usize)>>,
    /// Something this does not read was met: the answer is declined.
    bad: bool,
    steps: usize,
}

type Cont<'k> = &'k mut dyn FnMut(&mut Ctx<'_>, usize) -> bool;

/// Python's `str.isspace`, as `\s` reads it.
fn is_space(c: char) -> bool {
    c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
}

fn is_word(c: char) -> bool {
    c == '_' || c.is_alphanumeric()
}

/// The ASCII letters a non-ASCII character is the same as when case is ignored.
fn folds_to(c: char) -> &'static str {
    match c {
        '\u{130}' | '\u{131}' => "iI",
        '\u{17f}' => "sS",
        '\u{212a}' => "kK",
        _ => "",
    }
}

/// The decimal digits outside ASCII (Unicode category Nd, as of Unicode 15.0, which is Python
/// 3.12's): what Python's `\d` reads in a `str`, and which `std` cannot tell.
const DECIMAL_RANGES: [(u32, u32); 63] = [
    (0x660, 0x669),
    (0x6F0, 0x6F9),
    (0x7C0, 0x7C9),
    (0x966, 0x96F),
    (0x9E6, 0x9EF),
    (0xA66, 0xA6F),
    (0xAE6, 0xAEF),
    (0xB66, 0xB6F),
    (0xBE6, 0xBEF),
    (0xC66, 0xC6F),
    (0xCE6, 0xCEF),
    (0xD66, 0xD6F),
    (0xDE6, 0xDEF),
    (0xE50, 0xE59),
    (0xED0, 0xED9),
    (0xF20, 0xF29),
    (0x1040, 0x1049),
    (0x1090, 0x1099),
    (0x17E0, 0x17E9),
    (0x1810, 0x1819),
    (0x1946, 0x194F),
    (0x19D0, 0x19D9),
    (0x1A80, 0x1A89),
    (0x1A90, 0x1A99),
    (0x1B50, 0x1B59),
    (0x1BB0, 0x1BB9),
    (0x1C40, 0x1C49),
    (0x1C50, 0x1C59),
    (0xA620, 0xA629),
    (0xA8D0, 0xA8D9),
    (0xA900, 0xA909),
    (0xA9D0, 0xA9D9),
    (0xA9F0, 0xA9F9),
    (0xAA50, 0xAA59),
    (0xABF0, 0xABF9),
    (0xFF10, 0xFF19),
    (0x104A0, 0x104A9),
    (0x10D30, 0x10D39),
    (0x11066, 0x1106F),
    (0x110F0, 0x110F9),
    (0x11136, 0x1113F),
    (0x111D0, 0x111D9),
    (0x112F0, 0x112F9),
    (0x11450, 0x11459),
    (0x114D0, 0x114D9),
    (0x11650, 0x11659),
    (0x116C0, 0x116C9),
    (0x11730, 0x11739),
    (0x118E0, 0x118E9),
    (0x11950, 0x11959),
    (0x11C50, 0x11C59),
    (0x11D50, 0x11D59),
    (0x11DA0, 0x11DA9),
    (0x11F50, 0x11F59),
    (0x16A60, 0x16A69),
    (0x16AC0, 0x16AC9),
    (0x16B50, 0x16B59),
    (0x1D7CE, 0x1D7FF),
    (0x1E140, 0x1E149),
    (0x1E2F0, 0x1E2F9),
    (0x1E4F0, 0x1E4F9),
    (0x1E950, 0x1E959),
    (0x1FBF0, 0x1FBF9),
];

/// Python's `str.isdecimal`, as `\d` reads it.
fn is_decimal(c: char) -> bool {
    c.is_ascii_digit()
        || (!c.is_ascii() && DECIMAL_RANGES.iter().any(|&(a, b)| (a..=b).contains(&(c as u32))))
}

fn item_has(item: &Item, c: char) -> bool {
    match item {
        Item::Ch(x) => *x == c,
        Item::Range(a, b) => (*a..=*b).contains(&c),
        Item::Digit => is_decimal(c),
        Item::NotDigit => !is_decimal(c),
        Item::Word => is_word(c),
        Item::NotWord => !is_word(c),
        Item::Space => is_space(c),
        Item::NotSpace => !is_space(c),
    }
}

fn class_has(class: &Class, c: char, ctx: &mut Ctx<'_>) -> bool {
    let mut candidates = vec![c];
    if ctx.flags.ignore_case {
        if c.is_ascii_alphabetic() {
            candidates.push(if c.is_ascii_lowercase() {
                c.to_ascii_uppercase()
            } else {
                c.to_ascii_lowercase()
            });
        }
        candidates.extend(folds_to(c).chars());
    }
    let mut hit = false;
    for cand in candidates {
        for item in &class.items {
            if item_has(item, cand) {
                hit = true;
            }
        }
    }
    hit != class.neg
}

/// Whether the one-character node `node` takes `c`.
fn one(node: &Node, c: char, ctx: &mut Ctx<'_>) -> bool {
    match node {
        Node::Lit(x) => {
            *x == c || (ctx.flags.ignore_case && (x.eq_ignore_ascii_case(&c) || folds_to(c).contains(*x)))
        }
        Node::Any => c != '\n',
        Node::Class(class) => class_has(class, c, ctx),
        _ => false,
    }
}

/// Whether the node can match without taking a character.
fn nullable(node: &Node) -> bool {
    match node {
        Node::Lit(_) | Node::Any | Node::Class(_) => false,
        Node::Bol | Node::Eol | Node::WordB | Node::NotWordB => true,
        Node::Group(_, inner) => nullable(inner),
        Node::Cat(nodes) => nodes.iter().all(nullable),
        Node::Alt(branches) => branches.iter().any(nullable),
        Node::Rep { node, min, .. } => *min == 0 || nullable(node),
    }
}

fn is_single(node: &Node) -> bool {
    matches!(node, Node::Lit(_) | Node::Any | Node::Class(_))
}

fn word_at(ctx: &Ctx<'_>, i: usize) -> bool {
    ctx.s.get(i).is_some_and(|&c| is_word(c))
}

fn m(ctx: &mut Ctx<'_>, node: &Node, i: usize, k: Cont<'_>) -> bool {
    ctx.steps += 1;
    if ctx.steps > STEP_BUDGET {
        ctx.bad = true;
    }
    if ctx.bad {
        return false;
    }
    match node {
        Node::Lit(_) | Node::Any | Node::Class(_) => {
            i < ctx.s.len() && one(node, ctx.s[i], ctx) && k(ctx, i + 1)
        }
        Node::Bol => (i == 0 || (ctx.flags.multiline && ctx.s[i - 1] == '\n')) && k(ctx, i),
        Node::Eol => {
            let n = ctx.s.len();
            let at = if ctx.flags.multiline {
                i == n || ctx.s[i] == '\n'
            } else {
                i == n || (i + 1 == n && ctx.s[i] == '\n')
            };
            at && k(ctx, i)
        }
        Node::WordB | Node::NotWordB => {
            if ctx.s.is_empty() {
                if matches!(node, Node::NotWordB) {
                    ctx.bad = true; // the answer changed between Python versions
                }
                return false;
            }
            let before = i > 0 && word_at(ctx, i - 1);
            let after = word_at(ctx, i);
            ((before != after) == matches!(node, Node::WordB)) && k(ctx, i)
        }
        Node::Group(n, inner) => {
            let n = *n;
            m(ctx, inner, i, &mut |ctx, j| {
                let prev = ctx.caps[n];
                ctx.caps[n] = Some((i, j));
                if k(ctx, j) {
                    true
                } else {
                    ctx.caps[n] = prev;
                    false
                }
            })
        }
        Node::Cat(nodes) => cat(ctx, nodes, i, k),
        Node::Alt(branches) => {
            for b in branches {
                if m(ctx, b, i, &mut *k) {
                    return true;
                }
            }
            false
        }
        Node::Rep { node, min, max, lazy } => {
            if is_single(node) {
                rep_single(ctx, node, *min, *max, *lazy, i, k)
            } else {
                rep(ctx, node, (*min, *max, *lazy), 0, i, k)
            }
        }
    }
}

fn cat(ctx: &mut Ctx<'_>, nodes: &[Node], i: usize, k: Cont<'_>) -> bool {
    match nodes.split_first() {
        None => k(ctx, i),
        Some((first, rest)) => m(ctx, first, i, &mut |ctx, j| cat(ctx, rest, j, &mut *k)),
    }
}

fn rep_single(
    ctx: &mut Ctx<'_>,
    node: &Node,
    min: usize,
    max: usize,
    lazy: bool,
    i: usize,
    k: Cont<'_>,
) -> bool {
    let n = ctx.s.len();
    if lazy {
        let mut taken = 0;
        while taken < min {
            if i + taken >= n || !one(node, ctx.s[i + taken], ctx) {
                return false;
            }
            taken += 1;
        }
        loop {
            ctx.steps += 1;
            if k(ctx, i + taken) {
                return true;
            }
            if ctx.bad || taken >= max || i + taken >= n || !one(node, ctx.s[i + taken], ctx) {
                return false;
            }
            taken += 1;
        }
    }
    let mut count = 0;
    while count < max && i + count < n && one(node, ctx.s[i + count], ctx) {
        count += 1;
    }
    if count < min {
        return false;
    }
    loop {
        ctx.steps += 1;
        if k(ctx, i + count) {
            return true;
        }
        if ctx.bad || count == min {
            return false;
        }
        count -= 1;
    }
}

/// A repeat of something longer than a character: `count` iterations are done, at `i`.
fn rep(
    ctx: &mut Ctx<'_>,
    node: &Node,
    (min, max, lazy): (usize, usize, bool),
    count: usize,
    i: usize,
    k: Cont<'_>,
) -> bool {
    if lazy {
        if count >= min && k(ctx, i) {
            return true;
        }
        return rep_more(ctx, node, (min, max, lazy), count, i, k);
    }
    if rep_more(ctx, node, (min, max, lazy), count, i, &mut *k) {
        return true;
    }
    count >= min && k(ctx, i)
}

/// One more iteration of the repeat, then the rest.
fn rep_more(
    ctx: &mut Ctx<'_>,
    node: &Node,
    bounds: (usize, usize, bool),
    count: usize,
    i: usize,
    k: Cont<'_>,
) -> bool {
    let (min, max, _lazy) = bounds;
    count < max
        && m(ctx, node, i, &mut |ctx, j| {
            // an iteration that took nothing stops the repeat, as Python's does
            if j == i && count >= min {
                return false;
            }
            rep(ctx, node, bounds, count + 1, j, &mut *k)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chars(s: &str) -> Vec<char> {
        s.chars().collect()
    }

    fn find(p: &str, f: Flags, text: &str) -> Option<(usize, usize)> {
        let re = Regex::new(p, f).unwrap();
        re.search(&chars(text), 0).unwrap().map(|m| (m.start(), m.end()))
    }

    const NONE: Flags = Flags { ignore_case: false, multiline: false };
    const ICASE: Flags = Flags { ignore_case: true, multiline: false };

    #[test]
    fn literals_classes_and_quantifiers() {
        assert_eq!(find("a.c", NONE, "xxabc"), Some((2, 5)));
        assert_eq!(find("[a-c]+", NONE, "zzabcab"), Some((2, 7)));
        assert_eq!(find("a{2,3}", NONE, "aaaa"), Some((0, 3)));
        assert_eq!(find("a{2,3}?", NONE, "aaaa"), Some((0, 2)));
        assert_eq!(find("(?:ab)+c", NONE, "ababc"), Some((0, 5)));
        assert_eq!(find("x|yz", NONE, "ayz"), Some((1, 3)));
    }

    #[test]
    fn greedy_and_lazy_pick_the_matches_python_picks() {
        let re = Regex::new(r"^\[\s*VER\s*([=<>])\s*(\d+(?:[_.-]?\d+)*[a-zA-Z]?)\s*([^\]]+?)\s*\]$", ICASE)
            .unwrap();
        let t = chars("[VER = 1.0]");
        let got = re.match_at(&t, 0).unwrap().unwrap();
        // the version gives back its last digits so that a plugin is left: "1" and ".0"
        assert_eq!(got.group(2), Some((7, 8)));
        assert_eq!(got.group(3), Some((8, 10)));
    }

    #[test]
    fn ignoring_case_folds_the_few_non_ascii_letters_python_does() {
        assert_eq!(find("desc", ICASE, "xDE\u{17f}C"), Some((1, 5)));
        assert_eq!(find("desc", NONE, "xDE\u{17f}C"), None);
        assert_eq!(find("[a-z]k", ICASE, "a\u{212a}"), Some((0, 2)));
    }

    #[test]
    fn anchors_and_word_boundaries() {
        assert_eq!(find("^a", NONE, "ba"), None);
        assert_eq!(find("a$", NONE, "a\n"), Some((0, 1)));
        let ml = Flags { ignore_case: false, multiline: true };
        assert_eq!(find("^b", ml, "a\nb"), Some((2, 3)));
        assert_eq!(find(r"\bALL\b", NONE, "x ALL y"), Some((2, 5)));
        assert_eq!(find(r"\bALL\b", NONE, "xALL"), None);
    }

    #[test]
    fn what_python_rejects_is_invalid() {
        for p in ["(a", "a)", "*a", "a**", "[a", "[]", "[z-a]", r"\q", "a\\", "a{2,1}", "^*", r"[\d-z]"] {
            assert_eq!(Regex::new(p, NONE).unwrap_err(), PatError::Invalid, "{p}");
        }
        assert!(Regex::new("[]a]", NONE).is_ok());
        assert!(Regex::new("a{1", NONE).is_ok());
    }

    #[test]
    fn what_is_not_read_is_declined() {
        for p in ["(?i)a", "(?=a)", "(?P<n>a)", r"(a)\1", "a*+", r"\x41", "a{}", "a{1001}"] {
            assert_eq!(Regex::new(p, NONE).unwrap_err(), PatError::Unsupported, "{p}");
        }
        let re = Regex::new(r"\d", NONE).unwrap();
        assert_eq!(re.search(&chars("x\u{663}"), 0).unwrap().map(|m| m.start()), Some(1));
        assert!(Regex::new(r"\D", NONE).unwrap().search(&chars("\u{663}"), 0).unwrap().is_none());
    }
}
