//! `NiSwitchNode`: a node that shows one of its children at a time. `tes3` niswitchnode.rs.
//! Its update-only-active bit lives in the `NiAVObject` flag word
//! (`consts::switch_node_flags::UPDATE_ONLY_ACTIVE`, bit 5).

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiSwitchNode {
    pub base: NiNode,
    /// Which child is shown, an index into `base.children`. `tes3` holds it as a `usize`;
    /// the file's `u32` is kept here.
    pub active_index: u32,
}

impl Block for NiSwitchNode {
    const NAME: &'static str = "NiSwitchNode";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiSwitchNode { base: NiNode::read(r)?, active_index: r.u32()? })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)?;
        w.u32(self.active_index);
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
    fn a_switch_node_round_trips() {
        let mut n = NiSwitchNode { active_index: 1, ..Default::default() };
        n.base.children = vec![Link::new(1), Link::new(2)];
        n.base.base.flags = consts::switch_node_flags::UPDATE_ONLY_ACTIVE;
        let mut w = Writer::new();
        n.write(&mut w).unwrap();
        assert_eq!(NiSwitchNode::read(&mut Reader::new(&w.out)).unwrap(), n);
        let e = NiSwitchNode::read(&mut Reader::new(&w.out[..w.out.len() - 4])).unwrap_err();
        assert!(e.what.contains("4 bytes wanted"), "{e}");
    }
}
