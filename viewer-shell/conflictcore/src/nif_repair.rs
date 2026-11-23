//! A load order's meshes checked for the faults exporters are known to write, and the
//! ones that can be mended written as repaired copies.
//!
//! The meshes are the optimizer's ([`nif_batch::collect`]): the winning copy of each
//! `meshes/` path across the data folders, loose files only. Each is put through
//! `gardenfell_nif::netimmerse::repair`, which finds a mesh sound, mendable (every fix
//! proven to keep what the mesh means), mendable only by a guess (a node whose lists are
//! garbage, emptied), or unreadable for a reason no repair covers (another NIF version,
//! a block type outside Morrowind's format, not a NIF at all). Nothing is written into a
//! mod: the repaired copies go to one output folder, laid out as a data folder, which the
//! person adds last in the load order, as the optimizer's does. A guess is written only
//! when asked for.
//!
//! What was found in a mesh is kept in the scan cache (tag [`TAG`], valid while the
//! file's size and time are unchanged), so a second run reads only what changed.

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use editcore::cache::Cache;
use gardenfell_nif::netimmerse::repair::{Kind, repair};
use serde_json::{Value, json};

use crate::nif_batch::{Candidate, DONE, TOTAL, Written};
use crate::vfs::par_map;

/// The cache tag of a mesh's check.
pub const TAG: &str = "~WGNR1";
const VERSION: u64 = 1;

/// Where a mesh stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Status {
    /// Reads, and nothing to mend.
    Sound,
    /// Every fault found is mended, each proven to keep what the mesh means.
    Mendable,
    /// Mendable only by emptying a node whose lists do not read.
    Guess,
    /// Does not read, and no repair covers why.
    Unreadable,
}

impl Status {
    pub fn code(self) -> u64 {
        self as u64
    }
    pub fn label(self) -> &'static str {
        match self {
            Status::Sound => "sound",
            Status::Mendable => "mendable",
            Status::Guess => "mendable by a guess",
            Status::Unreadable => "unreadable",
        }
    }
}

/// One fault found, as listed.
#[derive(Clone, Debug, PartialEq)]
pub struct Fault {
    pub label: String,
    pub certain: bool,
    pub block: u64,
    pub name: String,
    pub note: String,
}

/// What was found in one mesh.
#[derive(Clone, Debug, PartialEq)]
pub struct Found {
    pub status: Status,
    pub bytes: u64,
    pub faults: Vec<Fault>,
    /// Why it does not read, when it is unreadable.
    pub why: String,
}

impl Found {
    fn to_json(&self) -> Value {
        json!({"v": VERSION, "status": self.status.code(), "bytes": self.bytes, "why": self.why,
            "faults": self.faults.iter().map(|f| json!({"label": f.label, "certain": f.certain, "block": f.block,
                "name": f.name, "note": f.note})).collect::<Vec<_>>()})
    }

    fn from_json(v: &Value) -> Option<Found> {
        if v["v"].as_u64()? != VERSION {
            return None;
        }
        let status = match v["status"].as_u64()? {
            0 => Status::Sound,
            1 => Status::Mendable,
            2 => Status::Guess,
            3 => Status::Unreadable,
            _ => return None,
        };
        let faults = v["faults"]
            .as_array()?
            .iter()
            .map(|f| {
                Some(Fault {
                    label: f["label"].as_str()?.to_string(),
                    certain: f["certain"].as_bool()?,
                    block: f["block"].as_u64()?,
                    name: f["name"].as_str()?.to_string(),
                    note: f["note"].as_str()?.to_string(),
                })
            })
            .collect::<Option<Vec<_>>>()?;
        Some(Found { status, bytes: v["bytes"].as_u64()?, faults, why: v["why"].as_str()?.to_string() })
    }
}

/// A mesh checked: what [`repair`] makes of its bytes, and the repaired bytes when there are
/// any.
pub fn check_bytes(bytes: &[u8]) -> (Found, Option<Vec<u8>>) {
    let r = std::panic::catch_unwind(|| repair(bytes));
    let Ok(r) = r else {
        let found = Found {
            status: Status::Unreadable,
            bytes: bytes.len() as u64,
            faults: Vec::new(),
            why: "the check failed on this mesh".into(),
        };
        return (found, None);
    };
    let faults = r
        .fixes
        .iter()
        .map(|f| Fault {
            label: f.kind.label().to_string(),
            certain: f.kind.certain(),
            block: f.block as u64,
            name: f.name.clone(),
            note: f.note.clone(),
        })
        .collect();
    let status = if r.clean() {
        Status::Sound
    } else if r.bytes.is_none() {
        Status::Unreadable
    } else if r.fixes.iter().any(|f| f.kind == Kind::BrokenNode) {
        Status::Guess
    } else {
        Status::Mendable
    };
    let why = r.left.clone().unwrap_or_default();
    (Found { status, bytes: bytes.len() as u64, faults, why }, r.bytes)
}

/// The bytes a mesh should be read from by tools that rewrite it: repaired, when every
/// fault found is mended and proven ([`Status::Mendable`]); otherwise as they are. The
/// optimizer reads through this, so a mesh with its UV flag off is optimized too.
pub fn mended(bytes: Vec<u8>) -> (Vec<u8>, Vec<String>) {
    match check_bytes(&bytes) {
        (f, Some(fixed)) if f.status == Status::Mendable => {
            (fixed, f.faults.iter().map(|x| format!("block {} ({}): {}", x.block, x.name, x.note)).collect())
        }
        _ => (bytes, Vec::new()),
    }
}

/// A candidate checked; from the cache when it holds the mesh as it is now.
pub fn check(cache: Option<&Cache>, c: &Candidate) -> Found {
    // A packed mesh's result is kept with its archive's (`check_all`), not under the archive's path.
    let cache = cache.filter(|_| c.packed.is_none());
    let kept = cache
        .and_then(|cc| cc.get(&c.path, TAG))
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .and_then(|v| Found::from_json(&v));
    if let Some(f) = kept {
        return f;
    }
    let bytes = match c.read() {
        Ok(b) => b,
        Err(e) => return Found { status: Status::Unreadable, bytes: 0, faults: Vec::new(), why: e.to_string() },
    };
    let (f, _) = check_bytes(&bytes);
    if let Some(cc) = cache {
        cc.put(&c.path, TAG, f.to_json().to_string().as_bytes(), None);
    }
    f
}

/// Every candidate checked, on a few threads, in the candidates' order. The optimizer's
/// [`DONE`] counts; a packed mesh's result is kept with its archive's
/// (`nif_batch::run_cached`).
pub fn check_all(cache: Option<&Cache>, cands: &[Candidate]) -> Vec<Found> {
    crate::nif_batch::run_cached(cache, cands, TAG, |c| check(cache, c), |c| check(None, c), Found::to_json, Found::from_json)
}

/// Whether a checked mesh is written: mendable, or a guess when guesses are asked for.
pub fn writes(f: &Found, guesses: bool) -> bool {
    f.status == Status::Mendable || (guesses && f.status == Status::Guess)
}

/// One mesh read again, repaired, and written under `out_dir` at its own path.
pub fn write_one(c: &Candidate, out_dir: &Path, guesses: bool) -> Written {
    let mut w = Written { rel: c.rel.clone(), from: PathBuf::from(c.from()), before: 0, after: 0, error: String::new() };
    let bytes = match c.read() {
        Ok(b) => b,
        Err(e) => {
            w.error = e.to_string();
            return w;
        }
    };
    w.before = bytes.len() as u64;
    let (found, fixed) = check_bytes(&bytes);
    let Some(fixed) = fixed.filter(|_| writes(&found, guesses)) else {
        w.error = match found.status {
            Status::Sound => "nothing to mend".into(),
            Status::Guess => "mendable only by a guess, and guesses were not asked for".into(),
            _ => found.why,
        };
        return w;
    };
    let dest = match crate::pathsafe::join_inside(out_dir, &c.rel) {
        Ok(d) => d,
        Err(e) => {
            w.error = e;
            return w;
        }
    };
    if let Some(parent) = dest.parent()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        w.error = e.to_string();
        return w;
    }
    match std::fs::write(&dest, &fixed) {
        Ok(()) => w.after = fixed.len() as u64,
        Err(e) => w.error = e.to_string(),
    }
    w
}

/// The candidates written, on a few threads, in their order. [`DONE`] counts.
pub fn write_all(cands: &[&Candidate], out_dir: &Path, guesses: bool) -> Vec<Written> {
    DONE.store(0, Ordering::Relaxed);
    TOTAL.store(cands.len(), Ordering::Relaxed);
    par_map(cands, |c| {
        let w = write_one(c, out_dir, guesses);
        DONE.fetch_add(1, Ordering::Relaxed);
        w
    })
}

/// A record of a run, written beside the meshes: what came from where, and what was mended.
pub fn manifest(written: &[Written], found: &[(&Candidate, &Found)], guesses: bool) -> Value {
    let faults_of = |rel: &str| -> Vec<Value> {
        found
            .iter()
            .find(|(c, _)| c.rel == rel)
            .map(|(_, f)| {
                f.faults
                    .iter()
                    .map(|x| json!({"fault": x.label, "certain": x.certain, "block": x.block, "type": x.name, "note": x.note}))
                    .collect()
            })
            .unwrap_or_default()
    };
    json!({
        "tool": "Wraithguard check meshes",
        "guesses": guesses,
        "meshes": written.iter().filter(|w| w.error.is_empty()).map(|w| json!({
            "path": w.rel, "from": w.from.to_string_lossy(), "bytes_before": w.before, "bytes_after": w.after,
            "faults": faults_of(&w.rel),
        })).collect::<Vec<_>>(),
        "failed": written.iter().filter(|w| !w.error.is_empty()).map(|w| json!({"path": w.rel, "why": w.error})).collect::<Vec<_>>(),
    })
}

/// The file [`manifest`] is written to, beside the meshes.
pub const MANIFEST: &str = "wraithguard-check-meshes.json";

/// [`manifest`] written into `out_dir` as [`MANIFEST`].
pub fn write_manifest(out_dir: &Path, written: &[Written], found: &[(&Candidate, &Found)], guesses: bool) -> Result<(), String> {
    let text = serde_json::to_string_pretty(&manifest(written, found, guesses)).map_err(|e| e.to_string())?;
    std::fs::write(out_dir.join(MANIFEST), text).map_err(|e| format!("the record could not be written: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nif_batch::collect;
    use std::path::PathBuf;

    const FIX: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures/gardenfell_anim/");

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("wg_repair_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// The first geometry block's `Has UV` word, in a mesh that has UV sets.
    fn uv_word(b: &[u8]) -> Option<usize> {
        let w = gardenfell_nif::netimmerse::stream::walk(b).ok()?;
        for s in &w.spans {
            if s.name != "NiTriShapeData" {
                continue;
            }
            let mut p = s.start + 4 + s.name.len();
            let rd = |p: usize, n: usize| -> u32 {
                let mut v = [0u8; 4];
                v[..n].copy_from_slice(&b[p..p + n]);
                u32::from_le_bytes(v)
            };
            let nv = rd(p, 2) as usize;
            p += 2;
            for size in [12, 12] {
                let has = rd(p, 4) != 0;
                p += 4 + if has { size * nv } else { 0 };
            }
            p += 16;
            let has = rd(p, 4) != 0;
            p += 4 + if has { 16 * nv } else { 0 };
            if nv > 0 && rd(p, 2) > 0 {
                return Some(p + 2);
            }
        }
        None
    }

    #[test]
    fn a_uv_flag_off_is_found_cached_and_written_mended() {
        let root = tmp("uv");
        let data = root.join("data");
        let good = std::fs::read(format!("{FIX}uvsets.nif")).unwrap();
        let at = uv_word(&good).expect("a shape with UV sets");
        let mut bad = good.clone();
        bad[at..at + 4].copy_from_slice(&0u32.to_le_bytes());
        for (rel, b) in [("meshes/a/bad.nif", &bad), ("meshes/a/good.nif", &good)] {
            let p = data.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, b).unwrap();
        }
        let cands = collect(&[data]);
        let cache = Cache::new(root.join("c.sqlite3"), "t", 1 << 20);
        let found = check_all(Some(&cache), &cands);
        assert_eq!(found, check_all(Some(&cache), &cands));
        let by = |name: &str| &found[cands.iter().position(|c| c.rel.ends_with(name)).unwrap()];
        assert_eq!(by("good.nif").status, Status::Sound);
        assert_eq!(by("bad.nif").status, Status::Mendable);
        assert_eq!(by("bad.nif").faults[0].label, Kind::UvFlag.label());
        let picked: Vec<&Candidate> = cands.iter().zip(&found).filter(|(_, f)| writes(f, false)).map(|(c, _)| c).collect();
        assert_eq!(picked.len(), 1);
        let out = root.join("out");
        let done = write_all(&picked, &out, false);
        assert!(done[0].error.is_empty(), "{}", done[0].error);
        assert_eq!(std::fs::read(out.join(&done[0].rel)).unwrap(), good);
        // And what the optimizer reads is the mended mesh.
        assert_eq!(mended(bad.clone()).0, good);
        assert_eq!(mended(good.clone()), (good, Vec::new()));
        cache.close();
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A version-0x100 archive with `files` (backslash names), laid out as Morrowind's are
    /// (`gardenfell-bsa`'s own test builder).
    fn archive(files: &[(&str, &[u8])]) -> Vec<u8> {
        let n = files.len();
        let mut names = Vec::new();
        let mut name_offs = Vec::new();
        for (name, _) in files {
            name_offs.push(names.len() as u32);
            names.extend_from_slice(name.as_bytes());
            names.push(0);
        }
        let hash_off = (8 * n + 4 * n + names.len()) as u32;
        let mut out = Vec::new();
        for v in [0x100u32, hash_off, n as u32] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        let mut off = 0u32;
        for (_, data) in files {
            out.extend_from_slice(&(data.len() as u32).to_le_bytes());
            out.extend_from_slice(&off.to_le_bytes());
            off += data.len() as u32;
        }
        for o in &name_offs {
            out.extend_from_slice(&o.to_le_bytes());
        }
        out.extend_from_slice(&names);
        out.extend(std::iter::repeat_n(0u8, 8 * n));
        for (_, data) in files {
            out.extend_from_slice(data);
        }
        out
    }

    #[test]
    fn meshes_in_archives_are_checked_behind_loose_files_and_written_loose() {
        use crate::nif_batch::collect_all;
        let root = tmp("bsa");
        let (a, b) = (root.join("a"), root.join("b"));
        let good = std::fs::read(format!("{FIX}uvsets.nif")).unwrap();
        let at = uv_word(&good).unwrap();
        let mut bad = good.clone();
        bad[at..at + 4].copy_from_slice(&0u32.to_le_bytes());
        // First.bsa in a: a broken mesh, one a later archive replaces, and one a loose file does.
        std::fs::create_dir_all(&a).unwrap();
        std::fs::write(a.join("First.bsa"), archive(&[("meshes\\x\\bad.nif", &bad), ("meshes\\x\\later.nif", &good), ("meshes\\x\\loose.nif", &bad)])).unwrap();
        // Second.bsa in b, later in the load order: its broken copy of later.nif wins.
        std::fs::create_dir_all(&b).unwrap();
        std::fs::write(b.join("second.BSA"), archive(&[("meshes\\x\\later.nif", &bad)])).unwrap();
        // A loose loose.nif in a, sound: it wins over First.bsa's.
        std::fs::create_dir_all(a.join("meshes/x")).unwrap();
        std::fs::write(a.join("meshes/x/loose.nif"), &good).unwrap();

        let names = ["First.bsa".to_string(), "Second.bsa".to_string(), "Gone.bsa".to_string()];
        let (cands, warnings) = collect_all(&[a.clone(), b.clone()], &names);
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(warnings[0].starts_with("Gone.bsa"));
        let by = |n: &str| cands.iter().find(|c| c.rel.ends_with(n)).unwrap();
        assert_eq!(cands.len(), 3);
        assert_eq!(by("bad.nif").archive_name().as_deref(), Some("First.bsa"));
        assert_eq!(by("later.nif").archive_name().as_deref(), Some("second.BSA"));
        assert!(by("loose.nif").packed.is_none());

        let cache = Cache::new(root.join("c.sqlite3"), "t", 1 << 20);
        let found = check_all(Some(&cache), &cands);
        // From the cache the second time: one entry an archive, the same answers.
        assert_eq!(found, check_all(Some(&cache), &cands));
        assert!(cache.get(&a.join("First.bsa"), TAG).is_some());
        let status = |n: &str| found[cands.iter().position(|c| c.rel.ends_with(n)).unwrap()].status;
        assert_eq!(status("bad.nif"), Status::Mendable);
        assert_eq!(status("later.nif"), Status::Mendable);
        assert_eq!(status("loose.nif"), Status::Sound);

        let picked: Vec<&Candidate> = cands.iter().zip(&found).filter(|(_, f)| writes(f, false)).map(|(c, _)| c).collect();
        let out = root.join("out");
        let done = write_all(&picked, &out, false);
        assert_eq!(done.len(), 2);
        for w in &done {
            assert!(w.error.is_empty(), "{}", w.error);
            assert!(w.from.to_string_lossy().to_lowercase().contains(".bsa::meshes/x/"), "{}", w.from.display());
            assert_eq!(std::fs::read(out.join(&w.rel)).unwrap(), good, "{}", w.rel);
        }
        cache.close();
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The audit's PoC (F1): an archive entry `meshes\..\..\escaped.nif`. It is not offered, says so,
    /// and nothing is written outside the output folder - and a candidate that does carry such a name
    /// (the writers do not trust their callers) is refused at the write.
    #[test]
    fn an_archive_entry_with_dot_dot_cannot_write_outside_the_output_folder() {
        use crate::nif_batch::collect_all;
        let root = tmp("traversal");
        let a = root.join("a");
        let good = std::fs::read(format!("{FIX}uvsets.nif")).unwrap();
        let at = uv_word(&good).unwrap();
        let mut bad = good.clone();
        bad[at..at + 4].copy_from_slice(&0u32.to_le_bytes());
        std::fs::create_dir_all(&a).unwrap();
        std::fs::write(a.join("Evil.bsa"), archive(&[("meshes\\..\\..\\escaped.nif", &bad), ("meshes\\x\\fine.nif", &bad)])).unwrap();
        let (cands, warnings) = collect_all(std::slice::from_ref(&a), &["Evil.bsa".to_string()]);
        assert_eq!(cands.len(), 1, "only the honest entry is offered");
        assert!(cands[0].rel.ends_with("fine.nif"));
        assert!(warnings.iter().any(|w| w.contains("not a safe path") && w.contains("escaped.nif")), "{warnings:?}");
        // Offered by hand all the same, the write refuses it.
        let mut hostile = cands[0].clone();
        hostile.rel = "meshes/../../escaped.nif".to_string();
        let out = root.join("out").join("inner");
        let w = write_one(&hostile, &out, false);
        assert!(w.error.contains("not a safe relative path"), "{}", w.error);
        assert!(!root.join("out").join("escaped.nif").exists() && !root.join("escaped.nif").exists());
        let _ = std::fs::remove_dir_all(&root);
    }
}
