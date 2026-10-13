//! Checks on a Morrowind script's source as it is typed (was `wraithguard/mwscript/check.py`,
//! which stays as the fallback and the reference this is held to).
//!
//! Not the compiler (`super::compiler`): the mistakes a line-by-line reading catches, some
//! the compiler lets through - the `begin`/`end` frame and a `begin` that names another
//! script, `if`/`while` blocks out of balance, a local declared twice, `set` on a name that
//! is neither a local, a global nor `object.variable`, a statement that is no function the
//! game (or MWSE, or MW-Enhanced) knows; and what the Construction Set compiles and the game
//! gets wrong (simania.nl's "Script quirks": a single `=` in a condition, `<>`, a stray `=`
//! after `set ... to`, `<id>->GetDeadCount`, an unquoted id with a hyphen, an apostrophe or
//! a leading digit, 20 or more `elseif`s in one chain). Each finding is `(line, level, message)`, lines
//! from 1, `level` `error` or `warning`; the messages are the Python module's, word for word.

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use regex::Regex;

use super::data::KNOWN_NAMES;

/// Words that open a statement without being functions.
const KEYWORDS: [&str; 13] =
    ["begin", "end", "short", "long", "float", "if", "elseif", "else", "endif", "while", "endwhile", "set", "return"];

/// One finding: line (from 1), `error` or `warning`, the message.
pub type Finding = (usize, &'static str, String);

fn decl_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)^(short|long|float)\s+([A-Za-z_][\w]*)").expect("pattern"))
}

fn set_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"(?i)^set\s+("[^"]*"\.\w+|[\w.]+)\s+to\b"#).expect("pattern"))
}

fn ref_word_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"^\s*(?:"[^"]*"|[\w.]+)\s*->\s*([A-Za-z_]\w*)"#).expect("pattern"))
}

fn word_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^\s*([A-Za-z_]\w*)").expect("pattern"))
}

fn re(cell: &'static OnceLock<Regex>, pattern: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pattern).expect("pattern"))
}

static NUMBER: OnceLock<Regex> = OnceLock::new();
static LEADING_DIGIT: OnceLock<Regex> = OnceLock::new();
static HYPHENATED: OnceLock<Regex> = OnceLock::new();
static TOKEN_SPLIT: OnceLock<Regex> = OnceLock::new();
static GETDEADCOUNT: OnceLock<Regex> = OnceLock::new();
static STRAY_TO: OnceLock<Regex> = OnceLock::new();

/// How many `elseif`s in one chain before the game may fail on a late branch.
pub const ELSEIF_CHAIN: usize = 20;

/// `code` with every quoted span (quotes included) blanked to spaces.
fn unquoted(code: &str) -> String {
    let mut quoted = false;
    code.chars()
        .map(|ch| {
            if ch == '"' {
                quoted = !quoted;
                ' '
            } else if quoted {
                ' '
            } else {
                ch
            }
        })
        .collect()
}

/// What the CS compiles and the game gets wrong, on one statement line (warnings).
fn quirks(code: &str, first: &str) -> Vec<String> {
    let mut out = Vec::new();
    let u = unquoted(code);
    let lu = u.to_lowercase();
    if matches!(first, "if" | "elseif" | "while") {
        let c: Vec<char> = u.chars().collect();
        for i in 0..c.len() {
            let before = if i > 0 { Some(c[i - 1]) } else { None };
            let after = c.get(i + 1).copied();
            if c[i] == '=' && !matches!(before, Some('=' | '!' | '<' | '>')) && after != Some('=') {
                out.push("a single '=' in a condition: the CS reads it as '==', MWEdit refuses it; write '=='".to_string());
                break;
            }
        }
    }
    if u.contains("<>") {
        out.push("'<>' is not 'not equal' (the game reads it as '<' or '>'): use '!='".to_string());
    }
    if first == "set"
        && let Some(m) = re(&STRAY_TO, r"\bto\s*=").find(&lu)
        && !lu[m.end()..].starts_with('=')
    {
        out.push("a stray '=' after 'to'".to_string());
    }
    if re(&GETDEADCOUNT, r"->\s*getdeadcount\b").is_match(&lu) {
        out.push("GetDeadCount takes the id after it ('GetDeadCount <id>'); with '<id>->' in front, the lines under it are ignored".to_string());
    }
    let mut seen: HashSet<&str> = HashSet::new();
    for tok in re(&TOKEN_SPLIT, r"->|[\s(),:]+").split(&u) {
        if tok.is_empty() || seen.contains(tok) {
            continue;
        }
        let bad = (tok.contains('\'') && tok.chars().any(char::is_alphanumeric))
            || (re(&LEADING_DIGIT, r"^\d+[A-Za-z_]").is_match(tok) && !re(&NUMBER, r"^\d+(\.\d*)?([eE][+-]?\d+)?$").is_match(tok))
            || (!matches!(first, "set" | "if" | "elseif" | "while") && re(&HYPHENATED, r"^[A-Za-z_]\w*-[A-Za-z_][\w-]*$").is_match(tok));
        if bad {
            seen.insert(tok);
            out.push(format!("{tok} needs quotes: an id with a hyphen, an apostrophe or a leading digit is misread without them"));
        }
    }
    out
}

fn known() -> &'static HashSet<String> {
    static SET: OnceLock<HashSet<String>> = OnceLock::new();
    SET.get_or_init(|| KNOWN_NAMES.iter().map(|n| String::from_utf8_lossy(n).into_owned()).collect())
}

/// A line without its comment (`;` outside quotes), trimmed.
fn code_of(line: &str) -> &str {
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

/// Python's `str.splitlines()` count, for "the last line".
fn line_count(text: &str) -> usize {
    let mut n = 0;
    let mut chars = text.chars().peekable();
    let mut open = false;
    while let Some(c) = chars.next() {
        open = true;
        match c {
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                n += 1;
                open = false;
            }
            '\n' | '\x0b' | '\x0c' | '\x1c' | '\x1d' | '\x1e' | '\u{85}' | '\u{2028}' | '\u{2029}' => {
                n += 1;
                open = false;
            }
            _ => {}
        }
    }
    n + usize::from(open)
}

/// Whether a script has an `end` line (comments aside).
pub fn found_end(text: &str) -> bool {
    text.lines().any(|raw| {
        let low = code_of(raw).to_lowercase();
        let words: Vec<&str> = low.split_whitespace().collect();
        words.first() == Some(&"end") && words.len() <= 2
    })
}

/// Checks a script's source: `script_id` is the record's id (`begin` should name it),
/// `globals` the load order's global variables (`set` may name one).
pub fn check_script(text: &str, script_id: &str, globals: &[String]) -> Vec<Finding> {
    let mut found: Vec<Finding> = Vec::new();
    let known_globals: HashSet<String> = globals.iter().map(|g| g.to_lowercase()).collect();
    let mut local: HashMap<String, usize> = HashMap::new();
    // (block, line, seen else, elseifs)
    let mut stack: Vec<(&'static str, usize, bool, usize)> = Vec::new();
    let (mut began, mut ended) = (false, false);
    let normal = text.replace("\r\n", "\n");
    for (i, raw) in normal.split('\n').enumerate() {
        let n = i + 1;
        let code = code_of(raw);
        if code.is_empty() {
            continue;
        }
        let low = code.to_lowercase();
        let words: Vec<&str> = low.split_whitespace().collect();
        let first = words.first().map_or("", |w| w.trim_end_matches(','));
        if ended {
            found.push((n, "warning", "text after 'end' is never run".into()));
            ended = false;
        }
        if !began {
            if first != "begin" {
                found.push((n, "error", "a script starts with 'begin <name>'".into()));
            } else {
                let name = code.split_whitespace().nth(1).unwrap_or("");
                if name.is_empty() {
                    found.push((n, "error", "'begin' needs the script's name".into()));
                } else if !script_id.is_empty() && name.trim_matches('"').to_lowercase() != script_id.to_lowercase() {
                    found.push((n, "warning", format!("'begin {name}' does not name this script ({script_id})")));
                }
            }
            began = true;
            continue;
        }
        if first == "begin" {
            found.push((n, "error", "a second 'begin'".into()));
            continue;
        }
        if first == "end" && words.len() <= 2 && !low.contains("->") {
            for (block, at, _, _) in stack.drain(..) {
                found.push((at, "error", format!("'{block}' is never closed")));
            }
            ended = true;
            continue;
        }
        if let Some(d) = decl_re().captures(code) {
            let var = d[2].to_lowercase();
            if let Some(&first_at) = local.get(&var) {
                found.push((n, "warning", format!("{} is declared again (first on line {first_at})", &d[2])));
            } else {
                local.insert(var, n);
            }
            continue;
        }
        for message in quirks(code, first) {
            found.push((n, "warning", message));
        }
        match first {
            "if" | "while" => {
                stack.push((if first == "if" { "if" } else { "while" }, n, false, 0));
                continue;
            }
            "elseif" | "else" => {
                match stack.last_mut() {
                    Some(top) if top.0 == "if" => {
                        if top.2 {
                            found.push((n, "error", format!("'{first}' after 'else'")));
                        } else if first == "else" {
                            top.2 = true;
                        } else {
                            top.3 += 1;
                            if top.3 == ELSEIF_CHAIN {
                                found.push((n, "warning", format!("{ELSEIF_CHAIN} 'elseif's in one chain: the game can fail on a late branch (RightEval/LeftEval); split it")));
                            }
                        }
                    }
                    _ => found.push((n, "error", format!("'{first}' without an 'if'"))),
                }
                continue;
            }
            "endif" | "endwhile" => {
                let want = if first == "endif" { "if" } else { "while" };
                if stack.last().is_some_and(|top| top.0 == want) {
                    stack.pop();
                } else {
                    found.push((n, "error", format!("'{first}' without a '{want}'")));
                }
                continue;
            }
            "set" => {
                match set_re().captures(code) {
                    None => found.push((n, "error", "'set' is 'set <variable> to <value>'".into())),
                    Some(m) => {
                        let var = m[1].to_lowercase();
                        if !var.contains('.') && !local.contains_key(&var) && !known_globals.contains(&var) {
                            found.push((n, "warning", format!("{} is not a local variable or a global", &m[1])));
                        }
                    }
                }
                continue;
            }
            "return" => continue,
            _ => {}
        }
        let name = ref_word_re()
            .captures(code)
            .or_else(|| word_re().captures(code))
            .map(|c| c[1].to_lowercase())
            .unwrap_or_default();
        if !name.is_empty() && !known().contains(&name) && !KEYWORDS.contains(&name.as_str()) && !local.contains_key(&name) {
            found.push((n, "warning", format!("{name} is not a function the game knows")));
        }
    }
    if !began {
        found.push((1, "error", "the script is empty".into()));
    } else if !ended && !found_end(text) {
        for (block, at, _, _) in stack.drain(..) {
            found.push((at, "error", format!("'{block}' is never closed")));
        }
        found.push((line_count(text).max(1), "error", "the script has no 'end'".into()));
    }
    found.sort_by_key(|f| f.0);
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msgs(text: &str) -> Vec<(usize, String, String)> {
        check_script(text, "", &[]).into_iter().map(|(n, l, m)| (n, l.to_string(), m)).collect()
    }

    #[test]
    fn the_python_modules_findings() {
        let m = msgs("begin x\nshort a\nlong A\nset b to 1\nset a 2\nFlyToTheMoon\nend");
        assert!(m.contains(&(3, "warning".into(), "A is declared again (first on line 2)".into())));
        assert!(m.contains(&(4, "warning".into(), "b is not a local variable or a global".into())));
        assert!(m.contains(&(5, "error".into(), "'set' is 'set <variable> to <value>'".into())));
        assert!(m.contains(&(6, "warning".into(), "flytothemoon is not a function the game knows".into())));
        assert!(!msgs("begin x\nshort a\nset a to 1\nend").iter().any(|f| f.0 == 4));
    }

    #[test]
    fn blocks_and_the_frame() {
        let m = msgs("begin x\nif ( 1 )\nelse\nelse\nwhile ( 1 )\nend");
        assert!(m.contains(&(4, "error".into(), "'else' after 'else'".into())));
        assert!(m.contains(&(2, "error".into(), "'if' is never closed".into())));
        assert!(m.contains(&(5, "error".into(), "'while' is never closed".into())));
        let bare = msgs("set a to 1");
        assert_eq!(bare.len(), 2);
        assert_eq!(bare[0].2, "a script starts with 'begin <name>'");
        assert_eq!(bare[1].2, "the script has no 'end'");
        assert!(check_script("begin x\nplayer->AddItem gold_001 1\nend", "y", &[]).iter().any(|f| f.2.contains("does not name this script (y)")));
    }

    /// The Python module's `test_what_the_cs_compiles_and_the_game_gets_wrong`, word for word.
    #[test]
    fn what_the_cs_compiles_and_the_game_gets_wrong() {
        let text = "begin x\nshort a\nif ( a = 1 )\n    set a to = ( a + 1 )\nelseif ( a <> 2 )\n    yakov->GetDeadCount\n    PositionCell 0 0 0 0 Ald-ruhn\n    AddTopic S'Bakha\n    Player->AddItem 1gr_comp2_wb 1\nendif\nif ( a == 1 )\n    set a to a-1\n    set a to 1.5e3\n    AddItem \"Zurgim_gro-Burug\" 2\nendif\nend\n";
        let m = msgs(text);
        assert!(m.contains(&(3, "warning".into(), "a single '=' in a condition: the CS reads it as '==', MWEdit refuses it; write '=='".into())));
        assert!(m.contains(&(4, "warning".into(), "a stray '=' after 'to'".into())));
        assert!(m.contains(&(5, "warning".into(), "'<>' is not 'not equal' (the game reads it as '<' or '>'): use '!='".into())));
        assert!(m.iter().any(|f| f.0 == 6 && f.2.starts_with("GetDeadCount takes the id after it")));
        let tail = "needs quotes: an id with a hyphen, an apostrophe or a leading digit is misread without them";
        assert!(m.contains(&(7, "warning".into(), format!("Ald-ruhn {tail}"))));
        assert!(m.contains(&(8, "warning".into(), format!("S'Bakha {tail}"))));
        assert!(m.contains(&(9, "warning".into(), format!("1gr_comp2_wb {tail}"))));
        assert!(!m.iter().any(|f| f.0 >= 11), "{m:?}");
        let body: String = (1..=20).map(|i| format!("elseif ( a == {i} )\n")).collect();
        let chain: Vec<_> = msgs(&format!("begin x\nshort a\nif ( a == 0 )\n{body}endif\nend")).into_iter().filter(|f| f.2.contains("'elseif's in one chain")).collect();
        assert_eq!(chain, vec![(23, "warning".to_string(), "20 'elseif's in one chain: the game can fail on a late branch (RightEval/LeftEval); split it".to_string())]);
    }
}
