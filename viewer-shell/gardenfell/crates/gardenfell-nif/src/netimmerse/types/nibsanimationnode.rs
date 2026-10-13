//! `NiBSAnimationNode`: a Bethesda node whose controllers the game starts on load. Nothing
//! of its own beyond `NiNode`; its two flag bits live in the `NiAVObject` flag word
//! (`consts::bs_animation_node_flags::*`: bit 5 animated, bit 6 not random). `tes3`
//! nibsanimationnode.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiBSAnimationNode {
    pub base: NiNode,
}

impl Block for NiBSAnimationNode {
    const NAME: &'static str = "NiBSAnimationNode";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiBSAnimationNode { base: NiNode::read(r)? })
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
    fn an_animation_node_round_trips_with_its_flag_bits() {
        use consts::bs_animation_node_flags::{ANIMATED, NOT_RANDOM};
        let mut n = NiBSAnimationNode::default();
        n.base.base.flags = ANIMATED | NOT_RANDOM;
        let mut w = Writer::new();
        n.write(&mut w).unwrap();
        let back = NiBSAnimationNode::read(&mut Reader::new(&w.out)).unwrap();
        assert_eq!(back, n);
        assert_eq!(back.base.base.flags, 0x0060);
    }
}
