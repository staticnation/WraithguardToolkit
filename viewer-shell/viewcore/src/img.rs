//! Reading a texture, so the window does not have to.
//!
//! Every texture used to be decoded in JavaScript on the thread the window is drawn on
//! and then uploaded uncompressed. Measured, that is **11.7 ms and 5.4 MB of video memory
//! for a 1024² DXT1** whose file is 512 KB — and a busy modded cell touches hundreds of
//! them, so connecting an install and looking at a cell spent seconds of blocked thread
//! and put eight times more in the GPU than the files hold.
//!
//! Two things follow, and this module is both of them.
//!
//! **Compressed textures stay compressed.** A DDS holding BC1/BC2/BC3 blocks with its own
//! mip chain is handed over untouched, block for block, for the page to upload with
//! `compressedTexImage2D`. Nothing is decoded, nothing is expanded, and the GPU holds what
//! the file holds.
//!
//! **Everything else is decoded here.** An uncompressed DDS, a TGA, a BC file with no mip
//! chain to upload — all decoded in Rust, on the thread the command already runs on
//! (§43, §46), never on the page's. The formats the browser decodes better than we could —
//! PNG, JPEG, BMP — are passed through as the file's own bytes for `createImageBitmap`,
//! which is off the main thread anyway.
//!
//! So the page has no image decoder in its hot path at all, and this file is where the
//! knowledge of DDS and TGA now lives — the same move as §17b's one NIF reader, for the
//! same reason.

/// What the page is being handed, and what it must do with it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// BC1/DXT1 blocks, 8 bytes per 4×4 block.
    Bc1,
    /// BC2/DXT3 blocks, 16 bytes.
    Bc2,
    /// BC3/DXT5 blocks, 16 bytes.
    Bc3,
    /// Wraithguard: BC4 (ATI1) blocks, 8 bytes - one channel.
    Bc4,
    /// BC5 (ATI2) blocks, 16 bytes - two channels, a normal map's X and Y; the page
    /// rebuilds Z.
    Bc5,
    /// BC7 blocks, 16 bytes.
    Bc7,
    /// Straight RGBA8, one level; the page generates the mips.
    Rgba,
    /// The file's own bytes, for a format the browser reads better than we do.
    File,
    /// A volume (round 15): straight RGBA8, `depth` slices of w×h in one level, for a
    /// 3D texture - MGE XE's `textures\MGE\water_NRM.dds`, its animated water normals.
    Volume,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::Bc1 => "bc1",
            Kind::Bc2 => "bc2",
            Kind::Bc3 => "bc3",
            Kind::Bc4 => "bc4",
            Kind::Bc5 => "bc5",
            Kind::Bc7 => "bc7",
            Kind::Rgba => "rgba",
            Kind::File => "file",
            Kind::Volume => "volume",
        }
    }
    fn block_bytes(self) -> usize {
        match self {
            Kind::Bc1 | Kind::Bc4 => 8,
            _ => 16,
        }
    }
}

/// Wraithguard: the block formats the page can take beyond S3TC - `rgtc` (BC4, BC5) and
/// `bptc` (BC7), as its WebGL has `EXT_texture_compression_rgtc` / `_bptc`. What it
/// cannot take is decoded here instead (bcx.rs).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Extra {
    pub rgtc: bool,
    pub bptc: bool,
}

/// A texture ready for the GPU.
/// What a texture's alpha channel is *for* — round 17n.
///
/// A NiAlphaProperty asking for SRC_ALPHA blending says how to draw the shape but not
/// what its texture holds, and the two want opposite treatment. Cutting a soft gradient
/// at a threshold turns a waterfall into a hard stencil (Robin: "a geometric pattern of
/// darker and brighter areas where it shouldn't be"); blending a hard cutout costs the
/// sorting and gains nothing. Measured on Morrowind's own textures at full resolution:
///
/// | texture | alpha |
/// |---|---|
/// | `Tx_waterfall_01` | spread across every value — **soft** |
/// | `Tx_GG_fence_01` (the Ghostfence) | spread, mostly mid — **soft** |
/// | `Tx_window_pane` | every texel at ~0.4 — **soft**, a uniformly translucent pane |
/// | `tx_dwrv_parch20` | 13k at 0, 51k at 255, 2.7% between — **binary** |
/// | `tx_bc_grass`, `Tx_metal_6th_bells` | all 255 — **opaque** |
///
/// Taken at the top mip and never from the thumbnail: downscaling averages a cutout's
/// 0s and 255s into everything between, so every mask would read as soft.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Alpha {
    /// Nothing below 240: the channel carries no information.
    #[default]
    Opaque,
    /// Two clusters at the ends — a stencil. Cut it.
    Binary,
    /// A real gradient, or a flat value that is not 1. Blend it.
    Soft,
}

impl Alpha {
    pub fn name(&self) -> &'static str {
        match self {
            Alpha::Opaque => "opaque",
            Alpha::Binary => "binary",
            Alpha::Soft => "soft",
        }
    }
}

/// Classifies one RGBA buffer's alpha. `SOFT_AT` is the share of texels strictly between
/// the ends that makes a channel a gradient rather than a stencil; the measured meshes
/// land at 2.7% (binary) and 95%+ (soft), so anything in the middle is a wide margin.
/// How wide the two ends are. A stencil's edge texels are not all exactly 0 and 255 —
/// `tx_dwrv_parch20` has 2.7% of its texels off the ends and is plainly a cutout — so the
/// ends have to be bands rather than values, or every mask reads as a gradient.
const ENDS: u8 = 31;
const HIGH: u8 = 224;

pub fn classify_alpha(rgba: &[u8]) -> Alpha {
    const SOFT_AT: u64 = 5; // percent
    let (mut low, mut mid) = (0u64, 0u64);
    let n = (rgba.len() / 4) as u64;
    if n == 0 {
        return Alpha::Opaque;
    }
    for c in rgba.chunks_exact(4) {
        match c[3] {
            0..=ENDS => low += 1,
            HIGH..=255 => {}
            _ => mid += 1,
        }
    }
    if mid * 100 > n * SOFT_AT {
        Alpha::Soft
    } else if low > 0 {
        Alpha::Binary
    } else {
        Alpha::Opaque
    }
}

/// The alpha of a compressed block image, without decoding its colours.
///
/// BC1 carries one bit of alpha and can only ever be a stencil; BC2 keeps four bits and
/// BC3 an interpolated eight, both in the block's first eight bytes, which is all this
/// reads. Cheaper than a full decode and exact at full resolution.
pub fn classify_alpha_blocks(data: &[u8], w: u32, h: u32, kind: Kind) -> Alpha {
    let (w, h) = (w.max(1) as usize, h.max(1) as usize);
    let (bw, bh) = (w.div_ceil(4), h.div_ceil(4));
    let step = kind.block_bytes();
    if kind == Kind::Bc1 {
        /* One bit, and only when the block says so (c0 <= c1). Nothing between, so it is
           a stencil if any texel is clear and opaque otherwise. */
        let mut p = 0usize;
        for _ in 0..bw * bh {
            if p + step > data.len() {
                break;
            }
            let (c0, c1) = (u16le(data, p), u16le(data, p + 2));
            if c0 <= c1 {
                let bits = u32le(data, p + 4);
                for nib in 0..16 {
                    if (bits >> (2 * nib)) & 3 == 3 {
                        return Alpha::Binary;
                    }
                }
            }
            p += step;
        }
        return Alpha::Opaque;
    }
    let (mut low, mut mid, mut n) = (0u64, 0u64, 0u64);
    let mut p = 0usize;
    for by in 0..bh {
        for bx in 0..bw {
            if p + step > data.len() {
                break;
            }
            for nib in 0..16usize {
                let (x, y) = ((bx << 2) + (nib & 3), (by << 2) + (nib >> 2));
                if x >= w || y >= h {
                    continue;
                }
                let a = match kind {
                    Kind::Bc2 => {
                        let byte = data[p + (nib >> 1)];
                        let v = if nib & 1 == 1 { byte >> 4 } else { byte & 0x0f };
                        (v as u32 * 255 / 15) as u8
                    }
                    _ => bc3_alpha(data, p, nib),
                };
                n += 1;
                match a {
                    0..=ENDS => low += 1,
                    HIGH..=255 => {}
                    _ => mid += 1,
                }
            }
            p += step;
        }
    }
    if n == 0 {
        Alpha::Opaque
    } else if mid * 100 > n * 5 {
        Alpha::Soft
    } else if low > 0 {
        Alpha::Binary
    } else {
        Alpha::Opaque
    }
}

pub struct Texture {
    pub kind: Kind,
    /// What the alpha channel is for (round 17n) — read at full resolution, before any
    /// level is dropped for the size cap.
    pub alpha: Alpha,
    pub w: u32,
    pub h: u32,
    /// Slices, for a `Kind::Volume`; 1 for everything else.
    pub depth: u32,
    /// One entry per mip level: its size in pixels and its bytes.
    pub levels: Vec<(u32, u32, Vec<u8>)>,
    /// A small RGBA copy for the interface's swatches, at most `THUMB` on its long side.
    /// Made here because the page would otherwise have to decode the whole texture to
    /// draw a 72-pixel square.
    pub thumb: Option<(u32, u32, Vec<u8>)>,
}

/// The longest side of the thumbnail handed back for a swatch.
pub const THUMB: u32 = 72;

fn u32le(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}
fn u16le(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}

/// Reads whatever the bytes turn out to be.
///
/// `want_bc` is the page saying whether it can upload compressed blocks — a machine
/// without `WEBGL_compressed_texture_s3tc` gets RGBA instead of a texture it cannot use.
/// Sniffed by magic first and extension second, because Morrowind lies about extensions
/// constantly: a great many `.tga` files in the wild are DDS inside.
pub fn read(buf: &[u8], path: &str, want_bc: bool) -> Result<Texture, String> {
    read_with(buf, path, want_bc, Extra::default())
}

/// [`read`], with the block formats beyond S3TC the page can take.
pub fn read_with(buf: &[u8], path: &str, want_bc: bool, extra: Extra) -> Result<Texture, String> {
    if buf.len() >= 4 && &buf[0..4] == b"DDS " {
        return dds(buf, want_bc, extra);
    }
    if buf.len() >= 2 && buf[0] == 0x89 && buf[1] == 0x50 {
        return Ok(passthrough(buf, Kind::File));
    }
    if buf.len() >= 2 && buf[0] == 0xFF && buf[1] == 0xD8 {
        return Ok(passthrough(buf, Kind::File));
    }
    if buf.len() >= 2 && buf[0] == b'B' && buf[1] == b'M' {
        return Ok(passthrough(buf, Kind::File));
    }
    // TGA has no magic at the front, so it is what is left. The extension only decides
    // which error to report when it is not one either.
    match tga(buf) {
        Ok(t) => Ok(t),
        Err(e) => {
            let ext = path.rsplit('.').next().unwrap_or("");
            Err(format!("unrecognised image format (.{ext}): {e}"))
        }
    }
}

/// A file the browser will decode, handed over as it is.
fn passthrough(buf: &[u8], kind: Kind) -> Texture {
    // A format handed on untouched: nothing decoded here, so nothing to classify.
    Texture { kind, alpha: Alpha::Opaque, w: 0, h: 0, depth: 1, levels: vec![(0, 0, buf.to_vec())], thumb: None }
}

fn rgba_texture(w: u32, h: u32, px: Vec<u8>) -> Texture {
    let thumb = shrink(w, h, &px);
    let alpha = classify_alpha(&px);
    Texture { kind: Kind::Rgba, alpha, w, h, depth: 1, levels: vec![(w, h, px)], thumb }
}

/* ---- DDS ---------------------------------------------------------------------------- */

/// The largest side a texture may claim. Round 18bc (B4).
///
/// Nothing in Morrowind is anywhere near this — a 4096 replacer is already a big
/// texture — and the cap is what stops `w * h * 4` from reaching for tens of gigabytes
/// on a header nobody wrote.
const MAX_SIDE: usize = 16_384;

/// Whether a decoder's header is plausible **against the file it came out of** — round
/// 18bc (B4).
///
/// `w`, `h` and `depth` used to come straight off the header and were never compared with
/// the buffer at all: a **truncated 18-byte file** made the TGA reader allocate **1.6 GB**
/// and spin for 12.3 seconds, and the 16-bit TGA fields top out at 65535² → 17 GB. That
/// path is not exotic, because `read` treats "not DDS, not PNG, not JPEG, not BMP" as
/// TGA: a half-finished download, a readme renamed `.tga`, a DDS whose magic got mangled.
/// `assets_bundle` runs several at once and the desktop shell aborts on a failed
/// allocation rather than unwinding, so it was a dead process rather than an error.
///
/// Two cheap tests, and both have to hold. The file must carry at least **one row** of
/// the format it claims (`row`), which is what a truncated file fails; and the pixels it
/// claims may be at most **128 times** what the file could hold, which is the theoretical
/// ceiling of TGA's own run-length packets and so cannot reject a real file. Decoding
/// itself stays lenient: a file that passes these and then runs out mid-way still decodes
/// the part that is there, which is what a slightly short texture used to do.
fn plausible(w: usize, h: usize, depth: usize, avail: usize, row: usize) -> Result<(), String> {
    if w == 0 || h == 0 || depth == 0 {
        return Err("zero dimension".into());
    }
    if w > MAX_SIDE || h > MAX_SIDE || depth > 512 {
        return Err(format!("implausible image size {w}×{h}×{depth}"));
    }
    if avail < row.max(1) {
        return Err(format!(
            "the file has {avail} bytes of image data, too few for one {w}-pixel row of a {w}×{h} image"
        ));
    }
    if w.saturating_mul(h).saturating_mul(depth) > avail.saturating_mul(128) {
        return Err(format!(
            "a {w}×{h}×{depth} image cannot fit in {avail} bytes — the header does not match the file"
        ));
    }
    Ok(())
}

fn dds(buf: &[u8], want_bc: bool, extra: Extra) -> Result<Texture, String> {
    if buf.len() < 128 {
        return Err("DDS header is truncated".into());
    }
    let height = u32le(buf, 12);
    let width = u32le(buf, 16);
    let flags = u32le(buf, 8);
    // DDSD_DEPTH: a volume, `depth` slices of the top level laid end to end.
    let depth = if flags & 0x80_0000 != 0 { u32le(buf, 24).max(1) } else { 1 };
    let mips = u32le(buf, 28).max(1);
    let pf_flags = u32le(buf, 80);
    let four = &buf[84..88];
    let rgb_bits = u32le(buf, 88);
    let (r_mask, g_mask, b_mask, a_mask) =
        (u32le(buf, 92), u32le(buf, 96), u32le(buf, 100), u32le(buf, 104));
    let mut off = 128usize;
    if four == b"DX10" {
        off += 20;
    }
    let (w, h) = (width.max(1), height.max(1));

    if pf_flags & 0x4 != 0 {
        /* A DX10 header (round 14): the format is a DXGI number in the extra 20 bytes,
           which is how modern texture packs spell the same three block formats — and
           BC7, which is not decoded here yet. The message names the format so a white
           pot on Robin's machine says which replacer to look at. */
        /* Wraithguard: BC4, BC5 and BC7 - a normal map is commonly BC5 (ATI2), and a
           modern pack BC7 - decoded here to RGBA (bcx.rs), top level, for the page to mip.
           BC5's blue is its Z, rebuilt from X and Y, so it reads as an ordinary map. */
        let special: Option<u8> = if four == b"DX10" && buf.len() >= 148 {
            match u32le(buf, 128) {
                79..=81 => Some(4),
                82..=84 => Some(5),
                97..=99 => Some(7),
                _ => None,
            }
        } else {
            match four {
                b"ATI1" | b"BC4U" | b"BC4S" => Some(4),
                b"ATI2" | b"BC5U" | b"BC5S" => Some(5),
                _ => None,
            }
        };
        if let Some(k) = special {
            /* Passed through as blocks when the page's WebGL takes them and the file has
               its mips - a normal map at the GPU's own compression, not four times the
               memory as RGBA. */
            let pass = want_bc && if k == 7 { extra.bptc } else { extra.rgtc };
            if pass {
                let kind = match k {
                    4 => Kind::Bc4,
                    5 => Kind::Bc5,
                    _ => Kind::Bc7,
                };
                let levels = block_levels(buf, off, w, h, mips, kind);
                if levels.len() > 1 {
                    let pick = levels.iter().rev().find(|(lw, lh, _)| (*lw).max(*lh) >= THUMB).or_else(|| levels.first());
                    let px = pick.and_then(|(lw, lh, bytes)| {
                        let (uw, uh) = (*lw as usize, *lh as usize);
                        let px = match kind {
                            Kind::Bc4 => crate::bcx::decode_bc4(bytes, uw, uh),
                            Kind::Bc5 => crate::bcx::decode_bc5(bytes, uw, uh),
                            _ => crate::bcx::decode_bc7(bytes, uw, uh),
                        }
                        .ok()?;
                        Some((*lw, *lh, px))
                    });
                    // The alpha from the swatch's mip: BC4 and BC5 have none.
                    let alpha = match (&px, kind) {
                        (Some((_, _, p)), Kind::Bc7) => classify_alpha(p),
                        _ => Alpha::Opaque,
                    };
                    let thumb = px.and_then(|(lw, lh, p)| shrink(lw, lh, &p));
                    return Ok(Texture { kind, alpha, w, h, depth: 1, levels, thumb });
                }
            }
            let block = if k == 4 { 8 } else { 16 };
            plausible(w as usize, h as usize, 1, buf.len().saturating_sub(off), (w as usize).div_ceil(4) * block)?;
            let data = &buf[off..];
            let (uw, uh) = (w as usize, h as usize);
            let px = match k {
                4 => crate::bcx::decode_bc4(data, uw, uh),
                5 => crate::bcx::decode_bc5(data, uw, uh),
                _ => crate::bcx::decode_bc7(data, uw, uh),
            }?;
            return Ok(rgba_texture(w, h, px));
        }
        let kind = if four == b"DX10" {
            if buf.len() < 148 {
                return Err("DDS DX10 header is truncated".into());
            }
            match u32le(buf, 128) {
                70..=72 => Kind::Bc1,
                73..=75 => Kind::Bc2,
                76..=78 => Kind::Bc3,
                79..=81 => return Err("unsupported DDS compression BC4 (DX10 header)".into()),
                82..=84 => return Err("unsupported DDS compression BC5 (DX10 header)".into()),
                94..=96 => return Err("unsupported DDS compression BC6H (DX10 header)".into()),
                97..=99 => return Err("unsupported DDS compression BC7 (DX10 header)".into()),
                other => return Err(format!("unsupported DDS DX10 format {other}")),
            }
        } else {
            match four {
                b"DXT1" => Kind::Bc1,
                b"DXT2" | b"DXT3" => Kind::Bc2,
                b"DXT4" | b"DXT5" => Kind::Bc3,
                other => {
                    let name = String::from_utf8_lossy(other).to_string();
                    return Err(format!("unsupported DDS compression \"{name}\""));
                }
            }
        };
        // Round 18bc (B4): one row of blocks has to be there before `decode_blocks` is
        // asked for a `w × h × 4` buffer — with no levels at all it would still allocate.
        plausible(
            w as usize,
            h as usize,
            1,
            buf.len().saturating_sub(off),
            (w as usize).div_ceil(4) * kind.block_bytes(),
        )?;
        let levels = block_levels(buf, off, w, h, mips, kind);
        /* Compressed and mipped is the good case: hand the blocks straight over. With no
           mip chain there is nothing to hand over but the top level, and a compressed
           texture cannot have its mips generated on the GPU — so it is decoded here
           instead and the page mips it as it always did. Sharper than a texture with no
           mips at all, which shimmers on ground seen at a distance. */
        if want_bc && levels.len() > 1 {
            /* The swatch comes off the **smallest mip that is still big enough to look
               like anything**, not off the last one. The last is where the chain ends —
               commonly 1x1 or 4x4 — and a thumbnail made from four pixels is a flat
               colour, which is exactly what the land-texture grid was showing for most of
               an install: Robin, "the grid view for textures mostly shows just a color".
               Decoding a 128-pixel mip instead of a 4096-pixel top level is still the
               cheap end of the chain, which was the point of taking a mip at all. */
            let pick = levels
                .iter()
                .rev()
                .find(|(lw, lh, _)| (*lw).max(*lh) >= THUMB)
                .or_else(|| levels.first());
            let thumb = pick.and_then(|(lw, lh, bytes)| {
                let px = decode_blocks(bytes, *lw, *lh, kind);
                shrink(*lw, *lh, &px)
            });
            /* From the top level's own blocks, at full resolution — the level that is
               about to be handed to the page, before the size cap drops any of them. */
            let alpha = levels
                .first()
                .map(|(lw, lh, b)| classify_alpha_blocks(b, *lw, *lh, kind))
                .unwrap_or(Alpha::Opaque);
            return Ok(Texture { kind, alpha, w, h, depth: 1, levels, thumb });
        }
        let first = levels.first().map(|(_, _, b)| b.clone()).unwrap_or_default();
        let px = decode_blocks(&first, w, h, kind);
        return Ok(rgba_texture(w, h, px));
    }

    // Uncompressed: any bit layout the masks describe.
    let bpp = (rgb_bits / 8) as usize;
    if !(1..=4).contains(&bpp) {
        return Err(format!("unsupported DDS bit depth {rgb_bits}"));
    }
    let has_a = pf_flags & 0x1 != 0 && a_mask != 0;
    let shift_of = |m: u32| if m == 0 { 0 } else { m.trailing_zeros() };
    let size_of = |m: u32| if m == 0 { 8 } else { (m >> m.trailing_zeros()).count_ones() };
    let (rs, gs, bs, a_s) = (shift_of(r_mask), shift_of(g_mask), shift_of(b_mask), shift_of(a_mask));
    let (rn, gn, bn, an) = (size_of(r_mask), size_of(g_mask), size_of(b_mask), size_of(a_mask));
    let scale = |v: u32, bits: u32| -> u8 {
        let max = (1u32 << bits) - 1;
        if max == 0 { 0 } else { ((v * 255 + max / 2) / max) as u8 }
    };
    // Every slice of a volume's top level; one slice for a plain texture.
    // Round 18bc (B4): and only once the header agrees with the file.
    plausible(w as usize, h as usize, depth as usize, buf.len().saturating_sub(off), w as usize * bpp)?;
    let n = (w as usize) * (h as usize) * (depth as usize);
    let mut px = vec![0u8; n * 4];
    for i in 0..n {
        let s = off + i * bpp;
        if s + bpp > buf.len() {
            break;
        }
        let mut v = 0u32;
        for k in 0..bpp {
            v |= (buf[s + k] as u32) << (8 * k);
        }
        let o = i * 4;
        px[o] = scale((v & r_mask) >> rs, rn);
        px[o + 1] = scale((v & g_mask) >> gs, gn);
        px[o + 2] = scale((v & b_mask) >> bs, bn);
        px[o + 3] = if has_a { scale((v & a_mask) >> a_s, an) } else { 255 };
    }
    if depth > 1 {
        return Ok(Texture { kind: Kind::Volume, alpha: Alpha::Opaque, w, h, depth, levels: vec![(w, h, px)], thumb: None });
    }
    Ok(rgba_texture(w, h, px))
}

/// Splits a compressed DDS body into its mip levels.
///
/// Stops early rather than reading past the end: a file whose header promises more mips
/// than it carries is common enough, and the levels it does have are still usable.
fn block_levels(buf: &[u8], mut off: usize, w: u32, h: u32, mips: u32, kind: Kind) -> Vec<(u32, u32, Vec<u8>)> {
    let mut out = Vec::new();
    let (mut lw, mut lh) = (w, h);
    for _ in 0..mips {
        let blocks = ((lw as usize).div_ceil(4)) * ((lh as usize).div_ceil(4));
        let want = blocks * kind.block_bytes();
        if off + want > buf.len() {
            break;
        }
        out.push((lw, lh, buf[off..off + want].to_vec()));
        off += want;
        if lw == 1 && lh == 1 {
            break;
        }
        lw = (lw / 2).max(1);
        lh = (lh / 2).max(1);
    }
    out
}

/// One 4×4 block's four colours, from the two 16-bit endpoints.
fn block_colours(c0: u16, c1: u16, punchthrough: bool) -> [[u8; 4]; 4] {
    let rgb = |v: u16| {
        [
            (((v >> 11) & 0x1f) as u32 * 255 / 31) as u8,
            (((v >> 5) & 0x3f) as u32 * 255 / 63) as u8,
            ((v & 0x1f) as u32 * 255 / 31) as u8,
            255,
        ]
    };
    let (a, b) = (rgb(c0), rgb(c1));
    let mut out = [a, b, [0, 0, 0, 255], [0, 0, 0, 255]];
    if punchthrough {
        // DXT1's one-bit alpha: the fourth colour is transparent black.
        for i in 0..3 {
            out[2][i] = ((a[i] as u16 + b[i] as u16) / 2) as u8;
            out[3][i] = 0;
        }
        out[3][3] = 0;
    } else {
        for i in 0..3 {
            out[2][i] = ((2 * a[i] as u16 + b[i] as u16 + 1) / 3) as u8;
            out[3][i] = ((a[i] as u16 + 2 * b[i] as u16 + 1) / 3) as u8;
        }
    }
    out
}

/// BC1/BC2/BC3 blocks to RGBA.
pub fn decode_blocks(data: &[u8], w: u32, h: u32, kind: Kind) -> Vec<u8> {
    let (w, h) = (w.max(1) as usize, h.max(1) as usize);
    let mut out = vec![0u8; w * h * 4];
    let (bw, bh) = (w.div_ceil(4), h.div_ceil(4));
    let step = kind.block_bytes();
    let mut p = 0usize;
    for by in 0..bh {
        for bx in 0..bw {
            if p + step > data.len() {
                return out;
            }
            let (alpha_at, colour_at) = match kind {
                Kind::Bc1 => (None, p),
                _ => (Some(p), p + 8),
            };
            let c0 = u16le(data, colour_at);
            let c1 = u16le(data, colour_at + 2);
            let bits = u32le(data, colour_at + 4);
            let cols = block_colours(c0, c1, kind == Kind::Bc1 && c0 <= c1);
            for ty in 0..4 {
                let y = (by << 2) + ty;
                if y >= h {
                    break;
                }
                for tx in 0..4 {
                    let x = (bx << 2) + tx;
                    if x >= w {
                        continue;
                    }
                    let nib = 4 * ty + tx;
                    let c = cols[((bits >> (2 * nib)) & 3) as usize];
                    let o = (y * w + x) * 4;
                    out[o] = c[0];
                    out[o + 1] = c[1];
                    out[o + 2] = c[2];
                    out[o + 3] = match (kind, alpha_at) {
                        (Kind::Bc2, Some(a)) => {
                            let byte = data[a + (nib >> 1)];
                            let v = if nib & 1 == 1 { byte >> 4 } else { byte & 0x0f };
                            (v as u32 * 255 / 15) as u8
                        }
                        (Kind::Bc3, Some(a)) => bc3_alpha(data, a, nib),
                        _ => c[3],
                    };
                }
            }
            p += step;
        }
    }
    out
}

/// One texel of a BC3 alpha block: two endpoints and a three-bit index each.
fn bc3_alpha(d: &[u8], at: usize, nib: usize) -> u8 {
    let (a0, a1) = (d[at] as u32, d[at + 1] as u32);
    let bits = if nib < 8 {
        (d[at + 2] as u32) | ((d[at + 3] as u32) << 8) | ((d[at + 4] as u32) << 16)
    } else {
        (d[at + 5] as u32) | ((d[at + 6] as u32) << 8) | ((d[at + 7] as u32) << 16)
    };
    let idx = (bits >> (3 * (nib % 8))) & 7;
    let v = match idx {
        0 => a0,
        1 => a1,
        _ if a0 > a1 => ((8 - idx) * a0 + (idx - 1) * a1) / 7,
        6 => 0,
        7 => 255,
        _ => ((6 - idx) * a0 + (idx - 1) * a1) / 5,
    };
    v as u8
}

/* ---- TGA ---------------------------------------------------------------------------- */

fn tga(buf: &[u8]) -> Result<Texture, String> {
    if buf.len() < 18 {
        return Err("too small to be a TGA".into());
    }
    let id_len = buf[0] as usize;
    let cm_type = buf[1];
    let img_type = buf[2];
    let cm_first = u16le(buf, 3) as i32;
    let cm_len = u16le(buf, 5) as usize;
    let cm_bits = buf[7] as usize;
    let w = u16le(buf, 12) as usize;
    let h = u16le(buf, 14) as usize;
    let bpp = buf[16] as usize;
    let desc = buf[17];
    if w == 0 || h == 0 {
        return Err("zero dimension".into());
    }
    let mut p = 18 + id_len;

    let mut palette: Vec<u8> = Vec::new();
    if cm_type == 1 {
        let psz = cm_bits / 8;
        palette = vec![0u8; cm_len * 4];
        for i in 0..cm_len {
            let s = p + i * psz;
            if s + psz > buf.len() {
                break;
            }
            let o = i * 4;
            match cm_bits {
                24 => {
                    palette[o] = buf[s + 2];
                    palette[o + 1] = buf[s + 1];
                    palette[o + 2] = buf[s];
                    palette[o + 3] = 255;
                }
                32 => {
                    palette[o] = buf[s + 2];
                    palette[o + 1] = buf[s + 1];
                    palette[o + 2] = buf[s];
                    palette[o + 3] = buf[s + 3];
                }
                16 => {
                    let v = u16le(buf, s) as u32;
                    palette[o] = (((v >> 10) & 31) * 255 / 31) as u8;
                    palette[o + 1] = (((v >> 5) & 31) * 255 / 31) as u8;
                    palette[o + 2] = ((v & 31) * 255 / 31) as u8;
                    palette[o + 3] = 255;
                }
                _ => {}
            }
        }
        p += cm_len * psz;
    }

    let bytes = (bpp / 8).max(1);
    /* Round 18bf (I14): how many bits of each pixel are alpha, which is the low nibble of
       the image descriptor and the only thing that says whether there is an alpha channel
       at all.

       A 16-bit TGA is 5-5-5 with one bit left over. With one attribute bit that bit is
       the alpha; with **none** it is unused, and reading it as alpha made every opaque
       16-bit TGA whose spare bit happened to be clear come back at alpha 0 — classified
       "binary" by the alpha survey and then cut to invisible. The palette reader three
       cases up has always forced 255 for exactly this layout, which is the two halves of
       one decoder disagreeing about one format. 32-bit is left alone on purpose: a
       four-byte pixel has an alpha byte whatever the descriptor says, and plenty of
       writers leave the nibble at zero on a perfectly good BGRA file. */
    let attr_bits = (desc & 0x0f) as usize;
    let rle = img_type & 8 != 0;
    let base = img_type & 7; // 1 colour-mapped, 2 truecolour, 3 greyscale
    /* Round 18bc (B4): the header against the file, before `w * h * 4` bytes are asked
       for. A run-length row is at worst one packet header plus one pixel per 128 pixels,
       so that is the floor for a compressed one and a plain row for the rest. */
    let row = if rle { w.div_ceil(128) * (1 + bytes) } else { w * bytes };
    plausible(w, h, 1, buf.len().saturating_sub(p), row)?;
    let n = w * h;
    let mut px = vec![0u8; n * 4];
    let put = |px: &mut Vec<u8>, i: usize, s: usize| {
        if s + bytes > buf.len() {
            return;
        }
        let o = i * 4;
        match base {
            1 => {
                let ix = if bytes == 1 { buf[s] as i32 } else { u16le(buf, s) as i32 } - cm_first;
                let last = if palette.is_empty() { 0 } else { palette.len() / 4 - 1 } as i32;
                let q = (ix.clamp(0, last) as usize) * 4;
                if q + 3 < palette.len() {
                    px[o..o + 4].copy_from_slice(&palette[q..q + 4]);
                }
            }
            3 => {
                let v = buf[s];
                px[o] = v;
                px[o + 1] = v;
                px[o + 2] = v;
                px[o + 3] = if bytes == 2 && attr_bits > 0 { buf[s + 1] } else { 255 };
            }
            _ => match bpp {
                32 => {
                    px[o] = buf[s + 2];
                    px[o + 1] = buf[s + 1];
                    px[o + 2] = buf[s];
                    px[o + 3] = buf[s + 3];
                }
                24 => {
                    px[o] = buf[s + 2];
                    px[o + 1] = buf[s + 1];
                    px[o + 2] = buf[s];
                    px[o + 3] = 255;
                }
                16 => {
                    let v = u16le(buf, s) as u32;
                    px[o] = (((v >> 10) & 31) * 255 / 31) as u8;
                    px[o + 1] = (((v >> 5) & 31) * 255 / 31) as u8;
                    px[o + 2] = ((v & 31) * 255 / 31) as u8;
                    px[o + 3] = if attr_bits == 0 || v & 0x8000 != 0 { 255 } else { 0 };
                }
                _ => {}
            },
        }
    };

    if !rle {
        for i in 0..n {
            put(&mut px, i, p + i * bytes);
        }
    } else {
        let mut i = 0usize;
        while i < n && p < buf.len() {
            let packet = buf[p];
            p += 1;
            let count = ((packet & 0x7f) as usize) + 1;
            if packet & 0x80 != 0 {
                for _ in 0..count {
                    if i >= n {
                        break;
                    }
                    put(&mut px, i, p);
                    i += 1;
                }
                p += bytes;
            } else {
                for _ in 0..count {
                    if i >= n {
                        break;
                    }
                    put(&mut px, i, p);
                    p += bytes;
                    i += 1;
                }
            }
        }
    }

    // Bit 5 of the descriptor means the rows are already top-down.
    if desc & 0x20 == 0 {
        let row = w * 4;
        for y in 0..h / 2 {
            let (a, b) = (y * row, (h - 1 - y) * row);
            for k in 0..row {
                px.swap(a + k, b + k);
            }
        }
    }
    Ok(rgba_texture(w as u32, h as u32, px))
}

/* ---- the swatch --------------------------------------------------------------------- */

/// A small RGBA copy, nearest-neighbour, at most `THUMB` on its long side.
///
/// Nearest rather than averaged on purpose: these are swatches at 72 pixels and a
/// land texture's character is in its speckle, which a box filter turns to mud.
fn shrink(w: u32, h: u32, px: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    if w == 0 || h == 0 || px.len() < (w as usize) * (h as usize) * 4 {
        return None;
    }
    let long = w.max(h);
    if long <= THUMB {
        return Some((w, h, px.to_vec()));
    }
    let tw = ((w * THUMB) / long).max(1);
    let th = ((h * THUMB) / long).max(1);
    let mut out = vec![0u8; (tw as usize) * (th as usize) * 4];
    for y in 0..th as usize {
        let sy = (y as u32 * h / th) as usize;
        for x in 0..tw as usize {
            let sx = (x as u32 * w / tw) as usize;
            let s = (sy * w as usize + sx) * 4;
            let d = (y * tw as usize + x) * 4;
            out[d..d + 4].copy_from_slice(&px[s..s + 4]);
        }
    }
    Some((tw, th, out))
}
