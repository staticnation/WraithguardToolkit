//! `NiFltAnimationNode`: a switch node that steps through its children on a period, the
//! flipbook animation of OpenFlight. `tes3` nifltanimationnode.rs. Its bounce bit lives in
//! the `NiAVObject` flag word (`consts::flt_animation_node_flags::BOUNCE`, bit 6).

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiFltAnimationNode {
    pub base: NiSwitchNode,
    /// Seconds for one pass through the children.
    pub period: f32,
}

impl Block for NiFltAnimationNode {
    const NAME: &'static str = "NiFltAnimationNode";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiFltAnimationNode { base: NiSwitchNode::read(r)?, period: r.f32()? })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)?;
        w.f32(self.period);
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
    fn a_flipbook_node_round_trips() {
        let mut n = NiFltAnimationNode { period: 0.5, ..Default::default() };
        n.base.active_index = 3;
        n.base.base.base.flags = consts::flt_animation_node_flags::BOUNCE;
        let mut w = Writer::new();
        n.write(&mut w).unwrap();
        assert_eq!(NiFltAnimationNode::read(&mut Reader::new(&w.out)).unwrap(), n);
    }
}
