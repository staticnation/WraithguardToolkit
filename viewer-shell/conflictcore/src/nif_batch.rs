//! A load order's meshes put in draw order in one go, only where it pays.
//!
//! The mesh viewer's Optimize (`nif_order`) does one mesh; this runs it over every mesh the
//! game would load: the winning copy of each `meshes/` path across the data folders
//! (the last folder wins, case folded, as OpenMW's VFS), and with [`collect_all`] the
//! meshes in the load order's archives that no loose file overrides (a later archive over
//! an earlier one). Nothing is written into a mod. The copies go to one output folder,
//! laid out as a data folder, which the person adds last in the load order (or does not):
//! a mesh from an archive is written there as a loose file, which wins over the archive.
//!
//! "Only what improves" is two checks. Per shape, `nif_order` already leaves a shape
//! alone unless its vertex cache, fetch or overdraw really comes out better. Per mesh, the
//! gain - how much better the mesh draws, weighted by triangles - must reach a threshold,
//! and the mesh must have enough triangles to matter, or it is not rewritten at all (a
//! file nobody would notice the difference in is not worth a second copy in the load
//! order). Every written mesh is read back and must mean the same mesh as the original
//! (`nif_order::optimise_checked`).
//!
//! What was measured of a mesh is kept in the scan cache (tag [`TAG`], valid while the
//! file's size and time are unchanged), so a second run measures only what changed, and
//! the thresholds can be changed without measuring again.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gardenfell_bsa::bsa::Bsa;
use std::sync::atomic::{AtomicUsize, Ordering};

use editcore::cache::Cache;
use serde_json::{Value, json};

use crate::nif_order;
use crate::vfs::par_map;

/// The cache tag of a mesh's measurement.
pub const TAG: &str = "~WGMO1";
const VERSION: u64 = 2;

/// How far a run has got (meshes done of meshes to do), for a progress readout.
pub static DONE: AtomicUsize = AtomicUsize::new(0);
pub static TOTAL: AtomicUsize = AtomicUsize::new(0);

/// What makes a mesh worth rewriting.
#[derive(Clone, Debug)]
pub struct Options {
    /// The least gain (0.03 = 3 %): triangle-weighted, per shape the best of the vertex
    /// cache's and the overdraw's relative gain, or a quarter of the vertex fetch's.
    pub min_gain: f64,
    /// Meshes with fewer triangles than this are left alone.
    pub min_triangles: u64,
    /// Only meshes whose winning data folder's path has this text (any case); empty: all.
    pub folder: String,
}

impl Default for Options {
    fn default() -> Self {
        Options { min_gain: 0.03, min_triangles: 64, folder: String::new() }
    }
}

impl Options {
    /// `{"minGain": percent, "minTriangles", "folder"}` as the page sends it; what is
    /// missing or not a number is the default.
    pub fn from_json(v: &Value) -> Options {
        let d = Options::default();
        Options {
            min_gain: v["minGain"].as_f64().map(|p| (p / 100.0).max(0.0)).unwrap_or(d.min_gain),
            min_triangles: v["minTriangles"].as_u64().unwrap_or(d.min_triangles),
            folder: v["folder"].as_str().unwrap_or("").trim().to_lowercase(),
        }
    }
}

/// A mesh the load order provides: the file that wins its path.
#[derive(Clone, Debug)]
pub struct Candidate {
    /// The path as the winning folder (or archive) has it, `/`-separated, from `meshes`: it
    /// is also the path under the output folder.
    pub rel: String,
    /// The data folder it comes from (for a packed mesh, the one holding the archive).
    pub dir: PathBuf,
    /// The loose file, or for a packed mesh the archive.
    pub path: PathBuf,
    /// For a mesh inside an archive: the archive and the entry's key.
    pub packed: Option<Packed>,
}

/// A mesh's place inside an archive.
#[derive(Clone)]
pub struct Packed {
    pub archive: Arc<Bsa>,
    /// The entry's key, normalized as the archive indexes it.
    pub key: String,
}

impl std::fmt::Debug for Packed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}::{}", self.archive.path.display(), self.key)
    }
}

impl Candidate {
    /// The mesh's bytes, from its file or its archive.
    pub fn read(&self) -> std::io::Result<Vec<u8>> {
        match &self.packed {
            None => std::fs::read(&self.path),
            Some(p) => p.archive.read(&p.key).ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::NotFound, format!("{} is not in {}", p.key, p.archive.path.display()))
            }),
        }
    }

    /// Where it comes from, in words: the file, or `archive.bsa::entry`.
    pub fn from(&self) -> String {
        match &self.packed {
            None => self.path.to_string_lossy().into_owned(),
            Some(p) => format!("{}::{}", p.archive.path.to_string_lossy(), self.rel),
        }
    }

    /// The folder a listing shows it under: its data folder, or for a packed mesh the archive
    /// (so the last part of the path is the archive's name).
    pub fn shown_from(&self) -> String {
        if self.packed.is_some() { &self.path } else { &self.dir }.to_string_lossy().into_owned()
    }

    /// The archive's file name, for a packed mesh.
    pub fn archive_name(&self) -> Option<String> {
        self.packed.as_ref().and_then(|p| p.archive.path.file_name()).map(|n| n.to_string_lossy().into_owned())
    }
}

/// The `.nif` files of the data folders' `meshes` folders (lowest priority first, as the
/// cfg gives them), one per path: a later folder's file replaces an earlier one's, paths
/// compared with the case folded. In path order.
pub fn collect(data: &[PathBuf]) -> Vec<Candidate> {
    let mut won: std::collections::HashMap<String, Candidate> = std::collections::HashMap::new();
    for dir in data {
        let Ok(rd) = std::fs::read_dir(dir) else { continue };
        let Some(meshes) = rd.flatten().find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case("meshes") && e.path().is_dir()) else {
            continue;
        };
        let top = meshes.file_name().to_string_lossy().into_owned();
        let mut stack = vec![(meshes.path(), top)];
        while let Some((p, rel)) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(&p) else { continue };
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                let Ok(ft) = e.file_type() else { continue };
                let path = e.path();
                let rel2 = format!("{rel}/{name}");
                if ft.is_dir() || (ft.is_symlink() && path.is_dir()) {
                    stack.push((path, rel2));
                } else if (ft.is_file() || ft.is_symlink()) && name.to_lowercase().ends_with(".nif") {
                    won.insert(rel2.to_lowercase(), Candidate { rel: rel2, dir: dir.clone(), path, packed: None });
                }
            }
        }
    }
    let mut v: Vec<Candidate> = won.into_values().collect();
    v.sort_by_key(|c| c.rel.to_lowercase());
    v
}

/// An archive the load order names, found as the game finds it: in the last data folder
/// that has a file of that name (any case), with the folder.
pub fn find_archive(data: &[PathBuf], name: &str) -> Option<(PathBuf, PathBuf)> {
    let want = name.trim().to_lowercase();
    data.iter().rev().find_map(|dir| {
        std::fs::read_dir(dir)
            .ok()?
            .flatten()
            .find(|e| e.file_name().to_string_lossy().to_lowercase() == want && e.path().is_file())
            .map(|e| (e.path(), dir.clone()))
    })
}

/// [`collect`], and the meshes of the `archives` (as the load order names them, in its
/// order) that no loose file overrides: a later archive's entry over an earlier one's, as
/// OpenMW and the viewer's overlay resolve them. An archive that is missing or does not
/// open is named in the second value and left out.
pub fn collect_all(data: &[PathBuf], archives: &[String]) -> (Vec<Candidate>, Vec<String>) {
    let mut out = collect(data);
    let loose: std::collections::HashSet<String> = out.iter().map(|c| c.rel.to_lowercase()).collect();
    let mut warnings = Vec::new();
    let mut packed: std::collections::HashMap<String, Candidate> = std::collections::HashMap::new();
    for name in archives {
        let Some((path, dir)) = find_archive(data, name) else {
            warnings.push(format!("{name}: not found in the data folders"));
            continue;
        };
        let bsa = match Bsa::open(&path) {
            Ok(b) => Arc::new(b),
            Err(e) => {
                warnings.push(format!("{name}: {e}"));
                continue;
            }
        };
        for entry in bsa.names() {
            let rel = entry.replace('\\', "/");
            let low = rel.to_lowercase();
            if !low.starts_with("meshes/") || !low.ends_with(".nif") || loose.contains(&low) {
                continue;
            }
            let key = gardenfell_bytes::paths::norm(entry);
            let c = Candidate { rel, dir: dir.clone(), path: path.clone(), packed: Some(Packed { archive: bsa.clone(), key }) };
            packed.insert(low, c);
        }
    }
    out.extend(packed.into_values());
    out.sort_by_key(|c| c.rel.to_lowercase());
    (out, warnings)
}

/// The candidates `opts.folder` allows.
pub fn filter(cands: Vec<Candidate>, opts: &Options) -> Vec<Candidate> {
    if opts.folder.is_empty() {
        return cands;
    }
    let folder = opts.folder.to_lowercase();
    cands.into_iter().filter(|c| c.dir.to_string_lossy().to_lowercase().contains(&folder)).collect()
}

/// What was measured of one mesh (the thresholds are applied to it later, [`qualifies`]).
#[derive(Clone, Debug, Default)]
pub struct Analysis {
    /// Why the mesh could not be measured; empty when it was.
    pub unreadable: String,
    pub bytes: u64,
    pub triangles: u64,
    /// Geometry data blocks, and those a pass would move (the rest are left as they are:
    /// already in good order, or skinned by a partition, or shared).
    pub shapes: u32,
    pub improvable: u32,
    pub improvable_triangles: u64,
    /// Over the whole mesh; see [`Options::min_gain`].
    pub gain: f64,
    /// The improvable shapes' figures, triangle-weighted: `[as the file has it, as it
    /// would be]`.
    pub acmr: [f64; 2],
    pub overdraw: [f64; 2],
    pub overfetch: [f64; 2],
    /// Why the other shapes are left alone.
    pub skipped: Vec<String>,
    /// The faults mended before measuring (`nif_repair::mended`): the mesh is optimized
    /// from the repaired copy, and the written copy is repaired too.
    pub repaired: Vec<String>,
}

impl Analysis {
    pub(crate) fn to_json(&self) -> Value {
        json!({"v": VERSION, "unreadable": self.unreadable, "bytes": self.bytes, "triangles": self.triangles,
            "shapes": self.shapes, "improvable": self.improvable, "improvable_triangles": self.improvable_triangles,
            "gain": self.gain, "acmr": self.acmr, "overdraw": self.overdraw, "overfetch": self.overfetch,
            "skipped": self.skipped, "repaired": self.repaired})
    }

    pub(crate) fn from_json(v: &Value) -> Option<Analysis> {
        if v["v"].as_u64()? != VERSION {
            return None;
        }
        let pair = |k: &str| -> Option<[f64; 2]> {
            let a = v[k].as_array()?;
            Some([a.first()?.as_f64()?, a.get(1)?.as_f64()?])
        };
        Some(Analysis {
            unreadable: v["unreadable"].as_str()?.to_string(),
            bytes: v["bytes"].as_u64()?,
            triangles: v["triangles"].as_u64()?,
            shapes: v["shapes"].as_u64()? as u32,
            improvable: v["improvable"].as_u64()? as u32,
            improvable_triangles: v["improvable_triangles"].as_u64()?,
            gain: v["gain"].as_f64()?,
            acmr: pair("acmr")?,
            overdraw: pair("overdraw")?,
            overfetch: pair("overfetch")?,
            skipped: v["skipped"].as_array()?.iter().filter_map(|x| x.as_str().map(String::from)).collect(),
            repaired: v["repaired"].as_array()?.iter().filter_map(|x| x.as_str().map(String::from)).collect(),
        })
    }
}

/// A mesh measured: `nif_order`'s dry run, summed over its shapes, on the mesh with the
/// faults mended that `nif_repair` proves (a UV flag left off, say), which it notes.
pub fn analyse_bytes(bytes: &[u8]) -> Analysis {
    let mut a = Analysis { bytes: bytes.len() as u64, ..Default::default() };
    let (bytes, repaired) = crate::nif_repair::mended(bytes.to_vec());
    a.repaired = repaired;
    let rep = match nif_order::report_bytes(&bytes) {
        Ok(r) => r,
        Err(e) => {
            a.unreadable = e;
            return a;
        }
    };
    let f = |v: &Value, k: &str| v[k].as_f64().unwrap_or(0.0);
    let mut reasons = BTreeSet::new();
    let (mut sb, mut sa) = ([0.0f64; 3], [0.0f64; 3]);
    let mut gain_sum = 0.0;
    for e in &rep {
        let tris = e["triangles"].as_u64().unwrap_or(0);
        a.shapes += 1;
        a.triangles += tris;
        if let Some(why) = e["skipped"].as_str() {
            reasons.insert(why.to_string());
            continue;
        }
        let (Some(b), Some(af)) = (e.get("before"), e.get("after")) else { continue };
        let w = tris as f64;
        a.improvable += 1;
        a.improvable_triangles += tris;
        for (i, k) in ["acmr", "overdraw", "overfetch"].iter().enumerate() {
            sb[i] += w * f(b, k);
            sa[i] += w * f(af, k);
        }
        let rel = |k: &str| {
            let (x, y) = (f(b, k), f(af, k));
            if x > 1e-9 { ((x - y) / x).max(0.0) } else { 0.0 }
        };
        gain_sum += w * rel("acmr").max(rel("overdraw")).max(0.25 * rel("overfetch"));
    }
    if a.triangles > 0 {
        a.gain = gain_sum / a.triangles as f64;
    }
    if a.improvable_triangles > 0 {
        let t = a.improvable_triangles as f64;
        a.acmr = [sb[0] / t, sa[0] / t];
        a.overdraw = [sb[1] / t, sa[1] / t];
        a.overfetch = [sb[2] / t, sa[2] / t];
    }
    a.skipped = reasons.into_iter().collect();
    a
}

/// A candidate measured; from the cache when it holds the mesh as it is now.
pub fn analyse(cache: Option<&Cache>, c: &Candidate) -> Analysis {
    // A packed mesh's result is kept with its archive's (`analyse_all`), not under the archive's path.
    let cache = cache.filter(|_| c.packed.is_none());
    let kept = cache
        .and_then(|cc| cc.get(&c.path, TAG))
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .and_then(|v| Analysis::from_json(&v));
    if let Some(a) = kept {
        return a;
    }
    let bytes = match c.read() {
        Ok(b) => b,
        Err(e) => return Analysis { unreadable: e.to_string(), ..Default::default() },
    };
    let a = analyse_bytes(&bytes);
    if let Some(cc) = cache {
        cc.put(&c.path, TAG, a.to_json().to_string().as_bytes(), None);
    }
    a
}

/// Every candidate measured, on a few threads, in the candidates' order. [`DONE`] counts.
pub fn analyse_all(cache: Option<&Cache>, cands: &[Candidate]) -> Vec<Analysis> {
    run_cached(cache, cands, TAG, |c| analyse(cache, c), |c| analyse(None, c), Analysis::to_json, Analysis::from_json)
}

/// `each` over the candidates, on a few threads, in their order, with [`DONE`] counting. A
/// loose mesh caches its own result (`loose`); a packed mesh's results are kept together, one
/// cache entry an archive under `tag` (an object from entry key to result, valid while the
/// archive's size and time are unchanged), read once before the run and written once after,
/// so an archive of thousands of meshes is not thousands of entries. `packed` computes one.
pub(crate) fn run_cached<T: Clone + Send + Sync>(
    cache: Option<&Cache>,
    cands: &[Candidate],
    tag: &str,
    loose: impl Fn(&Candidate) -> T + Sync,
    packed: impl Fn(&Candidate) -> T + Sync,
    to_json: impl Fn(&T) -> Value + Sync,
    from_json: impl Fn(&Value) -> Option<T> + Sync,
) -> Vec<T> {
    use std::collections::HashMap;
    use std::sync::Mutex;
    DONE.store(0, Ordering::Relaxed);
    TOTAL.store(cands.len(), Ordering::Relaxed);
    let mut archives: Vec<PathBuf> = cands.iter().filter(|c| c.packed.is_some()).map(|c| c.path.clone()).collect();
    archives.sort();
    archives.dedup();
    let kept: HashMap<PathBuf, serde_json::Map<String, Value>> = archives
        .iter()
        .map(|a| {
            let m = cache
                .and_then(|cc| cc.get(a, tag))
                .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
                .and_then(|v| v.as_object().cloned())
                .unwrap_or_default();
            (a.clone(), m)
        })
        .collect();
    let fresh: Mutex<HashMap<PathBuf, serde_json::Map<String, Value>>> = Mutex::new(HashMap::new());
    let out = par_map(cands, |c| {
        let r = match &c.packed {
            None => loose(c),
            Some(p) => match kept.get(&c.path).and_then(|m| m.get(&p.key)).and_then(&from_json) {
                Some(r) => r,
                None => {
                    let r = packed(c);
                    if let Ok(mut f) = fresh.lock() {
                        f.entry(c.path.clone()).or_default().insert(p.key.clone(), to_json(&r));
                    }
                    r
                }
            },
        };
        DONE.fetch_add(1, Ordering::Relaxed);
        r
    });
    if let Some(cc) = cache {
        for (archive, new) in fresh.into_inner().unwrap_or_default() {
            let mut all = kept.get(&archive).cloned().unwrap_or_default();
            all.extend(new);
            cc.put(&archive, tag, Value::Object(all).to_string().as_bytes(), None);
        }
    }
    out
}

/// Whether a measured mesh is worth rewriting under `opts`.
pub fn qualifies(a: &Analysis, opts: &Options) -> bool {
    a.unreadable.is_empty() && a.improvable > 0 && a.triangles >= opts.min_triangles && a.gain >= opts.min_gain
}

/// One mesh as the page lists it.
pub fn row_json(c: &Candidate, a: &Analysis, opts: &Options) -> Value {
    let note = if !a.unreadable.is_empty() {
        a.unreadable.clone()
    } else if a.improvable == 0 {
        a.skipped.join("; ")
    } else if a.triangles < opts.min_triangles {
        "too few triangles".to_string()
    } else if a.gain < opts.min_gain {
        "gain below the threshold".to_string()
    } else {
        String::new()
    };
    json!({"path": c.rel, "dir": c.shown_from(), "bytes": a.bytes, "triangles": a.triangles,
        "shapes": a.shapes, "improvable": a.improvable, "gain": a.gain, "acmr": a.acmr, "overdraw": a.overdraw,
        "overfetch": a.overfetch, "qualifies": qualifies(a, opts), "note": note, "repaired": a.repaired})
}

/// What writing one mesh came to.
#[derive(Clone, Debug)]
pub struct Written {
    pub rel: String,
    pub from: PathBuf,
    pub before: u64,
    pub after: u64,
    /// Why it was not written; empty when it was.
    pub error: String,
}

/// One mesh optimized, checked and written under `out_dir` at its own path.
pub fn write_one(c: &Candidate, out_dir: &Path) -> Written {
    let mut w = Written { rel: c.rel.clone(), from: PathBuf::from(c.from()), before: 0, after: 0, error: String::new() };
    let bytes = match c.read() {
        Ok(b) => b,
        Err(e) => {
            w.error = e.to_string();
            return w;
        }
    };
    w.before = bytes.len() as u64;
    let (bytes, _) = crate::nif_repair::mended(bytes);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        nif_order::optimise_checked(&bytes)
    }));
    match result {
        Err(_) => w.error = "the draw order pass panicked on this mesh".into(),
        Ok(Err(e)) => w.error = e,
        Ok(Ok((saved, _))) => {
            let dest = out_dir.join(&c.rel);
            if let Some(parent) = dest.parent()
                && let Err(e) = std::fs::create_dir_all(parent)
            {
                w.error = e.to_string();
                return w;
            }
            match std::fs::write(&dest, &saved) {
                Ok(()) => w.after = saved.len() as u64,
                Err(e) => w.error = e.to_string(),
            }
        }
    }
    w
}

/// The candidates written, on a few threads, in their order. [`DONE`] counts.
pub fn write_all(cands: &[&Candidate], out_dir: &Path) -> Vec<Written> {
    DONE.store(0, Ordering::Relaxed);
    TOTAL.store(cands.len(), Ordering::Relaxed);
    par_map(cands, |c| {
        let w = write_one(c, out_dir);
        DONE.fetch_add(1, Ordering::Relaxed);
        w
    })
}

/// A record of a run, written beside the meshes: what came from where.
pub fn manifest(written: &[Written], opts: &Options) -> Value {
    json!({
        "tool": "Wraithguard mesh optimizer",
        "min_gain_percent": opts.min_gain * 100.0,
        "min_triangles": opts.min_triangles,
        "meshes": written.iter().filter(|w| w.error.is_empty()).map(|w| json!({
            "path": w.rel, "from": w.from.to_string_lossy(), "bytes_before": w.before, "bytes_after": w.after,
        })).collect::<Vec<_>>(),
        "failed": written.iter().filter(|w| !w.error.is_empty()).map(|w| json!({"path": w.rel, "why": w.error})).collect::<Vec<_>>(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIX: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures/gardenfell_anim/");

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("wg_batch_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn put(dir: &Path, rel: &str, fixture: &str) {
        let p = dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::copy(format!("{FIX}{fixture}"), p).unwrap();
    }

    #[test]
    fn the_last_folder_wins_with_the_case_folded() {
        let root = tmp("wins");
        let (a, b) = (root.join("a"), root.join("b"));
        put(&a, "Meshes/x/m.nif", "flap.nif");
        put(&a, "Meshes/x/only_a.nif", "uvsets.nif");
        put(&b, "meshes/X/M.NIF", "morph.nif");
        let got = collect(&[a.clone(), b.clone()]);
        assert_eq!(got.len(), 2);
        let m = got.iter().find(|c| c.rel.to_lowercase() == "meshes/x/m.nif").unwrap();
        assert_eq!(m.dir, b);
        // Match the full folder path so parent-directory names cannot affect the result;
        // uppercase input also checks that the filter itself is case-insensitive.
        let only = filter(got, &Options { folder: a.to_string_lossy().to_uppercase(), ..Options::default() });
        assert_eq!(only.len(), 1);
        assert!(only.iter().all(|c| c.dir == a));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn only_meshes_over_the_threshold_are_written_and_they_mean_the_same() {
        let root = tmp("write");
        let data = root.join("data");
        for f in ["flap.nif", "morph.nif", "uvsets.nif", "anim.nif"] {
            put(&data, &format!("meshes/{f}"), f);
        }
        let cands = collect(&[data]);
        assert_eq!(cands.len(), 4);
        let cache = Cache::new(root.join("c.sqlite3"), "t", 1 << 20);
        let first = analyse_all(Some(&cache), &cands);
        // measured once, then from the cache: the same figures
        let again = analyse_all(Some(&cache), &cands);
        for (x, y) in first.iter().zip(&again) {
            assert_eq!(x.to_json(), y.to_json());
        }
        // a threshold nothing reaches writes nothing
        let strict = Options { min_gain: 10.0, min_triangles: 0, ..Options::default() };
        assert!(first.iter().all(|a| !qualifies(a, &strict)));
        // no threshold: whatever has a shape worth moving is written, and reads back the same
        let open = Options { min_gain: 0.0, min_triangles: 0, ..Options::default() };
        let picked: Vec<&Candidate> = cands.iter().zip(&first).filter(|(_, a)| qualifies(a, &open)).map(|(c, _)| c).collect();
        let out = root.join("out");
        let done = write_all(&picked, &out);
        for w in &done {
            assert!(w.error.is_empty(), "{}: {}", w.rel, w.error);
            let back = std::fs::read(out.join(&w.rel)).unwrap();
            let orig = std::fs::read(&w.from).unwrap();
            let (a, b) = (crate::mesh::load(&orig).unwrap(), crate::mesh::load(&back).unwrap());
            assert_eq!(nif_order::fingerprint(&a), nif_order::fingerprint(&b), "{}", w.rel);
        }
        cache.close();
        let _ = std::fs::remove_dir_all(&root);
    }
}
