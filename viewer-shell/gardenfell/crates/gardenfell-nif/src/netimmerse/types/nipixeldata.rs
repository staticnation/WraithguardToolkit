//! `NiPixelData`: a texture's pixels held in the stream, with its format, palette and
//! mipmap table. `tes3` nipixeldata.rs.
//!
//! The file holds the `u32` mipmap count before the pixel stride, then the mipmap table,
//! then a `u32` byte count and the bytes.

use super::*;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NiPixelData {
    pub base: NiObject,
    pub pixel_format: NiPixelFormat,
    pub palette: Link<NiPalette>,
    /// Bytes per pixel.
    pub pixel_stride: u32,
    /// One `[width, height, offset]` per mipmap level, the offset into `pixel_data`.
    pub mipmaps: Vec<[u32; 3]>,
    pub pixel_data: Vec<u8>,
}

impl Block for NiPixelData {
    const NAME: &'static str = "NiPixelData";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let base = NiObject::read(r)?;
        let pixel_format = NiPixelFormat::read(r)?;
        let palette = r.link()?;
        let num_mipmap_levels = r.count(12)?;
        let pixel_stride = r.u32()?;
        let mipmaps = r.seq(num_mipmap_levels, |r| r.u32s::<3>())?;
        let num_pixel_data = r.count(1)?;
        let pixel_data = r.bytes(num_pixel_data)?.to_vec();
        Ok(NiPixelData { base, pixel_format, palette, pixel_stride, mipmaps, pixel_data })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)?;
        self.pixel_format.write(w);
        w.link(self.palette);
        w.count(self.mipmaps.len());
        w.u32(self.pixel_stride);
        for m in &self.mipmaps {
            w.u32s(m);
        }
        w.count(self.pixel_data.len());
        w.bytes(&self.pixel_data);
        Ok(())
    }

    fn links(&mut self, f: &mut dyn FnMut(&mut i32)) {
        self.base.links(f);
        f(&mut self.palette.index);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixel_data_round_trips_and_a_bad_byte_count_is_refused() {
        let p = NiPixelData {
            pixel_format: NiPixelFormat { pixel_format: 0, bits_per_pixel: 24, ..Default::default() },
            palette: Link::NONE,
            pixel_stride: 3,
            mipmaps: vec![[2, 2, 0], [1, 1, 12]],
            pixel_data: (0..15).collect(),
            ..Default::default()
        };
        let mut w = Writer::new();
        p.write(&mut w).unwrap();
        assert_eq!(w.out.len(), NiPixelFormat::SIZE + 4 + 4 + 4 + 2 * 12 + 4 + 15);
        assert_eq!(NiPixelData::read(&mut Reader::new(&w.out)).unwrap(), p);
        // A byte count past the end is refused before anything is copied.
        let mut bad = w.out.clone();
        let at = NiPixelFormat::SIZE + 4 + 4 + 4 + 2 * 12;
        bad[at..at + 4].copy_from_slice(&1_000_000u32.to_le_bytes());
        let e = NiPixelData::read(&mut Reader::new(&bad)).unwrap_err();
        assert!(e.what.contains("1000000 items"), "{e}");
    }
}
