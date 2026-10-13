//! `RootCollisionNode`: the node whose shapes the game collides with instead of the
//! visible ones, nothing of its own beyond `NiNode`. `tes3` rootcollisionnode.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RootCollisionNode {
    pub base: NiNode,
}

impl Block for RootCollisionNode {
    const NAME: &'static str = "RootCollisionNode";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(RootCollisionNode { base: NiNode::read(r)? })
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
    fn a_root_collision_node_round_trips() {
        let mut n = RootCollisionNode::default();
        n.base.base.base.name = "RootCollisionNode".into();
        n.base.children = vec![Link::new(9)];
        let mut w = Writer::new();
        n.write(&mut w).unwrap();
        assert_eq!(RootCollisionNode::read(&mut Reader::new(&w.out)).unwrap(), n);
    }
}
