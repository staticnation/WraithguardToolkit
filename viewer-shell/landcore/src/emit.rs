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
use crate::grid::Value;

pub const DESCRIPTION: &str = "Merged landscape. Load last.";
pub const AUTHOR: &str = "Wraithguard Toolkit";

/// Python's `round`: half to even.
fn round_half_even(v: f64) -> f64 {
    let r = v.round();
    if (v - v.trunc()).abs() == 0.5 { 2.0 * (v / 2.0).round() } else { r }
}

/// The 9x9 world map the Python derived from a cell's heights (`world_map_from_heights`):
/// every eighth vertex over 128, rounded. Not what OpenMW generates ([`generated_world_map`],
/// which the merge writes); kept because the recorded cases build their input plugins with it.
pub fn world_map_from_heights(heights: &[Value]) -> Vec<Value> {
    let mut out = Vec::with_capacity(WNAM_SIZE * WNAM_SIZE);
    for gy in 0..WNAM_SIZE {
        for gx in 0..WNAM_SIZE {
            let h = heights[gy * WNAM_STEP * LAND_SIZE + gx * WNAM_STEP] as f64;
            out.push((round_half_even(h / WNAM_HEIGHT_SCALE) as Value).clamp(-128, 127));
        }
    }
    out
}

/// The height vertex, along one axis, that entry `i` of the generated world map reads: `i * 64 / 9`
/// in `f32`, truncated (0, 7, 14 ... 56), as the engine computes it.
fn generated_sample(i: usize) -> usize {
    let step = (LAND_SIZE - 1) as f32 / WNAM_SIZE as f32;
    (i as f32 * step) as usize
}

/// The 9x9 world map OpenMW generates for a `LAND` that has none (`generateWnam`,
/// `components/esm3/loadland.cpp`, 0.51.0): each axis is sampled at `floor(i * 64 / 9)`, so at
/// 0, 7, 14 ... 56, never at the cell's far edge; a height above zero is divided by 128 and any
/// other by 16 (so the water below reads steeply), and the result is truncated, in `f32` as the
/// engine does. A flat -1,024 gives -64.
pub fn generated_world_map(heights: &[Value]) -> Vec<Value> {
    (0..WNAM_SIZE * WNAM_SIZE).map(|k| generated_world_map_entry(heights[world_map_vertex(k)])).collect()
}

/// The height vertex entry `k` (row-major) of the generated world map reads.
pub fn world_map_vertex(k: usize) -> usize {
    generated_sample(k / WNAM_SIZE) * LAND_SIZE + generated_sample(k % WNAM_SIZE)
}

/// One entry of [`generated_world_map`], from the height it reads.
pub fn generated_world_map_entry(height: Value) -> Value {
    let mut h = height as f32;
    h /= if h > 0.0 { 128.0 } else { 16.0 };
    h.clamp(-128.0, 127.0) as Value
}

/// One merged `LAND` record (`build_landscape_record`): its layers, and the vertices whose
/// height step had to be clamped. Normals are recomputed from the heights when not given,
/// since terrain whose heights moved is lit by its normals. `Err` when given no heights or
/// colours (textures are attached after, by [`attach_texture_indices`]).
pub fn build_landscape_record(
    coords: (i64, i64),
    heights: Option<&[Value]>,
    normals: Option<&[Value]>,
    world_map: Option<&[Value]>,
    colors: Option<&[Value]>,
) -> Result<(Landscape, Vec<(usize, usize)>), String> {
    build_landscape_record_for(coords, heights, normals, world_map, colors, false)
}

/// [`build_landscape_record`] for a cell that may also carry textures (`has_textures`, which
/// [`attach_texture_indices`] then supplies): a record of textures alone is a record to write
/// (a restoration of the textures a later `LAND` leaves out), not an empty one.
pub fn build_landscape_record_for(
    coords: (i64, i64),
    heights: Option<&[Value]>,
    normals: Option<&[Value]>,
    world_map: Option<&[Value]>,
    colors: Option<&[Value]>,
    has_textures: bool,
) -> Result<(Landscape, Vec<(usize, usize)>), String> {
    if heights.is_none() && colors.is_none() && !has_textures {
        return Err(format!(
            "cell ({}, {}) was given no layers to write. An empty LAND record would replace whatever the load order had there with nothing.",
            coords.0, coords.1
        ));
    }
    let mut r = Landscape { grid: (coords.0 as i32, coords.1 as i32), ..Default::default() };
    let mut flags = LandscapeFlags::empty();
    let mut clamped = Vec::new();
    let mut world: Option<Vec<Value>> = world_map.map(<[Value]>::to_vec);
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
        // As OpenMW has it: a world map the record carries is the author's and is kept as it is
        // (the engine reads it back unchanged, sea or no sea under an entry); one it does not
        // carry (a record read with none, which decodes as all zeros) is generated by
        // `generateWnam` from the heights, and nothing is done to that either.
        world = Some(match world.take() {
            Some(w) if !w.iter().all(|&v| v == 0) => w,
            _ => generated_world_map(h),
        });
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
pub fn attach_texture_indices(r: &mut Landscape, rows: &[Value]) {
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
    /* Each record encoded on its own and dropped (Static, 2026-11-17): handed to tes3 together they
       were all held until the last was written, beside the file's bytes. tes3 writes a plugin's
       objects one after another with nothing carried between them (the writer's `context` is the
       NIF code's), so the bytes are the same; `the_streamed_plugin_is_tes3s_byte_for_byte` holds
       it to that. A plugin of one header is written with its count set to 0 (save_bytes counts the
       objects after it), so the count is put back where HEDR keeps it. */
    let one = |object: TES3Object| Plugin { objects: vec![object] }.save_bytes().map_err(|e| e.to_string());
    let count = records.len() as u32;
    let mut bytes = one(TES3Object::Header(header))?;
    set_object_count(&mut bytes, count)?;
    for record in records {
        bytes.extend_from_slice(&one(record)?);
    }
    Ok(bytes)
}

/// Where HEDR keeps the plugin's record count: after the TES3 record's 16-byte header, HEDR's tag
/// and size (8), the version (4), the file type (4), the author (32) and the description (256).
const HEDR_COUNT_AT: usize = 16 + 8 + 4 + 4 + 32 + 256;

/// Puts the record count into an encoded header, after checking the bytes are a TES3 record whose
/// first subrecord is HEDR, holding a count of 0 where it is put.
fn set_object_count(bytes: &mut [u8], count: u32) -> Result<(), String> {
    let at = HEDR_COUNT_AT;
    let laid_out = bytes.len() >= at + 4 && &bytes[..4] == b"TES3" && &bytes[16..20] == b"HEDR" && bytes[at..at + 4] == [0; 4];
    if !laid_out {
        return Err("the encoded header is not laid out as HEDR is: the record count has nowhere to go".into());
    }
    bytes[at..at + 4].copy_from_slice(&count.to_le_bytes());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The plugin written a record at a time is the one tes3 writes all at once.
    #[test]
    fn the_streamed_plugin_is_tes3s_byte_for_byte() {
        let records = || -> Vec<TES3Object> {
            let mut out = vec![TES3Object::LandscapeTexture(LandscapeTexture { id: "a".into(), index: 0, file_name: "a.dds".into(), ..Default::default() })];
            for x in 0..3 {
                let mut l = Landscape { grid: (x, -x), landscape_flags: LandscapeFlags::USES_VERTEX_HEIGHTS_AND_NORMALS, ..Default::default() };
                l.vertex_heights.offset = x as f32 * 8.0;
                l.vertex_heights.data[1][2] = 3;
                l.texture_indices.data[0][0] = 1;
                out.push(TES3Object::Landscape(l));
            }
            out
        };
        let masters = vec![("Morrowind.esm".to_string(), 79_837_557u64)];
        let streamed = plugin_bytes(records(), &masters).unwrap();
        let header = Header {
            version: 1.3,
            author: FixedString(AUTHOR.to_string()),
            description: FixedString(DESCRIPTION.to_string()),
            num_objects: 4,
            masters: masters.clone(),
            ..Default::default()
        };
        let mut objects = vec![TES3Object::Header(header)];
        objects.extend(records());
        let whole = Plugin { objects }.save_bytes().unwrap();
        assert_eq!(streamed, whole);
        assert_eq!(u32::from_le_bytes(streamed[HEDR_COUNT_AT..HEDR_COUNT_AT + 4].try_into().unwrap()), 4);
    }

    #[test]
    fn world_map_follows_the_heights() {
        let mut h: Vec<Value> = vec![0; LAND_NUM_VERTS];
        h[0] = 64; // 0.5: half to even is 0
        h[8] = 192; // 1.5 -> 2
        h[16] = -10; // underwater: -0.078 rounds to -0 (the Python then sank it to -1; OpenMW does not)
        let w = world_map_from_heights(&h);
        assert_eq!(&w[..3], &[0, 2, 0]);
    }

    /// WG-A06: a record of textures alone is built, declares them, and reads back as written.
    /// WG-A07: OpenMW's `generateWnam`, sample for sample.
    #[test]
    fn the_world_map_is_what_openmw_generates() {
        // Flat -1,024: OpenMW reads -64 (the old way -8).
        assert!(generated_world_map(&vec![-1024; LAND_NUM_VERTS]).iter().all(|&v| v == -64));
        // Columns sampled at 0, 7, 14 ... 56 (the old way 0, 8 ... 64): a ramp of 128 a column.
        let ramp: Vec<Value> = (0..LAND_NUM_VERTS).map(|i| (i % LAND_SIZE) as Value * 128).collect();
        let w = generated_world_map(&ramp);
        assert_eq!(&w[..9], &[0, 7, 14, 21, 28, 35, 42, 49, 56]);
        // And rows alike.
        let rows: Vec<Value> = (0..LAND_NUM_VERTS).map(|i| (i / LAND_SIZE) as Value * 128).collect();
        let w = generated_world_map(&rows);
        assert_eq!((0..9).map(|r| w[r * 9]).collect::<Vec<_>>(), [0, 7, 14, 21, 28, 35, 42, 49, 56]);
        // Above zero over 128, truncated (100 is 0, not 1); at or below over 16 (-15 is 0, -16 is -1).
        let mut h = vec![0; LAND_NUM_VERTS];
        h[0] = 100;
        h[7] = -15;
        h[14] = -16;
        h[21] = 128 * 130; // past a signed byte: clamped
        h[28] = -16 * 200;
        assert_eq!(&generated_world_map(&h)[..5], &[0, 0, -1, 127, -128]);
    }

    #[test]
    fn a_texture_only_record_is_written() {
        assert!(build_landscape_record((1, 2), None, None, None, None).is_err());
        assert!(build_landscape_record_for((1, 2), None, None, None, None, false).is_err());
        let (mut record, clamped) = build_landscape_record_for((1, 2), None, None, None, None, true).unwrap();
        assert!(clamped.is_empty());
        let rows: Vec<Value> = (0..256).map(|i| (i % 5) as Value).collect();
        attach_texture_indices(&mut record, &rows);
        assert_eq!(record.landscape_flags, LandscapeFlags::USES_TEXTURES);
        let bytes = plugin_bytes(vec![TES3Object::Landscape(record)], &[("Morrowind.esm".into(), 79837557)]).unwrap();
        let mut back = Plugin { objects: Vec::new() };
        back.load_bytes(&bytes).unwrap();
        let Some(TES3Object::Landscape(l)) = back.objects.iter().find(|o| matches!(o, TES3Object::Landscape(_))) else { panic!("no LAND saved") };
        assert_eq!((l.grid, l.landscape_flags), ((1, 2), LandscapeFlags::USES_TEXTURES));
        assert_eq!(crate::layers::swizzle_texture_indices(&rows), *l.texture_indices.data);
    }

    /// WG-A07 as OpenMW has it: no world-map entry is sunk or lifted by the sea under it. A map the
    /// record carries is written as it is; one it lacks is `generateWnam`'s, over a coast too.
    #[test]
    fn the_world_map_is_written_as_authored_or_as_openmw_generates() {
        let mut heights = vec![200; LAND_NUM_VERTS];
        heights[7] = -10; // read by entry 1 of row 0: -10 over 16 truncates to 0, and OpenMW leaves it at 0
        heights[8] = -400; // the eighth-vertex vertex the Python judged it by
        let authored: Vec<Value> = (0..81).map(|i| i as Value % 5).collect(); // positive entries over sea, kept
        let (r, _) = build_landscape_record_for((0, 0), Some(&heights), None, Some(&authored), None, false).unwrap();
        let got: Vec<Value> = r.world_map_data.data.iter().flatten().map(|&v| v as Value).collect();
        assert_eq!(got, authored, "an authored map is not touched");
        let (r, _) = build_landscape_record_for((0, 0), Some(&heights), None, None, None, false).unwrap();
        let got: Vec<Value> = r.world_map_data.data.iter().flatten().map(|&v| v as Value).collect();
        assert_eq!(got, generated_world_map(&heights), "none given: generateWnam, unaltered");
        assert_eq!(got[1], 0, "-10 over 16 is 0, not -1");
    }

    #[test]
    fn refuses_an_empty_cell_and_no_masters() {
        assert!(build_landscape_record((1, 2), None, None, None, None).unwrap_err().starts_with("cell (1, 2) was given no layers"));
        assert!(plugin_bytes(Vec::new(), &[]).is_err());
        let bytes = plugin_bytes(Vec::new(), &[("Morrowind.esm".into(), 79837557)]).unwrap();
        assert!(bytes.starts_with(b"TES3"));
    }
}
