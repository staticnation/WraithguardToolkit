//! The downloads of `wraithguard/net/updaters.py`: the mlox rule databases, MOMW's
//! `plugin-order.yml`, and a curated list's data-path order from MOMW's cfg-generator API.
//!
//! **Where the network lives.** Not here: this crate takes no HTTP stack. The caller gives a
//! [`Transport`] (Python's `urllib` while Python still calls this, the viewer's own client once
//! the page does) and this decides everything that is *safe to do with what comes back*:
//!
//! - every URL is untrusted input (it is read from a settings file and an environment
//!   variable), so [`check_url`] allows only `http` and `https`, and a host is required, before
//!   [`Transport::get`] is ever called; a hostile `file:///etc/passwd` is never fetched;
//! - a body over the size cap is refused rather than truncated ([`fetch_url_bytes`]);
//! - a download is checked against what it is supposed to be *before anything on disk is
//!   touched* (an HTML error page must not replace the curated list that decides which plugins
//!   are the user's own), the old file is kept as a timestamped `.bak`, and an unchanged
//!   download is reported as up to date and left alone.
//!
//! The things that are not this crate's to decide come in as arguments, so the flows are pure
//! apart from the files they write: the clock's stamp for a `.bak` name (`stamp`, local time as
//! the user reads it), the three environment variables ([`Env`]), and, for `plugin-order.yml`,
//! the YAML reader (`parse`, which answers the number of entries or why it cannot: Python's
//! `parse_plugin_order_yml` today, the Rust port when 2f lands).
//!
//! The report lines are the Python's, word for word. The detail a *parser* adds to a failure
//! (a JSON syntax error's position, a YAML reader's complaint) is that parser's own.
//!
//! What is not read is **declined** ([`UpdateError::Unsupported`]), as in
//! [`crate::patterns`]: a URL whose host has brackets or non-ASCII characters (Python's
//! `urlsplit` checks them in ways that differ between its patch releases), a URL template with
//! a format spec, conversion or attribute in its field, and a path value with non-ASCII
//! characters where Python case-folds. The caller keeps Python's answer.
//!
//! Written from the documented behaviour of the sources, not from their code.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::patterns::Unsupported;
use crate::rules::{is_space, splitlines};

/// Where MOMW publishes `plugin-order.yml` (the raw path and the API path have each been the
/// one that works at different times).
pub const PLUGIN_ORDER_URLS: [&str; 2] = [
    "https://gitlab.com/modding-openmw/modding-openmw.com/-/raw/master/momw/momw/data_seeds/data/plugin-order.yml?ref_type=heads&inline=false",
    "https://gitlab.com/api/v4/projects/modding-openmw%2Fmodding-openmw.com/repository/files/momw%2Fmomw%2Fdata_seeds%2Fdata%2Fplugin-order.yml/raw?ref=master",
];
/// MOMW's per-list cfg-generator API; `{host}` and `{list}` are filled per request.
pub const API_CFG_GENERATOR_URL: &str = "{host}/api/cfg-generator/{list}";
/// The API's host unless `$MOMW_API_HOST` or the caller says otherwise.
pub const DEFAULT_MOMW_HOST: &str = "https://modding-openmw.com";
/// The only schemes downloaded from.
pub const ALLOWED_URL_SCHEMES: [&str; 2] = ["http", "https"];
/// A single download's ceiling (the rule files are about 2 MB).
pub const MAX_DOWNLOAD_BYTES: usize = 32 * 1024 * 1024;
/// The rule filenames the upstream repository manages: the only ones ever overwritten.
pub const MANAGED_RULE_FILES: [&str; 2] = ["mlox_base.txt", "mlox_user.txt"];
/// One rule file's URL; `{name}` is the filename.
pub const RULES_URL_TEMPLATE: &str = "https://raw.githubusercontent.com/DanaePlays/mlox-rules/main/{name}";
/// The placeholder base of a cfg-generator response's `data=` paths.
pub const DEFAULT_BASE_DIR: &str = "C:/games/OpenMWMods";

/// The network, given by the caller.
pub trait Transport {
    /// Fetch `url` and answer at most `max_bytes` of the body (the caller may stop reading
    /// there), or why the request failed (its text goes into the report as it is).
    ///
    /// # Errors
    /// The request's own failure, as text.
    fn get(&self, url: &str, timeout_secs: u64, max_bytes: usize) -> Result<Vec<u8>, String>;
}

/// Why a download did not happen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FetchFail {
    /// The URL or the body was refused (Python's `ValueError`).
    Value(String),
    /// The request failed (Python's `OSError`).
    Os(String),
    /// A URL this does not read.
    Unsupported,
}

impl FetchFail {
    /// The text the reports carry.
    fn text(&self) -> &str {
        match self {
            FetchFail::Value(t) | FetchFail::Os(t) => t,
            FetchFail::Unsupported => "",
        }
    }
}

/// What can stop an update: something this does not read, or the disk.
#[derive(Debug)]
pub enum UpdateError {
    /// See the module.
    Unsupported,
    /// A file could not be read or written (Python raises it too).
    Io(io::Error),
}

impl From<Unsupported> for UpdateError {
    fn from(_: Unsupported) -> Self {
        UpdateError::Unsupported
    }
}

impl From<io::Error> for UpdateError {
    fn from(e: io::Error) -> Self {
        UpdateError::Io(e)
    }
}

/// What the flows are given that they must not look up themselves.
#[derive(Clone, Copy, Debug, Default)]
pub struct Env<'a> {
    /// The `.bak` stamp, `YYYYMMDD-HHMMSS` in the user's local time.
    pub stamp: &'a str,
    /// `$MLOX_PLUGIN_ORDER_URL`, if set (empty counts as not set).
    pub plugin_order_url: Option<&'a str>,
    /// `$MLOX_RULES_URL_TEMPLATE`, if set.
    pub rules_url_template: Option<&'a str>,
    /// `$MOMW_API_HOST`, if set.
    pub momw_api_host: Option<&'a str>,
}

fn truthy(s: Option<&str>) -> Option<&str> {
    s.filter(|v| !v.is_empty())
}

fn strip(s: &str) -> &str {
    s.trim_matches(is_space)
}

/// `f"{n:,}"`.
fn commas(n: usize) -> String {
    let digits = n.to_string();
    let groups: Vec<&str> = digits
        .as_bytes()
        .rchunks(3)
        .rev()
        .map(|chunk| std::str::from_utf8(chunk).unwrap_or_default())
        .collect();
    groups.join(",")
}

/// Python's `repr` of a string, for ASCII (the way a template is quoted in a report).
fn py_repr(s: &str) -> Result<String, Unsupported> {
    if !s.is_ascii() {
        return Err(Unsupported);
    }
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
            c if c < ' ' || c == '\u{7f}' => out.push_str(&format!("\\x{:02x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push(quote);
    Ok(out)
}

// ---------------------------------------------------------------- the URL guard and the size cap

/// The scheme and the host of a URL as Python's `urlsplit` reads them (3.12): leading control
/// characters and spaces stripped, tabs and line breaks removed, a scheme only if it is ASCII
/// letters, digits, `+`, `-` and `.` up to the first colon, and a host only after `//`.
fn scheme_and_netloc(url: &str) -> Result<(String, String), Unsupported> {
    let mut s: String = url.trim_start_matches(|c: char| c <= ' ').to_string();
    s.retain(|c| !matches!(c, '\t' | '\r' | '\n'));
    let mut scheme = String::new();
    if let Some(i) = s.find(':') {
        let head = &s[..i];
        let starts_alpha = s.chars().next().is_some_and(|c| c.is_ascii_alphabetic());
        if i > 0 && starts_alpha && head.chars().all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c)) {
            scheme = head.to_ascii_lowercase();
            s = s[i + 1..].to_string();
        }
    }
    let mut netloc = String::new();
    if let Some(rest) = s.strip_prefix("//") {
        let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
        netloc = rest[..end].to_string();
        if netloc.contains(['[', ']']) || !netloc.is_ascii() {
            return Err(Unsupported); // checked in ways that differ between Python's patch releases
        }
    }
    Ok((scheme, netloc))
}

/// Refuse a URL that is not `http(s)` with a host (`fetch_url_bytes`'s guard), before anything
/// is fetched.
///
/// # Errors
/// [`FetchFail::Value`] with Python's message; [`FetchFail::Unsupported`] for a URL not read.
pub fn check_url(url: &str) -> Result<(), FetchFail> {
    let (scheme, netloc) = scheme_and_netloc(url).map_err(|_| FetchFail::Unsupported)?;
    if !ALLOWED_URL_SCHEMES.contains(&scheme.as_str()) {
        let shown = if scheme.is_empty() { "scheme-less" } else { &scheme };
        return Err(FetchFail::Value(format!(
            "refusing to download from a '{shown}' URL -- only {} are allowed",
            ALLOWED_URL_SCHEMES.join("/")
        )));
    }
    if netloc.is_empty() {
        return Err(FetchFail::Value("URL has no host".to_string()));
    }
    Ok(())
}

/// Download `url` through the transport (`fetch_url_bytes`): the URL is checked first, and a
/// body over `max_bytes` is refused, not cut.
///
/// # Errors
/// [`FetchFail`].
pub fn fetch_url_bytes(
    transport: &dyn Transport,
    url: &str,
    timeout_secs: u64,
    max_bytes: usize,
) -> Result<Vec<u8>, FetchFail> {
    check_url(url)?;
    // one byte more than the cap, so a body of exactly the cap fits and a larger one is seen
    let data = transport.get(url, timeout_secs, max_bytes + 1).map_err(FetchFail::Os)?;
    if data.len() > max_bytes {
        return Err(FetchFail::Value(format!("response larger than the {}-byte limit", commas(max_bytes))));
    }
    Ok(data)
}

// ---------------------------------------------------------------- small readers

/// The relative tail of a `data=` value (`relative_tail`): what follows the mod base
/// directory, forward-slashed and unquoted; the whole value if the base is not in it.
///
/// # Errors
/// [`Unsupported`] for a non-ASCII value (Python case-folds, which changes lengths).
pub fn relative_tail(value: &str, base: &str) -> Result<String, Unsupported> {
    if !value.is_ascii() || !base.is_ascii() {
        return Err(Unsupported);
    }
    let v = strip(value).trim_matches('"').trim_matches('\'').replace('\\', "/");
    let marker = format!("{}/", base.replace('\\', "/").trim_end_matches('/').to_ascii_lowercase());
    Ok(match v.to_ascii_lowercase().find(&marker) {
        Some(i) => v[i + marker.len()..].to_string(),
        None => v,
    })
}

/// The ordered relative data-path tails of a cfg-generator response
/// (`_data_path_tails_from_api`): `openmw_cfg["data paths"]` is the finished `data=` block as
/// one string.
///
/// # Errors
/// `Ok(Err(text))` for Python's `ValueError` (the text goes into the report); [`Unsupported`].
pub fn data_path_tails_from_api(payload: &[u8]) -> Result<Result<Vec<String>, String>, Unsupported> {
    fn kind(v: &serde_json::Value) -> &'static str {
        match v {
            serde_json::Value::Null => "NoneType",
            serde_json::Value::Bool(_) => "bool",
            serde_json::Value::Number(n) if n.is_f64() => "float",
            serde_json::Value::Number(_) => "int",
            serde_json::Value::String(_) => "str",
            serde_json::Value::Array(_) => "list",
            serde_json::Value::Object(_) => "dict",
        }
    }
    fn index<'a>(v: &'a serde_json::Value, key: &str) -> Result<&'a serde_json::Value, String> {
        match v {
            serde_json::Value::Object(map) => map.get(key).ok_or_else(|| format!("'{key}'")),
            serde_json::Value::Array(_) => {
                Err("list indices must be integers or slices, not str".to_string())
            }
            serde_json::Value::String(_) => Err("string indices must be integers, not 'str'".to_string()),
            other => Err(format!("'{}' object is not subscriptable", kind(other))),
        }
    }
    let wrapped = |detail: String| Ok(Err(format!("not a cfg-generator response ({detail})")));
    let obj: serde_json::Value = match serde_json::from_slice(payload) {
        Ok(v) => v,
        Err(e) => return wrapped(e.to_string()),
    };
    let block = match index(&obj, "openmw_cfg").and_then(|v| index(v, "data paths")) {
        Ok(v) => v,
        Err(detail) => return wrapped(detail),
    };
    let serde_json::Value::String(block) = block else {
        return Ok(Err("'data paths' is not a string".to_string()));
    };
    let mut tails = Vec::new();
    for line in splitlines(block) {
        let stripped = strip(line);
        if let Some(rest) = stripped.strip_prefix("data=") {
            tails.push(relative_tail(rest, DEFAULT_BASE_DIR)?);
        }
    }
    Ok(Ok(tails))
}

/// The tail lines of a cache file's bytes, comments and blanks dropped (`parse_cache_paths`).
pub fn parse_cache_paths(payload: &[u8]) -> Vec<String> {
    let text = String::from_utf8_lossy(payload);
    splitlines(&text)
        .into_iter()
        .map(strip)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| l.replace('\\', "/"))
        .collect()
}

/// `template.format(name=name)` for the one placeholder this reads. `Ok(Err(text))` is the
/// exception Python raises (its message); a field with a spec, conversion or attribute is
/// declined.
fn format_name(template: &str, name: &str) -> Result<Result<String, String>, Unsupported> {
    let chars: Vec<char> = template.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '{' => {
                match chars.get(i + 1) {
                    Some('{') => {
                        out.push('{');
                        i += 2;
                        continue;
                    }
                    None => return Ok(Err("Single '{' encountered in format string".to_string())),
                    _ => {}
                }
                let mut j = i + 1;
                while j < chars.len() && chars[j] != '}' {
                    if chars[j] == '{' {
                        return Err(Unsupported);
                    }
                    j += 1;
                }
                if j == chars.len() {
                    return Ok(Err("expected '}' before end of string".to_string()));
                }
                let field: String = chars[i + 1..j].iter().collect();
                if field.contains([':', '!', '.', '[']) {
                    return Err(Unsupported);
                }
                if field == "name" {
                    out.push_str(name);
                } else if field.is_empty() {
                    return Ok(Err("Replacement index 0 out of range for positional args tuple".to_string()));
                } else if field.chars().all(|c| c.is_ascii_digit()) {
                    return Ok(Err(format!(
                        "Replacement index {field} out of range for positional args tuple"
                    )));
                } else {
                    return Ok(Err(py_repr(&field)?));
                }
                i = j + 1;
            }
            '}' => {
                if chars.get(i + 1) == Some(&'}') {
                    out.push('}');
                    i += 2;
                } else {
                    return Ok(Err("Single '}' encountered in format string".to_string()));
                }
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    Ok(Ok(out))
}

// ---------------------------------------------------------------- the flows

fn file_name(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

fn read_or_empty(p: &Path) -> Result<Vec<u8>, UpdateError> {
    if p.exists() {
        Ok(fs::read(p)?)
    } else {
        Ok(Vec::new())
    }
}

/// Keep the file as `<name>.bak-<stamp>` beside it.
fn keep_bak(p: &Path, old: &[u8], stamp: &str) -> Result<(), UpdateError> {
    let bak = p.with_file_name(format!("{}.bak-{stamp}", file_name(p)));
    fs::write(bak, old)?;
    Ok(())
}

/// Download the current MOMW `plugin-order.yml` over `path` (`update_plugin_order_yml`).
///
/// Validated before anything on disk is touched: the download must name `file_name` and
/// `on_lists`, parse (`parse`: the number of entries, or why not) and hold at least 100 entries.
/// `urls` (the Sources setting) beats `$MLOX_PLUGIN_ORDER_URL` beats the built-in two. Failures
/// are reported, not raised.
///
/// # Errors
/// [`UpdateError`].
pub fn update_plugin_order_yml(
    path: &Path,
    urls: Option<&[String]>,
    timeout_secs: u64,
    env: &Env<'_>,
    transport: &dyn Transport,
    parse: &dyn Fn(&[u8]) -> Result<usize, String>,
) -> Result<Vec<String>, UpdateError> {
    let candidates: Vec<String> = match urls {
        Some(list) if !list.is_empty() => list.to_vec(),
        _ => match truthy(env.plugin_order_url) {
            Some(url) => vec![url.to_string()],
            None => PLUGIN_ORDER_URLS.iter().map(|u| (*u).to_string()).collect(),
        },
    };
    let name = file_name(path);
    let mut report: Vec<String> = Vec::new();
    for url in &candidates {
        let data = match fetch_url_bytes(transport, url, timeout_secs, MAX_DOWNLOAD_BYTES) {
            Ok(d) => d,
            Err(FetchFail::Unsupported) => return Err(UpdateError::Unsupported),
            Err(e) => {
                report.push(format!("  {url}: {}", e.text()));
                continue;
            }
        };
        let has = |needle: &[u8]| data.windows(needle.len()).any(|w| w == needle);
        if !has(b"file_name") || !has(b"on_lists") {
            report.push(format!("  {url}: response doesn't look like plugin-order.yml"));
            continue;
        }
        let entries = match parse(&data) {
            Ok(n) => n,
            Err(e) => {
                report.push(format!("  {url}: downloaded but failed to parse ({e})"));
                continue;
            }
        };
        if entries < 100 {
            report.push(format!("  {url}: parsed but only {entries} entries -- refusing"));
            continue;
        }
        let old = read_or_empty(path)?;
        if old == data {
            report.push(format!("{name}: already up to date ({entries} entries)."));
            return Ok(report);
        }
        if path.exists() {
            keep_bak(path, &old, env.stamp)?;
        }
        fs::write(path, &data)?;
        // the "N -> M" count is cosmetic: the new file is written, so a failure to read the old
        // one must be invisible
        let from = if old.is_empty() { None } else { parse(&old).ok() };
        let frm = from.map(|n| format!("{n} -> ")).unwrap_or_default();
        report.push(format!(
            "{name}: updated from {url} ({frm}{entries} entries; previous version kept as .bak)."
        ));
        return Ok(report);
    }
    report.insert(0, "FAILED: no source produced a valid plugin-order.yml:".to_string());
    report.push("  (set $MLOX_PLUGIN_ORDER_URL if MOMW moved the file)".to_string());
    Ok(report)
}

/// Cache a curated list's data-path order from the MOMW cfg-generator API
/// (`fetch_list_data_paths`): the response must parse as a cfg-generator payload and yield at
/// least 10 paths, or the cache is left alone.
///
/// # Errors
/// [`UpdateError`].
pub fn fetch_list_data_paths(
    path: &Path,
    list_name: &str,
    host: Option<&str>,
    timeout_secs: u64,
    env: &Env<'_>,
    transport: &dyn Transport,
) -> Result<Vec<String>, UpdateError> {
    let name = file_name(path);
    let slug = strip(list_name);
    if slug.is_empty() {
        return Ok(vec!["FAILED: no list name -- set the list name field first.".to_string()]);
    }
    let api_host =
        truthy(host).or(truthy(env.momw_api_host)).unwrap_or(DEFAULT_MOMW_HOST).trim_end_matches('/');
    let url = API_CFG_GENERATOR_URL.replace("{host}", api_host).replace("{list}", slug);
    let payload = match fetch_url_bytes(transport, &url, timeout_secs, MAX_DOWNLOAD_BYTES) {
        Ok(p) => p,
        Err(FetchFail::Unsupported) => return Err(UpdateError::Unsupported),
        Err(e) => return Ok(vec![format!("FAILED: {url}: {}", e.text())]),
    };
    let tails = match data_path_tails_from_api(&payload)? {
        Ok(t) => t,
        Err(e) => return Ok(vec![format!("FAILED: {url}: {e} (is '{slug}' a real list?)")]),
    };
    if tails.len() < 10 {
        return Ok(vec![format!("FAILED: {url}: only {} data path(s) -- refusing to cache.", tails.len())]);
    }
    let header = format!(
        "# MOMW data paths for list: {slug}\n# fetched from {url}\n# one relative data-path tail per line, in load order -- do not edit\n"
    );
    let data = format!("{header}{}\n", tails.join("\n")).into_bytes();
    let old = read_or_empty(path)?;
    // compare on the path lines only, so a changed header alone is not a spurious "updated"
    if !old.is_empty() && parse_cache_paths(&old) == tails {
        return Ok(vec![format!("{name}: already up to date ({} data paths).", tails.len())]);
    }
    if path.exists() {
        keep_bak(path, &old, env.stamp)?;
    }
    fs::write(path, data)?;
    Ok(vec![format!("{name}: cached {} data paths for '{slug}' (previous kept as .bak).", tails.len())])
}

/// Download the maintained mlox rule databases over the configured files
/// (`update_rule_files`). Only the filenames the upstream repository manages are touched.
/// `url_template` beats `$MLOX_RULES_URL_TEMPLATE` beats the built-in; it must hold `{name}`.
///
/// # Errors
/// [`UpdateError`].
pub fn update_rule_files(
    rule_paths: &[PathBuf],
    url_template: Option<&str>,
    timeout_secs: u64,
    env: &Env<'_>,
    transport: &dyn Transport,
) -> Result<Vec<String>, UpdateError> {
    let template = truthy(url_template).or(truthy(env.rules_url_template)).unwrap_or(RULES_URL_TEMPLATE);
    if !template.contains("{name}") {
        return Ok(vec![format!("FAILED: rules URL template must contain '{{name}}': {template}")]);
    }
    let mut report = Vec::new();
    for p in rule_paths {
        let name = file_name(p);
        if !MANAGED_RULE_FILES.contains(&name.to_lowercase().as_str()) {
            report.push(format!("skipped {name}: not an upstream-managed filename"));
            continue;
        }
        let url = match format_name(template, &name)? {
            Ok(u) => u,
            Err(e) => {
                return Ok(vec![format!(
                    "FAILED: bad rules URL template {}: {e}. Only the '{{name}}' placeholder is supported.",
                    py_repr(template)?
                )]);
            }
        };
        let data = match fetch_url_bytes(transport, &url, timeout_secs, MAX_DOWNLOAD_BYTES) {
            Ok(d) => d,
            Err(FetchFail::Unsupported) => return Err(UpdateError::Unsupported),
            Err(e) => {
                report.push(format!("FAILED {name}: {}", e.text()));
                continue;
            }
        };
        if data.is_empty() || !data.windows(7).any(|w| w == b"[Order]") {
            report.push(format!("FAILED {name}: download doesn't look like an mlox rules file"));
            continue;
        }
        let old = read_or_empty(p)?;
        if old == data {
            report.push(format!("{name}: already up to date ({} bytes)", commas(data.len())));
            continue;
        }
        if p.exists() {
            keep_bak(p, &old, env.stamp)?;
        }
        fs::write(p, &data)?;
        report.push(format!(
            "{name}: updated {} -> {} bytes (previous version kept as .bak)",
            commas(old.len()),
            commas(data.len())
        ));
    }
    Ok(report)
}

/// How stale each rule file is (`rule_file_ages`): `(filename, age in whole days)`, `None` for a
/// file whose modification time cannot be read. `now` is the clock in seconds since the epoch.
pub fn rule_file_ages(rule_paths: &[PathBuf], now: f64) -> Vec<(String, Option<i64>)> {
    rule_paths
        .iter()
        .map(|p| {
            let mtime = fs::metadata(p)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs_f64());
            (file_name(p), mtime.map(|m| ((now - m) / 86400.0).floor() as i64))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    struct Fake {
        body: Result<Vec<u8>, String>,
        asked: RefCell<Vec<String>>,
    }

    impl Transport for Fake {
        fn get(&self, url: &str, _timeout: u64, max: usize) -> Result<Vec<u8>, String> {
            self.asked.borrow_mut().push(url.to_string());
            self.body.clone().map(|b| b[..b.len().min(max)].to_vec())
        }
    }

    fn fake(body: &[u8]) -> Fake {
        Fake { body: Ok(body.to_vec()), asked: RefCell::new(Vec::new()) }
    }

    #[test]
    fn only_http_and_https_with_a_host_get_through() {
        for ok in ["http://a.b/x", "HTTPS://a.b", "  \thttps://h/p?q#f", "ht\ntp://h"] {
            assert_eq!(check_url(ok), Ok(()), "{ok:?}");
        }
        let refused = |url: &str| match check_url(url) {
            Err(FetchFail::Value(m)) => m,
            other => panic!("{url:?}: {other:?}"),
        };
        assert_eq!(
            refused("file:///etc/passwd"),
            "refusing to download from a 'file' URL -- only http/https are allowed"
        );
        assert_eq!(
            refused("not-a-url"),
            "refusing to download from a 'scheme-less' URL -- only http/https are allowed"
        );
        assert_eq!(refused("https://"), "URL has no host");
        assert_eq!(refused("HTTP:///path"), "URL has no host");
        assert_eq!(check_url("http://[::1]/"), Err(FetchFail::Unsupported));
    }

    #[test]
    fn a_refused_url_is_never_fetched() {
        let t = fake(b"x");
        assert!(fetch_url_bytes(&t, "file:///etc/passwd", 5, 100).is_err());
        assert!(t.asked.borrow().is_empty());
    }

    #[test]
    fn a_body_over_the_cap_is_refused_and_one_at_the_cap_is_kept() {
        assert_eq!(fetch_url_bytes(&fake(&[1; 10]), "http://h/", 5, 10).unwrap().len(), 10);
        let got = fetch_url_bytes(&fake(&[1; 11]), "http://h/", 5, 10);
        assert_eq!(got, Err(FetchFail::Value("response larger than the 10-byte limit".to_string())));
        let big = fetch_url_bytes(&fake(&[1; 2000]), "http://h/", 5, 1500);
        assert_eq!(big, Err(FetchFail::Value("response larger than the 1,500-byte limit".to_string())));
    }

    #[test]
    fn the_template_is_formatted_as_python_formats_it() {
        let f = |t: &str| format_name(t, "mlox_base.txt").unwrap();
        assert_eq!(f("http://h/{name}"), Ok("http://h/mlox_base.txt".to_string()));
        assert_eq!(f("{{x}}{name}"), Ok("{x}mlox_base.txt".to_string()));
        assert_eq!(f("{branch}{name}"), Err("'branch'".to_string()));
        assert_eq!(f("{name"), Err("expected '}' before end of string".to_string()));
        assert_eq!(f("a{"), Err("Single '{' encountered in format string".to_string()));
        assert_eq!(f("a}"), Err("Single '}' encountered in format string".to_string()));
        assert_eq!(f("{}"), Err("Replacement index 0 out of range for positional args tuple".to_string()));
        assert_eq!(format_name("{name!r}", "x"), Err(Unsupported));
    }

    #[test]
    fn a_tail_is_what_follows_the_base() {
        let t = |v: &str| relative_tail(v, DEFAULT_BASE_DIR).unwrap();
        assert_eq!(t("\"C:\\games\\OpenMWMods\\Cat\\Mod\\dir\""), "Cat/Mod/dir");
        assert_eq!(t("c:/GAMES/openmwmods/Cat/Mod"), "Cat/Mod");
        assert_eq!(t("Cat/Mod"), "Cat/Mod");
    }
}
