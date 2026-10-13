//! `BSMirroredNode`: a Bethesda node whose children are drawn mirrored, nothing of its own
//! beyond `NiNode`. `tes3` bsmirrorednode.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct BSMirroredNode {
    pub base: NiNode,
}

impl Block for BSMirroredNode {
    const NAME: &'static str = "BSMirroredNode";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(BSMirroredNode { base: NiNode::read(r)? })
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
    fn a_mirrored_node_round_trips() {
        let mut n = BSMirroredNode::default();
        n.base.children = vec![Link::new(2)];
        let mut w = Writer::new();
        n.write(&mut w).unwrap();
        assert_eq!(BSMirroredNode::read(&mut Reader::new(&w.out)).unwrap(), n);
    }
}
