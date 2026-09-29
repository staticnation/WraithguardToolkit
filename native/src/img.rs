//! Texture decoding: DDS block formats (BC1-BC5, BC7), uncompressed DDS, Targa,
//! and the pixel-difference loops the conflict viewer uses to compare textures.
//!
//! This is the per-pixel half of `wraithguard/images`. The Python side keeps
//! the header parsing, mip selection, format dispatch and its error types
//! (`DdsError`, `TargaError`, `ImageError`); these functions raise `ValueError`
//! with the same messages the Python decoders used, and the wrappers re-raise
//! them as the right type. Every function here is a step-for-step port of the
//! Python it replaced -- including the integer rounding -- and
//! `tests/test_images_native_parity.py` checks the output byte for byte.
//!
//! The BC7 tables are the format's definition (Khronos BPTC / Direct3D 11 BC7
//! documentation), generated from the constants that were in `bc7.py`.

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyBytes;

type Res<T> = Result<T, String>;

fn to_py(e: String) -> PyErr {
    PyValueError::new_err(e)
}

// ---------------------------------------------------------------- DXT / BC1-BC5

/// 5:6:5 to 8 bits per channel, low bits replicated so white stays 255.
fn expand_565(v: u32) -> [u32; 3] {
    let r = (v >> 11) & 0x1F;
    let g = (v >> 5) & 0x3F;
    let b = v & 0x1F;
    [(r << 3) | (r >> 2), (g << 2) | (g >> 4), (b << 3) | (b >> 2)]
}

/// The four RGBA entries of a colour block. DXT1 picks the three-colour +
/// transparent mode by endpoint order; blocks with their own alpha never do.
fn color_table(c0: u32, c1: u32, punchthrough: bool) -> [[u8; 4]; 4] {
    let [r0, g0, b0] = expand_565(c0);
    let [r1, g1, b1] = expand_565(c1);
    let e0 = [r0 as u8, g0 as u8, b0 as u8, 255];
    let e1 = [r1 as u8, g1 as u8, b1 as u8, 255];
    if punchthrough && c0 <= c1 {
        [e0, e1, [((r0 + r1) / 2) as u8, ((g0 + g1) / 2) as u8, ((b0 + b1) / 2) as u8, 255], [0, 0, 0, 0]]
    } else {
        [
            e0,
            e1,
            [((2 * r0 + r1) / 3) as u8, ((2 * g0 + g1) / 3) as u8, ((2 * b0 + b1) / 3) as u8, 255],
            [((r0 + 2 * r1) / 3) as u8, ((g0 + 2 * g1) / 3) as u8, ((b0 + 2 * b1) / 3) as u8, 255],
        ]
    }
}

/// The eight-entry table of a DXT5-style block (DXT5 alpha, BC4, each BC5 channel).
fn alpha_table(a0: u32, a1: u32) -> [u8; 8] {
    let mut t = [0u8; 8];
    t[0] = a0 as u8;
    t[1] = a1 as u8;
    if a0 > a1 {
        for i in 1..7u32 {
            t[i as usize + 1] = (((7 - i) * a0 + i * a1) / 7) as u8;
        }
    } else {
        for i in 1..5u32 {
            t[i as usize + 1] = (((5 - i) * a0 + i * a1) / 5) as u8;
        }
        t[6] = 0;
        t[7] = 255;
    }
    t
}

/// Two endpoints and sixteen 3-bit indices.
fn interpolated_values(block: &[u8]) -> [u8; 16] {
    let table = alpha_table(block[0] as u32, block[1] as u32);
    let mut packed = 0u64;
    for (i, &b) in block[2..8].iter().enumerate() {
        packed |= (b as u64) << (8 * i);
    }
    let mut out = [0u8; 16];
    for (i, o) in out.iter_mut().enumerate() {
        *o = table[((packed >> (3 * i)) & 0x7) as usize];
    }
    out
}

/// A DXT3 alpha block: sixteen 4-bit values, replicated (x17) so 0xF is 255.
fn explicit_alphas(block: &[u8]) -> [u8; 16] {
    let mut out = [0u8; 16];
    for (i, &b) in block[..8].iter().enumerate() {
        out[2 * i] = (b & 0xF) * 17;
        out[2 * i + 1] = (b >> 4) * 17;
    }
    out
}

/// Visit every 4x4 block of a surface, clipping the ones on the right and
/// bottom edges. `put(block_index_pixel, x, y)` is called per visible pixel.
fn for_blocks(
    data: &[u8],
    width: usize,
    height: usize,
    stride: usize,
    short: impl Fn(usize) -> String,
    mut block: impl FnMut(&[u8], &mut [[u8; 4]; 16]),
) -> Res<Vec<u8>> {
    let mut out = vec![0u8; width * height * 4];
    let mut offset = 0usize;
    let mut pixels = [[0u8; 4]; 16];
    for by in (0..height).step_by(4) {
        for bx in (0..width).step_by(4) {
            if offset + stride > data.len() {
                return Err(short(offset));
            }
            block(&data[offset..offset + stride], &mut pixels);
            offset += stride;
            let cols = (width - bx).min(4);
            for row in 0..(height - by).min(4) {
                let start = ((by + row) * width + bx) * 4;
                for col in 0..cols {
                    out[start + col * 4..start + col * 4 + 4].copy_from_slice(&pixels[row * 4 + col]);
                }
            }
        }
    }
    Ok(out)
}

fn decode_dxt(data: &[u8], width: usize, height: usize, fourcc: &[u8]) -> Res<Vec<u8>> {
    let stride = if fourcc == b"DXT1" { 8 } else { 16 };
    let has_alpha = stride == 16;
    let explicit = fourcc == b"DXT2" || fourcc == b"DXT3";
    let len = data.len();
    for_blocks(
        data,
        width,
        height,
        stride,
        |offset| {
            format!("texture data ends early: wanted {stride} byte(s) at {offset}, file holds {}", len - offset)
        },
        |block, px| {
            let (alphas, colour) = if has_alpha {
                let a = if explicit { explicit_alphas(&block[..8]) } else { interpolated_values(&block[..8]) };
                (Some(a), &block[8..])
            } else {
                (None, block)
            };
            let c0 = u16::from_le_bytes([colour[0], colour[1]]) as u32;
            let c1 = u16::from_le_bytes([colour[2], colour[3]]) as u32;
            let bits = u32::from_le_bytes([colour[4], colour[5], colour[6], colour[7]]);
            let table = color_table(c0, c1, !has_alpha);
            for (i, p) in px.iter_mut().enumerate() {
                *p = table[((bits >> (2 * i)) & 0x3) as usize];
                if let Some(a) = &alphas {
                    p[3] = a[i];
                }
            }
        },
    )
}

/// BC4: one channel, shown as grey.
fn decode_bc4(data: &[u8], width: usize, height: usize) -> Res<Vec<u8>> {
    for_blocks(
        data,
        width,
        height,
        8,
        |offset| format!("BC4 data ends early at offset {offset}"),
        |block, px| {
            let v = interpolated_values(block);
            for (p, &l) in px.iter_mut().zip(v.iter()) {
                *p = [l, l, l, 255];
            }
        },
    )
}

/// The reconstructed Z of a BC5 normal, exactly as the Python computed it
/// (f64, `int()` truncation, capped at 255).
fn bc5_blue(red: u8, green: u8) -> u8 {
    let x = red as f64 / 127.5 - 1.0;
    let y = green as f64 / 127.5 - 1.0;
    let z = (1.0 - x * x - y * y).max(0.0).sqrt();
    (((z + 1.0) * 127.5) as i64).min(255) as u8
}

/// BC5: two channels (DirectX convention, green not flipped), Z rebuilt.
fn decode_bc5(data: &[u8], width: usize, height: usize) -> Res<Vec<u8>> {
    static BLUE: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
    let blue = BLUE.get_or_init(|| (0..65536u32).map(|k| bc5_blue((k >> 8) as u8, k as u8)).collect());
    for_blocks(
        data,
        width,
        height,
        16,
        |offset| format!("BC5 data ends early at offset {offset}"),
        |block, px| {
            let reds = interpolated_values(&block[..8]);
            let greens = interpolated_values(&block[8..16]);
            for i in 0..16 {
                let (r, g) = (reds[i], greens[i]);
                px[i] = [r, g, blue[((r as usize) << 8) | g as usize], 255];
            }
        },
    )
}

// ---------------------------------------------------------------- BC7

const SUBSETS: [u32; 8] = [3, 2, 3, 2, 1, 1, 1, 2];
const PARTITION_BITS: [u32; 8] = [4, 6, 6, 6, 0, 0, 0, 6];
const ROTATION_BITS: [u32; 8] = [0, 0, 0, 0, 2, 2, 0, 0];
const SELECTOR_BITS: [u32; 8] = [0, 0, 0, 0, 1, 0, 0, 0];
const COLOUR_BITS: [u32; 8] = [4, 6, 5, 7, 5, 7, 7, 5];
const ALPHA_BITS: [u32; 8] = [0, 0, 0, 0, 6, 8, 7, 5];
const ENDPOINT_P: [bool; 8] = [true, false, false, true, false, false, true, true];
const SHARED_P: [bool; 8] = [false, true, false, false, false, false, false, false];
const INDEX_BITS: [u32; 8] = [3, 3, 2, 2, 2, 2, 4, 2];
const INDEX_BITS_2: [u32; 8] = [0, 0, 0, 0, 3, 2, 0, 0];
const WEIGHTS_2: [u32; 4] = [0, 21, 43, 64];
const WEIGHTS_3: [u32; 8] = [0, 9, 18, 27, 37, 46, 55, 64];
const WEIGHTS_4: [u32; 16] = [0, 4, 9, 13, 17, 21, 26, 30, 34, 38, 43, 47, 51, 55, 60, 64];

const PARTITIONS_2: [[u8; 16]; 64] = [
    [0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1],
    [0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1],
    [0, 1, 1, 1, 0, 1, 1, 1, 0, 1, 1, 1, 0, 1, 1, 1],
    [0, 0, 0, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0, 1, 1, 1],
    [0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 1, 1],
    [0, 0, 1, 1, 0, 1, 1, 1, 0, 1, 1, 1, 1, 1, 1, 1],
    [0, 0, 0, 1, 0, 0, 1, 1, 0, 1, 1, 1, 1, 1, 1, 1],
    [0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 1, 1, 0, 1, 1, 1],
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 1, 1],
    [0, 0, 1, 1, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1],
    [0, 0, 0, 0, 0, 0, 0, 1, 0, 1, 1, 1, 1, 1, 1, 1],
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 1, 1, 1],
    [0, 0, 0, 1, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1],
    [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1],
    [0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1],
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1],
    [0, 0, 0, 0, 1, 0, 0, 0, 1, 1, 1, 0, 1, 1, 1, 1],
    [0, 1, 1, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 1, 1, 1, 0],
    [0, 1, 1, 1, 0, 0, 1, 1, 0, 0, 0, 1, 0, 0, 0, 0],
    [0, 0, 1, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0],
    [0, 0, 0, 0, 1, 0, 0, 0, 1, 1, 0, 0, 1, 1, 1, 0],
    [0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 1, 1, 0, 0],
    [0, 1, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 0, 1],
    [0, 0, 1, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 0],
    [0, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 1, 0, 0],
    [0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0],
    [0, 0, 1, 1, 0, 1, 1, 0, 0, 1, 1, 0, 1, 1, 0, 0],
    [0, 0, 0, 1, 0, 1, 1, 1, 1, 1, 1, 0, 1, 0, 0, 0],
    [0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0],
    [0, 1, 1, 1, 0, 0, 0, 1, 1, 0, 0, 0, 1, 1, 1, 0],
    [0, 0, 1, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 1, 0, 0],
    [0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1],
    [0, 0, 0, 0, 1, 1, 1, 1, 0, 0, 0, 0, 1, 1, 1, 1],
    [0, 1, 0, 1, 1, 0, 1, 0, 0, 1, 0, 1, 1, 0, 1, 0],
    [0, 0, 1, 1, 0, 0, 1, 1, 1, 1, 0, 0, 1, 1, 0, 0],
    [0, 0, 1, 1, 1, 1, 0, 0, 0, 0, 1, 1, 1, 1, 0, 0],
    [0, 1, 0, 1, 0, 1, 0, 1, 1, 0, 1, 0, 1, 0, 1, 0],
    [0, 1, 1, 0, 1, 0, 0, 1, 0, 1, 1, 0, 1, 0, 0, 1],
    [0, 1, 0, 1, 1, 0, 1, 0, 1, 0, 1, 0, 0, 1, 0, 1],
    [0, 1, 1, 1, 0, 0, 1, 1, 1, 1, 0, 0, 1, 1, 1, 0],
    [0, 0, 0, 1, 0, 0, 1, 1, 1, 1, 0, 0, 1, 0, 0, 0],
    [0, 0, 1, 1, 0, 0, 1, 0, 0, 1, 0, 0, 1, 1, 0, 0],
    [0, 0, 1, 1, 1, 0, 1, 1, 1, 1, 0, 1, 1, 1, 0, 0],
    [0, 1, 1, 0, 1, 0, 0, 1, 1, 0, 0, 1, 0, 1, 1, 0],
    [0, 0, 1, 1, 1, 1, 0, 0, 1, 1, 0, 0, 0, 0, 1, 1],
    [0, 1, 1, 0, 0, 1, 1, 0, 1, 0, 0, 1, 1, 0, 0, 1],
    [0, 0, 0, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 0, 0, 0],
    [0, 1, 0, 0, 1, 1, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0],
    [0, 0, 1, 0, 0, 1, 1, 1, 0, 0, 1, 0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0, 0, 1, 0, 0, 1, 1, 1, 0, 0, 1, 0],
    [0, 0, 0, 0, 0, 1, 0, 0, 1, 1, 1, 0, 0, 1, 0, 0],
    [0, 1, 1, 0, 1, 1, 0, 0, 1, 0, 0, 1, 0, 0, 1, 1],
    [0, 0, 1, 1, 0, 1, 1, 0, 1, 1, 0, 0, 1, 0, 0, 1],
    [0, 1, 1, 0, 0, 0, 1, 1, 1, 0, 0, 1, 1, 1, 0, 0],
    [0, 0, 1, 1, 1, 0, 0, 1, 1, 1, 0, 0, 0, 1, 1, 0],
    [0, 1, 1, 0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 0, 0, 1],
    [0, 1, 1, 0, 0, 0, 1, 1, 0, 0, 1, 1, 1, 0, 0, 1],
    [0, 1, 1, 1, 1, 1, 1, 0, 1, 0, 0, 0, 0, 0, 0, 1],
    [0, 0, 0, 1, 1, 0, 0, 0, 1, 1, 1, 0, 0, 1, 1, 1],
    [0, 0, 0, 0, 1, 1, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1],
    [0, 0, 1, 1, 0, 0, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0],
    [0, 0, 1, 0, 0, 0, 1, 0, 1, 1, 1, 0, 1, 1, 1, 0],
    [0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 1, 1, 0, 1, 1, 1],
];
const PARTITIONS_3: [[u8; 16]; 64] = [
    [0, 0, 1, 1, 0, 0, 1, 1, 0, 2, 2, 1, 2, 2, 2, 2],
    [0, 0, 0, 1, 0, 0, 1, 1, 2, 2, 1, 1, 2, 2, 2, 1],
    [0, 0, 0, 0, 2, 0, 0, 1, 2, 2, 1, 1, 2, 2, 1, 1],
    [0, 2, 2, 2, 0, 0, 2, 2, 0, 0, 1, 1, 0, 1, 1, 1],
    [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 2, 2, 1, 1, 2, 2],
    [0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 2, 2, 0, 0, 2, 2],
    [0, 0, 2, 2, 0, 0, 2, 2, 1, 1, 1, 1, 1, 1, 1, 1],
    [0, 0, 1, 1, 0, 0, 1, 1, 2, 2, 1, 1, 2, 2, 1, 1],
    [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2],
    [0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2],
    [0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 2, 2, 2, 2],
    [0, 0, 1, 2, 0, 0, 1, 2, 0, 0, 1, 2, 0, 0, 1, 2],
    [0, 1, 1, 2, 0, 1, 1, 2, 0, 1, 1, 2, 0, 1, 1, 2],
    [0, 1, 2, 2, 0, 1, 2, 2, 0, 1, 2, 2, 0, 1, 2, 2],
    [0, 0, 1, 1, 0, 1, 1, 2, 1, 1, 2, 2, 1, 2, 2, 2],
    [0, 0, 1, 1, 2, 0, 0, 1, 2, 2, 0, 0, 2, 2, 2, 0],
    [0, 0, 0, 1, 0, 0, 1, 1, 0, 1, 1, 2, 1, 1, 2, 2],
    [0, 1, 1, 1, 0, 0, 1, 1, 2, 0, 0, 1, 2, 2, 0, 0],
    [0, 0, 0, 0, 1, 1, 2, 2, 1, 1, 2, 2, 1, 1, 2, 2],
    [0, 0, 2, 2, 0, 0, 2, 2, 0, 0, 2, 2, 1, 1, 1, 1],
    [0, 1, 1, 1, 0, 1, 1, 1, 0, 2, 2, 2, 0, 2, 2, 2],
    [0, 0, 0, 1, 0, 0, 0, 1, 2, 2, 2, 1, 2, 2, 2, 1],
    [0, 0, 0, 0, 0, 0, 1, 1, 0, 1, 2, 2, 0, 1, 2, 2],
    [0, 0, 0, 0, 1, 1, 0, 0, 2, 2, 1, 0, 2, 2, 1, 0],
    [0, 1, 2, 2, 0, 1, 2, 2, 0, 0, 1, 1, 0, 0, 0, 0],
    [0, 0, 1, 2, 0, 0, 1, 2, 1, 1, 2, 2, 2, 2, 2, 2],
    [0, 1, 1, 0, 1, 2, 2, 1, 1, 2, 2, 1, 0, 1, 1, 0],
    [0, 0, 0, 0, 0, 1, 1, 0, 1, 2, 2, 1, 1, 2, 2, 1],
    [0, 0, 2, 2, 1, 1, 0, 2, 1, 1, 0, 2, 0, 0, 2, 2],
    [0, 1, 1, 0, 0, 1, 1, 0, 2, 0, 0, 2, 2, 2, 2, 2],
    [0, 0, 1, 1, 0, 1, 2, 2, 0, 1, 2, 2, 0, 0, 1, 1],
    [0, 0, 0, 0, 2, 0, 0, 0, 2, 2, 1, 1, 2, 2, 2, 1],
    [0, 0, 0, 0, 0, 0, 0, 2, 1, 1, 2, 2, 1, 2, 2, 2],
    [0, 2, 2, 2, 0, 0, 2, 2, 0, 0, 1, 2, 0, 0, 1, 1],
    [0, 0, 1, 1, 0, 0, 1, 2, 0, 0, 2, 2, 0, 2, 2, 2],
    [0, 1, 2, 0, 0, 1, 2, 0, 0, 1, 2, 0, 0, 1, 2, 0],
    [0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 0, 0, 0, 0],
    [0, 1, 2, 0, 1, 2, 0, 1, 2, 0, 1, 2, 0, 1, 2, 0],
    [0, 1, 2, 0, 2, 0, 1, 2, 1, 2, 0, 1, 0, 1, 2, 0],
    [0, 0, 1, 1, 2, 2, 0, 0, 1, 1, 2, 2, 0, 0, 1, 1],
    [0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 0, 0, 0, 0, 1, 1],
    [0, 1, 0, 1, 0, 1, 0, 1, 2, 2, 2, 2, 2, 2, 2, 2],
    [0, 0, 0, 0, 0, 0, 0, 0, 2, 1, 2, 1, 2, 1, 2, 1],
    [0, 0, 2, 2, 1, 1, 2, 2, 0, 0, 2, 2, 1, 1, 2, 2],
    [0, 0, 2, 2, 0, 0, 1, 1, 0, 0, 2, 2, 0, 0, 1, 1],
    [0, 2, 2, 0, 1, 2, 2, 1, 0, 2, 2, 0, 1, 2, 2, 1],
    [0, 1, 0, 1, 2, 2, 2, 2, 2, 2, 2, 2, 0, 1, 0, 1],
    [0, 0, 0, 0, 2, 1, 2, 1, 2, 1, 2, 1, 2, 1, 2, 1],
    [0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 2, 2, 2, 2],
    [0, 2, 2, 2, 0, 1, 1, 1, 0, 2, 2, 2, 0, 1, 1, 1],
    [0, 0, 0, 2, 1, 1, 1, 2, 0, 0, 0, 2, 1, 1, 1, 2],
    [0, 0, 0, 0, 2, 1, 1, 2, 2, 1, 1, 2, 2, 1, 1, 2],
    [0, 2, 2, 2, 0, 1, 1, 1, 0, 1, 1, 1, 0, 2, 2, 2],
    [0, 0, 0, 2, 1, 1, 1, 2, 1, 1, 1, 2, 0, 0, 0, 2],
    [0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0, 2, 2, 2, 2],
    [0, 0, 0, 0, 0, 0, 0, 0, 2, 1, 1, 2, 2, 1, 1, 2],
    [0, 1, 1, 0, 0, 1, 1, 0, 2, 2, 2, 2, 2, 2, 2, 2],
    [0, 0, 2, 2, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 2, 2],
    [0, 0, 2, 2, 1, 1, 2, 2, 1, 1, 2, 2, 0, 0, 2, 2],
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 1, 1, 2],
    [0, 0, 0, 2, 0, 0, 0, 1, 0, 0, 0, 2, 0, 0, 0, 1],
    [0, 2, 2, 2, 1, 2, 2, 2, 0, 2, 2, 2, 1, 2, 2, 2],
    [0, 1, 0, 1, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2],
    [0, 1, 1, 1, 2, 0, 1, 1, 2, 2, 0, 1, 2, 2, 2, 0],
];
const ANCHOR_2: [u8; 64] = [15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 2, 8, 2, 2, 8, 8, 15, 2, 8, 2, 2, 8, 8, 2, 2, 15, 15, 6, 8, 2, 8, 15, 15, 2, 8, 2, 2, 2, 15, 15, 6, 6, 2, 6, 8, 15, 15, 2, 2, 15, 15, 15, 15, 15, 2, 2, 15];
const ANCHOR_3_1: [u8; 64] = [3, 3, 15, 15, 8, 3, 15, 15, 8, 8, 6, 6, 6, 5, 3, 3, 3, 3, 8, 15, 3, 3, 6, 10, 5, 8, 8, 6, 8, 5, 15, 15, 8, 15, 3, 5, 6, 10, 8, 15, 15, 3, 15, 5, 15, 15, 15, 15, 3, 15, 5, 5, 5, 8, 5, 10, 5, 10, 8, 13, 15, 12, 3, 3];
const ANCHOR_3_2: [u8; 64] = [15, 8, 8, 3, 15, 15, 3, 8, 15, 15, 15, 15, 15, 15, 15, 8, 15, 8, 15, 3, 15, 8, 15, 8, 3, 15, 6, 10, 15, 15, 10, 8, 15, 3, 15, 10, 10, 8, 9, 10, 6, 15, 8, 15, 3, 6, 6, 8, 15, 3, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 3, 15, 15, 8];

fn weights(bits: u32) -> &'static [u32] {
    match bits {
        2 => &WEIGHTS_2,
        3 => &WEIGHTS_3,
        _ => &WEIGHTS_4,
    }
}

/// A little-endian bit cursor over one 128-bit block.
struct Bits {
    value: u128,
    pos: u32,
}

impl Bits {
    fn take(&mut self, count: u32) -> u32 {
        if count == 0 {
            return 0;
        }
        let out = self.value.checked_shr(self.pos).unwrap_or(0) & ((1u128 << count) - 1);
        self.pos += count;
        out as u32
    }
}

/// Widen an endpoint channel to a byte, low bits replicated.
fn unquantise(value: u32, bits: u32) -> u32 {
    if bits >= 8 {
        return value;
    }
    ((value << (8 - bits)) | (value >> (2 * bits - 8))) & 0xFF
}

fn interpolate(first: u32, second: u32, weight: u32) -> u32 {
    ((64 - weight) * first + weight * second + 32) >> 6
}

/// Decode one 16-byte BC7 block into sixteen RGBA pixels. A reserved mode
/// (all eight low bits zero) is transparent black, per the format.
fn bc7_block(block: &[u8], px: &mut [[u8; 4]; 16]) {
    let value = u128::from_le_bytes(block[..16].try_into().expect("16 bytes"));
    let Some(mode) = (0..8usize).find(|&m| (value >> m) & 1 == 1) else {
        *px = [[0; 4]; 16];
        return;
    };
    let mut bits = Bits { value, pos: mode as u32 + 1 };
    let partition = bits.take(PARTITION_BITS[mode]) as usize;
    let rotation = bits.take(ROTATION_BITS[mode]);
    let selector = bits.take(SELECTOR_BITS[mode]);
    let subsets = SUBSETS[mode] as usize;

    // Endpoints: stored plane by plane, P-bits after all of them.
    let count = subsets * 2;
    let (cb, ab) = (COLOUR_BITS[mode], ALPHA_BITS[mode]);
    let mut planes = [[0u32; 6]; 4];
    for plane in planes.iter_mut().take(3) {
        for v in plane.iter_mut().take(count) {
            *v = bits.take(cb);
        }
    }
    if ab > 0 {
        for v in planes[3].iter_mut().take(count) {
            *v = bits.take(ab);
        }
    }
    let mut parity: Option<[u32; 6]> = None;
    if ENDPOINT_P[mode] {
        let mut p = [0u32; 6];
        for v in p.iter_mut().take(count) {
            *v = bits.take(1);
        }
        parity = Some(p);
    } else if SHARED_P[mode] {
        let mut shared = [0u32; 3];
        for v in shared.iter_mut().take(subsets) {
            *v = bits.take(1);
        }
        let mut p = [0u32; 6];
        for (i, v) in p.iter_mut().enumerate().take(count) {
            *v = shared[i / 2];
        }
        parity = Some(p);
    }
    let extra = u32::from(parity.is_some());
    let (cw, aw) = (cb + extra, ab + extra);
    let mut endpoints = [[0u32; 4]; 6];
    for (i, e) in endpoints.iter_mut().enumerate().take(count) {
        for c in 0..3 {
            let mut raw = planes[c][i];
            if let Some(p) = &parity {
                raw = (raw << 1) | p[i];
            }
            e[c] = unquantise(raw, cw);
        }
        e[3] = if ab == 0 {
            255
        } else {
            let mut raw = planes[3][i];
            if let Some(p) = &parity {
                raw = (raw << 1) | p[i];
            }
            unquantise(raw, aw)
        };
    }

    let membership: &[u8; 16] = match subsets {
        1 => &[0; 16],
        2 => &PARTITIONS_2[partition],
        _ => &PARTITIONS_3[partition],
    };
    let anchors: [usize; 3] = match subsets {
        1 => [0, 0, 0],
        2 => [0, ANCHOR_2[partition] as usize, 0],
        _ => [0, ANCHOR_3_1[partition] as usize, ANCHOR_3_2[partition] as usize],
    };
    let is_anchor = |p: usize| anchors[..subsets].contains(&p);
    let (w1, w2) = (INDEX_BITS[mode], INDEX_BITS_2[mode]);
    let mut idx1 = [0u32; 16];
    for (p, v) in idx1.iter_mut().enumerate() {
        *v = bits.take(if is_anchor(p) { w1 - 1 } else { w1 });
    }
    let mut idx2 = [0u32; 16];
    if w2 > 0 {
        for (p, v) in idx2.iter_mut().enumerate() {
            *v = bits.take(if p == 0 { w2 - 1 } else { w2 });
        }
    }
    let (ci, cwid, ai, awid) = if w2 == 0 {
        (&idx1, w1, &idx1, w1)
    } else if selector != 0 {
        (&idx2, w2, &idx1, w1)
    } else {
        (&idx1, w1, &idx2, w2)
    };
    let (cws, aws) = (weights(cwid), weights(awid));
    for p in 0..16 {
        let s = membership[p] as usize;
        let (lo, hi) = (endpoints[s * 2], endpoints[s * 2 + 1]);
        let w = cws[ci[p] as usize];
        let mut r = interpolate(lo[0], hi[0], w);
        let mut g = interpolate(lo[1], hi[1], w);
        let mut b = interpolate(lo[2], hi[2], w);
        let mut a = interpolate(lo[3], hi[3], aws[ai[p] as usize]);
        match rotation {
            1 => std::mem::swap(&mut r, &mut a),
            2 => std::mem::swap(&mut g, &mut a),
            3 => std::mem::swap(&mut b, &mut a),
            _ => {}
        }
        px[p] = [r as u8, g as u8, b as u8, a as u8];
    }
}

fn decode_bc7(data: &[u8], width: usize, height: usize) -> Res<Vec<u8>> {
    let len = data.len();
    for_blocks(
        data,
        width,
        height,
        16,
        |offset| format!("BC7 data ends early: wanted 16 byte(s) at {offset}, file holds {}", len - offset),
        bc7_block,
    )
}

// ---------------------------------------------------------------- uncompressed DDS

fn decode_uncompressed(data: &[u8], width: usize, height: usize, bit_count: u32, masks: [u64; 4]) -> Res<Vec<u8>> {
    if ![8, 16, 24, 32].contains(&bit_count) {
        return Err(format!("unsupported uncompressed depth: {bit_count} bits per pixel"));
    }
    let step = (bit_count / 8) as usize;
    let needed = width * height * step;
    if data.len() < needed {
        return Err(format!("texture data ends early: wanted {needed} byte(s), got {}", data.len()));
    }
    let shifts: Vec<(u32, u64)> =
        masks.iter().map(|&m| (if m == 0 { 0 } else { m.trailing_zeros() }, m)).collect();
    let mut out = vec![0u8; width * height * 4];
    for (i, px) in data[..needed].chunks_exact(step).enumerate() {
        let mut raw = 0u64;
        for (k, &b) in px.iter().enumerate() {
            raw |= (b as u64) << (8 * k);
        }
        for (pos, &(shift, mask)) in shifts.iter().enumerate() {
            out[i * 4 + pos] = if mask == 0 {
                if pos == 3 { 255 } else { 0 }
            } else {
                let maximum = mask >> shift;
                (((raw & mask) >> shift) * 255 / maximum) as u8
            };
        }
    }
    Ok(out)
}

/// Decode a block-compressed DDS surface. `fourcc` is the resolved tag
/// (`DX10` meaning BC7), the same value `dds._decode_compressed` dispatches on.
#[pyfunction]
fn dds_decode<'py>(
    py: Python<'py>,
    surface: &[u8],
    width: usize,
    height: usize,
    fourcc: &[u8],
) -> PyResult<Bound<'py, PyBytes>> {
    let out = py
        .detach(|| match fourcc {
            b"DX10" => decode_bc7(surface, width, height),
            b"ATI2" | b"BC5U" | b"BC5S" => decode_bc5(surface, width, height),
            b"ATI1" | b"BC4U" | b"BC4S" => decode_bc4(surface, width, height),
            b"DXT1" | b"DXT2" | b"DXT3" | b"DXT4" | b"DXT5" => decode_dxt(surface, width, height, fourcc),
            _ => Err(format!("unsupported texture compression {:?}", String::from_utf8_lossy(fourcc))),
        })
        .map_err(to_py)?;
    Ok(PyBytes::new(py, &out))
}

/// Decode an uncompressed DDS surface described by R, G, B, A masks.
#[pyfunction]
fn dds_uncompressed<'py>(
    py: Python<'py>,
    surface: &[u8],
    width: usize,
    height: usize,
    bit_count: u32,
    masks: (u64, u64, u64, u64),
) -> PyResult<Bound<'py, PyBytes>> {
    let m = [masks.0, masks.1, masks.2, masks.3];
    let out = py.detach(|| decode_uncompressed(surface, width, height, bit_count, m)).map_err(to_py)?;
    Ok(PyBytes::new(py, &out))
}

// ---------------------------------------------------------------- Targa

const TGA_HEADER: usize = 18;
const TGA_MAX_PIXELS: u64 = 64 << 20;

fn tga_pixel(raw: &[u8], depth: u32) -> Res<[u8; 4]> {
    match depth {
        32 => Ok([raw[2], raw[1], raw[0], raw[3]]),
        24 => Ok([raw[2], raw[1], raw[0], 255]),
        16 => {
            let packed = raw[0] as u32 | ((raw[1] as u32) << 8);
            let e = |v: u32| ((v << 3) | (v >> 2)) as u8;
            Ok([e((packed >> 10) & 0x1F), e((packed >> 5) & 0x1F), e(packed & 0x1F), 255])
        }
        8 => Ok([raw[0], raw[0], raw[0], 255]),
        _ => Err(format!("unsupported Targa depth: {depth} bits per pixel")),
    }
}

/// Stored pixel slices, run-length packets expanded.
fn tga_rle(data: &[u8], offset: usize, count: usize, step: usize) -> Res<Vec<&[u8]>> {
    let mut pixels: Vec<&[u8]> = Vec::with_capacity(count);
    let mut pos = offset;
    while pixels.len() < count {
        if pos >= data.len() {
            return Err(format!(
                "Targa run-length data ends early: {} of {count} pixel(s) decoded",
                pixels.len()
            ));
        }
        let control = data[pos];
        pos += 1;
        let run = (control & 0x7F) as usize + 1;
        let take = run.min(count - pixels.len());
        if control & 0x80 != 0 {
            if pos + step > data.len() {
                return Err("Targa run packet is truncated".into());
            }
            let px = &data[pos..pos + step];
            pos += step;
            pixels.extend(std::iter::repeat_n(px, take));
            continue;
        }
        if pos + run * step > data.len() {
            return Err("Targa literal packet is truncated".into());
        }
        pixels.extend((0..take).map(|i| &data[pos + i * step..pos + i * step + step]));
        pos += run * step;
    }
    Ok(pixels)
}

fn decode_tga(data: &[u8]) -> Res<(usize, usize, Vec<u8>)> {
    if data.len() < TGA_HEADER {
        return Err("too short to be a Targa image".into());
    }
    let u16at = |o: usize| u16::from_le_bytes([data[o], data[o + 1]]) as usize;
    let id_length = data[0] as usize;
    let has_map = data[1] != 0;
    let image_type = data[2];
    let map_length = u16at(5);
    let map_depth = data[7] as u32;
    let width = u16at(12);
    let height = u16at(14);
    let depth = data[16] as u32;
    let descriptor = data[17];
    if width == 0 || height == 0 {
        return Err(format!("implausible dimensions {width}x{height}"));
    }
    if (width * height) as u64 > TGA_MAX_PIXELS {
        return Err(format!("implausible size: {width}x{height} is {} pixel(s)", width * height));
    }
    if ![1, 2, 3, 9, 10, 11].contains(&image_type) {
        return Err(format!("unsupported Targa image type {image_type}"));
    }
    let mapped = image_type == 1 || image_type == 9;
    let rle = image_type >= 9;
    let mut offset = TGA_HEADER + id_length;
    let mut palette: Vec<[u8; 4]> = Vec::new();
    if has_map || mapped {
        let step = map_depth.div_ceil(8) as usize;
        if offset + map_length * step > data.len() {
            return Err("Targa color map runs past the end of the file".into());
        }
        for i in 0..map_length {
            palette.push(tga_pixel(&data[offset + i * step..offset + i * step + step], map_depth)?);
        }
        offset += map_length * step;
    }
    if mapped && palette.is_empty() {
        return Err("color-mapped Targa carries no color map".into());
    }
    let step = depth.div_ceil(8) as usize;
    if step < 1 {
        return Err(format!("unsupported Targa depth: {depth} bits per pixel"));
    }
    let count = width * height;
    let stored: Vec<&[u8]> = if rle {
        tga_rle(data, offset, count, step)?
    } else {
        if offset + count * step > data.len() {
            return Err(format!(
                "Targa pixel data ends early: wanted {} byte(s), file holds {}",
                count * step,
                data.len() as i64 - offset as i64
            ));
        }
        data[offset..offset + count * step].chunks_exact(step).collect()
    };
    let mut rgba: Vec<[u8; 4]> = Vec::with_capacity(count);
    for entry in stored {
        if mapped {
            let mut index = 0u64;
            for (k, &b) in entry.iter().enumerate() {
                index |= (b as u64) << (8 * k);
            }
            let Some(p) = palette.get(index as usize) else {
                return Err(format!("color-map index {index} is outside a map of {}", palette.len()));
            };
            rgba.push(*p);
        } else {
            rgba.push(tga_pixel(entry, depth)?);
        }
    }
    // Bottom origin unless the descriptor says top; right-to-left if flagged.
    let top_down = descriptor & 0x20 != 0;
    let right_to_left = descriptor & 0x10 != 0;
    let mut out = vec![0u8; count * 4];
    for row in 0..height {
        let target = if top_down { row } else { height - 1 - row };
        for col in 0..width {
            let tc = if right_to_left { width - 1 - col } else { col };
            let at = (target * width + tc) * 4;
            out[at..at + 4].copy_from_slice(&rgba[row * width + col]);
        }
    }
    Ok((width, height, out))
}

/// Decode a whole Targa file: `(width, height, rgba)`, top-down.
#[pyfunction]
fn tga_decode<'py>(py: Python<'py>, data: &[u8]) -> PyResult<(usize, usize, Bound<'py, PyBytes>)> {
    let (w, h, out) = py.detach(|| decode_tga(data)).map_err(to_py)?;
    Ok((w, h, PyBytes::new(py, &out)))
}

// ---------------------------------------------------------------- comparison

/// Pixels changed beyond `same`, the largest single-channel gap, and the summed
/// absolute difference, over two equal-length RGBA buffers.
#[pyfunction]
fn image_measure(py: Python<'_>, left: &[u8], right: &[u8], same: u32) -> PyResult<(u64, u32, u64)> {
    if left.len() != right.len() {
        return Err(PyValueError::new_err(format!(
            "cannot measure {} byte(s) against {}",
            left.len(),
            right.len()
        )));
    }
    Ok(py.detach(|| {
        let (mut changed, mut worst, mut total) = (0u64, 0u32, 0u64);
        for (a, b) in left.chunks(4).zip(right.chunks(4)) {
            let mut biggest = 0u32;
            for (x, y) in a.iter().zip(b) {
                let gap = x.abs_diff(*y) as u32;
                total += gap as u64;
                biggest = biggest.max(gap);
            }
            if biggest > same {
                changed += 1;
            }
            worst = worst.max(biggest);
        }
        (changed, worst, total)
    }))
}

/// The amplified per-channel difference, alpha differences folded into RGB,
/// fully opaque.
#[pyfunction]
fn image_difference<'py>(
    py: Python<'py>,
    left: &[u8],
    right: &[u8],
    amplify: u32,
) -> PyResult<Bound<'py, PyBytes>> {
    if left.len() != right.len() {
        return Err(PyValueError::new_err(format!(
            "cannot difference {} byte(s) against {}",
            left.len(),
            right.len()
        )));
    }
    let out = py.detach(|| {
        let scale = |g: u8| (g as u64 * amplify as u64).min(255) as u8;
        let mut out = vec![0u8; left.len()];
        for ((o, a), b) in out.chunks_mut(4).zip(left.chunks(4)).zip(right.chunks(4)) {
            for c in 0..3 {
                o[c] = scale(a[c].abs_diff(b[c]));
            }
            let alpha_gap = a[3].abs_diff(b[3]);
            if alpha_gap != 0 {
                let capped = scale(alpha_gap);
                for v in o.iter_mut().take(3) {
                    *v = (*v).max(capped);
                }
            }
            o[3] = 255;
        }
        out
    });
    Ok(PyBytes::new(py, &out))
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(dds_decode, m)?)?;
    m.add_function(wrap_pyfunction!(dds_uncompressed, m)?)?;
    m.add_function(wrap_pyfunction!(tga_decode, m)?)?;
    m.add_function(wrap_pyfunction!(image_measure, m)?)?;
    m.add_function(wrap_pyfunction!(image_difference, m)?)?;
    Ok(())
}
