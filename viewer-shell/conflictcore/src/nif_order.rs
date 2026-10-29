//! A mesh's draw order: each NiTriShapeData's triangles and vertices put in the order a
//! GPU draws fastest, as greatness7's Blender exporter writes them - the vertex cache
//! and overdraw passes (Gardenfell's `triorder`, Robin Hjelte's port of meshoptimizer),
//! then the vertex fetch order (`meshopt-port`, ours). The mesh viewer's "Draw order"
//! report is a dry run of this (`report`); its Optimise is the real one (`optimize`,
//! the `{"op": "optimize"}` edit).
//!
//! What moves with the vertices, through one table: positions, normals, colours, every
//! UV set, the match groups (`shared_normals`), each skin bone's vertex weights and each
//! morph target. Nothing is dropped: a vertex no triangle uses goes to the end, in its
//! old order, so every count in the file stays as it was. The triangles are the same
//! triangles with the same corners, wound the same way; only their order changes.
//!
//! A shape is left as it is, and the report says why, when one table cannot move all of
//! it: a skin partition (its own vertex map), a skin or morph data another geometry also
//! uses, or arrays whose lengths do not match the vertex count.

use std::collections::{HashMap, HashSet};

use serde_json::{Value, json};
use tes3::nif::*;

use gardenfell_nif::triorder::{analyze_vertex_cache, reorder_triangles};
use meshopt_port::{analyze_overdraw, analyze_vertex_fetch, optimize_vertex_fetch_remap, remap_vertex_buffer};

/// How a part draws: vertex cache (transformed per triangle, and per vertex), vertex
/// fetch (bytes read over the buffer's size) and overdraw (shaded over covered pixels).
fn stats(idx: &[u32], pos: &[f32], n: usize, vertex_size: usize) -> Value {
    let (_, acmr, atvr) = analyze_vertex_cache(idx, n, 16);
    let vf = analyze_vertex_fetch(idx, n, vertex_size.clamp(1, 256));
    let od = analyze_overdraw(idx, pos, n, 3);
    json!({"acmr": acmr, "atvr": atvr, "overfetch": vf.overfetch, "overdraw": od.overdraw})
}

/// Whether the new order is worth writing: a better vertex cache; the same cache and less
/// fetched; or less overdraw for at most the 5 % of cache the overdraw pass may spend.
fn better(before: &Value, after: &Value) -> bool {
    let f = |v: &Value, k: &str| v[k].as_f64().unwrap_or(0.0);
    let (ab, aa) = (f(before, "acmr"), f(after, "acmr"));
    let eps = 1e-6;
    aa < ab - eps
        || ((aa - ab).abs() <= eps && f(after, "overfetch") < f(before, "overfetch") - eps)
        || (aa <= ab * 1.05 + eps && f(after, "overdraw") < f(before, "overdraw") - eps)
}

/// What else a geometry data's vertices are indexed by.
#[derive(Default)]
struct Users {
    shapes: Vec<String>,
    skins: HashSet<NiKey>,
    morphs: HashSet<NiKey>,
    /// Why it cannot be moved, if it cannot.
    blocked: Option<&'static str>,
}

fn users(stream: &NiStream) -> HashMap<NiKey, Users> {
    let mut by_data: HashMap<NiKey, Users> = HashMap::new();
    let mut shape_data: HashMap<NiKey, NiKey> = HashMap::new();
    for (link, shape) in stream.objects_of_type_with_link::<NiTriShape>() {
        let geom = &shape.base.base;
        let data = geom.geometry_data.key;
        if geom.geometry_data.is_null() {
            continue;
        }
        shape_data.insert(link.key, data);
        let u = by_data.entry(data).or_default();
        u.shapes.push(geom.base.base.name.clone());
        if !geom.skin_instance.is_null() {
            match stream.get(geom.skin_instance) {
                Some(inst) if !inst.data.is_null() => {
                    match stream.get(inst.data) {
                        Some(sd) if sd.skin_partition.is_null() => {
                            u.skins.insert(inst.data.key);
                        }
                        Some(_) => u.blocked = Some("skin partition"),
                        None => u.blocked = Some("missing skin data"),
                    }
                }
                Some(_) => {}
                None => u.blocked = Some("missing skin instance"),
            }
        }
    }
    for c in stream.objects_of_type::<NiGeomMorpherController>() {
        let target = c.base.base.target.key;
        if let Some(data) = shape_data.get(&target).filter(|_| !c.base.data.is_null()) {
            by_data.entry(*data).or_default().morphs.insert(c.base.data.key);
        }
    }
    // A skin or morph data one table cannot speak for: used by two geometry datas.
    let mut owners: HashMap<NiKey, usize> = HashMap::new();
    for u in by_data.values() {
        for k in u.skins.iter().chain(u.morphs.iter()) {
            *owners.entry(*k).or_default() += 1;
        }
    }
    for u in by_data.values_mut() {
        if u.skins.iter().chain(u.morphs.iter()).any(|k| owners[k] > 1) {
            u.blocked.get_or_insert("skin or morph data shared with another shape");
        }
    }
    by_data
}

fn flat_positions(d: &NiTriShapeData) -> Vec<f32> {
    d.base.base.vertices.iter().flat_map(|v| [v.x, v.y, v.z]).collect()
}

/// The bytes a vertex of this data takes in a vertex buffer, for the fetch analyser.
fn vertex_size(d: &NiTriShapeData) -> usize {
    let g = &d.base.base;
    let n = g.vertices.len().max(1);
    12 + if g.normals.is_empty() { 0 } else { 12 } + if g.vertex_colors.is_empty() { 0 } else { 16 } + 8 * (g.uv_sets.len() / n)
}

/// Why this data's arrays cannot be remapped by one table, if they cannot.
fn inconsistent(d: &NiTriShapeData, stream: &NiStream, u: &Users) -> Option<&'static str> {
    let g = &d.base.base;
    let n = g.vertices.len();
    if n == 0 || d.triangles.is_empty() {
        return Some("no triangles");
    }
    if !(g.normals.is_empty() || g.normals.len() == n) || !(g.vertex_colors.is_empty() || g.vertex_colors.len() == n) || !g.uv_sets.len().is_multiple_of(n) {
        return Some("arrays of different lengths");
    }
    if d.triangles.iter().flatten().any(|&i| i as usize >= n) || d.shared_normals.iter().flatten().any(|&i| i as usize >= n) {
        return Some("an index past the vertices");
    }
    for k in &u.skins {
        let Some(NiType::NiSkinData(sd)) = stream.objects.get(*k) else { return Some("missing skin data") };
        if sd.bone_data.iter().flat_map(|b| b.vertex_weights.iter()).any(|(i, _)| *i as usize >= n) {
            return Some("a skin weight past the vertices");
        }
    }
    for k in &u.morphs {
        let Some(NiType::NiMorphData(md)) = stream.objects.get(*k) else { return Some("missing morph data") };
        if md.targets.iter().any(|t| t.vertices.len() != n) {
            return Some("a morph target of another length");
        }
    }
    None
}

/// Each NiTriShapeData put in draw order (`apply`), or only measured. One entry per data
/// block, by its index in the file (`index`): its shapes' names, counts, the figures as
/// the file has them and as they would be, and why it was left alone, if it was.
fn run(stream: &mut NiStream, index: &HashMap<NiKey, usize>, apply: bool) -> Vec<Value> {
    let users = users(stream);
    let mut datas: Vec<(NiKey, usize)> = stream
        .objects
        .iter()
        .filter(|(_, o)| matches!(o, NiType::NiTriShapeData(_)))
        .filter_map(|(k, _)| index.get(&k).map(|i| (k, *i)))
        .collect();
    datas.sort_by_key(|(_, i)| *i);
    let mut out = Vec::new();
    for (key, block) in datas {
        let u = users.get(&key);
        let empty = Users::default();
        let u = u.unwrap_or(&empty);
        let Some(NiType::NiTriShapeData(d)) = stream.objects.get(key) else { continue };
        let n = d.base.base.vertices.len();
        let mut entry = json!({"block": block, "shapes": u.shapes, "vertices": n, "triangles": d.triangles.len()});
        if let Some(why) = u.blocked.or_else(|| inconsistent(d, stream, u)) {
            entry["skipped"] = json!(why);
            out.push(entry);
            continue;
        }
        let pos = flat_positions(d);
        let vsize = vertex_size(d);
        let idx: Vec<u32> = d.triangles.iter().flatten().map(|&i| i as u32).collect();
        entry["before"] = stats(&idx, &pos, n, vsize);

        // The triangles: cache order, then overdraw (triorder, as the viewer draws).
        let mut idx16: Vec<u16> = d.triangles.iter().flatten().copied().collect();
        reorder_triangles(&mut idx16, &pos);
        let ordered: Vec<u32> = idx16.iter().map(|&i| i as u32).collect();
        // The vertices: in the order the triangles first use them; unused ones after.
        let (mut remap, kept) = optimize_vertex_fetch_remap(&ordered, n);
        let mut next = kept as u32;
        for r in remap.iter_mut() {
            if *r == u32::MAX {
                *r = next;
                next += 1;
            }
        }
        let new_idx: Vec<u32> = ordered.iter().map(|&i| remap[i as usize]).collect();
        let new_pos: Vec<f32> = remap_vertex_buffer(pos.as_chunks::<3>().0, n, &remap).into_iter().flatten().collect();
        entry["after"] = stats(&new_idx, &new_pos, n, vsize);
        entry["unused_vertices"] = json!(n - kept);
        if !better(&entry["before"], &entry["after"]) {
            // meshoptimizer's passes are heuristics: on a part already in good order they
            // can come out a little worse (the overdraw pass may give up to 5 % of the
            // cache for overdraw it then does not find). Left as the file has it.
            entry["skipped"] = json!("already in good order");
            out.push(entry);
            continue;
        }
        if apply {
            let (skins, morphs) = (u.skins.clone(), u.morphs.clone());
            if let Some(NiType::NiTriShapeData(d)) = stream.objects.get_mut(key) {
                let g = &mut d.base.base;
                g.vertices = remap_vertex_buffer(&g.vertices, n, &remap);
                if !g.normals.is_empty() {
                    g.normals = remap_vertex_buffer(&g.normals, n, &remap);
                }
                if !g.vertex_colors.is_empty() {
                    g.vertex_colors = remap_vertex_buffer(&g.vertex_colors, n, &remap);
                }
                if !g.uv_sets.is_empty() {
                    // Stored set after set, `n` each.
                    g.uv_sets = g.uv_sets.chunks_exact(n).flat_map(|set| remap_vertex_buffer(set, n, &remap)).collect();
                }
                d.triangles = new_idx.as_chunks::<3>().0.iter().map(|t| [t[0] as u16, t[1] as u16, t[2] as u16]).collect();
                for group in d.shared_normals.iter_mut() {
                    for i in group.iter_mut() {
                        *i = remap[*i as usize] as u16;
                    }
                }
            }
            for k in skins {
                if let Some(NiType::NiSkinData(sd)) = stream.objects.get_mut(k) {
                    for b in sd.bone_data.iter_mut() {
                        for (i, _) in b.vertex_weights.iter_mut() {
                            *i = remap[*i as usize] as u16;
                        }
                    }
                }
            }
            for k in morphs {
                if let Some(NiType::NiMorphData(md)) = stream.objects.get_mut(k) {
                    for t in md.targets.iter_mut() {
                        t.vertices = remap_vertex_buffer(&t.vertices, n, &remap);
                    }
                }
            }
        }
        out.push(entry);
    }
    out
}

/// The mesh viewer's report: what Optimise would do to each shape, nothing changed.
pub fn report(stream: &mut NiStream, index: &HashMap<NiKey, usize>) -> Vec<Value> {
    run(stream, index, false)
}

/// Optimise: every shape that can be, put in draw order. The same entries as `report`.
pub fn optimize(stream: &mut NiStream, index: &HashMap<NiKey, usize>) -> Vec<Value> {
    run(stream, index, true)
}

// ---- the same mesh, checked ---------------------------------------------------------

fn index_of(s: &NiStream) -> HashMap<NiKey, usize> {
    s.objects.keys().enumerate().map(|(i, k)| (k, i)).collect()
}

fn b3(x: f32, y: f32, z: f32) -> [u32; 3] {
    [x.to_bits(), y.to_bits(), z.to_bits()]
}

/// A vertex as everything about it: position, normal, colour, every UV set.
fn corner_bits(d: &NiTriShapeData, i: usize) -> Vec<u32> {
    let g = &d.base.base;
    let n = g.vertices.len();
    let v = &g.vertices[i];
    let mut c = b3(v.x, v.y, v.z).to_vec();
    if !g.normals.is_empty() {
        let v = &g.normals[i];
        c.extend(b3(v.x, v.y, v.z));
    }
    if !g.vertex_colors.is_empty() {
        let col = &g.vertex_colors[i];
        c.extend([col.x.to_bits(), col.y.to_bits(), col.z.to_bits(), col.w.to_bits()]);
    }
    for set in g.uv_sets.chunks_exact(n.max(1)) {
        c.extend([set[i].x.to_bits(), set[i].y.to_bits()]);
    }
    c
}

/// What a mesh means, apart from the order it is written in: per geometry data, its
/// triangles as their corners (sorted), each skin bone's weights as (vertex, weight) and
/// each morph target as (vertex, target position). Equal before and after Optimise means
/// the same mesh (the tests' `meaning`, here so a batch can check what it writes).
#[allow(clippy::type_complexity)]
fn meaning_of(s: &NiStream) -> Vec<(Vec<Vec<Vec<u32>>>, Vec<Vec<(Vec<u32>, u32)>>, Vec<Vec<(Vec<u32>, [u32; 3])>>)> {
    let ix = index_of(s);
    let u = users(s);
    let mut out = Vec::new();
    for (k, o) in s.objects.iter() {
        let NiType::NiTriShapeData(d) = o else { continue };
        let mut tris: Vec<Vec<Vec<u32>>> = d.triangles.iter().map(|t| t.iter().map(|&i| corner_bits(d, i as usize)).collect()).collect();
        tris.sort();
        let mut skins = Vec::new();
        let mut morphs = Vec::new();
        if let Some(us) = u.get(&k) {
            let mut sk: Vec<_> = us.skins.iter().copied().collect();
            sk.sort_by_key(|k| ix[k]);
            for sk in sk {
                let Some(NiType::NiSkinData(sd)) = s.objects.get(sk) else { continue };
                for b in &sd.bone_data {
                    let mut w: Vec<_> = b.vertex_weights.iter().map(|(i, w)| (corner_bits(d, *i as usize), w.to_bits())).collect();
                    w.sort();
                    skins.push(w);
                }
            }
            let mut mk: Vec<_> = us.morphs.iter().copied().collect();
            mk.sort_by_key(|k| ix[k]);
            for mk in mk {
                let Some(NiType::NiMorphData(md)) = s.objects.get(mk) else { continue };
                for t in &md.targets {
                    let mut m: Vec<_> = t.vertices.iter().enumerate().map(|(i, v)| (corner_bits(d, i), b3(v.x, v.y, v.z))).collect();
                    m.sort();
                    morphs.push(m);
                }
            }
        }
        out.push((tris, skins, morphs));
    }
    out.sort();
    out
}

/// A hash of [`meaning_of`], for comparing two reads of a mesh (within one run).
pub fn fingerprint(s: &NiStream) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    meaning_of(s).hash(&mut h);
    h.finish()
}

/// Check that every vertex index `meaning_of` dereferences is valid before fingerprinting.
///
/// `run` already skips geometry whose indices or parallel arrays are inconsistent, but
/// `optimise_checked` fingerprints the entire stream before it gets to `run`. Without this
/// check, one malformed mesh can panic while a batch is writing files and take down its
/// scoped worker instead of being reported as a failed file.
fn fingerprint_indices_valid(s: &NiStream) -> Result<(), &'static str> {
    let users = users(s);
    for (key, object) in &s.objects {
        let NiType::NiTriShapeData(d) = object else { continue };
        let g = &d.base.base;
        let n = g.vertices.len();
        let corner_is_readable = |i: usize| {
            i < n
                && (g.normals.is_empty() || i < g.normals.len())
                && (g.vertex_colors.is_empty() || i < g.vertex_colors.len())
        };

        if d.triangles.iter().flatten().any(|&i| !corner_is_readable(i as usize)) {
            return Err("mesh contains a triangle vertex reference outside its geometry arrays");
        }

        if let Some(u) = users.get(&key) {
            for skin_key in &u.skins {
                if let Some(NiType::NiSkinData(sd)) = s.objects.get(*skin_key)
                    && sd
                        .bone_data
                        .iter()
                        .flat_map(|bone| bone.vertex_weights.iter())
                        .any(|(i, _)| !corner_is_readable(*i as usize))
                {
                    return Err("mesh contains a skin-weight vertex reference outside its geometry arrays");
                }
            }

            for morph_key in &u.morphs {
                if let Some(NiType::NiMorphData(md)) = s.objects.get(*morph_key)
                    && md.targets.iter().any(|target| {
                        target.vertices.iter().enumerate().any(|(i, _)| !corner_is_readable(i))
                    })
                {
                    return Err("mesh contains a morph-target vertex reference outside its geometry arrays");
                }
            }
        }
    }
    Ok(())
}

/// The mesh's figures as `report` gives them, for a mesh as bytes. A panic in the reader
/// or the passes is an error, not a crash.
pub fn report_bytes(bytes: &[u8]) -> Result<Vec<Value>, String> {
    let mut s = crate::mesh::load(bytes).map_err(|e| e.to_string())?;
    let ix = index_of(&s);
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| report(&mut s, &ix))).map_err(|_| "the draw order pass failed on this mesh".to_string())
}

/// A mesh optimised and saved as bytes, checked: read back, it must mean the same mesh
/// (every triangle with the same corners, every skin weight and morph target the same),
/// and at least one shape must have been worth moving. Otherwise nothing is returned, so
/// nothing is written. The entries are `optimize`'s.
pub fn optimise_checked(bytes: &[u8]) -> Result<(Vec<u8>, Vec<Value>), String> {
    let mut s = crate::mesh::load(bytes).map_err(|e| e.to_string())?;
    fingerprint_indices_valid(&s).map_err(str::to_string)?;
    let before = fingerprint(&s);
    let ix = index_of(&s);
    let rep = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| optimize(&mut s, &ix)))
        .map_err(|_| "the draw order pass failed on this mesh".to_string())?;
    if !rep.iter().any(|e| e.get("after").is_some() && e.get("skipped").is_none()) {
        return Err("no shape is worth moving".into());
    }
    let saved = editcore::guarded(|| s.save_bytes()).map_err(|e| format!("the mesh would not save: {e}"))?;
    let back = crate::mesh::load(&saved).map_err(|e| format!("the saved mesh would not read back: {e}"))?;
    fingerprint_indices_valid(&back).map_err(str::to_string)?;
    if fingerprint(&back) != before {
        return Err("the saved mesh differs from the original; not written".into());
    }
    Ok((saved, rep))
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIX: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures/gardenfell_anim/");
    const PAGE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../ui/tests/fixture/Data Files/meshes/");

    fn load(b: &[u8]) -> NiStream {
        NiStream::from_bytes(b).unwrap()
    }

    fn index(s: &NiStream) -> HashMap<NiKey, usize> {
        s.objects.keys().enumerate().map(|(i, k)| (k, i)).collect()
    }

    fn bits3(v: &glam::Vec3) -> [u32; 3] {
        [v.x.to_bits(), v.y.to_bits(), v.z.to_bits()]
    }

    /// A vertex as everything about it: position, normal, colour, every UV set.
    fn corner(d: &NiTriShapeData, i: usize) -> Vec<u32> {
        let g = &d.base.base;
        let n = g.vertices.len();
        let mut c = bits3(&g.vertices[i]).to_vec();
        if !g.normals.is_empty() {
            c.extend(bits3(&g.normals[i]));
        }
        if !g.vertex_colors.is_empty() {
            let col = &g.vertex_colors[i];
            c.extend([col.x.to_bits(), col.y.to_bits(), col.z.to_bits(), col.w.to_bits()]);
        }
        for set in g.uv_sets.chunks_exact(n.max(1)) {
            c.extend([set[i].x.to_bits(), set[i].y.to_bits()]);
        }
        c
    }

    /// Per data block (by file index): its triangles as their corners, sorted; each
    /// skin bone's weights as (vertex, weight), sorted; each morph target as (vertex,
    /// target position), sorted. Equal before and after means the same mesh.
    #[allow(clippy::type_complexity)]
    fn meaning(s: &NiStream) -> Vec<(usize, Vec<Vec<Vec<u32>>>, Vec<Vec<(Vec<u32>, u32)>>, Vec<Vec<(Vec<u32>, [u32; 3])>>)> {
        let ix = index(s);
        let u = users(s);
        let mut out = Vec::new();
        for (k, o) in s.objects.iter() {
            let NiType::NiTriShapeData(d) = o else { continue };
            let mut tris: Vec<Vec<Vec<u32>>> = d.triangles.iter().map(|t| t.iter().map(|&i| corner(d, i as usize)).collect()).collect();
            tris.sort();
            let mut skins = Vec::new();
            let mut morphs = Vec::new();
            if let Some(us) = u.get(&k) {
                let mut sk: Vec<_> = us.skins.iter().copied().collect();
                sk.sort_by_key(|k| ix[k]);
                for sk in sk {
                    let Some(NiType::NiSkinData(sd)) = s.objects.get(sk) else { continue };
                    for b in &sd.bone_data {
                        let mut w: Vec<_> = b.vertex_weights.iter().map(|(i, w)| (corner(d, *i as usize), w.to_bits())).collect();
                        w.sort();
                        skins.push(w);
                    }
                }
                let mut mk: Vec<_> = us.morphs.iter().copied().collect();
                mk.sort_by_key(|k| ix[k]);
                for mk in mk {
                    let Some(NiType::NiMorphData(md)) = s.objects.get(mk) else { continue };
                    for t in &md.targets {
                        let mut m: Vec<_> = t.vertices.iter().enumerate().map(|(i, v)| (corner(d, i), bits3(v))).collect();
                        m.sort();
                        morphs.push(m);
                    }
                }
            }
            out.push((ix[&k], tris, skins, morphs));
        }
        // By content, not position: saving writes the blocks in walk order, so a data
        // block's index can change while what it holds does not.
        for x in out.iter_mut() {
            x.0 = 0;
        }
        out.sort();
        out
    }

    /// Optimise, save, read back: the same mesh, drawn no worse.
    fn check(path: &str) -> Vec<Value> {
        let bytes = std::fs::read(path).unwrap();
        let mut s = load(&bytes);
        let before = meaning(&s);
        let ix = index(&s);
        let rep = optimize(&mut s, &ix);
        let saved = s.save_bytes().unwrap();
        let back = load(&saved);
        assert_eq!(meaning(&back), before, "{path}: the same triangles, weights and morphs");
        for e in &rep {
            if let (Some(b), Some(a), None) = (e.get("before"), e.get("after"), e.get("skipped")) {
                assert!(better(b, a), "{path}: written though not better: {e}");
            }
        }
        rep
    }

    #[test]
    fn a_skinned_mesh_keeps_its_weights() {
        let rep = check(&format!("{FIX}flap.nif"));
        assert!(rep.iter().any(|e| e.get("after").is_some()), "{rep:?}");
    }

    #[test]
    fn a_morphed_mesh_keeps_its_targets() {
        check(&format!("{FIX}morph.nif"));
    }

    #[test]
    fn every_uv_set_moves_with_its_vertices() {
        check(&format!("{FIX}uvsets.nif"));
    }

    #[test]
    fn invalid_triangle_vertex_index_is_reported_instead_of_panicking() {
        let bytes = std::fs::read(format!("{FIX}flap.nif")).unwrap();
        let mut s = load(&bytes);
        let data = s
            .objects
            .values_mut()
            .find_map(|object| match object {
                NiType::NiTriShapeData(data) => Some(data),
                _ => None,
            })
            .expect("fixture contains triangle data");
        assert!(!data.triangles.is_empty(), "fixture contains a triangle");
        let bad_index = data.base.base.vertices.len() + 43;
        data.triangles[0][0] = u16::try_from(bad_index).unwrap();

        let malformed = s.save_bytes().unwrap();
        let error = optimise_checked(&malformed).unwrap_err();
        assert!(error.contains("triangle vertex reference"), "{error}");
    }

    #[test]
    fn the_page_fixture_meshes_come_back_the_same() {
        for sub in std::fs::read_dir(PAGE).unwrap().flatten() {
            for f in std::fs::read_dir(sub.path()).into_iter().flatten().flatten() {
                if f.path().extension().is_some_and(|x| x.eq_ignore_ascii_case("nif")) {
                    check(f.path().to_str().unwrap());
                }
            }
        }
        check(&format!("{FIX}anim.nif"));
    }

    #[test]
    fn the_report_changes_nothing() {
        let bytes = std::fs::read(format!("{FIX}flap.nif")).unwrap();
        let mut s = load(&bytes);
        let ix = index(&s);
        let before = meaning(&s);
        let a = report(&mut s, &ix);
        assert_eq!(meaning(&s), before);
        let mut t = load(&bytes);
        let b = optimize(&mut t, &ix);
        assert_eq!(a, b, "the report is what Optimise does");
    }

    /// `WG_NIF_CORPUS=<folder>`: every mesh in it (NifCorpus: 768), the same check.
    #[test]
    fn a_corpus_comes_back_the_same() {
        let Ok(dir) = std::env::var("WG_NIF_CORPUS") else { return };
        fn walk(d: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
            for e in std::fs::read_dir(d).into_iter().flatten().flatten() {
                let p = e.path();
                if p.is_dir() {
                    walk(&p, out);
                } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("nif")) {
                    out.push(p);
                }
            }
        }
        let mut files = Vec::new();
        walk(std::path::Path::new(&dir), &mut files);
        let (mut done, mut skipped, mut unread) = (0, 0, 0);
        for f in files {
            let Ok(b) = std::fs::read(&f) else { continue };
            // What the crate cannot read or write back (one corpus mesh panics inside it,
            // TODO.md) is not this module's to check.
            let readable = std::panic::catch_unwind(|| NiStream::from_bytes(&b).ok().map(|mut s| s.save_bytes().is_ok()));
            if !matches!(readable, Ok(Some(true))) {
                unread += 1;
                continue;
            }
            for e in check(f.to_str().unwrap()) {
                if e.get("skipped").is_some() { skipped += 1 } else { done += 1 }
            }
        }
        eprintln!("corpus: {done} shapes ordered, {skipped} left alone, {unread} files the crate does not read");
    }
}
