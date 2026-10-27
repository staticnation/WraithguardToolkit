//! tes3cmd, launched (port step 4.4, slice 8, option (a): the window stays a launcher over the
//! external tool; its rewrite over the tes3 crate is `viewer-shell/TES3CMD-REWRITE.md`):
//! `wraithguard/gui/t3.py` and `wraithguard_toolkit.find_tes3cmd`, `tes3cmd_invocation`,
//! `stage_for_tes3cmd`, `sync_plugin_master_sizes`.
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

/// `plugin` staged for tes3cmd under `root` (`stage_for_tes3cmd`): `Data Files/` with its
/// masters (hard-linked when the volume allows, else copied; reused while size and time
/// match) and a private copy of the plugin, and a `Morrowind.ini` naming them. Answers the
/// staged plugin and the masters no data folder has (a non-empty list: do not run).
pub fn stage(root: &Path, plugin: &Path, find: &Find<'_>) -> std::io::Result<(PathBuf, Vec<String>)> {
    let df = root.join("Data Files");
    std::fs::create_dir_all(&df)?;
    let mut staged: Vec<String> = Vec::new();
    let mut missing = Vec::new();
    for m in ordercore::inputs::read_masters(plugin) {
        let Some(src) = find(&m) else {
            missing.push(m);
            continue;
        };
        let name = src.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let dest = df.join(&name);
        let fresh = || -> std::io::Result<()> {
            if let (Ok(s), Ok(d)) = (std::fs::metadata(&src), std::fs::metadata(&dest)) {
                let secs = |m: &std::fs::Metadata| m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs());
                if s.len() == d.len() && secs(&s) == secs(&d) {
                    return Ok(());
                }
                std::fs::remove_file(&dest)?;
            }
            if std::fs::hard_link(&src, &dest).is_ok() {
                return Ok(());
            }
            copy_with_time(&src, &dest)
        };
        match fresh() {
            Ok(()) => staged.push(name),
            Err(_) => missing.push(m),
        }
    }
    let name = plugin.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let copy = df.join(&name);
    if copy.exists() {
        std::fs::remove_file(&copy)?;
    }
    copy_with_time(plugin, &copy)?;
    staged.push(name);
    let ini: String = staged.iter().enumerate().map(|(i, n)| format!("GameFile{i}={n}\n")).collect();
    let latin1: Vec<u8> = format!("[Game Files]\n{ini}").chars().map(|c| if (c as u32) < 256 { c as u8 } else { b'?' }).collect();
    std::fs::write(root.join("Morrowind.ini"), latin1)?;
    Ok((copy, missing))
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

/// One tes3cmd run, its output joined.
fn run_one(argv: &[String], args: &[String], cwd: &Path) -> std::io::Result<(i32, String)> {
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
    pub status: String,
}

/// `clean`, `header` or a custom command (`_t3_worker`) over `files`, one run each: `clean`
/// and a custom command staged under `staging` with each plugin's masters (`find`), the
/// result copied back when it changed (backups `.preclean.bak` / `.precustom.bak`); `header`
/// in the plugin's own folder. `order` sorts a clean, masters before what depends on them.
pub fn run(argv: &[String], cmd: &str, extra: &[String], files: &[PathBuf], find: &Find<'_>, staging: &Path, order: &[String]) -> Run {
    let mut r = Run::default();
    let (tokens, staged, bak_suffix): (Vec<String>, bool, &str) = match cmd {
        "clean" => (["clean", "--replace", "--hide-backups"].iter().map(|s| (*s).to_string()).chain(extra.iter().cloned()).collect(), true, ".preclean.bak"),
        "custom" => (extra.to_vec(), true, ".precustom.bak"),
        _ => (std::iter::once("header".to_string()).chain(extra.iter().cloned()).collect(), false, ".pretes3cmd.bak"),
    };
    let mut files: Vec<PathBuf> = files.to_vec();
    if cmd == "clean" {
        let name = |f: &PathBuf| f.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
        let skipped: Vec<String> = files.iter().filter(|f| NEVER_CLEAN.contains(&name(f).as_str())).map(name).collect();
        if !skipped.is_empty() {
            r.lines.push(format!("Skipping the vanilla master(s): {} -- Morrowind.esm, Tribunal.esm and Bloodmoon.esm are never cleaned.", skipped.join(", ")));
        }
        files.retain(|f| !NEVER_CLEAN.contains(&name(f).as_str()));
        let at: std::collections::HashMap<String, usize> = order.iter().enumerate().map(|(i, n)| (n.to_lowercase(), i)).collect();
        files.sort_by_key(|f| {
            let n = name(f);
            (at.get(&n).copied().unwrap_or(1 << 30), !(n.ends_with(".esm") || n.ends_with(".omwgame")), n)
        });
    }
    r.lines.push("=".repeat(70));
    r.lines.push(format!(" TES3CMD {}{}", tokens.join(" ").to_uppercase(), if staged { " (staged, VFS-aware)" } else { "" }));
    r.lines.push("=".repeat(70));
    r.lines.push(format!("  Engine: {}", argv.join(" ")));
    if staged {
        r.lines.push(format!("  Staging dir: {}", staging.display()));
    }
    for f in &files {
        let name = f.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        r.lines.push(String::new());
        r.lines.push(format!("--- {}: {}", tokens.join(" "), f.display()));
        let mut step = || -> std::io::Result<()> {
            if !staged {
                let mut args = tokens.clone();
                args.push(name.clone());
                let (code, out) = run_one(argv, &args, f.parent().unwrap_or(Path::new(".")))?;
                r.lines.push(if out.is_empty() { "(no output)".into() } else { out });
                if code == 0 { r.ok += 1 } else { r.failed += 1 }
                return Ok(());
            }
            let (copy, missing) = stage(staging, f, find)?;
            if !missing.is_empty() {
                r.skipped += 1;
                r.lines.push(format!(
                    "  SKIPPED: master(s) not found in any data folder: {} -- running without the masters present gives wrong results.",
                    missing.join(", ")
                ));
                return Ok(());
            }
            let before = std::fs::read(&copy)?;
            let mut args = tokens.clone();
            args.push(name.clone());
            let (code, out) = run_one(argv, &args, copy.parent().unwrap_or(Path::new(".")))?;
            r.lines.push(if out.is_empty() { "(no output)".into() } else { out });
            if code != 0 {
                r.failed += 1;
                r.lines.push(format!("  (exit code {code}) -- original NOT touched"));
                return Ok(());
            }
            let after = std::fs::read(&copy)?;
            if after == before {
                r.ok += 1;
                r.lines.push(if cmd == "clean" { "  no changes -- already clean".into() } else { "  no changes (plugin left untouched)".into() });
                return Ok(());
            }
            let mut bak_name = f.file_name().map(|n| n.to_os_string()).unwrap_or_default();
            bak_name.push(bak_suffix);
            let bak = f.with_file_name(&bak_name);
            if !bak.exists() {
                copy_with_time(f, &bak)?;
            }
            std::fs::write(f, &after)?;
            r.ok += 1;
            r.changed += 1;
            r.lines.push(format!(
                "  {}: {} -> {} bytes (backup: {})",
                if cmd == "clean" { "cleaned" } else { "modified" },
                before.len(),
                after.len(),
                bak_name.to_string_lossy()
            ));
            Ok(())
        };
        if let Err(e) = step() {
            r.failed += 1;
            r.lines.push(format!("  ERROR: {e}"));
        }
    }
    let again = if r.changed > 0 { "  Sort again to refresh the checks." } else { "" };
    r.status = match cmd {
        "clean" => format!(
            "tes3cmd clean: {} cleaned, {} already clean, {} skipped (missing masters), {} failed.{again}",
            r.changed,
            r.ok - r.changed,
            r.skipped,
            r.failed
        ),
        "custom" => format!(
            "tes3cmd {}: {} modified, {} unchanged, {} skipped (missing masters), {} failed.{again}",
            extra.join(" "),
            r.changed,
            r.ok - r.changed,
            r.skipped,
            r.failed
        ),
        _ => format!("tes3cmd {cmd}: {} ok, {} failed. See the log.", r.ok, r.failed),
    };
    r
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let (updated, unresolved) = sync_master_sizes(&root.join("P.esp"), &find, true).unwrap();
        assert_eq!(updated, [("M.esm".to_string(), 7, 1234)]);
        assert!(unresolved.is_empty());
        assert!(root.join("P.esp.masterfix.bak").is_file());
        assert!(sync_master_sizes(&root.join("P.esp"), &find, true).unwrap().0.is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }
}
