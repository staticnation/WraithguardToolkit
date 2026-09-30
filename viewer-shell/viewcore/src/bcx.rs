//! BC4, BC5 and BC7 blocks to RGBA - Wraithguard. The decoders of the toolkit's own
//! Rust module (`native/src/img.rs`, MIT), carried here for the cell viewer's textures:
//! normal maps are commonly BC5 (ATI2, Z rebuilt from X and Y) and modern texture packs
//! BC7, and the viewer drew neither.
//!
//! The BC7 tables are the format's definition (Khronos BPTC / Direct3D 11 BC7
//! documentation).
#![allow(clippy::needless_range_loop)]

type Res<T> = Result<T, String>;

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


/// BC4: one channel, shown as grey.
pub fn decode_bc4(data: &[u8], width: usize, height: usize) -> Res<Vec<u8>> {
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
pub fn decode_bc5(data: &[u8], width: usize, height: usize) -> Res<Vec<u8>> {
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

pub fn decode_bc7(data: &[u8], width: usize, height: usize) -> Res<Vec<u8>> {
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

#[cfg(test)]
mod tests {
    use crate::img::{read_with, Extra, Kind};

    /// A DDS header for `w`×`h` with `mips` levels and a FourCC.
    fn dds(four: &[u8; 4], w: u32, h: u32, mips: u32, body: &[u8]) -> Vec<u8> {
        let mut b = vec![0u8; 128];
        b[0..4].copy_from_slice(b"DDS ");
        b[4..8].copy_from_slice(&124u32.to_le_bytes());
        b[8..12].copy_from_slice(&(0x1007u32 | 0x20000).to_le_bytes());
        b[12..16].copy_from_slice(&h.to_le_bytes());
        b[16..20].copy_from_slice(&w.to_le_bytes());
        b[28..32].copy_from_slice(&mips.to_le_bytes());
        b[76..80].copy_from_slice(&32u32.to_le_bytes());
        b[80..84].copy_from_slice(&4u32.to_le_bytes());
        b[84..88].copy_from_slice(four);
        b.extend_from_slice(body);
        b
    }

    /// One BC5 block whose X and Y are both mid-grey (a flat normal): each channel's two
    /// endpoints 128, every index 0.
    fn flat_bc5() -> Vec<u8> {
        let ch = [128u8, 128, 0, 0, 0, 0, 0, 0];
        [ch, ch].concat()
    }

    #[test]
    fn a_bc5_normal_map_is_decoded_with_its_z_when_the_page_cannot_take_it() {
        let buf = dds(b"ATI2", 4, 4, 1, &flat_bc5());
        let t = read_with(&buf, "x_n.dds", true, Extra::default()).unwrap();
        assert_eq!(t.kind, Kind::Rgba);
        let px = &t.levels[0].2;
        assert_eq!((px[0], px[1]), (128, 128));
        assert!(px[2] >= 250, "Z rebuilt as up, got {}", px[2]);
    }

    #[test]
    fn a_bc5_map_with_mips_goes_to_the_page_as_blocks_when_it_can() {
        let body = [flat_bc5(), flat_bc5(), flat_bc5()].concat();
        let buf = dds(b"ATI2", 4, 4, 3, &body);
        let t = read_with(&buf, "x_n.dds", true, Extra { rgtc: true, bptc: false }).unwrap();
        assert_eq!(t.kind, Kind::Bc5);
        assert_eq!(t.levels.len(), 3);
    }
}
