//! OpenMW's configuration as OpenMW reads it: not one `openmw.cfg` but a chain of them.
//!
//! Round 18gn (backend-plan part 2, item 15). OpenMW starts from the file beside its
//! executable (the "local" config) or, failing that, the system one (`/etc/openmw/openmw.cfg`
//! on Linux; Windows has no global config at all), follows every `config=` line in it to a
//! further folder holding an `openmw.cfg`, and so on, then merges what it read: list options
//! (`data=`, `content=`, `groundcover=`, `fallback-archive=`) are concatenated in the order the
//! files were read, a later file's `replace=<option>` discards what earlier files said for
//! that option, `fallback=` keys are a map in which the later file wins, single values take
//! the later file's (`encoding=` is the one read, round 18gq: the code page the plugins are
//! in), `data-local=` is appended after every `data=` regardless of where it
//! stood, and `?userconfig?`, `?userdata?`, `?global?` and `?local?` at the front of a path
//! stand for folders OpenMW knows. The shipped install config carries the `fallback=` lines
//! and `config="?userconfig?"`; the launcher writes `data=` and `content=` into the user
//! file. Reading either file alone therefore misses half the picture, which is what this
//! module ends.
//!
//! What is read is what OpenMW does in `components/files/configurationmanager.cpp`
//! (`readConfiguration`, `mergeComposingVariables`, `processPath`) and `apps/openmw/main.cpp`
//! (`data-local` appended last), written from reading them, not copied. The one liberty is
//! the starting point: OpenMW always starts from its executable's folder, while a person
//! points Gardenfell at whichever `openmw.cfg` they know, usually the user one. When that
//! file does not itself name further configs, the install config is looked for where
//! OpenMW keeps it, and when it is found and its chain leads to the file given, the chain
//! is read from the top; when it is not found, the file given is read alone, which is what
//! every earlier round did.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The merged configuration, with the files it came from.
#[derive(Debug, Clone, Default)]
pub struct Chain {
    /// The `openmw.cfg` files read, in the order OpenMW reads them.
    pub files: Vec<PathBuf>,
    /// `data=` in order of priority (lowest first, as OpenMW lists them), with `data-local=`
    /// appended last. Tokens resolved and relative paths made absolute; existence not checked.
    pub data: Vec<PathBuf>,
    /// `content=` in load order.
    pub content: Vec<String>,
    /// `groundcover=`: the plugins OpenMW loads as grass, which `content=` never names.
    pub groundcover: Vec<String>,
    /// `fallback-archive=` in order.
    pub archives: Vec<String>,
    /// `fallback=Key,Value` with the later file winning on a repeated key, in first-seen order.
    pub fallback: Vec<(String, String)>,
    /// Options a later file's `replace=` discarded from earlier files, for the log.
    pub replaced: Vec<String>,
    /// `encoding=`, the code page OpenMW reads the plugins in (`win1252`, `win1251`,
    /// `win1250`), the last file to say it winning. `None` when no file says.
    pub encoding: Option<String>,
}

/// One parsed file: the lines that matter, in order, before any merging.
#[derive(Debug, Default)]
struct File {
    dir: PathBuf,
    config: Vec<String>,
    replace: Vec<String>,
    data: Vec<String>,
    data_local: Option<String>,
    content: Vec<String>,
    groundcover: Vec<String>,
    archives: Vec<String>,
    fallback: Vec<(String, String)>,
    encoding: Option<String>,
}

impl File {
    fn parse(path: &Path) -> Option<File> {
        // Round 18gq: a cfg is UTF-8 by OpenMW's own rule, but a hand-edited one on a Russian
        // machine may well not be, and refusing the whole chain over a comment is no answer.
        let text = crate::codepage::read_text(path).ok()?;
        let mut f =
            File { dir: path.parent().map(Path::to_path_buf).unwrap_or_default(), ..Default::default() };
        for line in text.lines() {
            let t = line.trim();
            if t.is_empty() || t.starts_with('#') {
                continue;
            }
            let Some((key, value)) = t.split_once('=') else { continue };
            let (key, value) = (key.trim(), value.trim());
            match key {
                "config" => f.config.push(unquote(value)),
                "replace" => f.replace.push(value.to_string()),
                "data" => f.data.push(unquote(value)),
                "data-local" => f.data_local = Some(unquote(value)),
                "content" => f.content.push(value.to_string()),
                "groundcover" => f.groundcover.push(value.to_string()),
                "fallback-archive" => f.archives.push(value.to_string()),
                "encoding" => f.encoding = Some(value.to_string()),
                "fallback" => {
                    if let Some((k, v)) = value.split_once(',') {
                        f.fallback.push((k.trim().to_string(), v.trim().to_string()));
                    }
                }
                _ => {}
            }
        }
        Some(f)
    }

    fn replaces(&self, option: &str) -> bool {
        self.replace.iter().any(|r| r == option)
    }
}

/// A `MaybeQuotedPath`: the quotes are optional, and inside them OpenMW escapes `"` and `&`
/// with `&`.
/// What follows the closing quote is ignored, as OpenMW does (it uses the quoted part and
/// logs the rest: `operator>>` for `MaybeQuotedPath`, `std::quoted(.., '"', '&')`).
fn unquote(v: &str) -> String {
    let v = v.trim();
    let Some(rest) = v.strip_prefix('"') else { return v.to_string() };
    let mut out = String::new();
    let mut chars = rest.chars();
    while let Some(c) = chars.next() {
        match c {
            '&' => out.push(chars.next().unwrap_or('&')),
            '"' => break,
            _ => out.push(c),
        }
    }
    out
}

/// The folders OpenMW's path tokens stand for on this machine.
#[derive(Debug, Clone, Default)]
pub struct Places {
    /// `?userconfig?`: where the launcher writes the user's `openmw.cfg`.
    pub user_config: Option<PathBuf>,
    /// `?userdata?`: saves, screenshots and the `data-local` folder.
    pub user_data: Option<PathBuf>,
    /// `?global?`: the system data folder on Linux; nothing on Windows.
    pub global_data: Option<PathBuf>,
    /// The system config folder on Linux (`/etc/openmw`); nothing on Windows.
    pub global_config: Option<PathBuf>,
}

impl Places {
    /// Where OpenMW looks on the machine this runs on. Nothing here has to exist.
    pub fn here() -> Places {
        let mut p = Places::default();
        #[cfg(windows)]
        {
            if let Some(home) = std::env::var_os("USERPROFILE") {
                let docs = PathBuf::from(home).join("Documents").join("My Games").join("OpenMW");
                p.user_config = Some(docs.clone());
                p.user_data = Some(docs);
            }
        }
        #[cfg(target_os = "macos")]
        {
            if let Some(home) = std::env::var_os("HOME") {
                let home = PathBuf::from(home);
                p.user_config = Some(home.join("Library").join("Preferences").join("openmw"));
                p.user_data = Some(home.join("Library").join("Application Support").join("openmw"));
            }
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            let home = std::env::var_os("HOME").map(PathBuf::from);
            p.user_config = match std::env::var_os("XDG_CONFIG_HOME") {
                Some(x) => Some(PathBuf::from(x).join("openmw")),
                None => home.as_ref().map(|h| h.join(".config").join("openmw")),
            };
            p.user_data = match std::env::var_os("XDG_DATA_HOME") {
                Some(x) => Some(PathBuf::from(x).join("openmw")),
                None => home.as_ref().map(|h| h.join(".local").join("share").join("openmw")),
            };
            p.global_config = Some(PathBuf::from("/etc/openmw"));
            // GLOBAL_DATA_PATH is fixed when OpenMW is built; these are where the packages put it.
            p.global_data = ["/usr/share/games/openmw", "/usr/share/openmw", "/usr/local/share/games/openmw"]
                .iter()
                .map(PathBuf::from)
                .find(|d| d.is_dir());
        }
        p
    }
}

/// Reads the chain that leads to or from `start`, an `openmw.cfg`, with the machine's own
/// folders for the tokens.
pub fn read(start: &Path) -> Chain {
    read_with(start, &Places::here())
}

/// [`read`] with the token folders given, so a test can hold them still.
pub fn read_with(start: &Path, places: &Places) -> Chain {
    let Some(first) = File::parse(start) else {
        return Chain { files: vec![start.to_path_buf()], ..Default::default() };
    };
    // A file that names further configs is a root in its own right. One that does not is
    // read as OpenMW would read it, after the install's own, when that can be found and its
    // chain does lead here; a file the install's chain never reaches (a profile kept
    // elsewhere, a test's) is read alone, because mixing a stranger's config into it would
    // be a surprise.
    if first.config.is_empty() {
        if let Some((root_path, root)) =
            install_config(start, places).and_then(|p| File::parse(&p).map(|f| (p, f)))
        {
            let (files, seen) = walk(root_path, root, places);
            if seen.contains(&canon(&first.dir)) {
                return merge(files, places);
            }
        }
    }
    let (files, _) = walk(start.to_path_buf(), first, places);
    merge(files, places)
}

/// The walk from a root: a stack of folders still to read, the first-listed on top, each
/// read once. Returns the files in reading order and the folders visited.
fn walk(root_path: PathBuf, root: File, places: &Places) -> (Vec<(PathBuf, File)>, Vec<PathBuf>) {
    // `?local?` is the folder of the executable, which is the folder of its config.
    let local = root.dir.clone();
    let mut files: Vec<(PathBuf, File)> = Vec::new();
    let mut seen: Vec<PathBuf> = vec![canon(&root.dir)];
    let mut todo: Vec<PathBuf> = Vec::new();
    for c in root.config.iter().rev() {
        todo.push(resolve_path(c, &root.dir, &local, places));
    }
    files.push((root_path, root));
    while let Some(dir) = todo.pop() {
        let key = canon(&dir);
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        let path = dir.join("openmw.cfg");
        let Some(f) = File::parse(&path) else { continue };
        if f.replaces("config") && files.len() > 1 {
            files.truncate(1);
        }
        for c in f.config.iter().rev() {
            todo.push(resolve_path(c, &f.dir, &local, places));
        }
        files.push((path, f));
    }
    (files, seen)
}

/// The merge, as `mergeComposingVariables` does it: lists concatenate in reading order, a
/// later file's `replace=` discards what came before it for that option, maps and single
/// values take the later file's.
fn merge(files: Vec<(PathBuf, File)>, places: &Places) -> Chain {
    let local = files.first().map(|(_, f)| f.dir.clone()).unwrap_or_default();
    let resolve = |raw: &str, base: &Path| resolve_path(raw, base, &local, places);
    let mut out = Chain::default();
    let mut data: Vec<PathBuf> = Vec::new();
    let mut data_local: Option<PathBuf> = None;
    let mut fallback: BTreeMap<String, String> = BTreeMap::new();
    let mut order: Vec<String> = Vec::new();
    for (path, f) in files {
        for opt in &f.replace {
            let dropped = match opt.as_str() {
                "data" => std::mem::take(&mut data).len(),
                "content" => std::mem::take(&mut out.content).len(),
                "groundcover" => std::mem::take(&mut out.groundcover).len(),
                "fallback-archive" => std::mem::take(&mut out.archives).len(),
                "fallback" => {
                    let n = fallback.len();
                    fallback.clear();
                    order.clear();
                    n
                }
                _ => 0,
            };
            if dropped > 0 {
                out.replaced.push(opt.clone());
            }
        }
        data.extend(f.data.iter().map(|d| resolve(d, &f.dir)));
        if let Some(l) = &f.data_local {
            data_local = Some(resolve(l, &f.dir));
        }
        out.content.extend(f.content.iter().cloned());
        out.groundcover.extend(f.groundcover.iter().cloned());
        out.archives.extend(f.archives.iter().cloned());
        for (k, v) in &f.fallback {
            if fallback.insert(k.clone(), v.clone()).is_none() {
                order.push(k.clone());
            }
        }
        if let Some(e) = &f.encoding {
            out.encoding = Some(e.clone());
        }
        out.files.push(path);
    }
    if let Some(l) = data_local {
        data.push(l);
    }
    out.data = data;
    out.fallback = order.into_iter().filter_map(|k| fallback.remove(&k).map(|v| (k, v))).collect();
    out
}

/// A path as it stands in a config line: a token at the front names one of OpenMW's
/// folders, anything else relative is relative to the file it was read from.
fn resolve_path(raw: &str, base: &Path, local: &Path, places: &Places) -> PathBuf {
    if let Some(rest) = raw.strip_prefix('?') {
        if let Some((token, tail)) = rest.split_once('?') {
            let folder = match token {
                "local" => Some(local.to_path_buf()),
                "userconfig" => places.user_config.clone(),
                "userdata" => places.user_data.clone(),
                "global" => places.global_data.clone(),
                _ => None,
            };
            return match folder {
                Some(f) if tail.is_empty() => f,
                Some(f) => f.join(tail.trim_start_matches(['/', '\\'])),
                // OpenMW clears a path with an unknown or empty token; it then fails the
                // existence check, which is the same outcome.
                None => PathBuf::new(),
            };
        }
    }
    let p = PathBuf::from(raw);
    if p.is_absolute() {
        p
    } else {
        base.join(p)
    }
}

/// The install's own `openmw.cfg`, when it can be found from here: the system one on
/// Linux, or the one beside an OpenMW installed where Windows installers put programs.
/// `start` itself is never the answer.
fn install_config(start: &Path, places: &Places) -> Option<PathBuf> {
    let start_key = canon(start);
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(g) = &places.global_config {
        candidates.push(g.join("openmw.cfg"));
    }
    #[cfg(windows)]
    {
        for var in ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"] {
            if let Some(pf) = std::env::var_os(var) {
                if let Ok(rd) = std::fs::read_dir(&pf) {
                    for e in rd.flatten() {
                        if e.file_name().to_string_lossy().to_ascii_lowercase().starts_with("openmw") {
                            candidates.push(e.path().join("openmw.cfg"));
                        }
                    }
                }
            }
        }
    }
    candidates.into_iter().find(|c| c.is_file() && canon(c) != start_key)
}

fn canon(p: &Path) -> PathBuf {
    std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}

impl Chain {
    /// The merged configuration as one `openmw.cfg`-shaped text, for the readers that take
    /// the order file's text: `content=` and `groundcover=` in load order, `data=` lowest
    /// priority first with `data-local=` already in its place, `fallback-archive=` in order,
    /// and one `fallback=` line per key with the winning value.
    pub fn text(&self) -> String {
        let mut s = String::new();
        s.push_str("# openmw.cfg as OpenMW reads it, merged from:\n");
        for f in &self.files {
            s.push_str(&format!("#   {}\n", f.display()));
        }
        for d in &self.data {
            s.push_str(&format!(
                "data=\"{}\"\n",
                d.display().to_string().replace('&', "&&").replace('"', "&\"")
            ));
        }
        for c in &self.content {
            s.push_str(&format!("content={c}\n"));
        }
        for g in &self.groundcover {
            s.push_str(&format!("groundcover={g}\n"));
        }
        for a in &self.archives {
            s.push_str(&format!("fallback-archive={a}\n"));
        }
        for (k, v) in &self.fallback {
            s.push_str(&format!("fallback={k},{v}\n"));
        }
        if let Some(e) = &self.encoding {
            s.push_str(&format!("encoding={e}\n"));
        }
        s
    }

    /// The code page `encoding=` names, when a file names one the engine knows.
    pub fn code_page(&self) -> Option<crate::codepage::CodePage> {
        self.encoding.as_deref().and_then(crate::codepage::CodePage::from_name)
    }

    /// The folder OpenMW treats as the user's config folder: the last config read, where
    /// `settings.cfg` and the launcher's files live.
    pub fn user_config_dir(&self) -> Option<PathBuf> {
        self.files.last().and_then(|f| f.parent()).map(Path::to_path_buf)
    }
}

/// The text of an order file as the engine should read it: an `openmw.cfg` becomes its
/// whole chain merged, anything else (a `Morrowind.ini`) is the file as it is.
pub fn order_text(order_file: &Path) -> String {
    if crate::paths::is_openmw_cfg(order_file) {
        read(order_file).text()
    } else {
        crate::codepage::read_text(order_file).unwrap_or_default()
    }
}
