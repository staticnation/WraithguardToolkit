//! The merged plugin's records (`land/emit.py`): a header, one `LTEX` per land texture the
//! merge paints with, one `LAND` per merged cell. Written with the tes3 crate, as the
//! Python's native writer writes them.
//!
//! The masters list is the dangerous part: a merged plugin must declare the plugins its
//! terrain came from, with the sizes the engine expects, or the game resolves nothing
//! correctly - so it is the caller's to give, and an empty one is refused.

use tes3::esp::{FixedString, Header, Landscape, LandscapeFlags, LandscapeTexture, Plugin, TES3Object};

use crate::grid::{LAND_NUM_VERTS, LAND_SIZE, WNAM_HEIGHT_SCALE, WNAM_SIZE, WNAM_STEP};
use crate::heights::{encode_vertex_heights, normals_from_heights};
use crate::layers::swizzle_texture_indices;
use crate::textures::KnownTexture;

pub const DESCRIPTION: &str = "Merged landscape. Load last.";
pub const AUTHOR: &str = "Wraithguard Toolkit";

/// Python's `round`: half to even.
fn round_half_even(v: f64) -> f64 {
    let r = v.round();
    if (v - v.trunc()).abs() == 0.5 { 2.0 * (v / 2.0).round() } else { r }
}

/// The 9x9 world map from a cell's heights (`world_map_from_heights`): every eighth vertex
/// over 128, as the engine and the Construction Set derive it.
pub fn world_map_from_heights(heights: &[i64]) -> Vec<i64> {
    let mut out = Vec::with_capacity(WNAM_SIZE * WNAM_SIZE);
    for gy in 0..WNAM_SIZE {
        for gx in 0..WNAM_SIZE {
            let h = heights[gy * WNAM_STEP * LAND_SIZE + gx * WNAM_STEP] as f64;
            out.push((round_half_even(h / WNAM_HEIGHT_SCALE) as i64).clamp(-128, 127));
        }
    }
    out
}

/// A world-map vertex over terrain below sea level made to read as water
/// (`sink_underwater_land`); nothing is pushed toward land.
pub fn sink_underwater_land(world_map: &[i64], heights: &[i64]) -> Vec<i64> {
    let mut out = Vec::with_capacity(WNAM_SIZE * WNAM_SIZE);
    for gy in 0..WNAM_SIZE {
        for gx in 0..WNAM_SIZE {
            let h = heights[gy * WNAM_STEP * LAND_SIZE + gx * WNAM_STEP];
            let w = world_map[gy * WNAM_SIZE + gx];
            out.push(if h < 0 && w >= 0 { -1 } else { w });
        }
    }
    out
}

/// One merged `LAND` record (`build_landscape_record`): its layers, and the vertices whose
/// height step had to be clamped. Normals are recomputed from the heights when not given,
/// since terrain whose heights moved is lit by its normals. `Err` when given no heights or
/// colours (textures are attached after, by [`attach_texture_indices`]).
pub fn build_landscape_record(
    coords: (i64, i64),
    heights: Option<&[i64]>,
    normals: Option<&[i64]>,
    world_map: Option<&[i64]>,
    colors: Option<&[i64]>,
) -> Result<(Landscape, Vec<(usize, usize)>), String> {
    if heights.is_none() && colors.is_none() {
        return Err(format!(
            "cell ({}, {}) was given no layers to write. An empty LAND record would replace whatever the load order had there with nothing.",
            coords.0, coords.1
        ));
    }
    let mut r = Landscape { grid: (coords.0 as i32, coords.1 as i32), ..Default::default() };
    let mut flags = LandscapeFlags::empty();
    let mut clamped = Vec::new();
    let mut world: Option<Vec<i64>> = world_map.map(<[i64]>::to_vec);
    if let Some(h) = heights {
        let (offset, deltas, c) = encode_vertex_heights(h);
        clamped = c;
        r.vertex_heights.offset = offset;
        for (i, d) in deltas.iter().enumerate() {
            r.vertex_heights.data[i / LAND_SIZE][i % LAND_SIZE] = *d;
        }
        let computed;
        let n = match normals {
            Some(n) => n,
            None => {
                computed = normals_from_heights(&h.iter().map(|&v| v as f64).collect::<Vec<_>>());
                &computed
            }
        };
        for i in 0..LAND_NUM_VERTS {
            for k in 0..3 {
                r.vertex_normals.data[i / LAND_SIZE][i % LAND_SIZE][k] = n[i * 3 + k].clamp(-128, 127) as i8;
            }
        }
        flags |= LandscapeFlags::USES_VERTEX_HEIGHTS_AND_NORMALS;
        if world.as_ref().is_none_or(|w| w.iter().all(|&v| v == 0)) {
            world = Some(world_map_from_heights(h));
        }
        world = world.map(|w| sink_underwater_land(&w, h));
    }
    if let Some(w) = &world {
        for (i, v) in w.iter().enumerate() {
            r.world_map_data.data[i / WNAM_SIZE][i % WNAM_SIZE] = (*v).clamp(-128, 127) as i8;
        }
    }
    if let Some(c) = colors {
        for i in 0..LAND_NUM_VERTS {
            for k in 0..3 {
                r.vertex_colors.data[i / LAND_SIZE][i % LAND_SIZE][k] = c[i * 3 + k].clamp(0, 255) as u8;
            }
        }
        flags |= LandscapeFlags::USES_VERTEX_COLORS;
    }
    r.landscape_flags = flags;
    Ok((r, clamped))
}

/// A texture grid (row-major, written values) added to a built record, and declared
/// (`attach_texture_indices`).
pub fn attach_texture_indices(r: &mut Landscape, rows: &[i64]) {
    *r.texture_indices.data = swizzle_texture_indices(rows);
    r.landscape_flags |= LandscapeFlags::USES_TEXTURES;
}

/// One `LTEX` per texture kept, in index order (`build_texture_records`).
pub fn build_texture_records(textures: &[KnownTexture]) -> Vec<LandscapeTexture> {
    textures
        .iter()
        .map(|t| LandscapeTexture { id: t.identifier.clone(), index: t.index as u32, file_name: t.file_name.clone().unwrap_or_default(), ..Default::default() })
        .collect()
}

/// The plugin's bytes (`build_plugin`, then the native writer): the header declaring
/// `masters`, then `records` in order.
pub fn plugin_bytes(records: Vec<TES3Object>, masters: &[(String, u64)]) -> Result<Vec<u8>, String> {
    if masters.is_empty() {
        return Err("a merged landscape plugin must declare its masters. Without them the engine cannot resolve the terrain this plugin overrides, and the result loads but is wrong.".into());
    }
    for (name, size) in masters {
        if name.is_empty() || *size == 0 {
            return Err(format!("master '{name}' has no usable size ({size})"));
        }
    }
    let header = Header {
        version: 1.3,
        author: FixedString(AUTHOR.to_string()),
        description: FixedString(DESCRIPTION.to_string()),
        num_objects: records.len() as u32,
        masters: masters.to_vec(),
        ..Default::default()
    };
    let mut objects = vec![TES3Object::Header(header)];
    objects.extend(records);
    let mut plugin = Plugin { objects };
    plugin.save_bytes().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_map_follows_the_heights() {
        let mut h = vec![0i64; LAND_NUM_VERTS];
        h[0] = 64; // 0.5: half to even is 0
        h[8] = 192; // 1.5 -> 2
        h[16] = -10; // underwater: -0.078 rounds to -0, then sinks to -1
        let w = world_map_from_heights(&h);
        assert_eq!(&w[..3], &[0, 2, 0]);
        assert_eq!(&sink_underwater_land(&w, &h)[..3], &[0, 2, -1]);
    }

    #[test]
    fn refuses_an_empty_cell_and_no_masters() {
        assert!(build_landscape_record((1, 2), None, None, None, None).unwrap_err().starts_with("cell (1, 2) was given no layers"));
        assert!(plugin_bytes(Vec::new(), &[]).is_err());
        let bytes = plugin_bytes(Vec::new(), &[("Morrowind.esm".into(), 79837557)]).unwrap();
        assert!(bytes.starts_with(b"TES3"));
    }
}
