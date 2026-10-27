//! The configurator preview (`wraithguard/configurator/apply.py`, port step 4.2 slice 2k, part
//! 3): momw-configurator's apply step (`ApplyCustomizations` in its `cfg/custom.go`, read from its
//! documented behaviour) dry-run against the cfg, so the emitted TOML is checked to reproduce the
//! sorted order before anyone runs the real thing. Its sharp edges are kept on purpose: an insert
//! anchor matching more than one line is fatal, a replace source matching more than one is
//! skipped, a removal takes every line it matches.
//!
//! Declined ([`Unsupported`]): a TOML that is not the shape the export writes (a value of another
//! type where the Python would name its type or `repr` it), and a line to quote that holds a
//! character Python's `repr` would escape and this does not know.

use std::collections::HashSet;
use std::sync::OnceLock;

use crate::export::{configurator_remove_matches, normalize_data_path};
use crate::patterns::Unsupported;
use crate::pyre::{Flags, Regex};
use crate::rules::is_space;

/// The `remove*` keys the configurator reads, in the order the Python applies them.
const REMOVE_KEYS: [&str; 5] = ["removeData", "removeFallbackArchive", "removeContent", "removeFallback", "removeGroundcover"];

/// Python's `repr` of a string: single quotes unless it holds one and no double quote, `\\`,
/// `\n`, `\r`, `\t` and other controls escaped, the rest as it is.
fn py_repr(s: &str) -> Result<String, Unsupported> {
    let quote = if s.contains('\'') && !s.contains('"') { '"' } else { '\'' };
    let mut out = String::from(quote);
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
            c if (c as u32) < 0x20 || (0x7f..=0xa0).contains(&(c as u32)) => {
                out.push_str(&format!("\\x{:02x}", c as u32));
            }
            '\u{ad}' | '\u{2000}'..='\u{200f}' | '\u{2028}'..='\u{202f}' | '\u{205f}'..='\u{206f}' | '\u{3000}'
            | '\u{feff}' | '\u{e000}'..='\u{f8ff}' => return Err(Unsupported),
            c => out.push(c),
        }
    }
    out.push(quote);
    Ok(out)
}

fn get_str(t: &toml::Table, k: &str) -> Result<Option<String>, Unsupported> {
    match t.get(k) {
        None => Ok(None),
        Some(toml::Value::String(s)) => Ok(Some(s.clone())),
        Some(_) => Err(Unsupported),
    }
}

fn tables<'a>(t: &'a toml::Table, k: &str) -> Result<Vec<&'a toml::Table>, Unsupported> {
    match t.get(k) {
        None => Ok(Vec::new()),
        Some(toml::Value::Array(a)) => a.iter().map(|v| v.as_table().ok_or(Unsupported)).collect(),
        Some(_) => Err(Unsupported),
    }
}

fn removes(t: &toml::Table) -> Result<Vec<String>, Unsupported> {
    let mut out = Vec::new();
    for key in REMOVE_KEYS {
        match t.get(key) {
            None => {}
            Some(toml::Value::Array(a)) => {
                for v in a {
                    out.push(v.as_str().ok_or(Unsupported)?.to_string());
                }
            }
            Some(_) => return Err(Unsupported),
        }
    }
    Ok(out)
}

fn block_lines(text: &str) -> Vec<String> {
    text.split('\n').filter(|l| !l.is_empty()).map(|l| l.replace('\r', "")).collect()
}

fn matches_detail(lines: &[String], matches: &[usize]) -> Result<String, Unsupported> {
    let shown = &matches[..matches.len().min(5)];
    let mut parts = Vec::new();
    for &i in shown {
        parts.push(format!("line {}: {}", i + 1, py_repr(&lines[i])?));
    }
    let mut detail = parts.join("; ");
    if matches.len() > shown.len() {
        detail.push_str(&format!("; ... and {} more", matches.len() - shown.len()));
    }
    Ok(detail)
}

fn prefix_of(line: &str) -> &str {
    line.split('=').next().unwrap_or_default()
}

/// What [`simulate_configurator_apply`] gives: the cfg's lines after the apply (`None`: the run
/// would abort), the per-entry errors, and the notes.
pub type Simulated = (Option<Vec<String>>, Vec<String>, Vec<String>);

/// Dry-run the configurator's apply step on `cfg_lines` (`simulate_configurator_apply`): per
/// block, its inserts, replaces, removes and appends, in that order; only `list_name`'s blocks
/// when one is given (a block naming no list always applies).
///
/// # Errors
/// [`Unsupported`]: see the module.
pub fn simulate_configurator_apply(cfg_lines: &[String], toml_text: &str, list_name: Option<&str>) -> Result<Simulated, Unsupported> {
    let data = match toml_text.parse::<toml::Table>() {
        Ok(t) if !toml_text.starts_with('\u{feff}') => t,
        // the parse error is tomllib's to word
        _ => return Err(Unsupported),
    };
    let mut lines: Vec<String> = cfg_lines.to_vec();
    let mut errs = Vec::new();
    let mut notes = Vec::new();
    let check = |v: &str, notes: &mut Vec<String>| {
        if v.contains("{{") || v.contains('$') {
            notes.push(format!("'{v}': template/env var left unexpanded in the preview"));
        }
    };
    for cust in tables(&data, "Customizations")? {
        let block_list = get_str(cust, "listName")?.filter(|n| !n.is_empty());
        if let (Some(want), Some(have)) = (list_name.filter(|n| !n.is_empty()), &block_list) {
            if have != want {
                continue;
            }
        }
        for st in tables(cust, "insert")? {
            let insert = get_str(st, "insert")?;
            let block = get_str(st, "insertBlock")?;
            let after = get_str(st, "after")?;
            let before = get_str(st, "before")?;
            let Some(target) = after.clone().or_else(|| before.clone()) else {
                errs.push("insert entry needs insert/insertBlock plus after or before".to_string());
                continue;
            };
            if insert.is_none() && block.is_none() {
                errs.push("insert entry needs insert/insertBlock plus after or before".to_string());
                continue;
            }
            if after.is_some() && before.is_some() {
                errs.push("after and before cannot be used together".to_string());
                continue;
            }
            let found: Vec<usize> = lines.iter().enumerate().filter(|(_, l)| l.contains(&target)).map(|(i, _)| i).collect();
            if found.is_empty() {
                errs.push(format!("target line is not present in openmw.cfg: {target}"));
                continue;
            }
            if found.len() > 1 {
                errs.push(format!(
                    "FATAL: multiple matches for anchor '{target}' -- the real Configurator abandons the cfg here. \
                     Matching lines -> {}",
                    matches_detail(&lines, &found)?
                ));
                return Ok((None, errs, notes));
            }
            let idx = found[0];
            let prefix = prefix_of(&lines[idx]).to_string();
            let first = if after.is_some() { idx + 1 } else { idx };
            let vals = match insert {
                Some(v) => vec![v],
                None => block_lines(block.as_deref().unwrap_or_default()),
            };
            for (dest, v) in (first..).zip(vals) {
                check(&v, &mut notes);
                lines.insert(dest, format!("{prefix}={v}"));
            }
        }
        for st in tables(cust, "replace")? {
            let (Some(src), Some(dst)) = (get_str(st, "source")?, get_str(st, "dest")?) else {
                errs.push("replace entry needs source and dest".to_string());
                continue;
            };
            let found: Vec<usize> = lines.iter().enumerate().filter(|(_, l)| l.contains(&src)).map(|(i, _)| i).collect();
            if found.len() > 1 {
                errs.push(format!(
                    "replace source '{src}' matches more than one line -- skipped. Matching lines -> {}",
                    matches_detail(&lines, &found)?
                ));
                continue;
            }
            if let Some(&idx) = found.first() {
                let prefix = prefix_of(&lines[idx]).to_string();
                check(&dst, &mut notes);
                lines[idx] = if prefix == "data" { format!("{prefix}=\"{dst}\"") } else { format!("{prefix}={dst}") };
            }
        }
        let rm = removes(cust)?;
        if !rm.is_empty() {
            lines.retain(|l| !rm.iter().any(|v| configurator_remove_matches(v, l)));
        }
        let mut gc = Vec::new();
        let mut other = Vec::new();
        for st in tables(cust, "append")? {
            let vals = if st.contains_key("append") {
                vec![get_str(st, "append")?.unwrap_or_default()]
            } else {
                block_lines(&get_str(st, "appendBlock")?.unwrap_or_default())
            };
            for v in vals {
                check(&v, &mut notes);
                if v.starts_with("groundcover=") { gc.push(v) } else { other.push(v) }
            }
        }
        if !gc.is_empty() {
            match lines.iter().rposition(|l| l.starts_with("groundcover=")) {
                Some(last) => {
                    for (j, v) in gc.into_iter().enumerate() {
                        lines.insert(last + 1 + j, v);
                    }
                }
                None => {
                    lines.extend(["", "#                   #", "# GROUNDCOVER FILES #", "#                   #"].map(String::from));
                    lines.extend(gc);
                }
            }
        }
        if !other.is_empty() {
            lines.extend(["", "#                #", "# APPENDED LINES #", "#                #"].map(String::from));
            lines.extend(other);
        }
    }
    Ok((Some(lines), errs, notes))
}

fn keyed(cell: &'static OnceLock<Regex>, pattern: &str, line: &str) -> Result<Option<String>, Unsupported> {
    let re = cell.get_or_init(|| {
        Regex::new(pattern, Flags { ignore_case: true, multiline: false }).expect("a fixed expression compiles")
    });
    let chars: Vec<char> = line.chars().collect();
    Ok(re.match_at(&chars, 0)?.and_then(|m| m.group(1)).map(|(a, b)| chars[a..b].iter().collect()))
}

fn content_value(line: &str) -> Result<Option<String>, Unsupported> {
    static RE: OnceLock<Regex> = OnceLock::new();
    keyed(&RE, r"^\s*content\s*=\s*(.+?)\s*$", line)
}

fn data_value(line: &str) -> Result<Option<String>, Unsupported> {
    static RE: OnceLock<Regex> = OnceLock::new();
    keyed(&RE, r"^\s*data\s*=\s*(.+?)\s*$", line)
}

/// Dry-run the emitted TOML and check the round trip (`preview_configurator_result`): applied to
/// the cfg without this run's own `content=` and `data=` lines (the fresh curated cfg the
/// configurator starts from), does it give `expected` (less what the TOML removes)? `(ok, the
/// lines the Python prints)`.
///
/// # Errors
/// [`Unsupported`]: see the module.
pub fn preview_configurator_result(
    plan_lines: &[String],
    toml_text: &str,
    expected: &[String],
    subset: &[String],
    user_data_norms: &[String],
    list_name: Option<&str>,
) -> Result<(bool, Vec<String>), Unsupported> {
    let subset_lower: HashSet<String> = subset.iter().map(|s| s.to_lowercase()).collect();
    let user: HashSet<&str> = user_data_norms.iter().map(String::as_str).filter(|n| !n.is_empty()).collect();
    let mut base = Vec::new();
    for line in plan_lines {
        if content_value(line)?.is_some_and(|v| subset_lower.contains(&v.to_lowercase())) {
            continue;
        }
        if let Some(v) = data_value(line)? {
            if user.contains(normalize_data_path(v.trim_matches(is_space).trim_matches('"')).as_str()) {
                continue;
            }
        }
        base.push(line.clone());
    }
    let (sim, errs, notes) = simulate_configurator_apply(&base, toml_text, list_name)?;
    let mut report: Vec<String> = notes.iter().map(|n| format!("  NOTE: {n}")).collect();
    report.extend(errs.iter().map(|e| format!("  WARNING: {e}")));
    let Some(sim) = sim else {
        report.push("  PREVIEW ABORTED -- the real Configurator run would fail the same way.".into());
        return Ok((false, report));
    };
    let mut sim_content = Vec::new();
    for line in &sim {
        if let Some(v) = content_value(line)? {
            sim_content.push(v);
        }
    }
    let data = toml_text.parse::<toml::Table>().map_err(|_| Unsupported)?;
    let mut rm = Vec::new();
    for cust in tables(&data, "Customizations")? {
        rm.extend(removes(cust)?);
    }
    let expected: Vec<&String> =
        expected.iter().filter(|n| !rm.iter().any(|v| configurator_remove_matches(v, &format!("content={n}")))).collect();
    if sim_content.iter().eq(expected.iter().copied()) {
        report.push(format!(
            "  VERIFIED: simulated apply reproduces the sorted order exactly ({} content= lines).",
            sim_content.len()
        ));
        return Ok((true, report));
    }
    report.push("  MISMATCH: the simulated Configurator result differs from the sorted order!".into());
    if let Some((i, (a, b))) = sim_content.iter().zip(&expected).enumerate().find(|(_, (a, b))| a != *b) {
        report.push(format!("    first difference at #{i}: simulated '{a}' vs expected '{b}'"));
    }
    if sim_content.len() != expected.len() {
        report.push(format!("    lengths differ: simulated {} vs expected {}", sim_content.len(), expected.len()));
    }
    Ok((false, report))
}
