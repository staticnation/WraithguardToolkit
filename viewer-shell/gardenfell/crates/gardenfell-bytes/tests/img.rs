//! Reading a texture the way the viewport needs it.
//!
//! The point of this module is that the page stops decoding images on the thread it draws
//! with, so what these check is the two halves of that bargain: compressed files come back
//! *uncompressed by nobody*, with their mip chain intact, and everything else comes back
//! as pixels that are actually right.

use gardenfell_bytes::img::{self, Kind};

/* ---- files, built by hand ---------------------------------------------------------- */

/// A DDS with the given FourCC, size and mip count. Block bytes are filled with a
/// recognisable pattern so a passthrough can be told from a re-encode.
fn dds(four: &[u8; 4], w: u32, h: u32, mips: u32) -> Vec<u8> {
    let bpb = if four == b"DXT1" { 8 } else { 16 };
    let mut v = vec![0u8; 128];
    v[0..4].copy_from_slice(b"DDS ");
    v[4..8].copy_from_slice(&124u32.to_le_bytes());
    v[8..12].copy_from_slice(&0x0002_1007u32.to_le_bytes());
    v[12..16].copy_from_slice(&h.to_le_bytes());
    v[16..20].copy_from_slice(&w.to_le_bytes());
    v[28..32].copy_from_slice(&mips.to_le_bytes());
    v[76..80].copy_from_slice(&32u32.to_le_bytes());
    v[80..84].copy_from_slice(&4u32.to_le_bytes()); // FOURCC
    v[84..88].copy_from_slice(four);
    let (mut lw, mut lh) = (w, h);
    for level in 0..mips {
        let blocks = (lw.div_ceil(4) as usize) * (lh.div_ceil(4) as usize);
        for i in 0..blocks * bpb {
            v.push(((level as usize * 7 + i * 31) & 0xff) as u8);
        }
        lw = (lw / 2).max(1);
        lh = (lh / 2).max(1);
    }
    v
}

/// An uncompressed 32-bit BGRA DDS of one solid colour.
fn dds_raw(w: u32, h: u32, rgba: [u8; 4]) -> Vec<u8> {
    let mut v = vec![0u8; 128];
    v[0..4].copy_from_slice(b"DDS ");
    v[4..8].copy_from_slice(&124u32.to_le_bytes());
    v[8..12].copy_from_slice(&0x0000_100fu32.to_le_bytes());
    v[12..16].copy_from_slice(&h.to_le_bytes());
    v[16..20].copy_from_slice(&w.to_le_bytes());
    v[28..32].copy_from_slice(&1u32.to_le_bytes());
    v[76..80].copy_from_slice(&32u32.to_le_bytes());
    v[80..84].copy_from_slice(&0x41u32.to_le_bytes()); // RGB | ALPHAPIXELS
    v[88..92].copy_from_slice(&32u32.to_le_bytes());
    v[92..96].copy_from_slice(&0x00ff_0000u32.to_le_bytes()); // R
    v[96..100].copy_from_slice(&0x0000_ff00u32.to_le_bytes()); // G
    v[100..104].copy_from_slice(&0x0000_00ffu32.to_le_bytes()); // B
    v[104..108].copy_from_slice(&0xff00_0000u32.to_le_bytes()); // A
    for _ in 0..(w * h) {
        v.push(rgba[2]);
        v.push(rgba[1]);
        v.push(rgba[0]);
        v.push(rgba[3]);
    }
    v
}

/// An uncompressed 24-bit TGA, bottom-up like most of them, of one solid colour.
fn tga(w: u16, h: u16, rgb: [u8; 3], top_down: bool) -> Vec<u8> {
    let mut v = vec![0u8; 18];
    v[2] = 2; // truecolour, no RLE
    v[12..14].copy_from_slice(&w.to_le_bytes());
    v[14..16].copy_from_slice(&h.to_le_bytes());
    v[16] = 24;
    v[17] = if top_down { 0x20 } else { 0 };
    for _ in 0..(w as usize * h as usize) {
        v.push(rgb[2]);
        v.push(rgb[1]);
        v.push(rgb[0]);
    }
    v
}

/* ---- compressed files stay compressed ---------------------------------------------- */

#[test]
fn a_mipped_dxt1_is_handed_over_block_for_block() {
    let file = dds(b"DXT1", 64, 64, 7);
    let t = img::read(&file, "x.dds", true).unwrap();
    assert_eq!(t.kind, Kind::Bc1);
    assert_eq!((t.w, t.h), (64, 64));
    assert_eq!(t.levels.len(), 7, "every mip level should come across");
    assert_eq!(t.levels[0].2.len(), 16 * 16 * 8, "64² of DXT1 is 256 blocks of 8 bytes");
    assert_eq!(t.levels[6].2.len(), 8, "and the 1×1 level is one block");
    // Byte for byte what the file holds, from the offset the header implies.
    assert_eq!(&t.levels[0].2[..], &file[128..128 + 16 * 16 * 8]);
    let total: usize = t.levels.iter().map(|(_, _, b)| b.len()).sum();
    assert!(total < 64 * 64 * 4, "compressed and mipped is still smaller than one RGBA copy");
}

#[test]
fn dxt3_and_dxt5_come_back_as_their_own_kinds() {
    assert_eq!(img::read(&dds(b"DXT3", 32, 32, 6), "x.dds", true).unwrap().kind, Kind::Bc2);
    assert_eq!(img::read(&dds(b"DXT5", 32, 32, 6), "x.dds", true).unwrap().kind, Kind::Bc3);
}

#[test]
fn a_page_that_cannot_upload_blocks_gets_pixels() {
    /* A machine with no `WEBGL_compressed_texture_s3tc` must not be handed a texture it
       cannot use. The bargain is the same as everywhere else: the engine answers what the
       caller can act on, rather than the shape it happens to hold. */
    let t = img::read(&dds(b"DXT1", 32, 32, 6), "x.dds", false).unwrap();
    assert_eq!(t.kind, Kind::Rgba);
    assert_eq!(t.levels.len(), 1);
    assert_eq!(t.levels[0].2.len(), 32 * 32 * 4);
}

#[test]
fn a_compressed_file_with_no_mips_is_decoded_rather_than_left_blurry() {
    /* Mips cannot be generated on the GPU for a compressed texture, and grass ground seen
       at a distance with no mips shimmers. One level in the file means decode it here and
       let the page mip it, which is what it always did. */
    let t = img::read(&dds(b"DXT1", 32, 32, 1), "x.dds", true).unwrap();
    assert_eq!(t.kind, Kind::Rgba);
    assert_eq!(t.levels[0].2.len(), 32 * 32 * 4);
}

#[test]
fn a_header_promising_more_mips_than_it_carries_keeps_the_ones_it_has() {
    let mut file = dds(b"DXT1", 64, 64, 7);
    file.truncate(128 + 16 * 16 * 8 + 8 * 8 * 8); // two levels' worth
    let t = img::read(&file, "x.dds", true).unwrap();
    assert_eq!(t.kind, Kind::Bc1);
    assert_eq!(t.levels.len(), 2, "truncated, not refused");
}

/* ---- and everything else is decoded here ------------------------------------------- */

#[test]
fn an_uncompressed_dds_reads_its_masks() {
    let t = img::read(&dds_raw(4, 4, [200, 120, 40, 255]), "x.dds", true).unwrap();
    assert_eq!(t.kind, Kind::Rgba);
    assert_eq!(&t.levels[0].2[0..4], &[200, 120, 40, 255]);
}

#[test]
fn a_tga_reads_and_comes_out_the_right_way_up() {
    /* TGA rows run bottom-up unless the descriptor says otherwise, and a texture that
       arrives upside down is the kind of thing nobody notices on a rock and everybody
       notices on lettering. */
    let mut file = tga(2, 2, [10, 20, 30], false);
    // Make the first stored pixel distinguishable: bottom-up means it belongs to the
    // *last* row of the image once it is the right way up.
    file[18..21].copy_from_slice(&[99, 0, 0]); // B,G,R
    let t = img::read(&file, "x.tga", true).unwrap();
    assert_eq!(t.kind, Kind::Rgba);
    assert_eq!((t.w, t.h), (2, 2));
    let last_row = &t.levels[0].2[2 * 4..];
    assert_eq!(&last_row[0..3], &[0, 0, 99], "the first stored row belongs at the bottom");

    let top = img::read(&tga(2, 2, [10, 20, 30], true), "x.tga", true).unwrap();
    assert_eq!(&top.levels[0].2[0..4], &[10, 20, 30, 255], "and top-down is left alone");
}

#[test]
fn a_dds_wearing_a_tga_name_is_still_a_dds() {
    // Morrowind lies about extensions constantly; the magic decides.
    let t = img::read(&dds(b"DXT1", 16, 16, 5), "tx_ac_rock.tga", true).unwrap();
    assert_eq!(t.kind, Kind::Bc1);
}

#[test]
fn png_jpeg_and_bmp_are_left_to_the_browser() {
    /* Not laziness: `createImageBitmap` decodes them off the main thread already, and its
       decoders are better tested than anything written here would be. */
    for magic in [vec![0x89, 0x50, 0x4e, 0x47], vec![0xFF, 0xD8, 0xFF, 0xE0], vec![b'B', b'M', 0, 0]] {
        let t = img::read(&magic, "x.png", true).unwrap();
        assert_eq!(t.kind, Kind::File);
        assert_eq!(t.levels[0].2, magic, "the file's own bytes, untouched");
    }
}

#[test]
fn nonsense_is_an_error_rather_than_a_panic() {
    assert!(img::read(&[1, 2, 3], "x.dds", true).is_err());
    assert!(img::read(&[], "x.tga", true).is_err());
    // A DDS header claiming a compression nobody has.
    let mut bad = dds(b"DXT1", 8, 8, 1);
    bad[84..88].copy_from_slice(b"ATI2");
    assert!(img::read(&bad, "x.dds", true).is_err());
}

/* ---- the swatch --------------------------------------------------------------------- */

#[test]
fn a_thumbnail_comes_back_small_enough_to_be_a_swatch() {
    let t = img::read(&dds_raw(256, 128, [1, 2, 3, 255]), "x.dds", true).unwrap();
    let (w, h, px) = t.thumb.expect("no thumbnail");
    assert_eq!(w, img::THUMB, "the long side is the one that is capped");
    assert_eq!(h, img::THUMB / 2, "and the shape is kept");
    assert_eq!(px.len(), (w * h * 4) as usize);
    assert_eq!(&px[0..4], &[1, 2, 3, 255]);
}

#[test]
fn a_compressed_texture_still_gets_a_swatch() {
    /* Made from a mip rather than from the top level, so the page can draw a card without
       decoding four megabytes to fill 72 pixels. */
    let t = img::read(&dds(b"DXT1", 64, 64, 7), "x.dds", true).unwrap();
    assert_eq!(t.kind, Kind::Bc1);
    assert!(t.thumb.is_some(), "a compressed texture has no swatch");
}

#[test]
fn the_swatch_is_not_taken_from_the_last_mip() {
    /* The last mip of a full chain is one texel, and a swatch made from it is a flat
       colour — which is what the land-texture grid showed for most of an install. The
       rule is the smallest mip that is still at least a swatch across. A 512-pixel
       texture with a full chain has 10 levels; the one wanted here is the 128 or 64 one,
       and what it must not be is 1x1. */
    let t = img::read(&dds(b"DXT1", 512, 512, 10), "x.dds", true).unwrap();
    let (w, h, px) = t.thumb.expect("no thumbnail");
    assert!(w >= 32 && h >= 32, "swatch is {w}x{h} — that is a colour, not a picture");
    assert_eq!(px.len(), (w * h * 4) as usize);
}

#[test]
fn a_texture_smaller_than_a_swatch_is_left_alone() {
    let t = img::read(&dds_raw(8, 8, [9, 9, 9, 255]), "x.dds", true).unwrap();
    let (w, h, _) = t.thumb.unwrap();
    assert_eq!((w, h), (8, 8));
}

/* ---- the decoder itself ------------------------------------------------------------- */

#[test]
fn a_dxt1_block_decodes_to_the_colours_it_names() {
    /* One block, two endpoints, every texel on the first. Pinning the arithmetic rather
       than the plumbing: 565 red is 0xF800, which is 255 after scaling. */
    let mut block = vec![0u8; 8];
    block[0..2].copy_from_slice(&0xF800u16.to_le_bytes()); // c0 = red
    block[2..4].copy_from_slice(&0x001Fu16.to_le_bytes()); // c1 = blue
    // index bits all zero: every texel takes c0.
    let px = img::decode_blocks(&block, 4, 4, Kind::Bc1);
    assert_eq!(px.len(), 4 * 4 * 4);
    for i in 0..16 {
        assert_eq!(&px[i * 4..i * 4 + 4], &[255, 0, 0, 255], "texel {i}");
    }

    // And with every index at 1, every texel is the other endpoint.
    let mut b2 = block.clone();
    b2[4..8].copy_from_slice(&0x5555_5555u32.to_le_bytes());
    let px2 = img::decode_blocks(&b2, 4, 4, Kind::Bc1);
    assert_eq!(&px2[0..4], &[0, 0, 255, 255]);
}

#[test]
fn dxt1_punchthrough_alpha_is_honoured() {
    /* When c0 <= c1 the fourth colour is transparent — that is how a DXT1 file carries
       one-bit alpha, and grass meshes lean on it for every leaf edge. */
    let mut block = vec![0u8; 8];
    block[0..2].copy_from_slice(&0x0001u16.to_le_bytes()); // c0 low
    block[2..4].copy_from_slice(&0xFFFFu16.to_le_bytes()); // c1 high
    block[4..8].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes()); // every index = 3
    let px = img::decode_blocks(&block, 4, 4, Kind::Bc1);
    assert_eq!(px[3], 0, "the fourth colour of a punchthrough block is transparent");
}

#[test]
fn a_bc3_alpha_block_reads_its_endpoints() {
    let mut block = vec![0u8; 16];
    block[0] = 200; // a0
    block[1] = 40; // a1
                   // alpha indices all zero -> a0 everywhere; colour bits zero -> c0.
    block[8..10].copy_from_slice(&0xFFFFu16.to_le_bytes());
    let px = img::decode_blocks(&block, 4, 4, Kind::Bc3);
    assert_eq!(px[3], 200);
    assert_eq!(px[7], 200);
}

/// Round 14: a DX10 header — the way modern texture packs spell the same block formats.
/// The DXGI number sits in the 20 extra bytes after the 128; BC1..3 read as before, BC7
/// is refused by name (so the report can say which replacer to look at).
fn dds_dx10(dxgi: u32, w: u32, h: u32, mips: u32, bpb: usize) -> Vec<u8> {
    let mut v = dds(b"DXT1", w, h, 0);
    v.truncate(128);
    v[84..88].copy_from_slice(b"DX10");
    v[28..32].copy_from_slice(&mips.to_le_bytes());
    v.extend_from_slice(&dxgi.to_le_bytes());
    v.extend_from_slice(&3u32.to_le_bytes()); // D3D10_RESOURCE_DIMENSION_TEXTURE2D
    v.extend_from_slice(&[0u8; 12]);
    let (mut lw, mut lh) = (w, h);
    for level in 0..mips {
        let blocks = (lw.div_ceil(4) as usize) * (lh.div_ceil(4) as usize);
        for i in 0..blocks * bpb {
            v.push(((level as usize * 7 + i * 31) & 0xff) as u8);
        }
        lw = (lw / 2).max(1);
        lh = (lh / 2).max(1);
    }
    v
}

#[test]
fn a_dx10_header_reads_bc1_to_bc3_and_names_bc7() {
    let t = img::read(&dds_dx10(71, 64, 64, 7, 8), "x.dds", true).unwrap();
    assert_eq!(t.kind, img::Kind::Bc1);
    assert_eq!(t.levels.len(), 7);
    assert_eq!(t.levels[0].2.len(), 16 * 16 * 8, "the blocks start after the 148-byte header");
    let t = img::read(&dds_dx10(77, 32, 32, 6, 16), "x.dds", true).unwrap();
    assert_eq!(t.kind, img::Kind::Bc3);
    let e = match img::read(&dds_dx10(98, 32, 32, 1, 16), "x.dds", true) { Err(e) => e, Ok(_) => panic!("BC7 read") };
    assert!(e.contains("BC7"), "{e}");
}

/// Round 15: a volume DDS (DDSD_DEPTH) — MGE XE's water normals — comes back as every
/// slice of its top level, RGBA, with the depth said.
#[test]
fn a_volume_dds_hands_over_all_its_slices() {
    let (w, h, d) = (4u32, 2u32, 3u32);
    let mut v = dds_raw(w, h, [10, 20, 30, 255]);
    v.truncate(128);
    let flags = u32::from_le_bytes([v[8], v[9], v[10], v[11]]) | 0x80_0000;
    v[8..12].copy_from_slice(&flags.to_le_bytes());
    v[24..28].copy_from_slice(&d.to_le_bytes());
    for z in 0..d {
        for _ in 0..(w * h) {
            v.extend_from_slice(&[30, 20, 10 + z as u8, 255]); // BGRA
        }
    }
    let t = img::read(&v, "water_NRM.dds", true).unwrap();
    assert_eq!(t.kind, img::Kind::Volume);
    assert_eq!((t.w, t.h, t.depth), (w, h, d));
    let px = &t.levels[0].2;
    assert_eq!(px.len(), (w * h * d * 4) as usize);
    assert_eq!(&px[0..4], &[10, 20, 30, 255], "slice 0, red from the mask");
    let last = (w * h * (d - 1) * 4) as usize;
    assert_eq!(&px[last..last + 4], &[12, 20, 30, 255], "slice 2");
}
