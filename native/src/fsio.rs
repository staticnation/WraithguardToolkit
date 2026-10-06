//! Heavy file-system work, off the interpreter: walking data folders and comparing
//! files byte for byte.
//!
//! A MOMW setup has over a thousand data folders and hundreds of thousands of loose
//! files. Listing them with `os.walk` builds a Python object per directory entry and
//! holds the interpreter between system calls; comparing re-shipped assets reads
//! every byte through Python buffers. Here the walk and the reads run on plain
//! threads with Python released, and only the answer (one list of names per folder,
//! one bool per group) crosses back.
//!
//! - `walk_files(roots, skip_exts=None, lower=True)`: every file under each root,
//!   relative to it with `/` separators, in the order the walk met them; `None` for a
//!   root that is not a folder. What `os.walk` + `os.path.relpath` gave the callers.
//! - `files_identical(paths)`: whether every file holds the same bytes (sizes first,
//!   then the contents side by side, stopping at the first difference). A file that
//!   cannot be read makes the answer `False`.
//! - `files_identical_many(groups)`: the same for many groups, in parallel.
//! - `file_digest(path)` / `file_digests(paths)`: a file's BLAKE2b-128 as hex, the same
//!   digest `hashlib.blake2b(digest_size=16)` gives (the mesh analyser's cache keys stay
//!   valid); `""` for a file that cannot be read.
//! - `scan_mod_folders(start, asset_dirs, plugin_exts)`: the subset sort's scan for mod
//!   folders - each folder holding an asset folder or a plugin, not looked into further.

use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use pyo3::prelude::*;

/// More threads than this gain nothing: the disk is shared.
const MAX_WORKERS: usize = 8;
/// Folder links (symlinks, junctions) followed this deep at most, so a link that
/// points back up the tree cannot walk forever.
const MAX_DEPTH: usize = 64;
/// The compare's read size.
const CHUNK: usize = 1 << 20;

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
fn ext_of(name: &str) -> String {
    match name.rfind('.') {
        Some(i) if i > 0 => name[i..].to_lowercase(),
        _ => String::new(),
    }
}

fn lower(s: &str) -> String {
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

fn skip_set(exts: Option<Vec<String>>) -> HashSet<String> {
    exts.unwrap_or_default()
        .into_iter()
        .map(|e| {
            let e = lower(e.trim());
            if e.is_empty() || e.starts_with('.') { e } else { format!(".{e}") }
        })
        .filter(|e| !e.is_empty())
        .collect()
}

/* ---- BLAKE2b (RFC 7693), unkeyed, for content digests ------------------------------ */

const B2_IV: [u64; 8] = [
    0x6a09e667f3bcc908,
    0xbb67ae8584caa73b,
    0x3c6ef372fe94f82b,
    0xa54ff53a5f1d36f1,
    0x510e527fade682d1,
    0x9b05688c2b3e6c1f,
    0x1f83d9abfb41bd6b,
    0x5be0cd19137e2179,
];

const B2_SIGMA: [[usize; 16]; 10] = [
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
    [14, 10, 4, 8, 9, 15, 13, 6, 1, 12, 0, 2, 11, 7, 5, 3],
    [11, 8, 12, 0, 5, 2, 15, 13, 10, 14, 3, 6, 7, 1, 9, 4],
    [7, 9, 3, 1, 13, 12, 11, 14, 2, 6, 5, 10, 4, 0, 15, 8],
    [9, 0, 5, 7, 2, 4, 10, 15, 14, 1, 11, 12, 6, 8, 3, 13],
    [2, 12, 6, 10, 0, 11, 8, 3, 4, 13, 7, 5, 15, 14, 1, 9],
    [12, 5, 1, 15, 14, 13, 4, 10, 0, 7, 6, 3, 9, 2, 8, 11],
    [13, 11, 7, 14, 12, 1, 3, 9, 5, 0, 15, 4, 8, 6, 2, 10],
    [6, 15, 14, 9, 11, 3, 0, 8, 12, 2, 13, 7, 1, 4, 10, 5],
    [10, 2, 8, 4, 7, 6, 1, 5, 15, 11, 9, 14, 3, 12, 13, 0],
];

/// BLAKE2b, streamed: `update` as the bytes come, `finish` for the digest.
pub struct Blake2b {
    h: [u64; 8],
    t: u128,
    buf: [u8; 128],
    len: usize,
    out: usize,
}

impl Blake2b {
    /// An unkeyed hash with an `out`-byte digest (1..=64).
    pub fn new(out: usize) -> Self {
        let mut h = B2_IV;
        h[0] ^= 0x0101_0000 ^ out as u64;
        Blake2b { h, t: 0, buf: [0; 128], len: 0, out }
    }

    fn compress(&mut self, last: bool) {
        let mut m = [0u64; 16];
        for (i, w) in m.iter_mut().enumerate() {
            *w = u64::from_le_bytes(self.buf[i * 8..i * 8 + 8].try_into().expect("8 bytes"));
        }
        let mut v = [0u64; 16];
        v[..8].copy_from_slice(&self.h);
        v[8..].copy_from_slice(&B2_IV);
        v[12] ^= self.t as u64;
        v[13] ^= (self.t >> 64) as u64;
        if last {
            v[14] = !v[14];
        }
        fn g(v: &mut [u64; 16], a: usize, b: usize, c: usize, d: usize, x: u64, y: u64) {
            v[a] = v[a].wrapping_add(v[b]).wrapping_add(x);
            v[d] = (v[d] ^ v[a]).rotate_right(32);
            v[c] = v[c].wrapping_add(v[d]);
            v[b] = (v[b] ^ v[c]).rotate_right(24);
            v[a] = v[a].wrapping_add(v[b]).wrapping_add(y);
            v[d] = (v[d] ^ v[a]).rotate_right(16);
            v[c] = v[c].wrapping_add(v[d]);
            v[b] = (v[b] ^ v[c]).rotate_right(63);
        }
        for r in 0..12 {
            let s = &B2_SIGMA[r % 10];
            g(&mut v, 0, 4, 8, 12, m[s[0]], m[s[1]]);
            g(&mut v, 1, 5, 9, 13, m[s[2]], m[s[3]]);
            g(&mut v, 2, 6, 10, 14, m[s[4]], m[s[5]]);
            g(&mut v, 3, 7, 11, 15, m[s[6]], m[s[7]]);
            g(&mut v, 0, 5, 10, 15, m[s[8]], m[s[9]]);
            g(&mut v, 1, 6, 11, 12, m[s[10]], m[s[11]]);
            g(&mut v, 2, 7, 8, 13, m[s[12]], m[s[13]]);
            g(&mut v, 3, 4, 9, 14, m[s[14]], m[s[15]]);
        }
        for (i, h) in self.h.iter_mut().enumerate() {
            *h ^= v[i] ^ v[i + 8];
        }
    }

    pub fn update(&mut self, mut data: &[u8]) {
        while !data.is_empty() {
            // A full block is compressed only once more follows: the last is final.
            if self.len == 128 {
                self.t += 128;
                self.compress(false);
                self.len = 0;
            }
            let n = (128 - self.len).min(data.len());
            self.buf[self.len..self.len + n].copy_from_slice(&data[..n]);
            self.len += n;
            data = &data[n..];
        }
    }

    pub fn finish(mut self) -> Vec<u8> {
        self.t += self.len as u128;
        self.buf[self.len..].fill(0);
        self.compress(true);
        let mut out = Vec::with_capacity(64);
        for w in self.h {
            out.extend_from_slice(&w.to_le_bytes());
        }
        out.truncate(self.out);
        out
    }
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

/// A file's BLAKE2b-128 as hex, or `""` when it cannot be read.
pub fn digest_file(path: &Path) -> String {
    let Ok(mut f) = File::open(path) else { return String::new() };
    let mut h = Blake2b::new(16);
    let mut buf = vec![0u8; CHUNK];
    loop {
        match f.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => h.update(&buf[..n]),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => return String::new(),
        }
    }
    hex(&h.finish())
}

/* ---- the subset sort's scan for mod folders ------------------------------------------ */

/// Every folder under `start` (itself included) that holds one of `asset_dirs` (by
/// lower-case name) or a plugin: its path, and the plugin names directly inside it. A
/// match is not looked into further. In the order a top-down walk meets them.
pub fn mod_folders(start: &Path, asset_dirs: &HashSet<String>, plugin_exts: &HashSet<String>) -> Vec<(String, Vec<String>)> {
    let mut out = Vec::new();
    let mut seen: HashSet<PathBuf> = HashSet::new();
    if let Ok(c) = fs::canonicalize(start) {
        seen.insert(c);
    }
    let mut stack: Vec<(PathBuf, usize)> = vec![(start.to_path_buf(), 0)];
    while let Some((dir, depth)) = stack.pop() {
        let Ok(rd) = fs::read_dir(&dir) else { continue };
        let mut subdirs: Vec<(PathBuf, bool)> = Vec::new();
        let mut plugins = Vec::new();
        let mut has_assets = false;
        for entry in rd.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let Ok(ft) = entry.file_type() else { continue };
            let path = entry.path();
            let link = ft.is_symlink();
            let is_dir = if link { fs::metadata(&path).map(|m| m.is_dir()).unwrap_or(false) } else { ft.is_dir() };
            if is_dir {
                if asset_dirs.contains(&lower(&name)) {
                    has_assets = true;
                }
                subdirs.push((path, link));
            } else if plugin_exts.contains(&ext_of(&name)) {
                plugins.push(name);
            }
        }
        if has_assets || !plugins.is_empty() {
            out.push((dir.to_string_lossy().into_owned(), plugins));
            continue;
        }
        for (p, link) in subdirs.into_iter().rev() {
            if link {
                if depth >= MAX_DEPTH {
                    continue;
                }
                if !fs::canonicalize(&p).is_ok_and(|c| seen.insert(c)) {
                    continue;
                }
            }
            stack.push((p, depth + 1));
        }
    }
    out
}

/// `walk_files(roots, skip_exts=None, lower=True)`: see the module comment.
#[pyfunction]
#[pyo3(signature = (roots, skip_exts=None, lower=true))]
fn walk_files(py: Python<'_>, roots: Vec<PathBuf>, skip_exts: Option<Vec<String>>, lower: bool) -> Vec<Option<Vec<String>>> {
    let skip = skip_set(skip_exts);
    py.detach(|| par_map(&roots, |r| walk(r, &skip, lower)))
}

/// `files_identical(paths)`: see the module comment.
#[pyfunction]
fn files_identical(py: Python<'_>, paths: Vec<PathBuf>) -> bool {
    py.detach(|| identical(&paths))
}

/// `files_identical_many(groups)`: one answer per group, groups compared in parallel.
#[pyfunction]
fn files_identical_many(py: Python<'_>, groups: Vec<Vec<PathBuf>>) -> Vec<bool> {
    py.detach(|| par_map(&groups, |g| identical(g.as_slice())))
}

/// `file_digest(path)`: see the module comment.
#[pyfunction]
fn file_digest(py: Python<'_>, path: PathBuf) -> String {
    py.detach(|| digest_file(&path))
}

/// `file_digests(paths)`: `file_digest` for each, in parallel.
#[pyfunction]
fn file_digests(py: Python<'_>, paths: Vec<PathBuf>) -> Vec<String> {
    py.detach(|| par_map(&paths, |p| digest_file(p.as_path())))
}

/// `scan_mod_folders(start, asset_dirs, plugin_exts)`: see the module comment.
#[pyfunction]
fn scan_mod_folders(py: Python<'_>, start: PathBuf, asset_dirs: Vec<String>, plugin_exts: Vec<String>) -> Vec<(String, Vec<String>)> {
    let dirs: HashSet<String> = asset_dirs.iter().map(|d| lower(d.as_str())).collect();
    let exts = skip_set(Some(plugin_exts));
    py.detach(|| mod_folders(&start, &dirs, &exts))
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(walk_files, m)?)?;
    m.add_function(wrap_pyfunction!(file_digest, m)?)?;
    m.add_function(wrap_pyfunction!(file_digests, m)?)?;
    m.add_function(wrap_pyfunction!(scan_mod_folders, m)?)?;
    m.add_function(wrap_pyfunction!(files_identical, m)?)?;
    m.add_function(wrap_pyfunction!(files_identical_many, m)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> PathBuf {
        let d = std::env::temp_dir().join(format!("wg_fsio_{}_{}", std::process::id(), line!()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(d.join("Meshes/X")).unwrap();
        fs::write(d.join("a.esp"), b"p").unwrap();
        fs::write(d.join("Meshes/X/Rock.NIF"), b"rock").unwrap();
        fs::write(d.join("Meshes/b.nif"), b"b").unwrap();
        d
    }

    #[test]
    fn walks_relative_lowered_and_skips() {
        let d = tree();
        let skip = skip_set(Some(vec!["ESP".into()]));
        let mut got = walk(&d, &skip, true).unwrap();
        got.sort();
        assert_eq!(got, ["meshes/b.nif", "meshes/x/rock.nif"]);
        let kept = walk(&d, &HashSet::new(), false).unwrap();
        assert!(kept.contains(&"Meshes/X/Rock.NIF".to_string()) && kept.contains(&"a.esp".to_string()));
        assert!(walk(&d.join("nope"), &skip, true).is_none());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn blake2b_matches_hashlib() {
        // hashlib.blake2b(data, digest_size=16).hexdigest()
        let one = |d: &[u8]| {
            let mut h = Blake2b::new(16);
            h.update(d);
            hex(&h.finish())
        };
        assert_eq!(one(b""), "cae66941d9efbd404e4d88758ea67670");
        assert_eq!(one(b"abc"), "cf4ab791c62b8d2b2109c90275287816");
        let long: Vec<u8> = (0..5).flat_map(|_| 0u8..=255).collect();
        assert_eq!(one(&long), "825c7cfa8fc12c18f57b6046a548db9f");
        // Fed in odd pieces, the same.
        let mut h = Blake2b::new(16);
        for c in long.chunks(7) {
            h.update(c);
        }
        assert_eq!(hex(&h.finish()), "825c7cfa8fc12c18f57b6046a548db9f");
    }

    #[test]
    fn finds_mod_folders_and_stops_there() {
        let d = std::env::temp_dir().join(format!("wg_fsio_mods_{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(d.join("A/Meshes/deep/Textures")).unwrap();
        fs::create_dir_all(d.join("B")).unwrap();
        fs::create_dir_all(d.join("C/x")).unwrap();
        fs::write(d.join("B/b.ESP"), b"").unwrap();
        fs::write(d.join("B/readme.txt"), b"").unwrap();
        let dirs: HashSet<String> = ["meshes", "textures"].iter().map(|s| s.to_string()).collect();
        let exts = skip_set(Some(vec![".esp".into(), ".esm".into()]));
        let mut got = mod_folders(&d, &dirs, &exts);
        got.sort();
        let names: Vec<(String, Vec<String>)> = got
            .into_iter()
            .map(|(p, pl)| (Path::new(&p).file_name().unwrap().to_string_lossy().into_owned(), pl))
            .collect();
        assert_eq!(names, [("A".to_string(), vec![]), ("B".to_string(), vec!["b.ESP".to_string()])]);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn compares_bytes() {
        let d = std::env::temp_dir().join(format!("wg_fsio_cmp_{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        let big: Vec<u8> = (0..(CHUNK * 2 + 17)).map(|i| (i % 251) as u8).collect();
        let mut late = big.clone();
        *late.last_mut().unwrap() ^= 1;
        for (n, b) in [("a", &big), ("b", &big), ("c", &late)] {
            fs::write(d.join(n), b).unwrap();
        }
        fs::write(d.join("e1"), b"").unwrap();
        fs::write(d.join("e2"), b"").unwrap();
        assert!(identical(&[d.join("a"), d.join("b")]));
        assert!(!identical(&[d.join("a"), d.join("c")]));
        assert!(!identical(&[d.join("a"), d.join("missing")]));
        assert!(identical(&[d.join("e1"), d.join("e2")]));
        assert_eq!(par_map(&[vec![d.join("a"), d.join("b")], vec![d.join("a"), d.join("c")]], |g| identical(g.as_slice())), [true, false]);
        let _ = fs::remove_dir_all(&d);
    }
}
