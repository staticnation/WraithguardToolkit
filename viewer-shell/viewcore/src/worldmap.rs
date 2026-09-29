//! The cell picker's map — round 18cp.
//!
//! Robin: "When Map is selected, I want to paint a map of the world a similar way as a
//! fully discovered map is done by OpenMW, in the place where the list is." And, on
//! being shown the height-coloured version: "What I want is to make something like the
//! revealed ingame map from OpenMW, so we might also need to see the ground texture in
//! the cell tiles."
//!
//! He was right about which layer that is. OpenMW's map window has two: a base painted
//! from the LAND records' coarse WNAM heights — sea in blues, land green to brown to grey
//! — which is what you see where you have not been, and over it, for every cell you have
//! visited, the *local map*: a top-down render of the cell itself, ground textures and
//! all, scaled down to eighteen pixels. A fully revealed map is entirely the second
//! layer, and that is what this paints.
//!
//! # What a tile is
//!
//! One cell, [`TILE`] pixels square, RGBA, **north up** — row 0 is the cell's north
//! edge, which is the opposite of how a LAND record stores it (row 0 is south). Each
//! pixel is:
//!
//! 1. its ground texture's **one colour** — the average of the texture, from the same
//!    small mip the swatch grid uses (`img::Texture::thumb`). At eighteen pixels a cell
//!    a texel is about a pixel, so a texel can only ever be one colour; Robin chose this
//!    over sampling the real pixels, which is noisier at that size and slower to first
//!    paint. Blended bilinearly between the four nearest texels, so a border between two
//!    grounds is a soft edge as the game draws it rather than a step;
//! 2. multiplied by the **vertex colour** at that point — VCLR is where the brown of a
//!    worn path and the dark under a cliff come from, and a map without it is flatter
//!    and brighter than the world it is a map of (round 11 item 11 said the same of the
//!    viewport);
//! 3. and, where the ground is **below sea level**, the sea laid over it: a translucent
//!    blue that deepens with depth, so the seabed shows through in the shallows the way it
//!    does on the local map. Sea level is 0 in every exterior.
//!
//! Terrain only, at Robin's word — no objects. The engine knows every object's footprint
//! and could stamp them; that is the next step if towns need to read as towns.
//!
//! # Why the engine paints it
//!
//! The page could: it has the vtex, the colours and the heights of a cell already, for
//! the viewport. But it has them for *one* cell at a time, at 65×65 floats a cell, and a
//! Tamriel Rebuilt world is four thousand cells. Handing 21 KB a cell across the shell so
//! the page can reduce each one to a 4 KB tile is the wrong way round; the world is
//! already parsed on this side, so the tile is made here and only the tile crosses.

use std::collections::HashMap;

use crate::img;
use crate::land::{Land, CELL_SIZE, SUBDIV, VERTS};
use crate::vfs::Vfs;
use crate::world::{World, DEFAULT_LTEX};

/// Pixels per cell in a tile, on each side. Two per ground texel, so the blend between
/// texels has a pixel to happen in; the page shows it at eighteen by default and this
/// keeps a zoom to thirty-six from going soft.
pub const TILE: usize = 32;

/// The sea, as the local map shows it: a blue that reads as water on every ground the
/// world has, laid over the seabed rather than replacing it.
const SEA: [f32; 3] = [0.16, 0.34, 0.47];
/// How opaque the sea is in the shallows, and at [`SEA_DEEP`] units down and beyond.
const SEA_ALPHA_SHALLOW: f32 = 0.50;
const SEA_ALPHA_DEEP: f32 = 0.86;
const SEA_DEEP: f32 = 1500.0;

/// What a texel is painted when its texture cannot be read at all — a mid earth, so a
/// missing file is a dull patch rather than a hole or a shout.
pub const UNKNOWN: [u8; 3] = [0x6e, 0x64, 0x54];

/// The one colour of a land texture: the mean of its swatch thumbnail.
///
/// The thumbnail is decoded from the smallest mip that is still big enough to look like
/// anything (`img::dds`), which is the cheap end of the chain — a 4096² top level is never
/// touched. A texture with no mips is decoded whole, as the swatch grid already does.
pub fn texture_colour(v: &Vfs, file: &str) -> Option<[u8; 3]> {
    let n = crate::vfs::norm(file);
    let loc = v.resolve_texture(&n).or_else(|| v.resolve(&n))?;
    let bytes = v.load(&loc)?;
    let t = img::read(&bytes, &n, true).ok()?;
    let (w, h, px) = match &t.thumb {
        Some((w, h, px)) => (*w, *h, px.as_slice()),
        None => match t.kind {
            img::Kind::Rgba => {
                let (w, h, px) = t.levels.first()?;
                (*w, *h, px.as_slice())
            }
            // A PNG or JPEG the browser reads better than we do: nothing here can open
            // it. Rare for a land texture; the caller paints `UNKNOWN`.
            _ => return None,
        },
    };
    mean_rgb(w as usize, h as usize, px)
}

/// The mean of an RGBA buffer, alpha ignored — a land texture has none worth reading.
pub fn mean_rgb(w: usize, h: usize, px: &[u8]) -> Option<[u8; 3]> {
    let n = w * h;
    if n == 0 || px.len() < n * 4 {
        return None;
    }
    let (mut r, mut g, mut b) = (0u64, 0u64, 0u64);
    for p in px.chunks_exact(4).take(n) {
        r += p[0] as u64;
        g += p[1] as u64;
        b += p[2] as u64;
    }
    let n = n as u64;
    Some([(r / n) as u8, (g / n) as u8, (b / n) as u8])
}

/// The colour of every land texture the world's landscape uses, keyed by LTEX id in
/// lower case — the key `World::ltex_file` uses.
///
/// Walks the lands rather than the LTEX table: a load order carries hundreds of LTEX
/// records nothing paints with, and each one costs a file read.
pub fn palette(world: &World, vfs: &Vfs) -> HashMap<String, [u8; 3]> {
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
    // Each texture is one file read and one small decode; a few hundred of them is a
    // second or two on one thread and a fraction of that across the pool.
    let colours = crate::pool::par_map(&ids, |_, id| {
        let file = world.ltex_file.get(id).map(String::as_str).unwrap_or("");
        let c = if file.is_empty() { None } else { texture_colour(vfs, file) };
        (id.clone(), c.unwrap_or(UNKNOWN))
    });
    let mut out: HashMap<String, [u8; 3]> = colours.into_iter().collect();
    // The engine's own land texture, which index 0 stands for, has a file and no record.
    out.entry(DEFAULT_LTEX.to_string())
        .or_insert_with(|| texture_colour(vfs, crate::world::DEFAULT_LTEX_FILE).unwrap_or(UNKNOWN));
    out
}

/// The texture colour at texel `(ti, tj)` of a land, `tj` counted from the south.
#[inline]
fn texel_colour(world: &World, land: &Land, pal: &HashMap<String, [u8; 3]>, ti: usize, tj: usize) -> [f32; 3] {
    let v = land.vtex[tj * SUBDIV + ti];
    let c = world
        .ltex_for(v, land.plugin)
        .and_then(|id| pal.get(&id.to_ascii_lowercase()))
        .copied()
        .unwrap_or(UNKNOWN);
    [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0]
}

/// Bilinear over a 65×65 grid of `stride` channels, `y` counted from the south.
#[inline]
fn bilinear(grid: &[u8], stride: usize, x: f32, y: f32, ch: usize) -> f32 {
    let x = x.clamp(0.0, (VERTS - 1) as f32);
    let y = y.clamp(0.0, (VERTS - 1) as f32);
    let x0 = x.floor() as usize;
    let y0 = y.floor() as usize;
    let x1 = (x0 + 1).min(VERTS - 1);
    let y1 = (y0 + 1).min(VERTS - 1);
    let (tx, ty) = (x - x0 as f32, y - y0 as f32);
    let at = |xx: usize, yy: usize| grid[(yy * VERTS + xx) * stride + ch] as f32 / 255.0;
    let a = at(x0, y0) * (1.0 - tx) + at(x1, y0) * tx;
    let b = at(x0, y1) * (1.0 - tx) + at(x1, y1) * tx;
    a * (1.0 - ty) + b * ty
}

/// One cell's tile: `TILE × TILE` RGBA, north up.
pub fn tile(world: &World, land: &Land, pal: &HashMap<String, [u8; 3]>) -> Vec<u8> {
    let mut out = vec![0u8; TILE * TILE * 4];
    // The sixteen texel colours, resolved once: every pixel reads four of them.
    let mut tex = [[0f32; 3]; SUBDIV * SUBDIV];
    for tj in 0..SUBDIV {
        for ti in 0..SUBDIV {
            tex[tj * SUBDIV + ti] = texel_colour(world, land, pal, ti, tj);
        }
    }
    let step = CELL_SIZE / TILE as f32;
    for py in 0..TILE {
        // Row 0 is north: the cell's far edge in y.
        let ly = CELL_SIZE - (py as f32 + 0.5) * step;
        for px in 0..TILE {
            let lx = (px as f32 + 0.5) * step;

            // 1. The ground: the four nearest texel centres, blended.
            let u = (lx / (CELL_SIZE / SUBDIV as f32) - 0.5).clamp(0.0, (SUBDIV - 1) as f32);
            let w = (ly / (CELL_SIZE / SUBDIV as f32) - 0.5).clamp(0.0, (SUBDIV - 1) as f32);
            let (u0, w0) = (u.floor() as usize, w.floor() as usize);
            let (u1, w1) = ((u0 + 1).min(SUBDIV - 1), (w0 + 1).min(SUBDIV - 1));
            let (fu, fw) = (u - u0 as f32, w - w0 as f32);
            let mut c = [0f32; 3];
            for k in 0..3 {
                let a = tex[w0 * SUBDIV + u0][k] * (1.0 - fu) + tex[w0 * SUBDIV + u1][k] * fu;
                let b = tex[w1 * SUBDIV + u0][k] * (1.0 - fu) + tex[w1 * SUBDIV + u1][k] * fu;
                c[k] = a * (1.0 - fw) + b * fw;
            }

            // 2. The vertex colour, where the record has one; white where it does not,
            //    which is what the game draws too.
            if let Some(vc) = &land.colours {
                let vx = lx / (CELL_SIZE / (VERTS - 1) as f32);
                let vy = ly / (CELL_SIZE / (VERTS - 1) as f32);
                for k in 0..3 {
                    c[k] *= bilinear(vc.as_ref(), 3, vx, vy, k);
                }
            }

            // 3. The sea, over anything under it.
            let h = land.height_at(lx.min(CELL_SIZE - 0.01), ly.min(CELL_SIZE - 0.01));
            if h < 0.0 {
                let d = (-h / SEA_DEEP).min(1.0);
                let a = SEA_ALPHA_SHALLOW + (SEA_ALPHA_DEEP - SEA_ALPHA_SHALLOW) * d;
                for k in 0..3 {
                    c[k] = c[k] * (1.0 - a) + SEA[k] * a;
                }
            }

            let o = (py * TILE + px) * 4;
            out[o] = (c[0].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            out[o + 1] = (c[1].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            out[o + 2] = (c[2].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            out[o + 3] = 255;
        }
    }
    out
}

/// The grid positions of every real land in the world, north-up row order — by `y`
/// descending, then `x` — so a page reading them in batches fills the map from the top.
pub fn cells(world: &World) -> Vec<(i32, i32)> {
    let mut v: Vec<(i32, i32)> = world.real_lands().map(|(g, _)| *g).collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    v
}

/// A batch of tiles, packed for the page: `"GDNM"`, then a little-endian u32 count,
/// then per cell an i32 x, an i32 y and `TILE × TILE × 4` bytes.
pub fn pack(world: &World, pal: &HashMap<String, [u8; 3]>, grids: &[(i32, i32)]) -> Vec<u8> {
    let tiles = crate::pool::par_map(grids, |_, g| world.lands.get(g).map(|l| tile(world, l, pal)));
    let n = tiles.iter().filter(|t| t.is_some()).count();
    let mut out = Vec::with_capacity(8 + n * (8 + TILE * TILE * 4));
    out.extend_from_slice(b"GDNM");
    out.extend_from_slice(&(n as u32).to_le_bytes());
    for (g, t) in grids.iter().zip(tiles) {
        if let Some(t) = t {
            out.extend_from_slice(&g.0.to_le_bytes());
            out.extend_from_slice(&g.1.to_le_bytes());
            out.extend_from_slice(&t);
        }
    }
    out
}
