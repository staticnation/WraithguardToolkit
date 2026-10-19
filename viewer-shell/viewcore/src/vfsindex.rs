//! Wraithguard: the overlay's file index, kept between connects.
//!
//! A connect walks every file of every root to index the overlay (`Vfs::with_roots`): on
//! Static's install 205,535 files in 1,233 folders, about 4 s, every time, though between
//! most connects not one of them changed. This keeps each root's index in one file beside
//! the viewer and, on the next connect, asks each folder the walk read for its modified
//! time instead of reading the folder: a folder's time moves when anything in it is added,
//! removed or renamed, so a root whose folders all kept their times holds exactly the
//! files it held. A root that changed (a mod installed, updated or taken out) is walked
//! again on its own; the rest come from the file.
//!
//! What a root's index *is* stays Gardenfell's (`Root::index_one`, patch 0012): this only
//! stores and checks it. Anything unexpected in the file - another version, a short read,
//! a root it does not have - means walking that root, never a guess.

use crate::vfs::{Root, Vfs};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// The file's name, beside the viewer's other files.
pub const FILE: &str = "wraithguard_overlay_index.bin";
const MAGIC: &[u8; 4] = b"WGVI";
/// Bumped whenever what a root's index holds could differ for the same folders.
const VERSION: u32 = 1;

/// What one connect's index cost: roots taken from the file, roots walked, and whether
/// the file was written.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    pub reused: usize,
    pub walked: usize,
    pub saved: bool,
}

/// One root as the file keeps it: the folders the walk read with their times, and each
/// file's key (as the index spells it) and path under the root ('/' between the parts).
#[derive(Clone, Debug, PartialEq)]
struct Saved {
    dirs: Vec<(String, u64, u32)>,
    files: Vec<(String, String)>,
}

fn stamp(p: &Path) -> Option<(u64, u32)> {
    let t = std::fs::metadata(p).ok()?.modified().ok()?;
    let d = t.duration_since(UNIX_EPOCH).ok()?;
    Some((d.as_secs(), d.subsec_nanos()))
}

/// The overlay for `dirs` (highest priority first, as `Vfs::with_roots` takes them),
/// each root from `cache` where its folders are unchanged and walked otherwise; `cache`
/// is rewritten when anything was walked. No `cache` is `Vfs::with_roots`.
pub fn overlay(dirs: &[(String, PathBuf)], cache: Option<&Path>) -> (Vfs, Stats) {
    let Some(cache) = cache else {
        return (Vfs::with_roots(dirs), Stats { reused: 0, walked: dirs.len(), saved: false });
    };
    let mut saved: HashMap<String, Saved> =
        std::fs::read(cache).ok().and_then(|b| decode(&b)).unwrap_or_default();
    let keyed: Vec<(String, String, PathBuf)> =
        dirs.iter().map(|(l, d)| (d.to_string_lossy().to_string(), l.clone(), d.clone())).collect();
    let got: Vec<(Root, Option<Saved>, bool)> = crate::pool::par_map(&keyed, |_, (key, label, dir)| {
        if let Some(s) = saved.get(key).filter(|s| fresh(s)) {
            let index: HashMap<String, PathBuf> =
                s.files.iter().map(|(k, rel)| (k.clone(), join(dir, rel))).collect();
            return (Root::from_index(label.clone(), dir.clone(), index), None, true);
        }
        let started = SystemTime::now();
        let (root, read) = Root::index_one(label, dir);
        // Each folder's time, and not kept at all if one moved after the walk began:
        // what the walk saw of that folder may already be out of date.
        let mut stamps = Vec::with_capacity(read.len());
        let mut steady = true;
        for d in &read {
            match stamp(d) {
                Some((s, n)) => {
                    if UNIX_EPOCH + std::time::Duration::new(s, n) > started {
                        steady = false;
                    }
                    stamps.push((d.to_string_lossy().to_string(), s, n));
                }
                None => steady = false,
            }
        }
        let keep = steady.then(|| Saved {
            dirs: stamps,
            files: root
                .index()
                .iter()
                .filter_map(|(k, p)| rel_of(dir, p).map(|r| (k.clone(), r)))
                .collect(),
        });
        (root, keep, false)
    });
    let mut stats = Stats::default();
    let mut changed = false;
    let mut roots = Vec::with_capacity(got.len());
    for ((key, _, _), (root, keep, reused)) in keyed.iter().zip(got) {
        if reused {
            stats.reused += 1;
        } else {
            stats.walked += 1;
            changed = true;
            match keep {
                Some(s) => {
                    saved.insert(key.clone(), s);
                }
                None => {
                    saved.remove(key);
                }
            }
        }
        roots.push(root);
    }
    // Roots of other setups are kept (switching MO2 profiles back and forth), but not
    // once their folder is gone.
    let before = saved.len();
    saved.retain(|k, _| Path::new(k).is_dir());
    changed |= saved.len() != before;
    if changed {
        stats.saved = write(cache, &saved).is_ok();
    }
    (Vfs::from_roots(roots), stats)
}

/// Every folder the walk read still has the time it had.
fn fresh(s: &Saved) -> bool {
    !s.dirs.is_empty() && s.dirs.iter().all(|(d, sec, ns)| stamp(Path::new(d)) == Some((*sec, *ns)))
}

/// A file's path under its root, '/' between the parts; None if it is not under it (the
/// walk only finds files under the root, so that would be a file this cannot rebuild).
fn rel_of(dir: &Path, p: &Path) -> Option<String> {
    let r = p.strip_prefix(dir).ok()?;
    let parts: Vec<String> = r.components().map(|c| c.as_os_str().to_string_lossy().to_string()).collect();
    (!parts.is_empty()).then(|| parts.join("/"))
}

/// The path `rel_of` came from: the root joined part by part, as the walk built it.
fn join(dir: &Path, rel: &str) -> PathBuf {
    let mut p = dir.to_path_buf();
    for part in rel.split('/') {
        p.push(part);
    }
    p
}

fn put_str(out: &mut Vec<u8>, s: &str) {
    out.extend_from_slice(&(s.len() as u32).to_le_bytes());
    out.extend_from_slice(s.as_bytes());
}

fn encode(saved: &HashMap<String, Saved>) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.extend_from_slice(&(saved.len() as u32).to_le_bytes());
    let mut keys: Vec<&String> = saved.keys().collect();
    keys.sort();
    for k in keys {
        let s = &saved[k];
        put_str(&mut out, k);
        out.extend_from_slice(&(s.dirs.len() as u32).to_le_bytes());
        for (d, sec, ns) in &s.dirs {
            put_str(&mut out, d);
            out.extend_from_slice(&sec.to_le_bytes());
            out.extend_from_slice(&ns.to_le_bytes());
        }
        out.extend_from_slice(&(s.files.len() as u32).to_le_bytes());
        for (key, rel) in &s.files {
            // Nearly every key is its path in lower case: a flag rather than the key twice.
            if *key == rel.to_ascii_lowercase() {
                out.push(1);
            } else {
                out.push(0);
                put_str(&mut out, key);
            }
            put_str(&mut out, rel);
        }
    }
    out
}

fn decode(b: &[u8]) -> Option<HashMap<String, Saved>> {
    let mut at = 0usize;
    let take = |at: &mut usize, n: usize| -> Option<&[u8]> {
        let s = b.get(*at..at.checked_add(n)?)?;
        *at += n;
        Some(s)
    };
    let u32_ = |at: &mut usize| -> Option<u32> { Some(u32::from_le_bytes(take(at, 4)?.try_into().ok()?)) };
    let u64_ = |at: &mut usize| -> Option<u64> { Some(u64::from_le_bytes(take(at, 8)?.try_into().ok()?)) };
    let str_ = |at: &mut usize| -> Option<String> {
        let n = u32_(at)? as usize;
        String::from_utf8(take(at, n)?.to_vec()).ok()
    };
    if take(&mut at, 4)? != MAGIC || u32_(&mut at)? != VERSION {
        return None;
    }
    let roots = u32_(&mut at)?;
    let mut out = HashMap::new();
    for _ in 0..roots {
        let key = str_(&mut at)?;
        let nd = u32_(&mut at)? as usize;
        let mut dirs = Vec::with_capacity(nd.min(1 << 16));
        for _ in 0..nd {
            let d = str_(&mut at)?;
            let sec = u64_(&mut at)?;
            let ns = u32_(&mut at)?;
            dirs.push((d, sec, ns));
        }
        let nf = u32_(&mut at)? as usize;
        let mut files = Vec::with_capacity(nf.min(1 << 20));
        for _ in 0..nf {
            let flag = *take(&mut at, 1)?.first()?;
            let k = if flag == 1 { None } else { Some(str_(&mut at)?) };
            let rel = str_(&mut at)?;
            let k = k.unwrap_or_else(|| rel.to_ascii_lowercase());
            files.push((k, rel));
        }
        out.insert(key, Saved { dirs, files });
    }
    (at == b.len()).then_some(out)
}

/// Through a temporary file, so a crash never leaves half of one.
fn write(path: &Path, saved: &HashMap<String, Saved>) -> std::io::Result<()> {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d)?;
    }
    let tmp = path.with_extension("bin.tmp");
    std::fs::write(&tmp, encode(saved))?;
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(root: &Path, files: &[&str]) {
        for f in files {
            let p = root.join(f);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, b"x").unwrap();
        }
    }

    fn dump(v: &Vfs) -> Vec<Vec<(String, PathBuf)>> {
        v.roots
            .iter()
            .map(|r| {
                let mut e: Vec<(String, PathBuf)> = r.index().iter().map(|(k, p)| (k.clone(), p.clone())).collect();
                e.sort();
                e
            })
            .collect()
    }

    /// Settles the folders' times into the past, so the walk does not refuse to keep them
    /// for having moved during it (the files were written a moment ago).
    fn age(root: &Path) {
        std::thread::sleep(std::time::Duration::from_millis(20));
        let _ = root;
    }

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("wg_vfsindex_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_saved_index_is_the_walk_and_is_reused_until_a_folder_changes() {
        let base = scratch("reuse");
        let (a, b) = (base.join("ModA"), base.join("ModB"));
        tree(&a, &["Meshes/Rock.NIF", "Textures/tx_Rock.dds", "ModA.esp"]);
        tree(&b, &["meshes/rock.nif", "Textures/Sub/Deep/tx_b.dds"]);
        age(&base);
        let dirs = vec![("ModA".to_string(), a.clone()), ("ModB".to_string(), b.clone())];
        let cache = base.join("cache").join(FILE);

        let (v1, s1) = overlay(&dirs, Some(&cache));
        assert_eq!(s1, Stats { reused: 0, walked: 2, saved: true });
        assert_eq!(dump(&v1), dump(&Vfs::with_roots(&dirs)), "the first connect is the walk");

        let (v2, s2) = overlay(&dirs, Some(&cache));
        assert_eq!(s2, Stats { reused: 2, walked: 0, saved: false });
        assert_eq!(dump(&v2), dump(&v1), "the saved index answers as the walk did");
        assert_eq!(v2.resolve("meshes/rock.nif"), v1.resolve("meshes/rock.nif"), "priority kept");

        // A file added deep in ModB moves only that folder's time: ModB alone is walked.
        std::thread::sleep(std::time::Duration::from_millis(20));
        tree(&b, &["Textures/Sub/Deep/tx_new.dds"]);
        age(&base);
        let (v3, s3) = overlay(&dirs, Some(&cache));
        assert_eq!(s3, Stats { reused: 1, walked: 1, saved: true });
        assert!(v3.resolve("textures/sub/deep/tx_new.dds").is_some());
        assert_eq!(dump(&v3), dump(&Vfs::with_roots(&dirs)));

        // A file removed is gone from the overlay too.
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::remove_file(a.join("Textures/tx_Rock.dds")).unwrap();
        age(&base);
        let (v4, s4) = overlay(&dirs, Some(&cache));
        assert_eq!((s4.reused, s4.walked), (1, 1));
        assert!(v4.resolve("textures/tx_rock.dds").is_none());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_damaged_or_foreign_file_means_walking() {
        let base = scratch("damaged");
        let a = base.join("ModA");
        tree(&a, &["Meshes/x.nif"]);
        age(&base);
        let dirs = vec![("ModA".to_string(), a.clone())];
        let cache = base.join(FILE);
        for junk in [&b"nonsense"[..], &b"WGVI\x09\x00\x00\x00"[..], &b""[..]] {
            std::fs::write(&cache, junk).unwrap();
            let (v, s) = overlay(&dirs, Some(&cache));
            assert_eq!((s.reused, s.walked), (0, 1));
            assert!(v.resolve("meshes/x.nif").is_some());
        }
        // A file cut short is refused whole, not read as far as it goes.
        let good = std::fs::read(&cache).unwrap();
        assert!(decode(&good).is_some());
        assert!(decode(&good[..good.len() - 1]).is_none());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn keys_that_are_not_their_path_in_lower_case_survive_the_file() {
        let mut saved = HashMap::new();
        saved.insert(
            "C:/Mods/Трава".to_string(),
            Saved {
                dirs: vec![("C:/Mods/Трава".to_string(), 1_700_000_000, 5)],
                files: vec![
                    ("meshes/трава/трава.nif".to_string(), "Meshes/Трава/Трава.nif".to_string()),
                    ("meshes/plain.nif".to_string(), "Meshes/Plain.nif".to_string()),
                ],
            },
        );
        assert_eq!(decode(&encode(&saved)), Some(saved));
    }
}
