//! Gardenfell's `land`: Robin Hjelte's, from `gardenfell-tes3` (viewer-shell/gardenfell,
//! vendored), re-exported so viewcore's callers keep their paths. Below it, a LAND record
//! decoded straight from its bytes - what the Merged Lands preview reads each plugin's
//! version of a cell with (`mland`).

pub use gardenfell_tes3::land::*;

use crate::esp::{i32le, u16le, Subs};

/// VHGT is a base float followed by signed byte deltas: the first column
/// accumulates down the rows, and each row accumulates across from it.
pub fn decode_vhgt(data: &[u8]) -> Option<Box<[f32; VERTS * VERTS]>> {
    if data.len() < 4 + VERTS * VERTS {
        return None;
    }
    let base = f32::from_bits(u32::from_le_bytes([data[0], data[1], data[2], data[3]]));
    let d = &data[4..];
    let mut h = Box::new([0f32; VERTS * VERTS]);
    let mut row_acc = base;
    for y in 0..VERTS {
        row_acc += d[y * VERTS] as i8 as f32;
        let mut acc = row_acc;
        h[y * VERTS] = acc * HEIGHT_SCALE;
        for x in 1..VERTS {
            acc += d[y * VERTS + x] as i8 as f32;
            h[y * VERTS + x] = acc * HEIGHT_SCALE;
        }
    }
    Some(h)
}

/// One LAND record's body as the world keeps it, or `None` without a grid or heights.
pub fn decode_land(body: &[u8], plugin_idx: i32) -> Option<Land> {
    let (mut grid, mut heights, mut vtex, mut normals, mut colours) = (None, None, None, None, None);
    for s in Subs::new(body) {
        match &s.tag {
            b"INTV" if s.data.len() >= 8 => grid = Some((i32le(s.data, 0), i32le(s.data, 4))),
            b"VHGT" => heights = decode_vhgt(s.data),
            /* The authored per-vertex normals. Read because they are not a
               reconstruction of anything: measured against 300 vanilla cells,
               no triangulation and no gradient predicts VNML better than about
               half a degree at the median and several at p99, so deriving one
               is guessing at a number the file already states. It is what the
               game shades the ground with. See memory §11a. */
            b"VNML" => normals = decode_vnml(s.data),
            // The vertex colours, for the preview's ground. See `Land::colours`.
            b"VCLR" => colours = decode_vclr(s.data),
            b"VTEX" if s.data.len() >= 512 => {
                let mut raw = [0u16; 256];
                for (k, slot) in raw.iter_mut().enumerate() {
                    *slot = u16le(s.data, k * 2);
                }
                vtex = Some(unswizzle_vtex(&raw));
            }
            _ => {}
        }
    }
    let (grid, heights) = (grid?, heights?);
    Some(Land { grid, heights, normals, colours, vtex: vtex.unwrap_or_else(|| Box::new([0u16; 256])), plugin: plugin_idx })
}
