//! `NiRenderer`: the base of a renderer block, nothing of its own. `tes3` nirenderer.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NiRenderer {
    pub base: NiObject,
}

impl Block for NiRenderer {
    const NAME: &'static str = "NiRenderer";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiRenderer { base: NiObject::read(r)? })
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
    fn a_renderer_is_empty() {
        let mut w = Writer::new();
        NiRenderer::default().write(&mut w).unwrap();
        assert!(w.out.is_empty());
        assert_eq!(NiRenderer::read(&mut Reader::new(&w.out)).unwrap(), NiRenderer::default());
    }
}
