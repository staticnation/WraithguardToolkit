//! `NiPixelFormat`: how the bytes of an `NiPixelData` make a pixel, a shared structure
//! and not a block. `tes3` nipixelformat.rs.

use super::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NiPixelFormat {
    /// `consts::pixel_format::*`
    pub pixel_format: i32,
    /// The bit masks of red, green, blue and alpha within a pixel.
    pub color_masks: [u32; 4],
    pub bits_per_pixel: u32,
    pub compare_bits: [u8; 8],
}

impl NiPixelFormat {
    pub const SIZE: usize = 4 + 16 + 4 + 8;

    pub fn read(r: &mut Reader<'_>) -> Result<NiPixelFormat> {
        Ok(NiPixelFormat {
            pixel_format: r.i32()?,
            color_masks: r.u32s::<4>()?,
            bits_per_pixel: r.u32()?,
            compare_bits: r.array::<8>()?,
        })
    }

    pub fn write(&self, w: &mut Writer) {
        w.i32(self.pixel_format);
        w.u32s(&self.color_masks);
        w.u32(self.bits_per_pixel);
        w.bytes(&self.compare_bits);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pixel_format_round_trips() {
        let f = NiPixelFormat {
            pixel_format: 1,
            color_masks: [0xFF, 0xFF00, 0xFF_0000, 0xFF00_0000],
            bits_per_pixel: 32,
            compare_bits: [8, 8, 8, 8, 0, 0, 0, 0],
        };
        let mut w = Writer::new();
        f.write(&mut w);
        assert_eq!(w.out.len(), NiPixelFormat::SIZE);
        assert_eq!(NiPixelFormat::read(&mut Reader::new(&w.out)).unwrap(), f);
    }
}
