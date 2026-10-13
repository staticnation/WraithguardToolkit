//! `NiBltSource`: an image for 2D blitting, a texture source without the texture. `tes3`
//! nibltsource.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NiBltSource {
    pub base: NiObject,
    pub source: TextureSource,
}

impl Block for NiBltSource {
    const NAME: &'static str = "NiBltSource";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiBltSource { base: NiObject::read(r)?, source: TextureSource::read(r)? })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)?;
        self.source.write(w)
    }

    fn links(&mut self, f: &mut dyn FnMut(&mut i32)) {
        self.base.links(f);
        self.source.links(f);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_blt_source_round_trips() {
        let b = NiBltSource { source: TextureSource::External("menu.dds".into()), ..Default::default() };
        let mut w = Writer::new();
        b.write(&mut w).unwrap();
        assert_eq!(w.out.len(), 1 + 4 + 8);
        assert_eq!(NiBltSource::read(&mut Reader::new(&w.out)).unwrap(), b);
    }
}
