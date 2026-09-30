//! Wraithguard: where things are used - every cell a mesh, a texture or a base record
//! stands in - and how contested each cell's references are, for the cell map.

use std::collections::{HashMap, HashSet};

use crate::esp::{CellRef, RefNum};
use crate::vfs::{norm, Vfs};
use crate::world::{Gone, World};

/// One cell where something stands: how many times, and the first one's placement.
#[derive(Clone, Debug, PartialEq)]
pub struct Place {
    /// `"x,y"` for an exterior, `"int:<name>"` for a room - the cell viewer's own specs.
    pub spec: String,
    pub label: String,
    pub count: usize,
    pub pos: [f32; 3],
    pub rot: f32,
}

/// A mesh path as a record names it: lower case, forward slashes, no `meshes/`.
pub fn mesh_key(p: &str) -> String {
    let n = norm(p);
    n.strip_prefix("meshes/").unwrap_or(&n).to_string()
}

/// A texture's name without folder or extension: what a NIF and an LTEX both come down to.
pub fn texture_stem(p: &str) -> String {
    let n = norm(p);
    let file = n.rsplit('/').next().unwrap_or(&n).to_string();
    match file.rfind('.') {
        Some(d) => file[..d].to_string(),
        None => file,
    }
}

/// Every base record (lowercased id) drawn with `mesh`.
pub fn ids_using_mesh(w: &World, mesh: &str) -> HashSet<String> {
    let want = mesh_key(mesh);
    let mut out: HashSet<String> = HashSet::new();
    for (id, m) in &w.models {
        if mesh_key(m) == want {
            out.insert(id.clone());
        }
    }
    for (id, m) in &w.indoor_plain {
        if mesh_key(m) == want {
            out.insert(id.clone());
        }
    }
    for (id, a) in &w.actors {
        if mesh_key(&a.model) == want || a.twin.as_deref().map(mesh_key).as_deref() == Some(want.as_str()) {
            out.insert(id.clone());
        }
    }
    out
}

fn tally(refs: &HashMap<RefNum, CellRef>, ids: &HashSet<String>) -> Option<(usize, [f32; 3], f32)> {
    let mut hit: Vec<&CellRef> = refs.values().filter(|r| ids.contains(&r.id.to_ascii_lowercase())).collect();
    if hit.is_empty() {
        return None;
    }
    hit.sort_by_key(|r| (r.num.content_file, r.num.index));
    Some((hit.len(), hit[0].pos, hit[0].rot[2]))
}

/// Every cell and room where a reference to one of `ids` stands, most first.
pub fn places_of(w: &World, ids: &HashSet<String>) -> Vec<Place> {
    let mut out: Vec<Place> = Vec::new();
    for c in w.cells.values() {
        if let Some((count, pos, rot)) = tally(&c.refs, ids) {
            let (x, y) = c.grid;
            let label = if c.name.is_empty() { format!("({x}, {y})") } else { format!("{} ({x}, {y})", c.name) };
            out.push(Place { spec: format!("{x},{y}"), label, count, pos, rot });
        }
    }
    for r in w.interiors.values() {
        if let Some((count, pos, rot)) = tally(&r.refs, ids) {
            out.push(Place { spec: format!("int:{}", r.name), label: r.name.clone(), count, pos, rot });
        }
    }
    out.sort_by(|a, b| b.count.cmp(&a.count).then(a.label.cmp(&b.label)));
    out
}

/// The meshes (as records name them) whose file names the texture `tex` - read from the
/// files, every distinct mesh in the load order once, in parallel. A NIF names its
/// textures as plain strings, so a case-blind search for the name is the answer without
/// a parse.
pub fn meshes_naming_texture(w: &World, vfs: &Vfs, tex: &str) -> Vec<String> {
    let stem = texture_stem(tex);
    if stem.is_empty() {
        return Vec::new();
    }
    let mut meshes: Vec<String> = w.models.values().chain(w.indoor_plain.values()).map(|m| mesh_key(m)).collect();
    meshes.extend(w.actors.values().filter(|a| !a.model.is_empty()).map(|a| mesh_key(&a.model)));
    meshes.sort();
    meshes.dedup();
    let needle = stem.as_bytes().to_vec();
    let hits = crate::pool::par_map(&meshes, |_, m| {
        let Some(loc) = vfs.resolve_mesh(m) else { return false };
        let Some(bytes) = vfs.load(&loc) else { return false };
        names(&bytes, &needle)
    });
    meshes.into_iter().zip(hits).filter(|(_, h)| *h).map(|(m, _)| m).collect()
}

/// Whether `hay` holds `needle` as a file name: case-blind, with no name character just
/// before it and a `.` just after, so `tx_rock` matches neither `tx_rock_02` nor
/// `mytx_rock`.
fn names(hay: &[u8], needle: &[u8]) -> bool {
    let n = needle.len();
    if n == 0 || hay.len() < n {
        return false;
    }
    'outer: for i in 0..=hay.len() - n {
        for k in 0..n {
            if hay[i + k].to_ascii_lowercase() != needle[k] {
                continue 'outer;
            }
        }
        let before = if i == 0 { b'\\' } else { hay[i - 1] };
        let after = hay.get(i + n).copied().unwrap_or(b'.');
        if !(before.is_ascii_alphanumeric() || before == b'_' || before == b'-') && after == b'.' {
            return true;
        }
    }
    false
}

/// The exterior cells whose ground paints the texture `tex`.
pub fn land_places(w: &World, tex: &str) -> Vec<Place> {
    let stem = texture_stem(tex);
    let ids: HashSet<String> = w
        .ltex_file
        .iter()
        .filter(|(_, f)| texture_stem(f) == stem)
        .map(|(id, _)| id.clone())
        .collect();
    let mut out: Vec<Place> = Vec::new();
    for (grid, land) in &w.lands {
        let n = land
            .vtex
            .iter()
            .filter(|&&v| w.ltex_for(v, land.plugin).map(|t| ids.contains(&t.to_ascii_lowercase())).unwrap_or(false))
            .count();
        if n == 0 {
            continue;
        }
        let (x, y) = *grid;
        let name = w.cells.get(grid).map(|c| c.name.clone()).unwrap_or_default();
        let label = if name.is_empty() { format!("({x}, {y})") } else { format!("{name} ({x}, {y})") };
        let pos = [x as f32 * 8192.0 + 4096.0, y as f32 * 8192.0 + 4096.0, 0.0];
        out.push(Place { spec: format!("{x},{y}"), label, count: n, pos, rot: 0.0 });
    }
    out.sort_by(|a, b| b.count.cmp(&a.count).then(a.label.cmp(&b.label)));
    out
}

/// How contested a cell's references are: those two or more plugins supplied a version
/// of, and those a plugin deleted or moved away.
pub fn conflicts(refs: &HashMap<RefNum, CellRef>, gone: &[Gone]) -> usize {
    refs.values().filter(|r| r.touched.iter().any(|t| Some(t) != r.touched.first())).count() + gone.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_texture_is_matched_by_its_whole_name() {
        assert!(names(b"\x00\x00textures\\TX_Rock_01.dds\x00", b"tx_rock_01"));
        assert!(names(b"\x10\x00\x00\x00tx_rock_01.tga", b"tx_rock_01"));
        assert!(!names(b"textures\\tx_rock_01b.dds", b"tx_rock_01"));
        assert!(!names(b"textures\\mytx_rock_01.dds", b"tx_rock_01"));
        assert_eq!(texture_stem("Textures\\TX_Rock_01.DDS"), "tx_rock_01");
        assert_eq!(mesh_key("Meshes\\x\\Rock.NIF"), "x/rock.nif");
    }
}
