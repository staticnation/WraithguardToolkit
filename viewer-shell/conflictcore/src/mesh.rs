//! Mesh conflicts (port step 4.3, slice 6): what a winning `.nif` changes against the ones
//! it overrides (`wraithguard/nif/report.py`, `wraithguard/nif/analysis.py`,
//! `wraithguard_toolkit.analyse_mesh_conflicts`).
//!
//! Only meshes that conflict and differ in bytes are opened, each body once (the cache is
//! keyed on content: mods re-ship identical meshes). A read the crate gives up on is a
//! finding ("could not read"), never an error, and an absence (lost collision, lost
//! animation) is only claimed when both meshes were read whole: a partial read can prove a
//! node is there, never that it is not.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::sync::Mutex;

use tes3::nif::*;

use crate::resources::Resource;
use crate::vfs::par_map;

/// Below this ratio of the winner's triangles to the loser's, the swap is worth saying.
pub const SIMPLER_RATIO: f64 = 0.5;

/// One shape of a mesh.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shape {
    pub name: String,
    pub vertices: usize,
    pub triangles: usize,
}

/// What a mesh contains (`Structure`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Structure {
    pub shapes: Vec<Shape>,
    /// The textures it names, lower case with `/`, each once, in block order.
    pub textures: Vec<String>,
    pub has_collision: bool,
    pub has_animation: bool,
    pub node_count: usize,
    pub blocks_read: usize,
    pub blocks_declared: usize,
    /// Why the read stopped (empty: it did not).
    pub stopped_reason: String,
}

impl Structure {
    /// Every shape's triangles.
    pub fn total_triangles(&self) -> usize {
        self.shapes.iter().map(|s| s.triangles).sum()
    }

    /// Whether only part of the file was read.
    pub fn partial(&self) -> bool {
        !self.stopped_reason.is_empty() || self.blocks_read != self.blocks_declared
    }
}

/// NetImmerse 4.0.0.0 files, which Morrowind loads with 4.0.0.2's layout (the only version
/// the crate accepts), are handed to it as 4.0.0.2.
fn as_4002(bytes: &[u8]) -> std::borrow::Cow<'_, [u8]> {
    const V4000: &[u8; 40] = b"NetImmerse File Format, Version 4.0.0.0\n";
    let text_ok = bytes.len() >= 44 && (&bytes[..40] == V4000 || bytes[..40] == NiStream::HEADER);
    if text_ok && bytes[40..44] == 0x0400_0000u32.to_le_bytes() {
        let mut b = bytes.to_vec();
        b[..40].copy_from_slice(&NiStream::HEADER);
        b[40..44].copy_from_slice(&NiStream::VERSION.to_le_bytes());
        return std::borrow::Cow::Owned(b);
    }
    std::borrow::Cow::Borrowed(bytes)
}

/// A mesh read by the crate (4.0.0.0 taken as 4.0.0.2); a panic in the reader is an error.
pub fn load(bytes: &[u8]) -> std::io::Result<NiStream> {
    let b = as_4002(bytes);
    editcore::guarded(|| NiStream::from_bytes(&b))
}

/// A texture reference lower case, with `/`, trimmed (`normalise_texture`).
pub fn normalise_texture(p: &str) -> String {
    p.trim().replace('\\', "/").to_lowercase()
}

/// What a texture reference names, for comparing (`texture_key`): no leading `/`, no doubled
/// `/`, no `textures/` prefix, no extension (Morrowind loads the `.dds` a `.tga` names).
pub fn texture_key(path: &str) -> String {
    let mut key = normalise_texture(path).trim_start_matches('/').to_string();
    while key.contains("//") {
        key = key.replace("//", "/");
    }
    if let Some(rest) = key.strip_prefix("textures/") {
        key = rest.to_string();
    }
    let last = key.rsplit('/').next().unwrap_or("");
    if last.contains('.')
        && let Some(dot) = key.rfind('.')
    {
        key.truncate(dot);
    }
    key
}

/// The file a texture block names, when it names one (not embedded).
pub fn external_texture(o: &NiType) -> Option<String> {
    match <&NiSourceTexture>::try_from(o) {
        Ok(t) => match &t.source {
            TextureSource::External(s) => Some(s.clone()),
            _ => None,
        },
        Err(()) => None,
    }
}

/// What a mesh contains (`summarise`). A file the crate cannot read comes back with no blocks
/// read and the reason in `stopped_reason`.
pub fn summary(bytes: &[u8]) -> Structure {
    let declared = if bytes.len() >= 48 { u32::from_le_bytes(bytes[44..48].try_into().unwrap()) as usize } else { 0 };
    let stream = match load(bytes) {
        Ok(s) => s,
        Err(e) => return Structure { blocks_declared: declared, stopped_reason: e.to_string(), ..Structure::default() },
    };
    let mut out = Structure { blocks_read: stream.objects.len(), blocks_declared: declared, ..Structure::default() };
    for (_, o) in stream.objects.iter() {
        let t = String::from_utf8_lossy(o.type_name()).into_owned();
        if t == "NiTriShape"
            && let Ok(shape) = <&NiTriShape>::try_from(o)
        {
            let (mut vertices, mut triangles) = (0usize, 0usize);
            if let Some(NiType::NiTriShapeData(d)) = stream.objects.get(shape.geometry_data.key) {
                vertices = d.vertices.len();
                triangles = d.triangles.len();
            }
            out.shapes.push(Shape { name: shape.name.clone(), vertices, triangles });
        }
        if let Some(tex) = external_texture(o)
            && !tex.trim().is_empty()
        {
            let n = normalise_texture(&tex);
            if !out.textures.contains(&n) {
                out.textures.push(n);
            }
        }
        out.has_collision |= t == "RootCollisionNode";
        out.has_animation |= t.ends_with("Controller");
        out.node_count += usize::from(t.ends_with("Node"));
    }
    out
}

/// What the winning mesh changes against the one it overrides (`Difference`).
#[derive(Clone, Debug, PartialEq)]
pub struct Difference {
    /// The winner's triangles over the loser's; `None` when the loser has none.
    pub triangle_ratio: Option<f64>,
    pub lost_collision: bool,
    pub lost_animation: bool,
    /// Textures only the winner names (as written).
    pub added_textures: Vec<String>,
    /// Textures only the loser named.
    pub dropped_textures: Vec<String>,
    /// Either side was read only in part.
    pub unreliable: bool,
}

/// The difference when `winner` replaces `loser` (`compare`).
pub fn compare(loser: &Structure, winner: &Structure) -> Difference {
    let lt = loser.total_triangles();
    let triangle_ratio = (lt > 0).then(|| winner.total_triangles() as f64 / lt as f64);
    let lk: std::collections::HashSet<String> = loser.textures.iter().map(|t| texture_key(t)).collect();
    let wk: std::collections::HashSet<String> = winner.textures.iter().map(|t| texture_key(t)).collect();
    Difference {
        triangle_ratio,
        lost_collision: loser.has_collision && !winner.has_collision,
        lost_animation: loser.has_animation && !winner.has_animation,
        added_textures: winner.textures.iter().filter(|t| !lk.contains(&texture_key(t))).cloned().collect(),
        dropped_textures: loser.textures.iter().filter(|t| !wk.contains(&texture_key(t))).cloned().collect(),
        unreliable: loser.partial() || winner.partial(),
    }
}

/// Two providers of one mesh compared (`MeshFinding`): a difference, or why a side could not
/// be read.
#[derive(Clone, Debug, PartialEq)]
pub struct Finding {
    pub path: String,
    pub difference: Option<Difference>,
    pub unreadable: String,
    pub loser_partial: bool,
    pub winner_partial: bool,
}

impl Finding {
    /// Whether an absence in it can be believed.
    pub fn reliable(&self) -> bool {
        self.difference.is_some() && self.unreadable.is_empty() && !self.loser_partial && !self.winner_partial
    }

    /// Whether it says anything a person needs to see.
    pub fn worth_reporting(&self) -> bool {
        self.reliable()
            && self.difference.as_ref().is_some_and(|d| {
                d.lost_collision || d.lost_animation || !d.added_textures.is_empty() || !d.dropped_textures.is_empty()
            })
    }

    /// The one line the report gives it (`describe_mesh_finding`, English); empty: nothing
    /// worth saying.
    pub fn describe(&self) -> String {
        if !self.unreadable.is_empty() {
            return format!("could not read the mesh: {}", self.unreadable);
        }
        let Some(d) = self.difference.as_ref().filter(|_| self.reliable()) else { return String::new() };
        let mut parts = Vec::new();
        if d.lost_collision {
            parts.push("loses collision".to_string());
        }
        if d.lost_animation {
            parts.push("loses animation".to_string());
        }
        match d.added_textures.len() {
            0 => {}
            1 => parts.push("needs 1 texture the other does not ship".to_string()),
            n => parts.push(format!("needs {n} textures the other does not ship")),
        }
        if let Some(r) = d.triangle_ratio
            && r < SIMPLER_RATIO
        {
            parts.push(format!("{}% of the triangles", py_round(r * 100.0)));
        }
        parts.join(", ")
    }

    /// The CSV's mesh cells (`_mesh_columns`): note, lost collision, lost animation, ratio.
    pub fn columns(&self) -> (String, String, String, String) {
        let note = self.describe();
        let Some(d) = self.difference.as_ref().filter(|_| self.reliable()) else {
            return (note, String::new(), String::new(), String::new());
        };
        let yes = |b: bool| if b { "yes" } else { "no" }.to_string();
        (note, yes(d.lost_collision), yes(d.lost_animation), d.triangle_ratio.map(|r| format!("{r:.3}")).unwrap_or_default())
    }
}

/// Python's `round()` of a non-negative float: halves to even.
fn py_round(x: f64) -> i64 {
    let f = x.floor();
    let diff = x - f;
    #[allow(clippy::float_cmp)]
    let r = if diff > 0.5 || (diff == 0.5 && (f as i64) % 2 != 0) { f + 1.0 } else { f };
    r as i64
}

/// Reads meshes once per body (`MeshAnalyser`).
#[derive(Default)]
pub struct Analyser {
    by_content: Mutex<HashMap<(u64, usize), Result<Structure, String>>>,
}

impl Analyser {
    /// A mesh's structure, or why it could not be read.
    pub fn structure(&self, path: &Path) -> Result<Structure, String> {
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        let mut h = std::collections::hash_map::DefaultHasher::new();
        bytes.hash(&mut h);
        let key = (h.finish(), bytes.len());
        if let Some(found) = self.by_content.lock().unwrap_or_else(std::sync::PoisonError::into_inner).get(&key) {
            return found.clone();
        }
        let s = summary(&bytes);
        let out = if s.stopped_reason.is_empty() { Ok(s) } else { Err(s.stopped_reason) };
        self.by_content.lock().unwrap_or_else(std::sync::PoisonError::into_inner).insert(key, out.clone());
        out
    }

    /// The loser compared with the winner (`compare_providers`).
    pub fn compare_providers(&self, asset: &str, loser: &Path, winner: &Path) -> Finding {
        let l = self.structure(loser);
        let w = self.structure(winner);
        match (l, w) {
            (Ok(l), Ok(w)) => Finding {
                path: asset.to_string(),
                difference: Some(compare(&l, &w)),
                unreadable: String::new(),
                loser_partial: l.partial(),
                winner_partial: w.partial(),
            },
            (Err(why), _) | (_, Err(why)) => Finding {
                path: asset.to_string(),
                difference: None,
                unreadable: why,
                loser_partial: false,
                winner_partial: false,
            },
        }
    }
}

/// The analysis's counts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MeshStats {
    pub analysed: usize,
    pub findings: usize,
    pub unreadable: usize,
}

/// Each conflict's finding (`analyse_mesh_conflicts`): `None` for one not looked at (not a
/// mesh, identical, or past `limit` analysed; 0: no limit). Of a mesh's losers, the first
/// worth reporting stands for it, else the first reliable, else the first.
pub fn analyse(conflicts: &[Resource], limit: usize) -> (Vec<Option<Finding>>, MeshStats) {
    let mut chosen: Vec<usize> = Vec::new();
    for (i, c) in conflicts.iter().enumerate() {
        if !c.path.to_lowercase().ends_with(".nif") || c.identical == Some(true) {
            continue;
        }
        if !c.providers.iter().any(|p| *p != c.winner) {
            continue;
        }
        if limit != 0 && chosen.len() >= limit {
            break;
        }
        chosen.push(i);
    }
    let analyser = Analyser::default();
    let found = par_map(&chosen, |&i| {
        let c = &conflicts[i];
        let winner = Path::new(&c.winner).join(&c.path);
        let losses: Vec<Finding> = c
            .providers
            .iter()
            .filter(|p| **p != c.winner)
            .map(|p| analyser.compare_providers(&c.path, &Path::new(p).join(&c.path), &winner))
            .collect();
        let reportable = losses.iter().position(Finding::worth_reporting);
        let pick = reportable.or_else(|| losses.iter().position(Finding::reliable)).unwrap_or(0);
        (reportable.is_some(), losses.into_iter().nth(pick))
    });
    let mut out = vec![None; conflicts.len()];
    let mut stats = MeshStats::default();
    for (&i, (reportable, f)) in chosen.iter().zip(found) {
        let Some(f) = f else { continue };
        stats.analysed += 1;
        stats.unreadable += usize::from(!f.unreadable.is_empty());
        stats.findings += usize::from(reportable);
        out[i] = Some(f);
    }
    (out, stats)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_texture_is_its_name_without_the_folder_or_extension() {
        assert_eq!(texture_key("Textures\\\\tx_a.TGA"), "tx_a");
        assert_eq!(texture_key("/textures/sub/b.dds"), "sub/b");
        assert_eq!(texture_key("dir.v2/name"), "dir.v2/name");
    }

    #[test]
    fn only_losses_are_said_and_only_when_both_were_read_whole() {
        let s = |tris: usize, coll: bool, tex: &[&str]| Structure {
            shapes: vec![Shape { name: "a".into(), vertices: 3, triangles: tris }],
            textures: tex.iter().map(|t| (*t).to_string()).collect(),
            has_collision: coll,
            blocks_read: 1,
            blocks_declared: 1,
            ..Structure::default()
        };
        let d = compare(&s(100, true, &["a.tga"]), &s(25, false, &["a.dds", "b.dds"]));
        assert_eq!(d.triangle_ratio, Some(0.25));
        assert!(d.lost_collision);
        assert_eq!(d.added_textures, ["b.dds"]);
        assert!(d.dropped_textures.is_empty());
        let f = Finding { path: "m.nif".into(), difference: Some(d), unreadable: String::new(), loser_partial: false, winner_partial: false };
        assert_eq!(f.describe(), "loses collision, needs 1 texture the other does not ship, 25% of the triangles");
        assert_eq!(f.columns().3, "0.250");
        let partial = Finding { loser_partial: true, ..f.clone() };
        assert_eq!(partial.describe(), "");
        assert!(!partial.worth_reporting());
    }

    #[test]
    fn rounding_is_pythons() {
        assert_eq!(py_round(12.5), 12);
        assert_eq!(py_round(13.5), 14);
        assert_eq!(py_round(49.4), 49);
    }
}
