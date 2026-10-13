//! `NiBillboardNode`: a node that turns to face the camera, nothing of its own beyond
//! `NiNode`. `tes3` nibillboardnode.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiBillboardNode {
    pub base: NiNode,
}

impl Block for NiBillboardNode {
    const NAME: &'static str = "NiBillboardNode";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiBillboardNode { base: NiNode::read(r)? })
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
    fn a_billboard_node_round_trips() {
        let mut n = NiBillboardNode::default();
        n.base.base.scale = 2.0;
        let mut w = Writer::new();
        n.write(&mut w).unwrap();
        assert_eq!(NiBillboardNode::read(&mut Reader::new(&w.out)).unwrap(), n);
    }
}
