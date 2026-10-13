//! An OpenMW `openmw.cfg`, read by the rules OpenMW documents ("Paths",
//! openmw.readthedocs.io/en/latest/reference/modding/paths.html):
//!
//! - A line is blank, a comment (its first non-blank character is `#`), or an option:
//!   a name, `=`, a value. Names and values are trimmed at both ends; whitespace inside a
//!   value is kept. Names are compared exactly (OpenMW's options are lower case).
//! - A path may be quoted: `"..."`, with `&"` for a quote and `&&` for an ampersand
//!   inside, and anything after the closing quote ignored (OpenMW uses the quoted part
//!   and logs the rest). Unquoted, it is taken as is.
//! - A relative path is relative to the cfg it is in. A path may start with a token:
//!   `?userdata?`, `?userconfig?`, `?local?` or `?global?` ([`Tokens`]).
//! - `config=` names more configuration folders, each with its own `openmw.cfg`, read
//!   depth first as OpenMW's code reads them (`components/files/configurationmanager.cpp`,
//!   a stack): `dir1` naming `dir2` then `dir3`, and `dir2` naming `dir4`, is the order
//!   dir1, dir2, dir4, dir3. (The documentation's example says dir3 before dir4; the code
//!   does not.) `replace=config` in a chained cfg drops every cfg read before it but the
//!   first.
//! - An option that takes many values (`data`, `content`, `groundcover`,
//!   `fallback-archive`) gathers them from every cfg, lower priority first. `replace=X`
//!   in a cfg drops the `X` values of every cfg below it. An option with one value
//!   (`data-local`, `user-data`, `resources`) takes the highest-priority cfg's, the
//!   later line within one cfg.
//! - `data-local` is always the last data folder: it overrides every `data=`.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Plugin file endings: what a `content=` line names that has records in it (an
/// `.omwscripts` list names scripts, not records).
pub const PLUGIN_EXTS: [&str; 4] = [".esm", ".esp", ".omwgame", ".omwaddon"];

/// Whether a file name is a plugin's ([`PLUGIN_EXTS`], any case).
pub fn is_plugin(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    PLUGIN_EXTS.iter().any(|e| n.ends_with(e))
}

/// Where OpenMW's path tokens point. A token with no folder here (`?local?` is the
/// folder of the OpenMW binary, which a cfg alone does not tell) makes its line a
/// warning rather than a guess.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Tokens {
    pub userdata: Option<PathBuf>,
    pub userconfig: Option<PathBuf>,
    pub local: Option<PathBuf>,
    pub global: Option<PathBuf>,
}

impl Tokens {
    /// This machine's folders, as OpenMW's documentation lists them per system.
    pub fn host() -> Tokens {
        let mut t = Tokens::default();
        #[cfg(windows)]
        {
            if let Some(h) = std::env::var_os("USERPROFILE") {
                let mine = PathBuf::from(h).join("Documents").join("My Games").join("OpenMW");
                t.userdata = Some(mine.clone());
                t.userconfig = Some(mine);
            }
        }
        #[cfg(target_os = "macos")]
        {
            if let Some(h) = std::env::var_os("HOME").map(PathBuf::from) {
                t.userconfig = Some(h.join("Library").join("Preferences").join("openmw"));
                t.userdata = Some(h.join("Library").join("Application Support").join("openmw"));
            }
            t.global = Some(PathBuf::from("/Library/Application Support"));
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            let home = std::env::var_os("HOME").map(PathBuf::from);
            let xdg = |var: &str, fallback: &[&str]| -> Option<PathBuf> {
                match std::env::var_os(var) {
                    Some(x) if !x.is_empty() => Some(PathBuf::from(x).join("openmw")),
                    _ => home.as_ref().map(|h| fallback.iter().fold(h.clone(), |p, s| p.join(s)).join("openmw")),
                }
            };
            t.userconfig = xdg("XDG_CONFIG_HOME", &[".config"]);
            t.userdata = xdg("XDG_DATA_HOME", &[".local", "share"]);
        }
        t
    }

    /// The folders for a cfg: the Flatpak OpenMW's when the cfg is inside its sandbox
    /// (`~/.var/app/org.openmw.OpenMW/config/openmw`, as OpenMW's documentation gives
    /// it), else [`Tokens::host`].
    pub fn for_cfg(cfg: &Path) -> Tokens {
        let mut t = Tokens::host();
        let mut base = PathBuf::new();
        let parts: Vec<_> = cfg.components().collect();
        for (i, c) in parts.iter().enumerate() {
            base.push(c);
            let app = parts.get(i + 1).map(|n| n.as_os_str() == "app").unwrap_or(false);
            let id = parts.get(i + 2).map(|n| n.as_os_str() == "org.openmw.OpenMW").unwrap_or(false);
            if c.as_os_str() == ".var" && app && id {
                let sandbox = base.join("app").join("org.openmw.OpenMW");
                t.userconfig = Some(sandbox.join("config").join("openmw"));
                t.userdata = Some(sandbox.join("data").join("openmw"));
                break;
            }
        }
        t
    }
}

/// What a setup's cfgs say it loads.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct OpenmwCfg {
    /// The cfgs read, lowest priority first (the one asked for, then those it chains).
    pub files: Vec<PathBuf>,
    /// The data folders that exist, lowest priority first; `data-local` last.
    pub data: Vec<PathBuf>,
    /// Data folders named that are not there (not in `data`).
    pub missing_data: Vec<PathBuf>,
    /// The `data-local` folder, when one is set (also last in `data` when it exists).
    pub data_local: Option<PathBuf>,
    /// Every `content=` name, in load order (scripts lists included).
    pub content: Vec<String>,
    /// The `groundcover=` plugins, in order; they load after every content file.
    pub groundcover: Vec<String>,
    /// The `fallback-archive=` names, in order.
    pub archives: Vec<String>,
    pub user_data: Option<PathBuf>,
    pub resources: Option<PathBuf>,
    /// Lines that could not be followed, in words.
    pub warnings: Vec<String>,
}

impl OpenmwCfg {
    /// The load order: the content plugins, then the groundcover ones (scripts lists
    /// left out); a plugin named twice (on both lists, or in two cases) counts once,
    /// where it first appears.
    pub fn plugins(&self) -> Vec<String> {
        let mut seen = HashSet::new();
        self.content
            .iter()
            .filter(|n| is_plugin(n))
            .chain(self.groundcover.iter())
            .filter(|n| seen.insert(n.to_ascii_lowercase()))
            .cloned()
            .collect()
    }

    /// Each name's file: in the highest-priority data folder that has it, the name
    /// compared without case as OpenMW does. Each folder is listed once.
    pub fn locate(&self, names: &[String]) -> Vec<(String, Option<PathBuf>)> {
        let listings: Vec<std::collections::HashMap<String, PathBuf>> = self.data.iter().map(|d| listing(d)).collect();
        names
            .iter()
            .map(|n| {
                let low = n.to_ascii_lowercase();
                let hit = listings.iter().rev().find_map(|l| l.get(&low).cloned());
                (n.clone(), hit)
            })
            .collect()
    }
}

/// A folder's files by lower-case name (empty when it cannot be read).
fn listing(dir: &Path) -> std::collections::HashMap<String, PathBuf> {
    let mut out = std::collections::HashMap::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            if e.file_type().map(|t| t.is_file()).unwrap_or(false) {
                out.insert(e.file_name().to_string_lossy().to_ascii_lowercase(), e.path());
            }
        }
    }
    out
}

/// A cfg's options, `(name, value)` in file order: comments and blank lines skipped, a
/// line without `=` skipped, name and value trimmed.
pub fn options(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|line| {
            let t = line.trim();
            if t.is_empty() || t.starts_with('#') {
                return None;
            }
            let (k, v) = t.split_once('=')?;
            Some((k.trim().to_string(), v.trim().to_string()))
        })
        .collect()
}

/// A path value as written: the quoted form unquoted (`&"` a quote, `&&` an ampersand,
/// anything after the closing quote dropped); an unquoted one as it is.
pub fn path_text(value: &str) -> String {
    let Some(rest) = value.strip_prefix('"') else { return value.to_string() };
    let mut out = String::new();
    let mut chars = rest.chars();
    while let Some(c) = chars.next() {
        match c {
            '&' => match chars.next() {
                Some(n) => out.push(n),
                None => out.push('&'),
            },
            '"' => break,
            _ => out.push(c),
        }
    }
    out
}

/// A path value as a path: unquoted, its token replaced, a relative one taken from `dir`
/// (the cfg's own folder).
///
/// # Errors
/// A token this machine has no folder for, in words.
pub fn resolve(value: &str, dir: &Path, tokens: &Tokens) -> Result<PathBuf, String> {
    let text = path_text(value);
    for (token, at) in [
        ("?userdata?", &tokens.userdata),
        ("?userconfig?", &tokens.userconfig),
        ("?local?", &tokens.local),
        ("?global?", &tokens.global),
    ] {
        if let Some(rest) = text.strip_prefix(token) {
            let base = at.as_ref().ok_or_else(|| format!("{token} is not known here: {text}"))?;
            let rest = rest.trim_start_matches(['/', '\\']);
            return Ok(if rest.is_empty() { base.clone() } else { base.join(rest) });
        }
    }
    let p = PathBuf::from(&text);
    if p.is_absolute() || text.starts_with('/') || text.starts_with('\\') {
        Ok(p)
    } else {
        Ok(dir.join(p))
    }
}

/// The options that gather values from every cfg.
const MANY: [&str; 4] = ["data", "content", "groundcover", "fallback-archive"];

/// One cfg's options, with its folder.
struct Source {
    dir: PathBuf,
    opts: Vec<(String, String)>,
}

/// Read a setup: `cfg` and every cfg it chains with `config=`.
///
/// # Errors
/// When `cfg` itself cannot be read. A chained folder without a readable `openmw.cfg`
/// is a warning.
pub fn read(cfg: &Path, tokens: &Tokens) -> std::io::Result<OpenmwCfg> {
    let first = std::fs::read_to_string(cfg)?;
    let mut warnings = Vec::new();
    let mut files = vec![cfg.to_path_buf()];
    let dir_of = |p: &Path| p.parent().map(Path::to_path_buf).unwrap_or_default();
    let mut sources = vec![Source { dir: dir_of(cfg), opts: options(&first) }];
    let mut seen: HashSet<PathBuf> = HashSet::from([norm(cfg)]);
    /* Depth first, as OpenMW's code reads them (a stack, each cfg's `config=` folders
       pushed in reverse): `dir1` naming `dir2` then `dir3`, and `dir2` naming `dir4`, is
       dir1, dir2, dir4, dir3 - not the breadth-first order its documentation's example
       gives. */
    let mut stack: Vec<(String, PathBuf)> = Vec::new();
    let push = |stack: &mut Vec<(String, PathBuf)>, src: &Source, warnings: &mut Vec<String>| {
        let mut found = Vec::new();
        for v in src.opts.iter().filter(|(k, _)| k == "config").map(|(_, v)| v) {
            match resolve(v, &src.dir, tokens) {
                Ok(f) => found.push((v.clone(), f)),
                Err(e) => warnings.push(format!("config={v}: {e}")),
            }
        }
        stack.extend(found.into_iter().rev());
    };
    push(&mut stack, &sources[0], &mut warnings);
    while let Some((v, folder)) = stack.pop() {
        let file = folder.join("openmw.cfg");
        if !seen.insert(norm(&file)) {
            continue;
        }
        match std::fs::read_to_string(&file) {
            Ok(text) => {
                let src = Source { dir: folder, opts: options(&text) };
                // `replace=config` in a chained cfg: every cfg read so far but the first
                // is dropped (OpenMW's own rule).
                if src.opts.iter().any(|(k, v)| k == "replace" && v == "config") && sources.len() > 1 {
                    sources.truncate(1);
                    files.truncate(1);
                }
                push(&mut stack, &src, &mut warnings);
                files.push(file);
                sources.push(src);
            }
            Err(_) if !folder.is_dir() => warnings.push(format!("config={v}: no folder {}", folder.display())),
            Err(_) => {} // a configuration folder need not hold an openmw.cfg
        }
    }
    let mut out = combine(&sources, tokens);
    out.files = files;
    warnings.append(&mut out.warnings);
    out.warnings = warnings;
    Ok(out)
}

/// One cfg's text on its own (no chained cfgs followed; a `config=` line is a warning):
/// what a caller that has only the text gets.
pub fn read_text(text: &str, dir: &Path, tokens: &Tokens) -> OpenmwCfg {
    let src = Source { dir: dir.to_path_buf(), opts: options(text) };
    let mut out = combine(std::slice::from_ref(&src), tokens);
    if src.opts.iter().any(|(k, _)| k == "config") {
        out.warnings.push("config= lines are not followed from text alone".to_string());
    }
    out
}

/// A path for comparing: the same file however it was spelled.
fn norm(p: &Path) -> PathBuf {
    std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}

/// The sources (lowest priority first) as one setup.
fn combine(sources: &[Source], tokens: &Tokens) -> OpenmwCfg {
    let mut out = OpenmwCfg::default();
    // For each many-valued option, the lowest source still counted: the highest that
    // replaces it.
    let floor = |key: &str| -> usize {
        sources.iter().rposition(|s| s.opts.iter().any(|(k, v)| k == "replace" && v == key)).unwrap_or(0)
    };
    let floors: Vec<(&str, usize)> = MANY.iter().map(|k| (*k, floor(k))).collect();
    let floor_of = |k: &str| floors.iter().find(|(n, _)| *n == k).map(|(_, f)| *f).unwrap_or(0);
    let mut data: Vec<PathBuf> = Vec::new();
    let single = |key: &str, out_warn: &mut Vec<String>| -> Option<PathBuf> {
        // An empty value (`user-data=`) sets nothing.
        let (src, v) = sources
            .iter()
            .rev()
            .find_map(|s| s.opts.iter().rev().find(|(k, v)| k == key && !v.is_empty()).map(|(_, v)| (s, v)))?;
        match resolve(v, &src.dir, tokens) {
            Ok(p) => Some(p),
            Err(e) => {
                out_warn.push(format!("{key}={v}: {e}"));
                None
            }
        }
    };
    let mut warn = Vec::new();
    for (i, s) in sources.iter().enumerate() {
        for (k, v) in &s.opts {
            if !MANY.contains(&k.as_str()) || i < floor_of(k) {
                continue;
            }
            match k.as_str() {
                "data" if !v.is_empty() => match resolve(v, &s.dir, tokens) {
                    Ok(p) => data.push(p),
                    Err(e) => warn.push(format!("data={v}: {e}")),
                },
                "content" if !v.is_empty() => out.content.push(v.clone()),
                "groundcover" if !v.is_empty() => out.groundcover.push(v.clone()),
                "fallback-archive" if !v.is_empty() => out.archives.push(v.clone()),
                _ => {}
            }
        }
    }
    out.data_local = single("data-local", &mut warn);
    out.user_data = single("user-data", &mut warn);
    out.resources = single("resources", &mut warn);
    if let Some(local) = &out.data_local {
        data.push(local.clone());
    }
    for d in data {
        if d.is_dir() {
            out.data.push(d);
        } else {
            out.missing_data.push(d);
        }
    }
    out.warnings = warn;
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options_are_trimmed_and_comments_skipped() {
        let o = options("# a comment\n  data = some/dir \n\ncontent=Morrowind.esm\nnot an option\n   # also a comment\n");
        assert_eq!(o, [("data".to_string(), "some/dir".to_string()), ("content".to_string(), "Morrowind.esm".to_string())]);
    }

    #[test]
    fn quoted_paths_unescape_and_drop_the_tail() {
        assert_eq!(path_text(r#""a/path/with a &" symbol && an ampersand""#), r#"a/path/with a " symbol & an ampersand"#);
        assert_eq!(path_text(r#""C:\Games\Data Files" trailing"#), r"C:\Games\Data Files");
        assert_eq!(path_text("plain/ & path"), "plain/ & path");
    }

    #[test]
    fn tokens_resolve_or_say_why_not() {
        let t = Tokens { userdata: Some(PathBuf::from("/u")), ..Tokens::default() };
        assert_eq!(resolve("?userdata?data", Path::new("/cfg"), &t).unwrap(), PathBuf::from("/u/data"));
        assert_eq!(resolve("\"?userdata?\"", Path::new("/cfg"), &t).unwrap(), PathBuf::from("/u"));
        assert!(resolve("?local?x", Path::new("/cfg"), &t).unwrap_err().contains("?local?"));
        assert_eq!(resolve("rel/x", Path::new("/cfg"), &t).unwrap(), PathBuf::from("/cfg/rel/x"));
    }

    #[test]
    fn the_flatpak_sandbox_has_its_own_folders() {
        let t = Tokens::for_cfg(Path::new("/home/me/.var/app/org.openmw.OpenMW/config/openmw/openmw.cfg"));
        assert_eq!(t.userdata, Some(PathBuf::from("/home/me/.var/app/org.openmw.OpenMW/data/openmw")));
        assert_eq!(t.userconfig, Some(PathBuf::from("/home/me/.var/app/org.openmw.OpenMW/config/openmw")));
    }
}
