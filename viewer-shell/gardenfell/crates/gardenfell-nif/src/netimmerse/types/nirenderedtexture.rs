//! `NiRenderedTexture`: a texture the renderer draws into, nothing beyond the base. `tes3`
//! nirenderedtexture.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiRenderedTexture {
    pub base: NiTexture,
}

impl Block for NiRenderedTexture {
    const NAME: &'static str = "NiRenderedTexture";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiRenderedTexture { base: NiTexture::read(r)? })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)
    }

    fn links(&mut self, f: &mut dyn FnMut(&mut i32)) {
        self.base.links(f);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rendered_texture_round_trips() {
        let mut t = NiRenderedTexture::default();
        t.base.base.name = "rt".into();
        let mut w = Writer::new();
        t.write(&mut w).unwrap();
        assert_eq!(NiRenderedTexture::read(&mut Reader::new(&w.out)).unwrap(), t);
    }
}
