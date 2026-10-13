//! Wraithguard's additions to Gardenfell's mesh reading (`crate::nif` re-exports them):
//! a texture held inside a NIF named after its mesh and decoded on request, and one call
//! for what the viewer draws and the collision it reports.

use crate::nif::{parse_with_kf, read_for_draw, KfSequence, MeshGeom, MeshRead, EMBEDDED};
use tes3::nif::NiStream;

/// `(record index, mesh path)` from an embedded texture's name, or None for a file name.
pub fn embedded_name(name: &str) -> Option<(usize, &str)> {
    let rest = name.strip_prefix(EMBEDDED)?;
    let (ix, owner) = rest.split_once('@')?;
    Some((ix.parse().ok()?, owner))
}

/// The pixels of the texture record `index` holds inside a NIF (`NiPixelData`), as RGBA:
/// `(width, height, rgba)` of its first level. RGB and RGBA by their channel masks,
/// paletted (with the file's `NiPalette`), and the DXT1/3/5 blocks; None for anything
/// else or a record that is not pixel data.
pub fn embedded_texture(buf: &[u8], index: usize) -> Option<(u32, u32, Vec<u8>)> {
    use tes3::nif::{NiStream, NiType, PixelFormat};
    if buf.len() < 48 || !buf.starts_with(b"NetImmerse File Format") {
        return None;
    }
    let bytes = as_4002(buf);
    let stream = std::panic::catch_unwind(|| NiStream::from_bytes(&bytes).ok()).ok().flatten()?;
    let NiType::NiPixelData(px) = stream.objects.values().nth(index)? else { return None };
    let [w, h, off] = *px.mipmaps.first()?;
    let (w32, h32) = (w, h);
    let (w, h, off) = (w as usize, h as usize, off as usize);
    if w == 0 || h == 0 || w > 8192 || h > 8192 {
        return None;
    }
    let data = px.pixel_data.get(off..)?;
    let mut out = vec![0u8; w * h * 4];
    let fmt = &px.pixel_format;
    match fmt.pixel_format {
        PixelFormat::RGB | PixelFormat::RGBA => {
            let bytes = (fmt.bits_per_pixel as usize).div_ceil(8).max(1);
            if bytes > 4 || data.len() < w * h * bytes {
                return None;
            }
            let chan = |v: u32, mask: u32, absent: u8| -> u8 {
                if mask == 0 {
                    return absent;
                }
                let shift = mask.trailing_zeros();
                let max = mask >> shift;
                (((v & mask) >> shift) as u64 * 255 / max as u64) as u8
            };
            let [rm, gm, bm, am] = fmt.color_masks;
            for i in 0..w * h {
                let mut v = 0u32;
                for k in 0..bytes {
                    v |= (data[i * bytes + k] as u32) << (8 * k);
                }
                out[i * 4] = chan(v, rm, 0);
                out[i * 4 + 1] = chan(v, gm, 0);
                out[i * 4 + 2] = chan(v, bm, 0);
                out[i * 4 + 3] = chan(v, am, 255);
            }
        }
        PixelFormat::PAL | PixelFormat::PALAlpha => {
            let pal = stream.get(px.palette)?;
            if data.len() < w * h {
                return None;
            }
            let alpha = matches!(fmt.pixel_format, PixelFormat::PALAlpha);
            for i in 0..w * h {
                let c = pal.palettes.get(data[i] as usize).copied().unwrap_or([0, 0, 0, 255]);
                out[i * 4..i * 4 + 4].copy_from_slice(&[c[0], c[1], c[2], if alpha { c[3] } else { 255 }]);
            }
        }
        PixelFormat::Compress1 | PixelFormat::Compress3 | PixelFormat::Compress5 => {
            let kind = match fmt.pixel_format {
                PixelFormat::Compress1 => crate::img::Kind::Bc1,
                PixelFormat::Compress3 => crate::img::Kind::Bc2,
                _ => crate::img::Kind::Bc3,
            };
            out = crate::img::decode_blocks(data, w32, h32, kind);
            if out.len() != w * h * 4 {
                return None;
            }
        }
        _ => return None,
    }
    Some((w32, h32, out))
}

/// Names every embedded texture a mesh's parts and particles use after the mesh they are
/// in (`__nifpx/12` -> `__nifpx/12@meshes\x\chair.nif`).
pub fn own_embedded(read: &mut MeshRead, owner: &str) {
    let fix = |t: &mut String| {
        if t.starts_with(EMBEDDED) && !t.contains('@') {
            t.push('@');
            t.push_str(owner);
        }
    };
    for p in &mut read.parts {
        for t in [
            &mut p.tex,
            &mut p.detail_tex,
            &mut p.glow_tex,
            &mut p.gloss_tex,
            &mut p.dark_tex,
            &mut p.decal_tex,
            &mut p.env_tex,
            &mut p.bump_tex,
        ] {
            fix(t);
        }
    }
    for s in &mut read.systems {
        fix(&mut s.tex);
    }
}

/// Everything the viewer wants from one mesh file: what `read_for_draw` gives, and the
/// collision geometry `parse_with_kf` builds (two reads of the file; the world's own
/// geometry cache answers for placed meshes).
pub fn read_for_draw_and_geom(buf: &[u8], kf: Option<&KfSequence>) -> Option<(MeshRead, Option<MeshGeom>)> {
    let read = read_for_draw(buf, kf)?;
    Some((read, parse_with_kf(buf, kf)))
}

/// `buf` as the crate reads it: Morrowind's handful of 4.0.0.0 files carry the same
/// layout under an older header, so the header is rewritten to 4.0.0.2 first.
fn as_4002(bytes: &[u8]) -> std::borrow::Cow<'_, [u8]> {
    const V4000: &[u8; 40] = b"NetImmerse File Format, Version 4.0.0.0\n";
    let text_ok = bytes.len() >= 44 && (&bytes[..40] == V4000 || bytes[..40] == NiStream::HEADER);
    if text_ok && bytes[40..44] == 0x0400_0000u32.to_le_bytes() {
        let mut b = bytes.to_vec();
        b[..40].copy_from_slice(&NiStream::HEADER);
        b[40..44].copy_from_slice(&NiStream::VERSION.to_le_bytes());
        return std::borrow::Cow::Owned(b);
    }
    std::borrow::Cow::Borrowed(bytes)
}

#[cfg(test)]
mod embedded_tests {
    use super::*;
    use crate::nif::DrawPart;
    use tes3::nif::{NiPixelData, NiPixelFormat, NiStream, PixelFormat};

    #[test]
    fn a_texture_inside_the_file_is_read() {
        let mut s = NiStream::new();
        let px = NiPixelData {
            pixel_format: NiPixelFormat {
                pixel_format: PixelFormat::RGB,
                color_masks: [0xff, 0xff00, 0xff0000, 0],
                bits_per_pixel: 24,
                ..Default::default()
            },
            pixel_stride: 3,
            mipmaps: vec![[2, 1, 0]],
            pixel_data: vec![10, 20, 30, 200, 100, 50],
            ..Default::default()
        };
        // A root, so it is written: the crate saves only what the roots reach.
        let link = s.insert(px);
        s.roots.push(link.cast());
        let bytes = s.save_bytes().unwrap();
        let (w, h, rgba) = embedded_texture(&bytes, 0).unwrap();
        assert_eq!((w, h), (2, 1));
        assert_eq!(rgba, vec![10, 20, 30, 255, 200, 100, 50, 255]);
        assert!(embedded_texture(&bytes, 5).is_none());
    }

    #[test]
    fn embedded_names_carry_their_mesh() {
        let mut r = MeshRead { parts: vec![DrawPart { tex: format!("{EMBEDDED}7"), ..Default::default() }], attach: None, systems: vec![], light: None };
        own_embedded(&mut r, "meshes\\q\\chair.nif");
        assert_eq!(r.parts[0].tex, "__nifpx/7@meshes\\q\\chair.nif");
        assert_eq!(embedded_name(&r.parts[0].tex), Some((7, "meshes\\q\\chair.nif")));
        assert_eq!(embedded_name("tx_wood.dds"), None);
    }
}
