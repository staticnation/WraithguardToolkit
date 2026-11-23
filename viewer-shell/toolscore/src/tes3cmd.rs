//! tes3cmd for the Tools page: built in (`staticnation/tes3cmd-rs`, run inside the app by
//! [`Engine::Builtin`], Static 2026-11-17), or the external tes3cmd launched as before
//! ([`Engine::External`]) - chosen by the person, and taken for the Perl-code commands the
//! port leaves to Perl (`needs_perl`). Ported from `wraithguard/gui/t3.py` and
//! `wraithguard_toolkit.find_tes3cmd`, `tes3cmd_invocation`, `stage_for_tes3cmd`,
//! `sync_plugin_master_sizes`; the plan is `viewer-shell/TES3CMD-REWRITE.md`.
//!
//! tes3cmd assumes one flat `Data Files` beside a `Morrowind.ini`; an OpenMW setup spreads its
//! mods over many folders, so run against a mod folder tes3cmd cannot see the masters (clean
//! silently degrades, `header --synchronize` writes empty sizes). So `clean` and a custom
//! command run on a private copy staged with the plugin's masters, and the result is copied
//! back over the original (a one-time backup kept) only when it changed; `sync` (master sizes)
//! is done here, resolving each master across every data folder.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

/// The vanilla masters `clean` never touches: even a careful clean rewrites bytes other
/// content depends on.
pub const NEVER_CLEAN: [&str; 3] = ["morrowind.esm", "tribunal.esm", "bloodmoon.esm"];

/// A plugin's file by name (`PluginFileIndex.find`).
pub type Find<'a> = dyn Fn(&str) -> Option<PathBuf> + 'a;

/// Whether a path the page names is a tes3cmd to run: its file name starts with `tes3cmd` (security
/// audit 2026-11-20, F5), so a command from the page cannot make this run any program at all. A
/// tes3cmd renamed past that is chosen by `$MLOX_TES3CMD`, which only the user's own environment sets.
pub fn looks_like_tes3cmd(path: &str) -> bool {
    // Either separator, on any system: a Windows path is one string on Linux.
    path.rsplit(['/', '\\']).next().is_some_and(|n| n.to_ascii_lowercase().starts_with("tes3cmd"))
}

/// tes3cmd's file (`find_tes3cmd`): `explicit`, `$MLOX_TES3CMD`, `PATH`, then `dirs`; the
/// compiled build first.
pub fn find(explicit: Option<&str>, dirs: &[PathBuf]) -> Option<PathBuf> {
    let names = ["tes3cmd.exe", "tes3cmd.bat", "tes3cmd"];
    let mut cands: Vec<PathBuf> = Vec::new();
    if let Some(e) = explicit.filter(|e| !e.is_empty()) {
        cands.push(PathBuf::from(e));
    }
    if let Some(e) = std::env::var_os("MLOX_TES3CMD").filter(|e| !e.is_empty()) {
        cands.push(PathBuf::from(e));
    }
    if let Some(path) = std::env::var_os("PATH") {
        for n in names {
            if let Some(hit) = std::env::split_paths(&path).map(|d| d.join(n)).find(|p| p.is_file()) {
                cands.push(hit);
            }
        }
    }
    for d in dirs {
        cands.extend(names.iter().map(|n| d.join(n)));
    }
    cands.into_iter().find(|c| c.is_file())
}

fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let exts: Vec<String> = if cfg!(windows) { vec![".exe".into(), String::new()] } else { vec![String::new()] };
    std::env::split_paths(&path).flat_map(|d| exts.iter().map(move |e| d.join(format!("{name}{e}")))).find(|p| p.is_file())
}

/// How to run the tes3cmd at `path` (`tes3cmd_invocation`): directly, or the Perl script
/// through a `perl` on `PATH`; why not, when it cannot be.
pub fn invocation(path: &Path) -> Result<Vec<String>, String> {
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    let p = path.to_string_lossy().into_owned();
    if matches!(ext.as_str(), "exe" | "bat" | "cmd") {
        return Ok(vec![p]);
    }
    let mut head = vec![0u8; 256];
    let n = {
        use std::io::Read as _;
        let mut f = std::fs::File::open(path).map_err(|e| format!("can't read '{p}': {e}"))?;
        f.read(&mut head).map_err(|e| format!("can't read '{p}': {e}"))?
    };
    head.truncate(n);
    if head.starts_with(b"#!") || head.to_ascii_lowercase().windows(4).any(|w| w == b"perl") {
        let Some(perl) = which("perl") else {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            return Err(format!(
                "'{name}' is the pure-perl tes3cmd but no perl interpreter was found on PATH. Point this at the compiled tes3cmd.exe from the MOMW Tools Pack instead (or install perl)."
            ));
        };
        return Ok(vec![perl.to_string_lossy().into_owned(), p]);
    }
    Ok(vec![p])
}

/// `plugins` staged for tes3cmd under `root` (`stage_for_tes3cmd`): a `Data Files/` with a
/// private copy of each, and a `Morrowind.ini` naming them with their masters in load order
/// (`order`). The masters are copied there too (reused while size and time match) when
/// `copy_masters`, for an external tes3cmd; the built-in one reads them where they are
/// (`Staged::masters`, `tes3cmd_rs::cli::run_over`), and its `Data Files` starts empty each
/// time, so no copy left from an earlier run stands in for a master.
///
/// Wraithguard: copies, never hard links as `stage_for_tes3cmd` made them when the volume
/// allowed: a tes3cmd command that writes the masters it finds (`fixit`'s header sync and date
/// reset, `multipatch`, `resetdates`, a `modify` matching a master) wrote the person's own
/// Morrowind.esm through the link (Static 2026-11-17, found comparing the built-in tes3cmd
/// with the Perl). `run` drops them after a custom command; over the built-in one's overlay a
/// write lands on a private copy.
pub fn stage_set(root: &Path, plugins: &[PathBuf], find: &Find<'_>, copy_masters: bool, order: &[String]) -> std::io::Result<Staged> {
    let df = root.join("Data Files");
    if !copy_masters {
        let _ = std::fs::remove_dir_all(&df);
    }
    std::fs::create_dir_all(&df)?;
    let file_name = |p: &Path| p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let ours: std::collections::HashSet<String> = plugins.iter().map(|p| file_name(p).to_lowercase()).collect();
    let mut s = Staged::default();
    let mut names: Vec<String> = Vec::new();
    for plugin in plugins {
        for m in ordercore::inputs::read_masters(plugin) {
            if ours.contains(&m.to_lowercase()) || s.masters.contains_key(&m.to_lowercase()) || s.missing.contains(&m) {
                continue;
            }
            let Some(src) = find(&m) else {
                s.missing.push(m);
                continue;
            };
            let name = file_name(&src);
            if copy_masters {
                let dest = df.join(&name);
                let fresh = || -> std::io::Result<()> {
                    if let (Ok(a), Ok(d)) = (std::fs::metadata(&src), std::fs::metadata(&dest)) {
                        let secs = |m: &std::fs::Metadata| m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs());
                        if a.len() == d.len() && secs(&a) == secs(&d) {
                            return Ok(());
                        }
                        std::fs::remove_file(&dest)?;
                    }
                    copy_with_time(&src, &dest)
                };
                if fresh().is_err() {
                    s.missing.push(m);
                    continue;
                }
            }
            s.masters.insert(name.to_lowercase(), src);
            names.push(name);
        }
    }
    for plugin in plugins {
        let name = file_name(plugin);
        let copy = df.join(&name);
        if copy.exists() {
            std::fs::remove_file(&copy)?;
        }
        copy_with_time(plugin, &copy)?;
        s.copies.push((plugin.clone(), copy));
        names.push(name);
    }
    let at: std::collections::HashMap<String, usize> = order.iter().enumerate().map(|(i, n)| (n.to_lowercase(), i)).collect();
    // Load order: the setup's where it has them, else masters before plugins, as found.
    let mut ranked: Vec<(usize, bool, usize, String)> = names
        .into_iter()
        .enumerate()
        .map(|(i, n)| {
            let l = n.to_lowercase();
            (at.get(&l).copied().unwrap_or(usize::MAX), !(l.ends_with(".esm") || l.ends_with(".omwgame")), i, n)
        })
        .collect();
    ranked.sort();
    let ini: String = ranked.iter().enumerate().map(|(i, r)| format!("GameFile{i}={}\n", r.3)).collect();
    let latin1: Vec<u8> = format!("[Game Files]\n{ini}").chars().map(|c| if (c as u32) < 256 { c as u8 } else { b'?' }).collect();
    std::fs::write(root.join("Morrowind.ini"), latin1)?;
    s.data_files = df;
    Ok(s)
}

/// `plugin` staged with its masters copied (`stage_set` for one, as an external tes3cmd needs
/// it): the staged plugin and the masters no data folder has (a non-empty list: do not run).
pub fn stage(root: &Path, plugin: &Path, find: &Find<'_>) -> std::io::Result<(PathBuf, Vec<String>)> {
    let s = stage_set(root, &[plugin.to_path_buf()], find, true, &[])?;
    Ok((s.copies[0].1.clone(), s.missing))
}

/// What `stage_set` made: each plugin with its staged copy, the masters by lower-case name
/// where they are, the ones not found, and the staged `Data Files`.
#[derive(Default)]
pub struct Staged {
    pub copies: Vec<(PathBuf, PathBuf)>,
    pub masters: std::collections::HashMap<String, PathBuf>,
    pub missing: Vec<String>,
    pub data_files: PathBuf,
}

/// `shutil.copy2`: the bytes and the modification time.
fn copy_with_time(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::copy(from, to)?;
    let t = std::fs::metadata(from)?.modified()?;
    std::fs::File::options().write(true).open(to)?.set_modified(t)
}

/// One plugin's master sizes made to match the installed masters (`sync_plugin_master_sizes`,
/// the VFS-aware `header --synchronize`): only the 8-byte size fields change, a one-time
/// `.masterfix.bak` kept. Answers the fields rewritten `(master, old, new)` and the masters
/// no data folder has (left as they are).
pub fn sync_master_sizes(path: &Path, find: &Find<'_>, backup: bool) -> Result<(Vec<(String, u64, u64)>, Vec<String>), String> {
    let mut raw = std::fs::read(path).map_err(|e| format!("can't read: {e}"))?;
    if raw.len() < 16 || &raw[..4] != b"TES3" {
        return Err("not a TES3 plugin (no TES3 header)".into());
    }
    let size = u32::from_le_bytes(raw[4..8].try_into().expect("4 bytes")) as usize;
    let end = (16 + size).min(raw.len());
    let (mut updated, mut unresolved) = (Vec::new(), Vec::new());
    let mut pending: Option<String> = None;
    let mut i = 16;
    while i + 8 <= end {
        let tag: [u8; 4] = raw[i..i + 4].try_into().expect("4 bytes");
        let sz = u32::from_le_bytes(raw[i + 4..i + 8].try_into().expect("4 bytes")) as usize;
        let off = i + 8;
        if off + sz > end {
            break;
        }
        if &tag == b"MAST" {
            let b = &raw[off..off + sz];
            let b = b.split(|&c| c == 0).next().unwrap_or(&[]);
            pending = Some(b.iter().map(|&c| c as char).collect::<String>().trim().to_string()).filter(|s| !s.is_empty());
        } else if &tag == b"DATA"
            && let Some(m) = pending.take()
            && sz >= 8
        {
            match find(&m) {
                None => unresolved.push(m),
                Some(mp) => {
                    if let Ok(actual) = std::fs::metadata(&mp).map(|md| md.len()) {
                        let old = u64::from_le_bytes(raw[off..off + 8].try_into().expect("8 bytes"));
                        if old != actual {
                            raw[off..off + 8].copy_from_slice(&actual.to_le_bytes());
                            updated.push((m, old, actual));
                        }
                    }
                }
            }
        }
        i = off + sz;
    }
    if !updated.is_empty() {
        if backup {
            let mut name = path.file_name().map(|n| n.to_os_string()).unwrap_or_default();
            name.push(".masterfix.bak");
            let bak = path.with_file_name(name);
            if !bak.exists() {
                copy_with_time(path, &bak).map_err(|e| format!("couldn't write backup ({e}) -- plugin NOT modified"))?;
            }
        }
        std::fs::write(path, &raw).map_err(|e| format!("couldn't write plugin: {e}"))?;
    }
    Ok((updated, unresolved))
}

/// Which tes3cmd runs: the one built in, or an external one (its invocation, `invocation`).
#[derive(Clone, Debug)]
pub enum Engine {
    Builtin,
    External(Vec<String>),
}

impl Engine {
    /// The log's `Engine:` line.
    pub fn describe(&self) -> String {
        match self {
            Engine::Builtin => format!("built in (tes3cmd-rs, as tes3cmd {})", tes3cmd_rs::TES3CMD_VERSION),
            Engine::External(argv) => argv.join(" "),
        }
    }
}

/// Whether a command runs Perl code, which the built-in tes3cmd leaves to the Perl: `modify`
/// with `--run` or `--program-file`, or a `--replace` whose flags have `e` (`/a/b/e`).
pub fn needs_perl(tokens: &[String]) -> bool {
    let low: Vec<String> = tokens.iter().map(|t| t.to_lowercase()).collect();
    if !low.iter().any(|t| t == "modify") {
        return false;
    }
    let code = |t: &str| matches!(t.trim_start_matches('-'), "run" | "program-file") && t.starts_with('-');
    let eval = |t: &str| {
        let Some(end) = t.strip_prefix('/') else { return false };
        // The flags after the substitution's last `/`: `e` among them evaluates Perl.
        end.rsplit_once('/').is_some_and(|(_, flags)| !flags.is_empty() && flags.chars().all(|c| c.is_ascii_alphabetic()) && flags.contains('e'))
    };
    low.iter().any(|t| code(t)) || tokens.iter().any(|t| eval(t))
}

/// One built-in run in `cwd`, as the external one answers: the exit code, and standard output
/// with standard error after it. A panic in the port is a failed run, not a lost app.
///
/// `over`: the masters where they are, read by name over tes3cmd-rs's overlay (`run_over`:
/// never copied, never written); none for a run in a plugin's own folder (`header`).
fn run_builtin(args: &[String], cwd: &Path, over: Option<&std::collections::HashMap<String, PathBuf>>) -> std::io::Result<(i32, String)> {
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let code = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match over {
        Some(files) => tes3cmd_rs::cli::run_over(cwd, files.clone(), args, &mut out, &mut err),
        None => tes3cmd_rs::cli::run_in(cwd, args, &mut out, &mut err),
    }))
        .map_err(|_| std::io::Error::other("the built-in tes3cmd failed (a bug in tes3cmd-rs: please report it with the plugin)"))?;
    let mut text = String::from_utf8_lossy(&out).into_owned();
    let err = String::from_utf8_lossy(&err);
    if !err.is_empty() {
        text.push('\n');
        text.push_str(&err);
    }
    Ok((code, text.trim().to_string()))
}

/// One tes3cmd run, its output joined: built in, or the external one launched.
fn run_one(engine: &Engine, args: &[String], cwd: &Path, over: Option<&std::collections::HashMap<String, PathBuf>>) -> std::io::Result<(i32, String)> {
    match engine {
        Engine::Builtin => run_builtin(args, cwd, over),
        Engine::External(argv) => run_external(argv, args, cwd),
    }
}

/// One external tes3cmd run, its output joined.
fn run_external(argv: &[String], args: &[String], cwd: &Path) -> std::io::Result<(i32, String)> {
    let mut c = Command::new(&argv[0]);
    c.args(&argv[1..]).args(args).current_dir(cwd).env("PWD", cwd);
    c.stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let child = c.spawn()?;
    // Fifteen minutes, as the Python gave it; the output is read on its own threads so a full
    // pipe never stalls the run.
    let start = Instant::now();
    let id = child.id();
    let out = std::thread::spawn(move || child.wait_with_output());
    while !out.is_finished() {
        if start.elapsed() > Duration::from_secs(900) {
            return Err(std::io::Error::new(std::io::ErrorKind::TimedOut, format!("tes3cmd (process {id}) ran past 15 minutes")));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let o = out.join().map_err(|_| std::io::Error::other("tes3cmd's runner stopped"))??;
    let mut text = String::from_utf8_lossy(&o.stdout).into_owned();
    let err = String::from_utf8_lossy(&o.stderr);
    if !err.is_empty() {
        text.push('\n');
        text.push_str(&err);
    }
    Ok((o.status.code().unwrap_or(-1), text.trim().to_string()))
}

/// What a run did, and its log.
#[derive(Clone, Debug, Default)]
pub struct Run {
    pub lines: Vec<String>,
    pub ok: usize,
    pub failed: usize,
    pub skipped: usize,
    pub changed: usize,
    /// New files a run made (multipatch.esp), put beside the plugins.
    pub made: usize,
    pub status: String,
    /// This run's name, and the folder its plugins' "before" copies are kept in (`keep`).
    pub id: String,
    /// Each plugin the run changed: its record by record changes (`recdiff`), and where the
    /// plugin as it was is kept, for undo.
    pub files: Vec<Changed>,
}

/// A plugin a run changed: the file, its copy from before the run, and what changed.
#[derive(Clone, Debug)]
pub struct Changed {
    pub path: PathBuf,
    pub before: PathBuf,
    pub changes: Vec<crate::recdiff::Change>,
}

/// The runs whose "before" copies are kept (the newest; older ones are deleted).
pub const KEEP_RUNS: usize = 10;

/// The folder a run keeps its plugins' "before" copies in: `staging/results/<id>`. The oldest
/// runs past `KEEP_RUNS` are deleted when a new one starts.
fn keep_dir(staging: &Path) -> (String, PathBuf) {
    let root = staging.join("results");
    let id = format!("{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0));
    if let Ok(rd) = std::fs::read_dir(&root) {
        let mut old: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
        old.sort();
        let drop = old.len().saturating_sub(KEEP_RUNS - 1);
        for d in old.into_iter().take(drop) {
            let _ = std::fs::remove_dir_all(d);
        }
    }
    let dir = root.join(&id);
    (id, dir)
}

/// `orig` changed from `before` to `after` by this run: its "before" copy kept under `keep`,
/// and its changes listed.
fn record_change(r: &mut Run, keep: &Path, orig: &Path, before: &[u8], after: &[u8]) {
    let n = r.files.len();
    let name = orig.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let copy = keep.join(format!("{n}_{name}"));
    if std::fs::create_dir_all(keep).and_then(|()| std::fs::write(&copy, before)).is_ok() {
        r.files.push(Changed { path: orig.to_path_buf(), before: copy, changes: crate::recdiff::diff(before, after) });
    }
}

/// The commands that act on the whole active load order, run once over the plugins chosen
/// (staged as that load order) rather than once per plugin.
pub const WHOLE_ORDER: [&str; 3] = ["fixit", "multipatch", "resetdates"];
/// The commands that compare two plugins, run once on the pair.
pub const PAIR: [&str; 2] = ["common", "diff"];

/// The folder a new file a run made goes to (multipatch.esp): the one holding most of the
/// plugins chosen, the later one in the list on a tie.
fn home_folder(files: &[PathBuf]) -> Option<PathBuf> {
    let mut count: Vec<(PathBuf, usize, usize)> = Vec::new();
    for (i, f) in files.iter().enumerate() {
        let d = f.parent()?.to_path_buf();
        match count.iter_mut().find(|c| c.0 == d) {
            Some(c) => {
                c.1 += 1;
                c.2 = i;
            }
            None => count.push((d, 1, i)),
        }
    }
    count.into_iter().max_by_key(|c| (c.1, c.2)).map(|c| c.0)
}

/// `clean`, `header` or a custom command (`_t3_worker`) over `files`: `clean` and a custom
/// command staged under `staging` with each plugin's masters (`find`, `stage_set`), the result
/// copied back when it changed (backups `.preclean.bak` / `.precustom.bak`); `header` in the
/// plugin's own folder. One run per plugin, but for the commands of the whole load order
/// (`WHOLE_ORDER`: one run over the plugins staged together, as that load order) and the ones
/// comparing two (`PAIR`: one run on the pair). A new file such a run makes (multipatch.esp)
/// goes to the folder holding most of the plugins. `order` sorts a clean (masters before what
/// depends on them) and the staged load order.
pub fn run(engine: &Engine, cmd: &str, extra: &[String], files: &[PathBuf], find: &Find<'_>, staging: &Path, order: &[String]) -> Run {
    let mut r = Run::default();
    let (id, keep) = keep_dir(staging);
    r.id = id;
    let (tokens, staged, bak_suffix): (Vec<String>, bool, &str) = match cmd {
        "clean" => (["clean", "--replace", "--hide-backups"].iter().map(|s| (*s).to_string()).chain(extra.iter().cloned()).collect(), true, ".preclean.bak"),
        "custom" => (extra.to_vec(), true, ".precustom.bak"),
        _ => (std::iter::once("header".to_string()).chain(extra.iter().cloned()).collect(), false, ".pretes3cmd.bak"),
    };
    let verb = if cmd == "custom" { tokens.first().map(|s| s.to_lowercase()).unwrap_or_default() } else { String::new() };
    let (whole, pair) = (WHOLE_ORDER.contains(&verb.as_str()), PAIR.contains(&verb.as_str()));
    let builtin = matches!(engine, Engine::Builtin);
    let name = |f: &PathBuf| f.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
    let mut files: Vec<PathBuf> = files.to_vec();
    if cmd == "clean" {
        let skipped: Vec<String> = files.iter().filter(|f| NEVER_CLEAN.contains(&name(f).as_str())).map(name).collect();
        if !skipped.is_empty() {
            r.lines.push(format!("Skipping the vanilla master(s): {} -- Morrowind.esm, Tribunal.esm and Bloodmoon.esm are never cleaned.", skipped.join(", ")));
        }
        files.retain(|f| !NEVER_CLEAN.contains(&name(f).as_str()));
    }
    if cmd == "clean" || whole {
        let at: std::collections::HashMap<String, usize> = order.iter().enumerate().map(|(i, n)| (n.to_lowercase(), i)).collect();
        files.sort_by_key(|f| {
            let n = name(f);
            (at.get(&n).copied().unwrap_or(1 << 30), !(n.ends_with(".esm") || n.ends_with(".omwgame")), n)
        });
    }
    r.lines.push("=".repeat(70));
    r.lines.push(format!(" TES3CMD {}{}", tokens.join(" ").to_uppercase(), if staged { " (staged, VFS-aware)" } else { "" }));
    r.lines.push("=".repeat(70));
    r.lines.push(format!("  Engine: {}", engine.describe()));
    if staged {
        r.lines.push(format!("  Staging dir: {}", staging.display()));
    }
    if pair && files.len() != 2 {
        r.failed += files.len().max(1);
        r.lines.push(format!("  {verb} compares two plugins: add exactly two (there are {}).", files.len()));
    }
    // The runs: each plugin alone, or (whole, pair) all of them at once.
    let groups: Vec<Vec<PathBuf>> = if pair && files.len() != 2 {
        Vec::new()
    } else if whole || pair {
        vec![files.clone()]
    } else {
        files.iter().map(|f| vec![f.clone()]).collect()
    };
    let home = home_folder(&files);
    for group in groups {
        r.lines.push(String::new());
        let shown: Vec<String> = group.iter().map(|f| f.display().to_string()).collect();
        r.lines.push(format!("--- {}: {}", tokens.join(" "), shown.join(", ")));
        let n = group.len();
        let mut step = || -> std::io::Result<()> {
            if !staged {
                let f = &group[0];
                let mut args = tokens.clone();
                args.push(f.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
                let before = std::fs::read(f).ok();
                let (code, out) = run_one(engine, &args, f.parent().unwrap_or(Path::new(".")), None)?;
                r.lines.push(if out.is_empty() { "(no output)".into() } else { out });
                if code == 0 { r.ok += 1 } else { r.failed += 1 }
                if let (Some(before), Ok(after)) = (before, std::fs::read(f))
                    && after != before
                {
                    r.changed += 1;
                    record_change(&mut r, &keep, f, &before, &after);
                }
                return Ok(());
            }
            let s = stage_set(staging, &group, find, !builtin, order)?;
            if !s.missing.is_empty() {
                r.skipped += n;
                r.lines.push(format!(
                    "  SKIPPED: master(s) not found in any data folder: {} -- running without the masters present gives wrong results.",
                    s.missing.join(", ")
                ));
                return Ok(());
            }
            let before: Vec<Vec<u8>> = s.copies.iter().map(|c| std::fs::read(&c.1)).collect::<std::io::Result<_>>()?;
            let listed = |d: &Path| -> std::collections::HashSet<String> {
                std::fs::read_dir(d).map(|rd| rd.flatten().filter(|e| e.path().is_file()).map(|e| e.file_name().to_string_lossy().to_lowercase()).collect()).unwrap_or_default()
            };
            let was_there = listed(&s.data_files);
            let mut args = tokens.clone();
            if !whole {
                args.extend(s.copies.iter().map(|c| c.1.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()));
            }
            let (code, out) = run_one(engine, &args, &s.data_files, builtin.then_some(&s.masters))?;
            r.lines.push(if out.is_empty() { "(no output)".into() } else { out });
            if code != 0 {
                r.failed += n;
                r.lines.push(format!("  (exit code {code}) -- original NOT touched"));
                return Ok(());
            }
            for ((orig, copy), before) in s.copies.iter().zip(before) {
                let after = std::fs::read(copy)?;
                let shown = orig.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                let lead = if n > 1 { format!("  {shown}: ") } else { "  ".to_string() };
                if after == before {
                    r.ok += 1;
                    r.lines.push(if cmd == "clean" { format!("{lead}no changes -- already clean") } else { format!("{lead}no changes (plugin left untouched)") });
                    continue;
                }
                let mut bak_name = orig.file_name().map(|n| n.to_os_string()).unwrap_or_default();
                bak_name.push(bak_suffix);
                let bak = orig.with_file_name(&bak_name);
                if !bak.exists() {
                    copy_with_time(orig, &bak)?;
                }
                std::fs::write(orig, &after)?;
                record_change(&mut r, &keep, orig, &before, &after);
                r.ok += 1;
                r.changed += 1;
                r.lines.push(format!(
                    "{lead}{}: {} -> {} bytes (backup: {})",
                    if cmd == "clean" { "cleaned" } else { "modified" },
                    before.len(),
                    after.len(),
                    bak_name.to_string_lossy()
                ));
            }
            // A plugin the run made (multipatch.esp), beside the plugins.
            let staged_names: std::collections::HashSet<String> = s.masters.keys().cloned().chain(s.copies.iter().map(|c| name(&c.1))).collect();
            // Only plugins, and never tes3cmd's own backups (`Name~1.esp`, kept beside the plugin
            // when a command is not told `--hide-backups`).
            let plugin_made = |f: &String| {
                let ext = f.rsplit_once('.').filter(|(_, e)| matches!(*e, "esp" | "esm" | "omwaddon" | "omwgame"));
                ext.is_some_and(|(stem, _)| !stem.rsplit_once('~').is_some_and(|(_, n)| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())))
            };
            let mut made: Vec<String> = listed(&s.data_files).into_iter().filter(|f| !was_there.contains(f) && !staged_names.contains(f) && plugin_made(f)).collect();
            made.sort();
            for low in made {
                let Some(src) = std::fs::read_dir(&s.data_files)?.flatten().map(|e| e.path()).find(|p| name(p) == low) else { continue };
                let (Some(dir), Some(file)) = (home.as_ref(), src.file_name()) else { continue };
                let dest = dir.join(file);
                if dest.exists() {
                    let mut bak_name = file.to_os_string();
                    bak_name.push(bak_suffix);
                    let bak = dest.with_file_name(&bak_name);
                    if !bak.exists() {
                        copy_with_time(&dest, &bak)?;
                    }
                }
                std::fs::copy(&src, &dest)?;
                r.made += 1;
                r.lines.push(format!(
                    "  made: {} -> {} (add it to the load order if it is new: content={})",
                    file.to_string_lossy(),
                    dir.display(),
                    file.to_string_lossy()
                ));
            }
            Ok(())
        };
        if let Err(e) = step() {
            r.failed += n;
            r.lines.push(format!("  ERROR: {e}"));
        }
        // A custom command may have written a staged master copy (same size, its date put
        // back), which the next plugin would take as fresh: staged again from the source.
        if staged && cmd == "custom" && !builtin {
            let _ = std::fs::remove_dir_all(staging.join("Data Files"));
        }
    }
    let again = if r.changed + r.made > 0 { "  Sort again to refresh the checks." } else { "" };
    r.status = match cmd {
        "clean" => format!(
            "tes3cmd clean: {} cleaned, {} already clean, {} skipped (missing masters), {} failed.{again}",
            r.changed,
            r.ok - r.changed.min(r.ok),
            r.skipped,
            r.failed
        ),
        "custom" => format!(
            "tes3cmd {}: {} modified, {} unchanged, {} skipped (missing masters), {} failed{}.{again}",
            extra.join(" "),
            r.changed,
            r.ok - r.changed.min(r.ok),
            r.skipped,
            r.failed,
            if r.made > 0 { format!(", {} made", r.made) } else { String::new() }
        ),
        _ => format!("tes3cmd {cmd}: {} ok, {} failed. See the log.", r.ok, r.failed),
    };
    r
}

#[cfg(test)]
mod tests {
    #[test]
    fn only_a_tes3cmd_by_name_is_run_from_the_page() {
        for ok in ["tes3cmd", "C:\\tools\\tes3cmd.exe", "/opt/TES3CMD-0.40/tes3cmd.pl", "tes3cmd.bat"] {
            assert!(super::looks_like_tes3cmd(ok), "{ok}");
        }
        for bad in ["", "/bin/sh", "C:\\Windows\\System32\\cmd.exe", "/tmp/notes/tes3cmd/run", "perl"] {
            assert!(!super::looks_like_tes3cmd(bad), "{bad}");
        }
    }
    use super::*;

    #[test]
    fn perl_code_is_told_from_the_rest() {
        let v = |s: &str| s.split_whitespace().map(String::from).collect::<Vec<_>>();
        assert!(needs_perl(&v("modify --type NPC_ --run $R->set({f=>'name'},'x')")));
        assert!(needs_perl(&v("modify -program-file fix.pl")));
        assert!(needs_perl(&v("modify --replace /(\\d+)/$1*2/e")));
        assert!(!needs_perl(&v("modify --type SOUN --replace /a/b/i")));
        assert!(!needs_perl(&v("dump --run")), "only modify runs code");
        assert!(!needs_perl(&v("clean --replace")));
    }

    #[test]
    fn master_sizes_are_synced_and_a_plugin_is_staged_with_its_masters() {
        let root = std::env::temp_dir().join(format!("wg_t3_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("data")).unwrap();
        std::fs::write(root.join("data/M.esm"), vec![0u8; 1234]).unwrap();
        let mut hedr = vec![0u8; 300];
        hedr[0..4].copy_from_slice(&1.3f32.to_le_bytes());
        let sub = |t: &[u8; 4], d: &[u8]| [t.to_vec(), u32::try_from(d.len()).unwrap().to_le_bytes().to_vec(), d.to_vec()].concat();
        let body = [sub(b"HEDR", &hedr), sub(b"MAST", b"M.esm\0"), sub(b"DATA", &7u64.to_le_bytes())].concat();
        let plugin = [b"TES3".to_vec(), u32::try_from(body.len()).unwrap().to_le_bytes().to_vec(), vec![0; 8], body].concat();
        std::fs::write(root.join("P.esp"), &plugin).unwrap();
        let data = root.join("data");
        let find = |n: &str| Some(data.join(n)).filter(|p| p.is_file());
        let (copy, missing) = stage(&root.join("stage"), &root.join("P.esp"), &find).unwrap();
        assert!(missing.is_empty() && copy.is_file());
        assert!(std::fs::read_to_string(root.join("stage/Morrowind.ini")).unwrap().contains("GameFile0=M.esm\nGameFile1=P.esp"));
        // The staged master is a copy: tes3cmd writing it leaves the person's master alone.
        let source = std::fs::read(data.join("M.esm")).unwrap();
        std::fs::write(root.join("stage/Data Files/M.esm"), b"written by tes3cmd").unwrap();
        assert_eq!(std::fs::read(data.join("M.esm")).unwrap(), source);
        let (updated, unresolved) = sync_master_sizes(&root.join("P.esp"), &find, true).unwrap();
        assert_eq!(updated, [("M.esm".to_string(), 7, 1234)]);
        assert!(unresolved.is_empty());
        assert!(root.join("P.esp.masterfix.bak").is_file());
        assert!(sync_master_sizes(&root.join("P.esp"), &find, true).unwrap().0.is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }
}
