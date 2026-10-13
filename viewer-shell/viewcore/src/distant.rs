//! Wraithguard: distant land - the exteriors past the loaded rings, drawn coarse.
//!
//! The rings (Preview, the cells pill) load whole cells: terrain, every object. Past
//! them nothing was drawn, and the world stopped at the edge of the slab. OpenMW draws
//! on to its viewing distance (`[Camera] viewing distance`, with `[Terrain] distant
//! terrain`), so the viewer now does the terrain of what is out there too: per cell a
//! height grid coarser the further it is, and at each point the ground's colour - the
//! land texture's one colour (the cell map's palette, `worldmap::palette`) times the
//! record's vertex colour (VCLR), as Robin's map tile paints it (`worldmap::tile`, MIT,
//! whose texel lookup and blend this follows). No textures cross: at that distance a
//! texel of the ground is a few pixels, and its colour is all that reads.
//!
//! Unlike the map tile, no sea is painted on: the seabed is the seabed, at its own
//! height and colour, and the viewer's water sheet goes on over it (26_water.js), so the
//! sea out there is the same water as the sea close by, deep where the ground is deep. A
//! cell with no landscape record is OpenMW's default ground: flat, at
//! `DEFAULT_HEIGHT` (-2048, ESM::Land), in the default land texture's colour - open sea,
//! as the game has it.
//!
//! Packed for the page: `"GDND"`, a little-endian u32 count, then per cell an i32 x, an
//! i32 y, a u32 `n` (points a side), `n * n` f32 heights (row 0 south, as the record) and
//! `n * n` RGB bytes.

use std::collections::HashMap;

use crate::land::{Land, CELL_SIZE, SUBDIV, VERTS};
use crate::world::World;
use crate::worldmap::UNKNOWN;

/// The height of a cell with no landscape record (OpenMW's `ESM::Land::DEFAULT_HEIGHT`).
pub const DEFAULT_HEIGHT: f32 = -2048.0;

/// One land texture's colour - Robin's `worldmap::texture_colour` (the mean of its small
/// image), reading a DDS only as far as a 64-pixel mip (`img::dds_trim`) rather than
/// whole. His reads every land texture of the load order in full: on a large modded
/// install, gigabytes, minutes, and every worker thread with them - which held a cell's
/// own load up behind it (2026-10-13). The colour is the same mean, of a smaller copy.
pub fn texture_colour(v: &crate::vfs::Vfs, file: &str) -> Option<[u8; 3]> {
    let n = crate::vfs::norm(file);
    let loc = v.resolve_texture(&n).or_else(|| v.resolve(&n))?;
    let bytes = v
        .load_range(&loc, 0, crate::img::DDS_HEAD)
        .and_then(|head| crate::img::dds_trim(&head, 64))
        .and_then(|(at, mut head)| {
            head.extend(v.load_range(&loc, at, usize::MAX)?);
            Some(head)
        })
        .or_else(|| v.load(&loc))?;
    let t = crate::img::read_with(&bytes, &n, true, crate::img::Extra::default()).ok()?;
    let (w, h, px) = match &t.thumb {
        Some((w, h, px)) => (*w, *h, px.as_slice()),
        None => match t.kind {
            crate::img::Kind::Rgba => {
                let (w, h, px) = t.levels.first()?;
                (*w, *h, px.as_slice())
            }
            _ => return None,
        },
    };
    crate::worldmap::mean_rgb(w as usize, h as usize, px)
}

/// The palette every land texture is painted with (Robin's `worldmap::palette`, MIT, the
/// same walk), with the cheap read above. The cell map uses it too.
pub fn palette(world: &World, vfs: &crate::vfs::Vfs) -> HashMap<String, [u8; 3]> {
    let mut ids: Vec<String> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (_, l) in world.real_lands() {
        for v in l.vtex.iter() {
            if let Some(id) = world.ltex_for(*v, l.plugin) {
                let k = id.to_ascii_lowercase();
                if seen.insert(k.clone()) {
                    ids.push(k);
                }
            }
        }
    }
    let colours = crate::pool::par_map(&ids, |_, id| {
        let file = world.ltex_file.get(id).map(String::as_str).unwrap_or("");
        let c = if file.is_empty() { None } else { texture_colour(vfs, file) };
        (id.clone(), c.unwrap_or(UNKNOWN))
    });
    let mut out: HashMap<String, [u8; 3]> = colours.into_iter().collect();
    // The engine's own land texture, which index 0 stands for, has a file and no record.
    out.entry(crate::world::DEFAULT_LTEX.to_string())
        .or_insert_with(|| texture_colour(vfs, crate::world::DEFAULT_LTEX_FILE).unwrap_or(UNKNOWN));
    out
}

/// Points a side for a cell `d` rings past the loaded ones: 17 (a point every 512
/// units, the texture grid's own spacing) for the nearest band, then 9, then 5.
pub fn points_for(d: i32) -> usize {
    if d <= 4 {
        17
    } else if d <= 12 {
        9
    } else {
        5
    }
}

/// The ground's colour at one of the record's 16x16 texels.
fn texel_colour(world: &World, land: &Land, pal: &HashMap<String, [u8; 3]>, ti: usize, tj: usize) -> [f32; 3] {
    let v = land.vtex[tj * SUBDIV + ti];
    let c = world.ltex_for(v, land.plugin).and_then(|id| pal.get(&id.to_ascii_lowercase())).copied().unwrap_or(UNKNOWN);
    [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0]
}

/// One cell at `n` points a side: its heights and colours.
pub fn cell(world: &World, land: &Land, pal: &HashMap<String, [u8; 3]>, n: usize) -> (Vec<f32>, Vec<u8>) {
    let n = n.clamp(2, VERTS);
    let stride = (VERTS - 1) / (n - 1);
    let mut tex = [[0f32; 3]; SUBDIV * SUBDIV];
    for tj in 0..SUBDIV {
        for ti in 0..SUBDIV {
            tex[tj * SUBDIV + ti] = texel_colour(world, land, pal, ti, tj);
        }
    }
    let texel = CELL_SIZE / SUBDIV as f32;
    let mut heights = Vec::with_capacity(n * n);
    let mut colours = Vec::with_capacity(n * n * 3);
    for j in 0..n {
        for i in 0..n {
            let (vx, vy) = ((i * stride).min(VERTS - 1), (j * stride).min(VERTS - 1));
            let h = land.heights[vy * VERTS + vx];
            let (lx, ly) = (vx as f32 * CELL_SIZE / (VERTS - 1) as f32, vy as f32 * CELL_SIZE / (VERTS - 1) as f32);
            // The four nearest texel centres, blended (as the map tile does).
            let u = (lx / texel - 0.5).clamp(0.0, (SUBDIV - 1) as f32);
            let w = (ly / texel - 0.5).clamp(0.0, (SUBDIV - 1) as f32);
            let (u0, w0) = (u.floor() as usize, w.floor() as usize);
            let (u1, w1) = ((u0 + 1).min(SUBDIV - 1), (w0 + 1).min(SUBDIV - 1));
            let (fu, fw) = (u - u0 as f32, w - w0 as f32);
            let mut c = [0f32; 3];
            for k in 0..3 {
                let a = tex[w0 * SUBDIV + u0][k] * (1.0 - fu) + tex[w0 * SUBDIV + u1][k] * fu;
                let b = tex[w1 * SUBDIV + u0][k] * (1.0 - fu) + tex[w1 * SUBDIV + u1][k] * fu;
                c[k] = a * (1.0 - fw) + b * fw;
            }
            // The vertex colour, white where the record has none (as the game draws it).
            if let Some(vc) = &land.colours {
                for k in 0..3 {
                    c[k] *= vc[(vy * VERTS + vx) * 3 + k] as f32 / 255.0;
                }
            }
            heights.push(h);
            for v in c {
                colours.push((v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8);
            }
        }
    }
    (heights, colours)
}

/// Every exterior cell within `radius` of `centre` and outside the `inner` rings the
/// viewer loads whole, packed (see the module). A cell with no landscape is the default
/// ground, flat at `DEFAULT_HEIGHT`, at two points a side.
pub fn pack(world: &World, pal: &HashMap<String, [u8; 3]>, centre: (i32, i32), inner: i32, radius: i32) -> Vec<u8> {
    let default_colour: Vec<u8> = pal.get(&crate::world::DEFAULT_LTEX.to_ascii_lowercase()).copied().unwrap_or(UNKNOWN).to_vec();
    let mut grids = Vec::new();
    for y in centre.1 - radius..=centre.1 + radius {
        for x in centre.0 - radius..=centre.0 + radius {
            let d = (x - centre.0).abs().max((y - centre.1).abs());
            if d > inner {
                grids.push(((x, y), d - inner));
            }
        }
    }
    let cells = crate::pool::par_map(&grids, |_, (g, d)| match world.lands.get(g).filter(|_| *g != crate::world::PATCH_GRID) {
        Some(land) => {
            let n = points_for(*d);
            let (h, c) = cell(world, land, pal, n);
            (n, h, c)
        }
        None => (2, vec![DEFAULT_HEIGHT; 4], default_colour.repeat(4)),
    });
    let mut out = Vec::new();
    out.extend_from_slice(b"GDND");
    out.extend_from_slice(&(grids.len() as u32).to_le_bytes());
    for ((g, _), (n, h, c)) in grids.iter().zip(cells) {
        out.extend_from_slice(&g.0.to_le_bytes());
        out.extend_from_slice(&g.1.to_le_bytes());
        out.extend_from_slice(&(n as u32).to_le_bytes());
        for v in h {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(&c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coarser_with_distance() {
        assert_eq!(points_for(1), 17);
        assert_eq!(points_for(8), 9);
        assert_eq!(points_for(20), 5);
        // Every count divides the record's 64 intervals evenly, so a point is a vertex.
        for n in [17usize, 9, 5, 2] {
            assert_eq!((VERTS - 1) % (n - 1), 0);
        }
    }
}
