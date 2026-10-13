//! `NiBSPNode`: a node that splits its children by a plane. `tes3` nibspnode.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiBSPNode {
    pub base: NiNode,
    /// The normal and the constant of an `NiPlane`.
    pub plane: [f32; 4],
}

impl Block for NiBSPNode {
    const NAME: &'static str = "NiBSPNode";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiBSPNode { base: NiNode::read(r)?, plane: r.vec4()? })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)?;
        w.vec4(&self.plane);
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
    fn a_bsp_node_round_trips() {
        let n = NiBSPNode { plane: [0.0, 0.0, 1.0, -5.0], ..Default::default() };
        let mut w = Writer::new();
        n.write(&mut w).unwrap();
        assert_eq!(NiBSPNode::read(&mut Reader::new(&w.out)).unwrap(), n);
        let e = NiBSPNode::read(&mut Reader::new(&w.out[..w.out.len() - 2])).unwrap_err();
        assert!(e.what.contains("4 bytes wanted"), "{e}");
    }
}
