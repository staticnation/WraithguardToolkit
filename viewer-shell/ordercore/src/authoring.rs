//! Writing mlox rules to the rule-base's own standards, as `wraithguard/rules/authoring.py` and
//! the user's rule file of `wraithguard_toolkit.py` do: build a [`Rule`] as data, [`render_rule`]
//! it as the text a rules file holds, [`validate`] it against the guidelines, and
//! [`append_authored_rule`] it to the personal rules file.
//!
//! The engine already *reads* every rule form ([`crate::rules`], [`crate::predicates`]); this
//! writes them, and enforces the conventions the community guidelines set for editors, because a
//! rule that parses is not the same as a rule that is any good:
//!
//! - every rule cites a source (`(Ref: ...)`), with whitespace around a URL so that pages that
//!   auto-link do not swallow the closing parenthesis;
//! - the shape is right (`[Order]` needs two plugins, `[Requires]` and `[Patch]` exactly two
//!   expressions, `[Conflict]` two or more) and `[Patch]` cannot express `NOT`;
//! - a plugin name cannot contain what mlox reads as syntax (`[`, `]`, `;`, a line break): a rule
//!   that quietly refers to a different plugin than the one typed is the worst outcome available;
//! - `[NearStart]`/`[NearEnd]` and filename expansion are allowed and warned about.
//!
//! Nothing here but [`append_authored_rule`] touches the disk. Each [`Problem`] carries its
//! severity so the caller decides what is fatal: errors refuse, warnings do not.
//!
//! What is not read is **declined** ([`Unsupported`]), as in [`crate::patterns`]: a `[DESC]` regex
//! that Python's `re.compile` rejects or that [`crate::pyre`] does not read (Python's complaint is
//! worded by Python), and a problem about a name with non-ASCII characters (its `repr`, which
//! escapes what Python calls unprintable, is Python's table), and an existing rules file that is
//! not UTF-8 (Python would replace the bytes).
//!
//! Known and left: `lower()` is Rust's `to_lowercase`. Two checks of the Python can never fire
//! (an inline message holding `]`, which the block form always takes; a URL citation that
//! [`format_ref`] has not spaced); they are ported as written.
//!
//! Written from the rule-base guidelines, not from mlox's code.

use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::Path;
use std::sync::OnceLock;

use crate::export::{decode_cfg, LineEnding};
use crate::patterns::{has_meta, Unsupported};
use crate::pyre::{Flags, Regex};
use crate::rules::{is_space, splitlines};
use crate::updates::py_repr;

/// The rule labels this module can write, in the order the guidelines introduce them. `Version`
/// is absent: it is a rule-base header, not something a user writes about their own load order.
pub const RULE_KINDS: [&str; 7] = ["Order", "NearStart", "NearEnd", "Note", "Requires", "Conflict", "Patch"];
/// Rules that are a plain list of plugins: no message, no expressions.
pub const ORDERING_KINDS: [&str; 3] = ["Order", "NearStart", "NearEnd"];
/// Rules built from boolean expressions and carrying a message.
pub const WARNING_KINDS: [&str; 4] = ["Note", "Requires", "Conflict", "Patch"];
/// The comparison operators `[VER]` accepts.
pub const VERSION_OPERATORS: [&str; 3] = ["<", "=", ">"];
/// What every personal rules file starts with.
pub const USER_RULES_HEADER: &str = ";; Personal mlox rules -- written by you (with help from Wraithguard Toolkit's\n;; rule maker). Keep this file LAST in the rule-files list: later files win\n;; rule conflicts, so your rules override mlox_base/mlox_user.\n;; Syntax: https://morrowind-modding.github.io/modding-tools/sorting-plugin-load-order/mlox/mlox-rule-guidelines\n";

/// Characters a plugin name may never contain: mlox gives them structural meaning.
const FORBIDDEN_IN_NAMES: [char; 5] = ['[', ']', ';', '\n', '\r'];
/// What makes a filename an expansion pattern rather than a literal.
const EXPANSION_CHARS: [&str; 3] = ["?", "*", "<VER>"];

const NAME_FULL: &str = r"\S[^\n]*?\.(?:esp|esm|omwaddon|omwgame|omwscripts)\*?$";
const VERSION_ONLY: &str = r"^(\d+(?:[_.-]?\d+)*[a-zA-Z]?)$";
const URL: &str = r"\b(?:https?://|www\.)\S+";

fn strip(s: &str) -> &str {
    s.trim_matches(is_space)
}

fn rstrip(s: &str) -> &str {
    s.trim_end_matches(is_space)
}

fn compiled(cell: &'static OnceLock<Regex>, pattern: &str, ignore_case: bool) -> &'static Regex {
    cell.get_or_init(|| {
        Regex::new(pattern, Flags { ignore_case, multiline: false }).expect("a fixed expression compiles")
    })
}

/// `pattern.fullmatch(text)`: whether the whole text matches (the patterns here end in `$`, so a
/// match that stops before a final newline is not a full one).
fn fullmatch(
    cell: &'static OnceLock<Regex>,
    pattern: &str,
    ignore_case: bool,
    text: &str,
) -> Result<bool, Unsupported> {
    let chars: Vec<char> = text.chars().collect();
    Ok(compiled(cell, pattern, ignore_case).match_at(&chars, 0)?.is_some_and(|m| m.end() == chars.len()))
}

fn url_in(text: &str) -> Result<bool, Unsupported> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let chars: Vec<char> = text.chars().collect();
    Ok(compiled(&RE, URL, true).search(&chars, 0)?.is_some())
}

/// The mark a highlight level is written with, or `None` for a level that has none.
fn priority_mark(priority: i64) -> Option<&'static str> {
    match priority {
        0 => Some(""),
        1 => Some("!"),
        2 => Some("!!"),
        3 => Some("!!!"),
        _ => None,
    }
}

/// The colour mlox itself highlights a rule kind with, if it does.
fn self_highlighting(kind: &str) -> Option<&'static str> {
    match kind {
        "Requires" => Some("red"),
        "Conflict" | "Patch" => Some("yellow"),
        _ => None,
    }
}

/// How serious a [`Problem`] is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    /// mlox would misread or reject the rule.
    Error,
    /// Valid, but short of the guidelines.
    Warning,
}

impl Severity {
    /// The word the Python uses.
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
        }
    }
}

/// Something wrong with, or questionable about, a rule.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    /// Whether mlox would misread the rule, or it merely falls short.
    pub severity: Severity,
    /// What is wrong, in the guidelines' terms.
    pub message: String,
    /// What to do instead, when the guidelines say; empty otherwise.
    pub remedy: String,
}

impl Problem {
    fn new(severity: Severity, message: impl Into<String>, remedy: impl Into<String>) -> Problem {
        Problem { severity, message: message.into(), remedy: remedy.into() }
    }

    fn error(message: impl Into<String>) -> Problem {
        Problem::new(Severity::Error, message, "")
    }

    fn warning(message: impl Into<String>) -> Problem {
        Problem::new(Severity::Warning, message, "")
    }

    /// Render for display, e.g. `error: [Patch] cannot express NOT -- use a ... instead`.
    pub fn describe(&self) -> String {
        let tail = if self.remedy.is_empty() { String::new() } else { format!(" -- {}", self.remedy) };
        format!("{}: {}{tail}", self.severity.as_str(), self.message)
    }
}

// ---------------------------------------------------------------- expressions

/// A boolean expression of a warning rule.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expr {
    /// The basic predicate: a plugin is active; the name may hold `?`, `*` or `<VER>`.
    Plugin(String),
    /// `[DESC /regex/ plugin]`: the plugin's description matches a regex.
    Desc {
        /// The regular expression, without its slashes.
        pattern: String,
        /// The plugin whose header is searched.
        plugin: String,
        /// Whether the description must *not* match.
        negated: bool,
    },
    /// `[SIZE ### plugin]`: the plugin's file size in bytes.
    Size {
        /// The size.
        size: i64,
        /// The plugin measured.
        plugin: String,
        /// Whether the size must *not* match.
        negated: bool,
    },
    /// `[VER op version plugin]`: compare against the plugin's version.
    Ver {
        /// `<`, `=` or `>`.
        operator: String,
        /// The version, in any of mlox's accepted forms.
        version: String,
        /// The plugin compared.
        plugin: String,
    },
    /// A boolean group: `[ALL ...]`, `[ANY ...]` or `[NOT ...]`.
    Group {
        /// `ALL`, `ANY` or `NOT`.
        operator: String,
        /// The nested expressions.
        parts: Vec<Expr>,
    },
}

impl Expr {
    /// Render as rule text, prefixed by `indent` spaces. A group breaks across lines when it is
    /// long (over 78 columns), with continuation lines indented, which mlox's reader accepts.
    pub fn render(&self, indent: usize) -> String {
        let pad = " ".repeat(indent);
        match self {
            Expr::Plugin(name) => format!("{pad}{}", strip(name)),
            Expr::Desc { pattern, plugin, negated } => {
                format!("{pad}[DESC {}/{pattern}/ {plugin}]", if *negated { "!" } else { "" })
            }
            Expr::Size { size, plugin, negated } => {
                format!("{pad}[SIZE {}{size} {plugin}]", if *negated { "!" } else { "" })
            }
            Expr::Ver { operator, version, plugin } => format!("{pad}[VER {operator} {version} {plugin}]"),
            Expr::Group { operator, parts } => {
                let rendered: Vec<String> = parts.iter().map(|p| p.render(0)).collect();
                let one_line = format!("{pad}[{operator} {}]", rendered.join(" "));
                if one_line.chars().count() <= 78 || parts.is_empty() {
                    return one_line;
                }
                let head = format!("{pad}[{operator} {}", rendered[0]);
                let mut lines = vec![head];
                lines.extend(rendered[1..].iter().map(|t| format!("{}{t}", " ".repeat(indent + 1))));
                format!("{}]", lines.join("\n"))
            }
        }
    }

    /// Every plugin filename this expression mentions, in encounter order, duplicates kept.
    pub fn plugins(&self) -> Vec<String> {
        match self {
            Expr::Plugin(name) => vec![name.clone()],
            Expr::Desc { plugin, .. } | Expr::Size { plugin, .. } | Expr::Ver { plugin, .. } => {
                vec![plugin.clone()]
            }
            Expr::Group { parts, .. } => parts.iter().flat_map(Expr::plugins).collect(),
        }
    }

    /// This expression and every one nested inside it, this one first.
    pub fn walk(&self) -> Vec<&Expr> {
        let mut out = vec![self];
        if let Expr::Group { parts, .. } = self {
            for part in parts {
                out.extend(part.walk());
            }
        }
        out
    }
}

/// An `[ALL ...]` group.
pub fn all_of(parts: Vec<Expr>) -> Expr {
    Expr::Group { operator: "ALL".to_string(), parts }
}

/// An `[ANY ...]` group.
pub fn any_of(parts: Vec<Expr>) -> Expr {
    Expr::Group { operator: "ANY".to_string(), parts }
}

/// A `[NOT ...]` group.
pub fn not_(part: Expr) -> Expr {
    Expr::Group { operator: "NOT".to_string(), parts: vec![part] }
}

// ---------------------------------------------------------------- rules

/// One mlox rule, ready to render and check.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Rule {
    /// One of [`RULE_KINDS`].
    pub kind: String,
    /// For an ordering rule, the plugins in order.
    pub plugins: Vec<String>,
    /// For a warning rule, its expressions.
    pub expressions: Vec<Expr>,
    /// The message (ordering rules have none).
    pub message: String,
    /// The source the rule is based on, rendered as `(Ref: ...)`.
    pub ref_: String,
    /// 0-3, rendered as the `!`/`!!`/`!!!` highlight prefix.
    pub priority: i64,
    /// An optional `@Section` heading written above the rule.
    pub section: String,
    /// Optional `;` comment lines written above the rule.
    pub comment: String,
    /// Render the block form even where the message would fit inline.
    pub force_block: bool,
}

impl Rule {
    /// A rule of a kind, with nothing else set.
    pub fn new(kind: &str) -> Rule {
        Rule { kind: kind.to_string(), ..Rule::default() }
    }

    fn is_ordering(&self) -> bool {
        ORDERING_KINDS.contains(&self.kind.as_str())
    }

    /// Every plugin this rule names, from either half, in encounter order.
    pub fn mentioned_plugins(&self) -> Vec<String> {
        if self.is_ordering() {
            return self.plugins.clone();
        }
        self.expressions.iter().flat_map(Expr::plugins).collect()
    }
}

/// A citation, with the whitespace the guidelines require: a URL right before `)` is swallowed
/// into the link by forums that auto-link, so a citation holding one gets a space before it.
/// Empty when there is nothing to cite.
///
/// # Errors
/// [`Unsupported`] only if the matcher declines the text.
pub fn format_ref(ref_: &str) -> Result<String, Unsupported> {
    let body = ref_.split(is_space).filter(|w| !w.is_empty()).collect::<Vec<_>>().join(" ");
    if body.is_empty() {
        return Ok(String::new());
    }
    Ok(if url_in(&body)? { format!("(Ref: {body} )") } else { format!("(Ref: {body})") })
}

fn message_lines(rule: &Rule) -> Result<Vec<String>, Unsupported> {
    let mark = priority_mark(rule.priority).unwrap_or("");
    let mut lines: Vec<String> = splitlines(&rule.message)
        .into_iter()
        .filter(|l| !strip(l).is_empty())
        .map(|l| rstrip(l).to_string())
        .collect();
    if !mark.is_empty() && !lines.is_empty() {
        lines[0] = format!("{mark} {}", lines[0]);
    } else if !mark.is_empty() {
        lines = vec![mark.to_string()];
    }
    let citation = format_ref(&rule.ref_)?;
    if !citation.is_empty() {
        lines.push(citation);
    }
    Ok(lines)
}

fn wants_block(rule: &Rule, lines: &[String]) -> bool {
    if rule.force_block || lines.len() > 1 {
        return true;
    }
    match lines.first() {
        None => false,
        Some(first) => first.contains(']') || first.chars().count() > 60,
    }
}

/// Render a rule as the text that goes in a rules file, without a trailing newline: the section
/// heading and comments first when present, then the rule.
///
/// # Errors
/// [`Unsupported`] only if the matcher declines the citation.
pub fn render_rule(rule: &Rule) -> Result<String, Unsupported> {
    let mut out: Vec<String> = Vec::new();
    if !strip(&rule.section).is_empty() {
        // "@Name" is how the guidelines write sections, so a typed "@" is the marker; two would
        // be a section literally called "@My Mod"
        out.push(format!("@{}", strip(strip(&rule.section).trim_start_matches('@'))));
    }
    out.extend(
        splitlines(&rule.comment)
            .into_iter()
            .filter(|l| !strip(l).is_empty())
            .map(|l| rstrip(&format!("; {l}")).to_string()),
    );
    if rule.is_ordering() {
        out.push(format!("[{}]", rule.kind));
        // stripped: mlox reads a line that starts with whitespace as message text, not a plugin
        out.extend(rule.plugins.iter().map(|n| strip(n).to_string()));
        return Ok(out.join("\n"));
    }
    let lines = message_lines(rule)?;
    if wants_block(rule, &lines) {
        out.push(format!("[{}]", rule.kind));
        out.extend(lines.iter().map(|l| format!(" {l}")));
    } else if let Some(first) = lines.first() {
        out.push(format!("[{} {first}]", rule.kind));
    } else {
        out.push(format!("[{}]", rule.kind));
    }
    out.extend(rule.expressions.iter().map(|e| e.render(0)));
    Ok(out.join("\n"))
}

// ---------------------------------------------------------------- validation

fn looks_like_plugin(name: &str) -> Result<bool, Unsupported> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let candidate = strip(name);
    if candidate.is_empty() || candidate.contains(FORBIDDEN_IN_NAMES) {
        return Ok(false);
    }
    if EXPANSION_CHARS.iter().any(|t| candidate.contains(t)) {
        // an expansion cannot be matched against the literal-name pattern; the extension is what
        // still has to look right
        let stem = candidate.replace("<VER>", "0").replace(['?', '*'], "a");
        return fullmatch(&RE, NAME_FULL, true, &stem);
    }
    fullmatch(&RE, NAME_FULL, true, candidate)
}

fn name_problem(name: &str) -> Result<Option<Problem>, Unsupported> {
    if looks_like_plugin(name)? {
        return Ok(None);
    }
    let bad: Vec<String> = FORBIDDEN_IN_NAMES
        .iter()
        .filter(|c| name.contains(**c))
        .map(|c| py_repr(&c.to_string()))
        .collect::<Result<_, _>>()?;
    if !bad.is_empty() {
        return Ok(Some(Problem::new(
            Severity::Error,
            format!("{} contains {}, which mlox reads as rule syntax", py_repr(name)?, bad.join(", ")),
            "';' starts a comment and brackets delimit expressions, so the name would be read as something other than what you typed",
        )));
    }
    Ok(Some(Problem::new(
        Severity::Error,
        format!("{} does not look like a plugin filename", py_repr(name)?),
        "it needs a .esp/.esm/.omwaddon/.omwgame/.omwscripts extension",
    )))
}

fn check_expression(expr: &Expr, problems: &mut Vec<Problem>) -> Result<(), Unsupported> {
    static VERSION: OnceLock<Regex> = OnceLock::new();
    for node in expr.walk() {
        match node {
            Expr::Group { operator, parts } => {
                if operator == "NOT" && parts.len() != 1 {
                    problems.push(Problem::error(format!(
                        "[NOT] takes exactly one expression, got {}",
                        parts.len()
                    )));
                } else if (operator == "ALL" || operator == "ANY") && parts.is_empty() {
                    problems.push(Problem::error(format!("[{operator}] needs at least one expression")));
                } else if (operator == "ALL" || operator == "ANY") && parts.len() == 1 {
                    problems.push(Problem::new(
                        Severity::Warning,
                        format!("[{operator}] with a single expression has no effect"),
                        "drop the group, or add the other expressions you meant",
                    ));
                }
            }
            Expr::Ver { operator, version, .. } => {
                if !VERSION_OPERATORS.contains(&operator.as_str()) {
                    problems.push(Problem::error(format!(
                        "[VER] operator {} is not one of {}",
                        py_repr(operator)?,
                        VERSION_OPERATORS.join(", ")
                    )));
                }
                if !fullmatch(&VERSION, VERSION_ONLY, false, strip(version))? {
                    problems.push(Problem::new(
                        Severity::Error,
                        format!("{} is not a version mlox recognises", py_repr(version)?),
                        "up to three numbers separated by . _ or -, optionally followed by a letter (1.2.3a, 1_3a, 77g)",
                    ));
                }
            }
            Expr::Size { size, .. } => {
                if *size < 0 {
                    problems.push(Problem::error("[SIZE] needs a byte count of zero or more"));
                }
            }
            Expr::Desc { pattern, .. } => {
                // Python words a rejected regex itself ("missing ), unterminated subpattern at
                // position 0"): a regex that does not compile is declined, not paraphrased
                Regex::new(pattern, Flags::default()).map_err(|_| Unsupported)?;
            }
            Expr::Plugin(_) => {}
        }
        if !matches!(node, Expr::Group { .. }) {
            for name in node.plugins() {
                problems.extend(name_problem(&name)?);
            }
        }
    }
    Ok(())
}

fn check_shape(rule: &Rule, problems: &mut Vec<Problem>) -> Result<(), Unsupported> {
    if !RULE_KINDS.contains(&rule.kind.as_str()) {
        problems.push(Problem::new(
            Severity::Error,
            format!("{} is not a rule mlox knows", py_repr(&rule.kind)?),
            format!("one of {}", RULE_KINDS.join(", ")),
        ));
        return Ok(());
    }
    if rule.is_ordering() {
        if rule.kind == "Order" && rule.plugins.len() < 2 {
            problems.push(Problem::new(
                Severity::Error,
                "an [Order] rule needs at least two plugins",
                "it states that one plugin precedes another",
            ));
        } else if rule.plugins.is_empty() {
            problems.push(Problem::error(format!("a [{}] rule needs at least one plugin", rule.kind)));
        }
        return Ok(());
    }
    let count = rule.expressions.len();
    if rule.kind == "Note" && count < 1 {
        problems.push(Problem::error("a [Note] rule needs at least one expression"));
    } else if rule.kind == "Conflict" && count < 2 {
        problems.push(Problem::new(
            Severity::Error,
            "a [Conflict] rule needs at least two expressions",
            "it warns when any two of them are true at once",
        ));
    } else if (rule.kind == "Requires" || rule.kind == "Patch") && count != 2 {
        problems.push(Problem::new(
            Severity::Error,
            format!("a [{}] rule takes exactly two expressions, got {count}", rule.kind),
            "the dependant first, then what it needs",
        ));
    }
    Ok(())
}

fn check_conventions(rule: &Rule, problems: &mut Vec<Problem>) -> Result<(), Unsupported> {
    if strip(&rule.ref_).is_empty() {
        problems.push(Problem::new(
            Severity::Warning,
            "no (Ref:) citation",
            "name the readme, forum post or URL this is based on, so someone else can check it",
        ));
    }
    if priority_mark(rule.priority).is_none() {
        problems.push(Problem::new(
            Severity::Error,
            format!("highlight level {} is not one of [0, 1, 2, 3]", rule.priority),
            "0 none, 1 blue, 2 yellow, 3 red -- anything else renders no mark at all, which is not what asking for one means",
        ));
    }
    if let (Some(colour), true) = (self_highlighting(&rule.kind), rule.priority != 0) {
        problems.push(Problem::new(
            Severity::Warning,
            format!("[{}] is already highlighted {colour} by mlox", rule.kind),
            "the ! prefix adds nothing here",
        ));
    }
    if rule.kind == "NearStart" || rule.kind == "NearEnd" {
        problems.push(Problem::new(
            Severity::Warning,
            format!("[{}] rules are discouraged by the guidelines", rule.kind),
            "place plugins relative to each other with [Order] where you can",
        ));
    }
    if WARNING_KINDS.contains(&rule.kind.as_str()) && strip(&rule.message).is_empty() {
        problems.push(Problem::new(
            Severity::Warning,
            format!("a [{}] rule with no message tells the reader nothing", rule.kind),
            "say what is wrong and what to do about it",
        ));
    }
    let expanded: BTreeSet<String> = rule
        .mentioned_plugins()
        .into_iter()
        .filter(|n| EXPANSION_CHARS.iter().any(|t| n.contains(t)))
        .collect();
    if !expanded.is_empty() {
        problems.push(Problem::new(
            Severity::Warning,
            format!("filename expansion used in {}", expanded.into_iter().collect::<Vec<_>>().join(", ")),
            "expansion is CPU intensive; the guidelines ask for it to be used sparingly",
        ));
    }
    if !strip(&rule.ref_).is_empty() && url_in(&rule.ref_)? && !format_ref(&rule.ref_)?.ends_with(" )") {
        problems.push(Problem::warning("a URL citation needs whitespace before the closing parenthesis"));
    }
    Ok(())
}

/// Check a rule against the guidelines: every problem found, errors and warnings together, in the
/// order detected. Empty means the rule is both valid and conventional.
///
/// # Errors
/// [`Unsupported`], see the module.
pub fn validate(rule: &Rule) -> Result<Vec<Problem>, Unsupported> {
    let mut problems = Vec::new();
    check_shape(rule, &mut problems)?;
    if rule.kind == "Patch"
        && rule
            .expressions
            .iter()
            .any(|e| e.walk().iter().any(|n| matches!(n, Expr::Group { operator, .. } if operator == "NOT")))
    {
        problems.push(Problem::new(
            Severity::Error,
            "[Patch] does not recognise the NOT expression",
            "write a [Conflict] rule for what must not be present and a [Requires] rule for what must, as the guidelines describe",
        ));
    }
    if rule.is_ordering() {
        for name in &rule.plugins {
            problems.extend(name_problem(name)?);
        }
        let mut seen: BTreeSet<String> = BTreeSet::new();
        for name in &rule.plugins {
            if !seen.insert(name.to_lowercase()) {
                problems.push(Problem::new(
                    Severity::Error,
                    format!("{} is listed twice", py_repr(name)?),
                    "a rule ordering a plugin against itself is a cycle and mlox discards it",
                ));
            }
        }
    } else {
        for expr in &rule.expressions {
            check_expression(expr, &mut problems)?;
        }
    }
    let lines = message_lines(rule)?;
    if let Some(first) = lines.first() {
        if !wants_block(rule, &lines) && first.contains(']') {
            problems.push(Problem::new(
                Severity::Error,
                "an inline message cannot contain ']'",
                "use the block form, which has no such limit",
            ));
        }
    }
    check_conventions(rule, &mut problems)?;
    Ok(problems)
}

/// Only the fatal ones.
pub fn errors(problems: &[Problem]) -> Vec<Problem> {
    problems.iter().filter(|p| p.severity == Severity::Error).cloned().collect()
}

// ---------------------------------------------------------------- the user's rule file

/// Why a rule was not appended.
#[derive(Debug)]
pub enum AppendError {
    /// The rule has errors (Python's `ValueError`): every one of them, or why the call was
    /// refused.
    Refused(String),
    /// See the module.
    Unsupported,
    /// The file could not be read or written (Python raises it too).
    Io(io::Error),
}

impl From<Unsupported> for AppendError {
    fn from(_: Unsupported) -> Self {
        AppendError::Unsupported
    }
}

impl From<io::Error> for AppendError {
    fn from(e: io::Error) -> Self {
        AppendError::Io(e)
    }
}

fn newline(text: &str, ending: LineEnding) -> String {
    match ending {
        LineEnding::Lf => text.to_string(),
        LineEnding::Crlf => text.replace('\n', "\r\n"),
    }
}

/// Append any mlox rule to a personal rules file (`append_authored_rule`): the file is created
/// with an explanatory header when it does not exist yet. Errors refuse, listing every one;
/// warnings do not. Returns the text appended. `ending` is how Python's text mode ends a line on
/// this platform (`LineEnding::platform()`).
///
/// # Errors
/// [`AppendError`].
pub fn append_authored_rule(path: &Path, rule: &Rule, ending: LineEnding) -> Result<String, AppendError> {
    let fatal = errors(&validate(rule)?);
    if !fatal.is_empty() {
        return Err(AppendError::Refused(fatal.iter().map(Problem::describe).collect::<Vec<_>>().join("; ")));
    }
    let text = format!("{}\n", render_rule(rule)?);
    if path.exists() {
        let existing = decode_cfg(&fs::read(path)?)?;
        let sep = if existing.ends_with("\n\n") {
            ""
        } else if existing.ends_with('\n') {
            "\n"
        } else {
            "\n\n"
        };
        fs::write(path, newline(&format!("{existing}{sep}{text}"), ending))?;
    } else {
        fs::write(path, newline(&format!("{USER_RULES_HEADER}\n{text}"), ending))?;
    }
    Ok(text)
}

/// The narrow form for an ordering rule (`append_user_rule`): `keyword` is `order`, `nearstart`
/// or `nearend` in any case; the names are trimmed and the blank ones dropped.
///
/// # Errors
/// [`AppendError`]; [`AppendError::Refused`] also for an unsupported keyword or no names.
pub fn append_user_rule(
    path: &Path,
    keyword: &str,
    names: &[String],
    comment: Option<&str>,
    ending: LineEnding,
) -> Result<String, AppendError> {
    let kind = match strip(keyword).to_lowercase().as_str() {
        "order" => "Order",
        "nearstart" => "NearStart",
        "nearend" => "NearEnd",
        _ => return Err(AppendError::Refused(format!("unsupported rule type: {}", py_repr(keyword)?))),
    };
    let cleaned: Vec<String> = names.iter().map(|n| strip(n).to_string()).filter(|n| !n.is_empty()).collect();
    if cleaned.is_empty() {
        return Err(AppendError::Refused("no plugin names given".to_string()));
    }
    let rule = Rule {
        kind: kind.to_string(),
        plugins: cleaned,
        comment: strip(comment.unwrap_or("")).to_string(),
        ..Rule::default()
    };
    append_authored_rule(path, &rule, ending)
}

/// The pairs a proposed `[Order]` rule would fight the cfg over (`order_rule_frozen_conflicts`):
/// consecutive (earlier, later) names that are both curated plugins the sort will not reorder,
/// but that the rule wants opposite to how they sit. mlox discards such edges as cycles, so the
/// rule would silently not take effect for them. Names with wildcards are skipped. Advisory.
pub fn order_rule_frozen_conflicts(
    names: &[String],
    final_order: &[String],
    curated_lower: &[String],
) -> Vec<(String, String)> {
    let pos: std::collections::HashMap<String, usize> =
        final_order.iter().enumerate().map(|(i, n)| (n.to_lowercase(), i)).collect();
    let curated: std::collections::HashSet<String> = curated_lower.iter().map(|c| c.to_lowercase()).collect();
    let mut out = Vec::new();
    for pair in names.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        let (al, bl) = (a.to_lowercase(), b.to_lowercase());
        if has_meta(a) || has_meta(b) {
            continue;
        }
        if curated.contains(&al)
            && curated.contains(&bl)
            && matches!((pos.get(&al), pos.get(&bl)), (Some(x), Some(y)) if x > y)
        {
            out.push((a.clone(), b.clone()));
        }
    }
    out
}

// ---------------------------------------------------------------- the plain form

/// An expression from its plain JSON form: `["plugin", name]`, `["desc", pattern, plugin,
/// negated]`, `["size", size, plugin, negated]`, `["ver", operator, version, plugin]` or
/// `["group", operator, [parts]]` (`wraithguard.rules.authoring.expr_plain`); `None` for any
/// other shape.
pub fn expr_from_json(v: &serde_json::Value) -> Option<Expr> {
    let a = v.as_array()?;
    let s = |i: usize| a.get(i)?.as_str().map(str::to_string);
    let b = |i: usize| a.get(i)?.as_bool();
    Some(match a.first()?.as_str()? {
        "plugin" => Expr::Plugin(s(1)?),
        "desc" => Expr::Desc { pattern: s(1)?, plugin: s(2)?, negated: b(3)? },
        "size" => Expr::Size { size: a.get(1)?.as_i64()?, plugin: s(2)?, negated: b(3)? },
        "ver" => Expr::Ver { operator: s(1)?, version: s(2)?, plugin: s(3)? },
        "group" => Expr::Group { operator: s(1)?, parts: a.get(2)?.as_array()?.iter().map(expr_from_json).collect::<Option<_>>()? },
        _ => return None,
    })
}

/// An expression from a line of rule text, as mlox writes it: a plugin name, an atomic form
/// (`[DESC /regex/ plugin]`, `[DESC !/regex/ plugin]`, `[SIZE 123 plugin]`,
/// `[SIZE !123 plugin]`, `[VER < 1.2 plugin]`) or a group of them (`[ANY a.esp b.esp]`,
/// `[ALL ...]`, `[NOT ...]`), read by the predicate reader the sort uses. `Err` names what
/// did not read, so the rule maker can say so rather than write a rule mlox misreads.
///
/// # Errors
/// A line that is empty, holds a `/message/`, an `[MWSE-LUA]` form (nothing under OpenMW), a
/// group without its operator, or more than one expression.
pub fn expr_from_text(line: &str) -> Result<Expr, String> {
    use crate::predicates::{parse, read_form, tokenize, Form, Node};
    let declined = |_| format!("'{line}' holds something the rule maker does not read");
    fn convert(node: &Node, line: &str) -> Result<Expr, String> {
        match node {
            Node::Leaf(t) if t.starts_with('[') => match read_form(t).map_err(|_| format!("'{t}' holds something the rule maker does not read"))? {
                Some(Form::Desc { bang, pat, plugin }) => Ok(Expr::Desc { pattern: pat, plugin, negated: bang }),
                Some(Form::Size { bang, want, plugin }) => {
                    Ok(Expr::Size { size: want.parse().map_err(|_| format!("'{t}': the size is too large"))?, plugin, negated: bang })
                }
                Some(Form::Ver { op, want, plugin }) => Ok(Expr::Ver { operator: op, version: want, plugin }),
                Some(Form::MwseLua { .. }) => Err(format!("'{t}': MWSE-Lua does not exist under OpenMW")),
                None => Err(format!("'{t}' is not a [DESC], [SIZE] or [VER] form mlox reads")),
            },
            Node::Leaf(t) if t.starts_with('/') => Err(format!("'{t}' is a message; it belongs in the Message box")),
            Node::Leaf(t) => Ok(Expr::Plugin(t.clone())),
            Node::Group(items) => {
                let op = match items.first() {
                    Some(Node::Leaf(s)) if matches!(s.to_uppercase().as_str(), "ALL" | "ANY" | "NOT") => s.to_uppercase(),
                    _ => return Err(format!("'{line}': a group starts with ALL, ANY or NOT")),
                };
                let parts = items[1..].iter().map(|n| convert(n, line)).collect::<Result<Vec<_>, _>>()?;
                if parts.is_empty() || (op == "NOT" && parts.len() != 1) {
                    return Err(format!("'{line}': [{op}] needs {}", if op == "NOT" { "exactly one expression" } else { "something inside it" }));
                }
                Ok(Expr::Group { operator: op, parts })
            }
        }
    }
    let nodes = parse(&tokenize(line).map_err(declined)?).map_err(declined)?;
    match nodes.as_slice() {
        [one] => convert(one, line),
        [] => Err(format!("'{line}' names no plugin")),
        _ => Err(format!("'{line}' holds more than one expression: one a line, or group them with [ALL ...] or [ANY ...]")),
    }
}

/// A rule from its plain JSON form (`wraithguard.rules.authoring.rule_plain`): `kind`,
/// `plugins`, `expressions`, `message`, `ref`, `priority`, `section`, `comment`, `force_block`.
/// Missing text fields are empty, a missing priority 0; `None` for a field of another type.
pub fn rule_from_json(v: &serde_json::Value) -> Option<Rule> {
    let s = |k: &str| match v.get(k) {
        None | Some(serde_json::Value::Null) => Some(String::new()),
        Some(x) => x.as_str().map(str::to_string),
    };
    Some(Rule {
        kind: s("kind")?,
        plugins: match v.get("plugins") {
            None => Vec::new(),
            Some(p) => p.as_array()?.iter().map(|x| x.as_str().map(str::to_string)).collect::<Option<_>>()?,
        },
        expressions: match v.get("expressions") {
            None => Vec::new(),
            Some(e) => e.as_array()?.iter().map(expr_from_json).collect::<Option<_>>()?,
        },
        message: s("message")?,
        ref_: s("ref")?,
        priority: v.get("priority").map_or(Some(0), serde_json::Value::as_i64)?,
        section: s("section")?,
        comment: s("comment")?,
        force_block: v.get("force_block").map_or(Some(false), serde_json::Value::as_bool)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plugin(n: &str) -> Expr {
        Expr::Plugin(n.to_string())
    }

    #[test]
    fn expressions_read_from_rule_text() {
        assert_eq!(expr_from_text("Some Mod [Main].esp").unwrap(), plugin("Some Mod [Main].esp"));
        assert_eq!(
            expr_from_text("[DESC !/foo bar/ a.esp]").unwrap(),
            Expr::Desc { pattern: "foo bar".into(), plugin: "a.esp".into(), negated: true }
        );
        assert_eq!(expr_from_text("[SIZE 1234 a.esp]").unwrap(), Expr::Size { size: 1234, plugin: "a.esp".into(), negated: false });
        assert_eq!(expr_from_text("[VER < 1.2 a.esp]").unwrap(), Expr::Ver { operator: "<".into(), version: "1.2".into(), plugin: "a.esp".into() });
        let g = expr_from_text("[ANY a.esp [NOT b.esp] [SIZE !9 c.esp]]").unwrap();
        assert_eq!(g.render(0), "[ANY a.esp [NOT b.esp] [SIZE !9 c.esp]]");
        assert!(expr_from_text("a.esp b.esp").unwrap_err().contains("more than one"));
        assert!(expr_from_text("[a.esp b.esp]").unwrap_err().contains("ALL, ANY or NOT"));
        assert!(expr_from_text("[NOT a.esp b.esp]").unwrap_err().contains("exactly one"));
        assert!(expr_from_text("/hello/").unwrap_err().contains("Message box"));
    }

    fn msgs(rule: &Rule) -> Vec<String> {
        validate(rule).unwrap().iter().map(Problem::describe).collect()
    }

    #[test]
    fn an_ordering_rule_is_its_plugins_stripped() {
        let rule = Rule {
            kind: "Order".into(),
            plugins: vec![" A.esp ".into(), "B.esp".into()],
            comment: "why\n\nwhat".into(),
            section: "@@My Mod ".into(),
            ..Rule::default()
        };
        assert_eq!(render_rule(&rule).unwrap(), "@My Mod\n; why\n; what\n[Order]\nA.esp\nB.esp");
    }

    #[test]
    fn a_short_message_is_inline_and_a_long_one_is_a_block() {
        let mut rule = Rule {
            kind: "Requires".into(),
            expressions: vec![plugin("A.esp"), plugin("B.esp")],
            message: "needs B".into(),
            ref_: "readme".into(),
            priority: 2,
            ..Rule::default()
        };
        assert_eq!(render_rule(&rule).unwrap(), "[Requires]\n !! needs B\n (Ref: readme)\nA.esp\nB.esp");
        rule.message = "x".into();
        rule.ref_ = String::new();
        rule.priority = 0;
        assert_eq!(render_rule(&rule).unwrap(), "[Requires x]\nA.esp\nB.esp");
        rule.message = "a ] b".into();
        assert_eq!(render_rule(&rule).unwrap(), "[Requires]\n a ] b\nA.esp\nB.esp");
    }

    #[test]
    fn a_url_citation_gets_a_space_before_the_parenthesis() {
        assert_eq!(format_ref("see  https://x.y/z").unwrap(), "(Ref: see https://x.y/z )");
        assert_eq!(format_ref("readme").unwrap(), "(Ref: readme)");
        assert_eq!(format_ref("  ").unwrap(), "");
    }

    #[test]
    fn a_long_group_breaks_across_lines() {
        let names: Vec<Expr> = (0..8).map(|i| plugin(&format!("Some Long Plugin Name {i}.esp"))).collect();
        let text = all_of(names).render(0);
        assert!(text.starts_with("[ALL Some Long Plugin Name 0.esp\n Some Long"));
        assert!(text.ends_with("7.esp]"));
    }

    #[test]
    fn what_the_guidelines_forbid_is_an_error_and_the_rest_is_a_warning() {
        let rule = Rule {
            kind: "Patch".into(),
            expressions: vec![plugin("A.esp"), not_(plugin("B.esp"))],
            message: "m".into(),
            ref_: "r".into(),
            ..Rule::default()
        };
        let got = msgs(&rule);
        assert!(got
            .iter()
            .any(|m| m
                .starts_with("error: [Patch] does not recognise the NOT expression -- write a [Conflict]")));
        assert!(
            got.iter().any(|m| m.starts_with("warning: [Patch] is already") || m.is_empty())
                || got.len() == 1
        );
        let order = Rule {
            kind: "Order".into(),
            plugins: vec!["a.esp".into(), "A.ESP".into()],
            ref_: "r".into(),
            ..Rule::default()
        };
        assert_eq!(msgs(&order), ["error: 'A.ESP' is listed twice -- a rule ordering a plugin against itself is a cycle and mlox discards it"]);
    }

    #[test]
    fn a_name_with_syntax_in_it_is_refused_by_name() {
        let rule = Rule {
            kind: "Order".into(),
            plugins: vec!["semi;colon.esp".into(), "B.esp".into()],
            ref_: "r".into(),
            ..Rule::default()
        };
        assert!(msgs(&rule)[0]
            .starts_with("error: 'semi;colon.esp' contains ';', which mlox reads as rule syntax"));
    }

    #[test]
    fn a_regex_python_rejects_is_declined_not_paraphrased() {
        let bad = Rule {
            kind: "Note".into(),
            expressions: vec![Expr::Desc { pattern: "(".into(), plugin: "A.esp".into(), negated: false }],
            message: "m".into(),
            ref_: "r".into(),
            ..Rule::default()
        };
        assert_eq!(validate(&bad), Err(Unsupported));
    }

    #[test]
    fn frozen_conflicts_are_the_pairs_against_the_cfg() {
        let names: Vec<String> = ["B.esp", "A.esp", "C*.esp"].iter().map(|s| (*s).to_string()).collect();
        let order: Vec<String> = ["A.esp", "B.esp", "Cx.esp"].iter().map(|s| (*s).to_string()).collect();
        let cur = order.clone();
        assert_eq!(
            order_rule_frozen_conflicts(&names, &order, &cur),
            [("B.esp".to_string(), "A.esp".to_string())]
        );
    }
}
