//! `NiPalette`: the color table of a palettized `NiPixelData`. `tes3` nipalette.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NiPalette {
    pub base: NiObject,
    /// One byte in the file.
    pub has_alpha: bool,
    /// The entries, each `[r, g, b, a]`.
    pub palettes: Vec<[u8; 4]>,
}

impl Block for NiPalette {
    const NAME: &'static str = "NiPalette";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let base = NiObject::read(r)?;
        let has_alpha = r.bool_u8()?;
        let palettes = r.counted(4, |r| r.array::<4>())?;
        Ok(NiPalette { base, has_alpha, palettes })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)?;
        w.bool_u8(self.has_alpha);
        w.count(self.palettes.len());
        for p in &self.palettes {
            w.bytes(p);
        }
        Ok(())
    }

    fn links(&mut self, f: &mut dyn FnMut(&mut i32)) {
        self.base.links(f);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_palette_round_trips_and_a_bad_count_is_refused() {
        let p = NiPalette {
            has_alpha: true,
            palettes: vec![[0, 0, 0, 255], [255, 255, 255, 255]],
            ..Default::default()
        };
        let mut w = Writer::new();
        p.write(&mut w).unwrap();
        assert_eq!(w.out.len(), 1 + 4 + 8);
        assert_eq!(NiPalette::read(&mut Reader::new(&w.out)).unwrap(), p);
        let bad = [1u8, 0xFF, 0xFF, 0xFF, 0x7F, 1, 2, 3, 4];
        let e = NiPalette::read(&mut Reader::new(&bad)).unwrap_err();
        assert!(e.what.contains("items of 4 bytes"), "{e}");
    }
}
