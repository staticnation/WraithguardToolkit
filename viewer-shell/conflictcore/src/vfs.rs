//! The data folders' files, read off the interpreter (moved here from `native/src/fsio.rs`
//! so the viewer's Conflicts page walks them too): every file under a folder, and whether
//! several files hold the same bytes, on a few threads.

use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

/// More threads than this gain nothing: the disk is shared.
pub const MAX_WORKERS: usize = 8;
/// Folder links (symlinks, junctions) followed this deep at most, so a link that
/// points back up the tree cannot walk forever.
pub const MAX_DEPTH: usize = 64;
/// The compare's read size.
pub const CHUNK: usize = 1 << 20;

fn workers(n: usize) -> usize {
    let cpus = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
    n.clamp(1, cpus.min(MAX_WORKERS))
}

/// `f(item)` for every item on a few threads, results in the items' order.
pub fn par_map<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let n = workers(items.len());
    if n <= 1 {
        return items.iter().map(&f).collect();
    }
    let next = AtomicUsize::new(0);
    let mut slots: Vec<Option<R>> = Vec::with_capacity(items.len());
    slots.resize_with(items.len(), || None);
    let results = std::sync::Mutex::new(slots);
    std::thread::scope(|s| {
        for _ in 0..n {
            s.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                if i >= items.len() {
                    break;
                }
                let r = f(&items[i]);
                if let Ok(mut g) = results.lock() {
                    g[i] = Some(r);
                }
            });
        }
    });
    results
        .into_inner()
        .unwrap_or_default()
        .into_iter()
        .map(|r| r.expect("every item was mapped"))
        .collect()
}

/// The lower-case extension of a file name, with its dot (`".esp"`), or `""`.
pub fn ext_of(name: &str) -> String {
    match name.rfind('.') {
        Some(i) if i > 0 => name[i..].to_lowercase(),
        _ => String::new(),
    }
}

pub fn lower(s: &str) -> String {
    if s.is_ascii() { s.to_ascii_lowercase() } else { s.to_lowercase() }
}

/// Every file under `root`, relative to it (`a/b/c.dds`). Errors inside the tree
/// (a folder that cannot be listed) leave out that folder, as `os.walk` does; what
/// was listed before stands.
pub fn walk(root: &Path, skip: &HashSet<String>, lowered: bool) -> Option<Vec<String>> {
    if !root.is_dir() {
        return None;
    }
    let mut out = Vec::new();
    let mut seen: HashSet<PathBuf> = HashSet::new();
    if let Ok(c) = fs::canonicalize(root) {
        seen.insert(c);
    }
    // Depth first, each folder's files before its sub-folders' (os.walk's top-down).
    let mut stack: Vec<(PathBuf, String, usize)> = vec![(root.to_path_buf(), String::new(), 0)];
    while let Some((dir, prefix, depth)) = stack.pop() {
        let Ok(rd) = fs::read_dir(&dir) else { continue };
        let mut subdirs: Vec<(PathBuf, String)> = Vec::new();
        for entry in rd.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let Ok(ft) = entry.file_type() else { continue };
            let path = entry.path();
            let is_dir = if ft.is_symlink() {
                // A link: what it points at decides (a junction or symlinked mod folder).
                fs::metadata(&path).map(|m| m.is_dir()).unwrap_or(false)
            } else {
                ft.is_dir()
            };
            let rel = if prefix.is_empty() { name.clone() } else { format!("{prefix}/{name}") };
            if is_dir {
                if ft.is_symlink() {
                    if depth >= MAX_DEPTH {
                        continue;
                    }
                    // Been there (a link back up the tree): not again.
                    if !fs::canonicalize(&path).is_ok_and(|c| seen.insert(c)) {
                        continue;
                    }
                }
                subdirs.push((path, rel));
                continue;
            }
            if !skip.is_empty() && skip.contains(&ext_of(&name)) {
                continue;
            }
            out.push(if lowered { lower(&rel) } else { rel });
        }
        // Reversed onto the stack so they come off in the order they were listed.
        for (p, r) in subdirs.into_iter().rev() {
            stack.push((p, r, depth + 1));
        }
    }
    Some(out)
}

/// Whether every file holds the same bytes. Sizes first; then all of them read side by
/// side, a chunk at a time, stopping at the first difference.
pub fn identical(paths: &[PathBuf]) -> bool {
    if paths.len() < 2 {
        return paths.len() == 1 && paths[0].is_file();
    }
    let mut size = None;
    for p in paths {
        match fs::metadata(p) {
            Ok(m) if m.is_file() => {
                if size.is_some_and(|s| s != m.len()) {
                    return false;
                }
                size = Some(m.len());
            }
            _ => return false,
        }
    }
    let files: io::Result<Vec<File>> = paths.iter().map(File::open).collect();
    let Ok(mut files) = files else { return false };
    let mut first = vec![0u8; CHUNK];
    let mut other = vec![0u8; CHUNK];
    loop {
        let n = match read_full(&mut files[0], &mut first) {
            Ok(n) => n,
            Err(_) => return false,
        };
        for f in &mut files[1..] {
            match read_full(f, &mut other[..n.max(1)]) {
                Ok(m) if m == n && first[..n] == other[..n] => {}
                _ => return false,
            }
        }
        if n == 0 {
            return true;
        }
    }
}

/// Reads until `buf` is full or the file ends; how much was read.
fn read_full(f: &mut File, buf: &mut [u8]) -> io::Result<usize> {
    let mut got = 0;
    while got < buf.len() {
        match f.read(&mut buf[got..]) {
            Ok(0) => break,
            Ok(n) => got += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(got)
}

