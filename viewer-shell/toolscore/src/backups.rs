//! Backups (port step 4.4, slice 3): the copies Wraithguard and the tools it drives leave
//! beside a file before changing it, found, restored over their originals, or deleted
//! (`wraithguard_toolkit.scan_backups`, the Backups window).
//!
//! What counts as one, by its name: Wraithguard's own suffixes (`.preclean.bak`,
//! `.masterfix.bak`, `.precustom.bak`, `.pretes3cmd.bak`, `.premaster.bak`), tes3cmd's
//! `name~1.ext`, a timestamped `name.bak-YYYYMMDD-HHMMSS` (the export and the patch writers)
//! and the Configurator's `name.backup.*`.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Wraithguard's own backup suffixes (`BACKUP_PATTERNS`), in the order they are tried.
pub const SUFFIXES: [&str; 5] = [".preclean.bak", ".masterfix.bak", ".precustom.bak", ".pretes3cmd.bak", ".premaster.bak"];

/// One backup: where it is, the file it was taken of (which may be gone: restoring it is then
/// a recovery), and what made it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Backup {
    pub path: PathBuf,
    pub original: PathBuf,
    pub kind: String,
}

/// What a file's name says it is a backup of: the original's name and the kind; `None` for a
/// file that is not one.
pub fn classify(name: &str) -> Option<(String, String)> {
    let low = name.to_lowercase();
    for suf in SUFFIXES {
        if low.ends_with(suf) && name.len() > suf.len() {
            return Some((name[..name.len() - suf.len()].to_string(), suf.trim_start_matches('.').to_string()));
        }
    }
    // tes3cmd's `name~N.ext`: `^(.+)~\d+(\.[^.]+)$`.
    if let Some(dot) = name.rfind('.') {
        let (stem, ext) = name.split_at(dot);
        let digits = stem.len() - stem.trim_end_matches(|c: char| c.is_ascii_digit()).len();
        let head = &stem[..stem.len() - digits];
        if ext.len() > 1 && digits > 0 && head.len() > 1 && head.ends_with('~') {
            return Some((format!("{}{ext}", &head[..head.len() - 1]), "tes3cmd ~N".to_string()));
        }
    }
    // `^(.+)\.bak-\d{8}-\d{6}$`: 20 characters after the original's name.
    if name.len() > 20 && name.is_char_boundary(name.len() - 20) {
        let tail = &name[name.len() - 20..];
        let tb = tail.as_bytes();
        let digits = |r: std::ops::Range<usize>| tb[r].iter().all(u8::is_ascii_digit);
        if tail[..5].eq_ignore_ascii_case(".bak-") && digits(5..13) && tb[13] == b'-' && digits(14..20) {
            return Some((name[..name.len() - 20].to_string(), "timestamped .bak".to_string()));
        }
    }
    // `^(.+)\.backup\.`: the last `.backup.` with something before it.
    if let Some(at) = low.rfind(".backup.").filter(|&a| a > 0) {
        return Some((name[..at].to_string(), "configurator .backup".to_string()));
    }
    None
}

/// The backups under `roots`, each looked into `max_depth` folders deep (a folder at that depth
/// has its files listed but is not descended), by path without case (`scan_backups`). A root
/// that is not a folder is passed over, and a file reached twice is listed once.
pub fn scan(roots: &[PathBuf], max_depth: usize) -> Vec<Backup> {
    fn walk(dir: &Path, depth: usize, max: usize, seen: &mut HashSet<String>, out: &mut Vec<Backup>) {
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        let mut subdirs = Vec::new();
        for e in rd.flatten() {
            let Ok(ft) = e.file_type() else { continue };
            let path = e.path();
            if ft.is_dir() {
                subdirs.push(path);
                continue;
            }
            let name = e.file_name().to_string_lossy().into_owned();
            let key = path.to_string_lossy().to_lowercase();
            if seen.contains(&key) {
                continue;
            }
            if let Some((orig, kind)) = classify(&name) {
                seen.insert(key);
                out.push(Backup { original: path.with_file_name(orig), path, kind });
            }
        }
        if depth < max {
            for d in subdirs {
                walk(&d, depth + 1, max, seen, out);
            }
        }
    }
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for r in roots {
        if r.is_dir() {
            walk(r, 0, max_depth, &mut seen, &mut out);
        }
    }
    out.sort_by_key(|b| b.path.to_string_lossy().to_lowercase());
    out
}

/// Each backup copied over its original (`shutil.copy2`: the times go with it). How many were
/// restored, and the ones that failed with why.
pub fn restore(backups: &[(PathBuf, PathBuf)]) -> (usize, Vec<(PathBuf, String)>) {
    let mut ok = 0;
    let mut failed = Vec::new();
    for (from, to) in backups {
        let done = std::fs::copy(from, to).and_then(|_| {
            let meta = std::fs::metadata(from)?;
            let f = std::fs::File::options().write(true).open(to)?;
            f.set_modified(meta.modified()?)
        });
        match done {
            Ok(()) => ok += 1,
            Err(e) => failed.push((from.clone(), e.to_string())),
        }
    }
    (ok, failed)
}

/// Each backup deleted. How many were.
pub fn delete(paths: &[PathBuf]) -> usize {
    paths.iter().filter(|p| std::fs::remove_file(p).is_ok()).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_say_what_they_back_up() {
        let c = |n: &str| classify(n).map(|(o, k)| format!("{o} | {k}"));
        assert_eq!(c("Mod.esp.preclean.bak").as_deref(), Some("Mod.esp | preclean.bak"));
        assert_eq!(c("Mod.ESP.PREMASTER.BAK").as_deref(), Some("Mod.ESP | premaster.bak"));
        assert_eq!(c("Mod~1.esp").as_deref(), Some("Mod.esp | tes3cmd ~N"));
        assert_eq!(c("My~Mod~12.esp").as_deref(), Some("My~Mod.esp | tes3cmd ~N"));
        assert_eq!(c("openmw.cfg.bak-20260101-120000").as_deref(), Some("openmw.cfg | timestamped .bak"));
        assert_eq!(c("openmw.cfg.BAK-20260101-120000").as_deref(), Some("openmw.cfg | timestamped .bak"));
        assert_eq!(c("openmw.cfg.backup.1").as_deref(), Some("openmw.cfg | configurator .backup"));
        assert_eq!(c("a.backup.b.backup.c").as_deref(), Some("a.backup.b | configurator .backup"));
        assert_eq!(c("Mod.esp"), None);
        assert_eq!(c("~1.esp"), None);
        assert_eq!(c("Mod~.esp"), None);
        assert_eq!(c(".bak-20260101-120000"), None);
    }

    #[test]
    fn a_tree_is_scanned_to_its_depth_and_restored() {
        let root = std::env::temp_dir().join(format!("wg_bk_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("a/b/c")).unwrap();
        std::fs::write(root.join("Mod.esp"), "new").unwrap();
        std::fs::write(root.join("Mod.esp.preclean.bak"), "old").unwrap();
        std::fs::write(root.join("a/b/X~1.esp"), "x").unwrap();
        std::fs::write(root.join("a/b/c/Y~1.esp"), "y").unwrap();
        let found = scan(std::slice::from_ref(&root), 2);
        let names: Vec<String> = found.iter().map(|b| b.path.file_name().unwrap().to_string_lossy().into_owned()).collect();
        assert_eq!(names, ["X~1.esp", "Mod.esp.preclean.bak"]);
        let (ok, failed) = restore(&[(found[1].path.clone(), found[1].original.clone())]);
        assert_eq!((ok, failed.len()), (1, 0));
        assert_eq!(std::fs::read_to_string(root.join("Mod.esp")).unwrap(), "old");
        assert_eq!(delete(&[found[0].path.clone()]), 1);
        let _ = std::fs::remove_dir_all(&root);
    }
}
