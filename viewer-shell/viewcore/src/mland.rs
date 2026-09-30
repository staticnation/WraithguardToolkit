//! Wraithguard: the merged-lands preview - one cell's landscape as Merged Lands would
//! build it, next to the load order's.
//!
//! The load order keeps the last plugin's LAND whole. Merged Lands (David Von Derau, MIT;
//! Wraithguard's Python port is `wraithguard/land`) measures each plugin's edit against a
//! reference - the masters' ground - and folds the edits together: a vertex only one mod
//! moved keeps that mod's edit, and a vertex two mods moved is settled by a strategy.
//! This is that fold for one cell, on demand, so the cell viewer can show a cell either
//! way: the same per-vertex rules as `native/src/land.rs` `merge_grids` (heights, vertex
//! colours and textures; textures only ever overwrite or ignore, never blend). The whole
//! run's seam repair and slope limiting are not here - they work across cells, and this
//! answers one.
//!
//! The versions are read back from the plugin files (`World::land_edits` keeps where
//! each LAND record is), so the world holds no second copy of anybody's landscape.

use std::io::{Read, Seek, SeekFrom};

use crate::land::{Land, SUBDIV, VERTS};
use crate::world::{decode_land, World, DEFAULT_LTEX};

/// How a vertex two plugins both moved is settled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Strategy {
    /// The later plugin's edit (Merged Lands' default, and the load order's own rule
    /// applied per vertex).
    Overwrite,
    /// The earlier plugin's edit.
    Ignore,
    /// The two edits averaged, weighted towards the larger.
    Resolve,
    /// Weighted by how much structure each edit adds (heights only).
    Curvature,
}

impl Strategy {
    pub fn parse(s: &str) -> Option<Strategy> {
        match s.to_ascii_lowercase().as_str() {
            "overwrite" | "merged" | "auto" => Some(Strategy::Overwrite),
            "ignore" => Some(Strategy::Ignore),
            "resolve" => Some(Strategy::Resolve),
            "curvature" => Some(Strategy::Curvature),
            _ => None,
        }
    }
}

/// One cell, merged.
pub struct Merged {
    /// The merged ground, normals left to be derived from the heights. `vtex` holds
    /// indices into `textures` plus one (0 is the default ground).
    pub land: Land,
    /// The texture ids the merged `vtex` names, by index minus one.
    pub textures: Vec<String>,
    /// Height vertices two or more plugins moved (row-major index into 65 x 65).
    pub contested: Vec<u32>,
    /// Of those, the ones whose compromise sits far from an edit (Resolve / Curvature).
    pub major: Vec<u32>,
    /// Height vertices only one later plugin moved - the edits the load order throws away
    /// when a later plugin rewrites the cell.
    pub mergeable: usize,
    /// The plugins whose LAND took part, reference first (indices into `World::plugins`).
    pub editors: Vec<usize>,
}

/// Every plugin's LAND for `grid`, in load order, read back from the files.
pub fn versions(w: &World, grid: (i32, i32)) -> Vec<Land> {
    let Some(list) = w.land_edits.get(&grid) else { return Vec::new() };
    let mut out = Vec::new();
    for &(pi, at, len) in list {
        let Some(path) = w.plugin_paths.get(pi) else { continue };
        let Ok(mut f) = std::fs::File::open(path) else { continue };
        if f.seek(SeekFrom::Start(at)).is_err() {
            continue;
        }
        let mut buf = vec![0u8; len as usize];
        if f.read_exact(&mut buf).is_err() || buf.len() < 16 {
            continue;
        }
        if let Some(l) = decode_land(&buf[16..], pi as i32) {
            out.push(l);
        }
    }
    out
}

fn is_master(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.ends_with(".esm") || n.ends_with(".omwgame")
}

/// A layer's edits against the reference: the deltas, `comps` per vertex, and which
/// vertices moved.
struct Rel {
    delta: Vec<i64>,
    changed: Vec<bool>,
}

fn rel(reference: &[i64], plugin: &[i64], comps: usize) -> Rel {
    let n = reference.len() / comps;
    let mut delta = vec![0i64; reference.len()];
    let mut changed = vec![false; n];
    for v in 0..n {
        for c in 0..comps {
            let d = plugin[v * comps + c] - reference[v * comps + c];
            delta[v * comps + c] = d;
            if d != 0 {
                changed[v] = true;
            }
        }
    }
    Rel { delta, changed }
}

/// `native/src/land.rs` `average_delta`: towards the larger edit, and whether the
/// compromise is major.
fn average_delta(first: i64, second: i64) -> (i64, bool) {
    let (s1, s2) = (first.abs() as f64, second.abs() as f64);
    let total = s1 + s2;
    if total == 0.0 {
        return (0, false);
    }
    let weight = s1 / total;
    let biased = weight.powf(1.5);
    let other = (1.0 - weight).powf(1.5);
    let weight = biased / (biased + other);
    let blended = weight * first as f64 + (1.0 - weight) * second as f64;
    (blended.trunc() as i64, is_major(first, second, blended))
}

fn is_major(first: i64, second: i64, blended: f64) -> bool {
    let smaller = first.min(second) as f64;
    let threshold = (0.3 * smaller).max(10.0).min(64.0);
    (smaller - blended).abs() >= threshold
}

fn weighted_delta(first: i64, second: i64, w1: f64, w2: f64) -> (i64, bool) {
    let total = w1 + w2;
    if total <= 0.0 {
        return average_delta(first, second);
    }
    let share = w1 / total;
    let blended = share * first as f64 + (1.0 - share) * second as f64;
    (blended.trunc() as i64, is_major(first, second, blended))
}

const HS: f64 = 8.0;

fn normal_at(rows: &[f64], x: usize, y: usize) -> (f64, f64, f64) {
    let limit = VERTS - 1;
    let fx = if x == limit { x - 1 } else { x };
    let fy = if y == limit { y - 1 } else { y };
    let step = 128.0 / HS;
    let here = rows[fy * VERTS + fx] / HS;
    let east = rows[fy * VERTS + fx + 1] / HS;
    let north = rows[(fy + 1) * VERTS + fx] / HS;
    let (nx, ny, nz) = (-(east - here) * step, -(north - here) * step, step * step);
    let l = (nx * nx + ny * ny + nz * nz).sqrt();
    if l == 0.0 { (0.0, 0.0, 1.0) } else { (nx / l, ny / l, nz / l) }
}

fn curvature_at(rows: &[f64], x: usize, y: usize) -> f64 {
    let here = normal_at(rows, x, y);
    let (mut total, mut n) = (0.0, 0);
    for (dx, dy) in [(1i64, 0i64), (0, 1), (-1, 0), (0, -1)] {
        let (nx, ny) = (x as i64 + dx, y as i64 + dy);
        if nx < 0 || ny < 0 || nx >= VERTS as i64 || ny >= VERTS as i64 {
            continue;
        }
        let o = normal_at(rows, nx as usize, ny as usize);
        total += (here.0 * o.0 + here.1 * o.1 + here.2 * o.2).clamp(-1.0, 1.0).acos();
        n += 1;
    }
    if n > 0 { total / n as f64 } else { 0.0 }
}

/// Folds `second` into `acc` (both against `reference`), vertex by vertex. Returns the
/// vertices both moved, the major ones, and how many only `second` moved.
fn fold(acc: &mut Rel, second: &Rel, reference: &[i64], comps: usize, s: Strategy) -> (Vec<u32>, Vec<u32>, usize) {
    let n = acc.changed.len();
    let (mut contested, mut major, mut only_two) = (Vec::new(), Vec::new(), 0usize);
    let surfaces = (s == Strategy::Curvature && comps == 1).then(|| {
        let r: Vec<f64> = reference.iter().map(|&v| v as f64).collect();
        let t1: Vec<f64> = reference.iter().zip(&acc.delta).map(|(r, d)| (r + d) as f64).collect();
        let t2: Vec<f64> = reference.iter().zip(&second.delta).map(|(r, d)| (r + d) as f64).collect();
        (r, t1, t2)
    });
    for v in 0..n {
        let (m1, m2) = (acc.changed[v], second.changed[v]);
        if !m2 {
            continue;
        }
        if !m1 {
            only_two += 1;
            acc.delta[v * comps..(v + 1) * comps].copy_from_slice(&second.delta[v * comps..(v + 1) * comps]);
            acc.changed[v] = true;
            continue;
        }
        contested.push(v as u32);
        match s {
            Strategy::Overwrite => {
                acc.delta[v * comps..(v + 1) * comps].copy_from_slice(&second.delta[v * comps..(v + 1) * comps]);
            }
            Strategy::Ignore => {}
            Strategy::Resolve | Strategy::Curvature => {
                let mut worst = false;
                if let Some((r, t1, t2)) = &surfaces {
                    let (x, y) = (v % VERTS, v / VERTS);
                    let base = curvature_at(r, x, y);
                    let a1 = (curvature_at(t1, x, y) - base).max(0.0);
                    let a2 = (curvature_at(t2, x, y) - base).max(0.0);
                    let (d1, d2) = (acc.delta[v], second.delta[v]);
                    let (val, sev) = weighted_delta(d1, d2, d1.abs() as f64 * (1.0 + a1 * 8.0), d2.abs() as f64 * (1.0 + a2 * 8.0));
                    acc.delta[v] = val;
                    worst = sev;
                } else {
                    for c in 0..comps {
                        let (val, sev) = average_delta(acc.delta[v * comps + c], second.delta[v * comps + c]);
                        acc.delta[v * comps + c] = val;
                        worst = worst || sev;
                    }
                }
                if worst {
                    major.push(v as u32);
                }
            }
        }
    }
    (contested, major, only_two)
}

/// `grid`'s landscape as Merged Lands would build it under `s`, or `None` when fewer than
/// two plugins edit it (then there is nothing to merge: the load order's ground is it).
pub fn merge(w: &World, grid: (i32, i32), s: Strategy) -> Option<Merged> {
    let vs = versions(w, grid);
    if vs.len() < 2 {
        return None;
    }
    // The reference: the last master's ground; the first version when no master has one.
    let name = |l: &Land| w.plugins.get(l.plugin.max(0) as usize).map(|s| s.as_str()).unwrap_or("");
    let ri = vs.iter().rposition(|l| is_master(name(l))).unwrap_or(0);
    let reference = &vs[ri];
    let later: Vec<&Land> = vs.iter().skip(ri + 1).collect();
    let mut editors = vec![reference.plugin.max(0) as usize];
    editors.extend(later.iter().map(|l| l.plugin.max(0) as usize));
    if later.is_empty() {
        return None;
    }

    // Heights, in whole units.
    let h = |l: &Land| l.heights.iter().map(|v| v.round() as i64).collect::<Vec<i64>>();
    let href = h(reference);
    let mut hacc = Rel { delta: vec![0; href.len()], changed: vec![false; href.len()] };
    let (mut contested, mut major, mut mergeable) = (Vec::new(), Vec::new(), 0usize);
    for (k, l) in later.iter().enumerate() {
        let r = rel(&href, &h(l), 1);
        let (c, m, two) = fold(&mut hacc, &r, &href, 1, s);
        if k > 0 {
            contested.extend(c);
            major.extend(m);
        }
        mergeable += two;
    }
    contested.sort_unstable();
    contested.dedup();
    major.sort_unstable();
    major.dedup();
    let mut heights = Box::new([0f32; VERTS * VERTS]);
    for (i, v) in heights.iter_mut().enumerate() {
        *v = (href[i] + hacc.delta[i]) as f32;
    }

    // Vertex colours: white where a record has none, as the game draws it.
    let c = |l: &Land| match &l.colours {
        Some(c) => c.iter().map(|&b| b as i64).collect::<Vec<i64>>(),
        None => vec![255i64; VERTS * VERTS * 3],
    };
    let any_colour = vs.iter().skip(ri).any(|l| l.colours.is_some());
    let colours = if any_colour {
        let cref = c(reference);
        let mut cacc = Rel { delta: vec![0; cref.len()], changed: vec![false; VERTS * VERTS] };
        let cs = if s == Strategy::Curvature { Strategy::Resolve } else { s };
        for l in &later {
            let r = rel(&cref, &c(l), 3);
            fold(&mut cacc, &r, &cref, 3, cs);
        }
        let mut out = Box::new([0u8; VERTS * VERTS * 3]);
        for (i, v) in out.iter_mut().enumerate() {
            *v = (cref[i] + cacc.delta[i]).clamp(0, 255) as u8;
        }
        Some(out)
    } else {
        None
    };

    // Textures: ids, never blended.
    let tex = |l: &Land| -> Vec<String> {
        l.vtex.iter().map(|&v| w.ltex_for(v, l.plugin).unwrap_or(DEFAULT_LTEX).to_string()).collect()
    };
    let tref = tex(reference);
    let mut tout = tref.clone();
    let mut tset = vec![false; SUBDIV * SUBDIV];
    for l in &later {
        let t = tex(l);
        for i in 0..t.len() {
            if t[i] == tref[i] {
                continue;
            }
            if tset[i] && s == Strategy::Ignore {
                continue;
            }
            tout[i] = t[i].clone();
            tset[i] = true;
        }
    }
    let mut textures: Vec<String> = Vec::new();
    let mut vtex = Box::new([0u16; SUBDIV * SUBDIV]);
    for (i, id) in tout.iter().enumerate() {
        if id == DEFAULT_LTEX {
            continue;
        }
        let k = match textures.iter().position(|x| x == id) {
            Some(k) => k,
            None => {
                textures.push(id.clone());
                textures.len() - 1
            }
        };
        vtex[i] = (k + 1) as u16;
    }

    let land = Land { grid, heights, normals: None, colours, vtex, plugin: later.last().map(|l| l.plugin).unwrap_or(-1) };
    Some(Merged { land, textures, contested, major, mergeable, editors })
}

/// The texture id at a merged cell's sub-cell `(i, j)` (0..16), `""` for default ground.
pub fn texture_of(m: &Merged, i: usize, j: usize) -> &str {
    let v = m.land.vtex[j * SUBDIV + i];
    if v == 0 { DEFAULT_LTEX } else { m.textures.get(v as usize - 1).map(|s| s.as_str()).unwrap_or("") }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rel_of(reference: &[i64], plugin: &[i64]) -> Rel {
        rel(reference, plugin, 1)
    }

    #[test]
    fn disjoint_edits_both_survive() {
        let r = vec![0i64; 4];
        let mut acc = rel_of(&r, &[10, 0, 0, 0]);
        let b = rel_of(&r, &[0, 0, 20, 0]);
        let (c, _, only) = fold(&mut acc, &b, &r, 1, Strategy::Overwrite);
        assert!(c.is_empty());
        assert_eq!(only, 1);
        assert_eq!(acc.delta, vec![10, 0, 20, 0]);
    }

    #[test]
    fn a_contested_vertex_follows_the_strategy() {
        let r = vec![0i64; 2];
        for (s, want) in [(Strategy::Overwrite, 20), (Strategy::Ignore, 800)] {
            let mut acc = rel_of(&r, &[800, 0]);
            let b = rel_of(&r, &[20, 0]);
            let (c, _, _) = fold(&mut acc, &b, &r, 1, s);
            assert_eq!(c, vec![0]);
            assert_eq!(acc.delta[0], want, "{s:?}");
        }
        // Resolve leans towards the larger edit.
        let mut acc = rel_of(&r, &[800, 0]);
        fold(&mut acc, &rel_of(&r, &[20, 0]), &r, 1, Strategy::Resolve);
        assert!(acc.delta[0] > 700, "{}", acc.delta[0]);
    }
}
