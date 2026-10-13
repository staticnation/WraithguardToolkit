//! `NiDX8Renderer`: the Direct3D 8 renderer block, nothing beyond the base. `tes3`
//! nidx8renderer.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NiDX8Renderer {
    pub base: NiRenderer,
}

impl Block for NiDX8Renderer {
    const NAME: &'static str = "NiDX8Renderer";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiDX8Renderer { base: NiRenderer::read(r)? })
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
    fn a_dx8_renderer_is_empty() {
        let mut w = Writer::new();
        NiDX8Renderer::default().write(&mut w).unwrap();
        assert!(w.out.is_empty());
        assert_eq!(NiDX8Renderer::read(&mut Reader::new(&w.out)).unwrap(), NiDX8Renderer::default());
    }
}
