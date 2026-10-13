//! `AvoidNode`: a node the engine's path finding steers around, nothing of its own beyond
//! `NiNode`. `tes3` avoidnode.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct AvoidNode {
    pub base: NiNode,
}

impl Block for AvoidNode {
    const NAME: &'static str = "AvoidNode";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(AvoidNode { base: NiNode::read(r)? })
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
    fn an_avoid_node_round_trips() {
        let mut n = AvoidNode::default();
        n.base.base.base.name = "avoid".into();
        let mut w = Writer::new();
        n.write(&mut w).unwrap();
        assert_eq!(AvoidNode::read(&mut Reader::new(&w.out)).unwrap(), n);
    }
}
