//! `NiRenderedCubeMap`: a rendered texture with six faces, nothing beyond the base. `tes3`
//! nirenderedcubemap.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiRenderedCubeMap {
    pub base: NiRenderedTexture,
}

impl Block for NiRenderedCubeMap {
    const NAME: &'static str = "NiRenderedCubeMap";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiRenderedCubeMap { base: NiRenderedTexture::read(r)? })
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
    fn a_rendered_cube_map_round_trips() {
        let mut t = NiRenderedCubeMap::default();
        t.base.base.base.name = "cube".into();
        let mut w = Writer::new();
        t.write(&mut w).unwrap();
        assert_eq!(w.out.len(), 4 + 4 + 4 + 4);
        assert_eq!(NiRenderedCubeMap::read(&mut Reader::new(&w.out)).unwrap(), t);
    }
}
