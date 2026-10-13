//! `NiCollisionSwitch`: a node that turns collision on or off for what is under it.
//! Nothing of its own beyond `NiNode`; its propagate bit lives in the `NiAVObject` flag
//! word (`consts::collision_switch_flags::PROPAGATE`, bit 5). `tes3` nicollisionswitch.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiCollisionSwitch {
    pub base: NiNode,
}

impl Block for NiCollisionSwitch {
    const NAME: &'static str = "NiCollisionSwitch";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiCollisionSwitch { base: NiNode::read(r)? })
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
    fn a_collision_switch_round_trips() {
        let mut n = NiCollisionSwitch::default();
        n.base.base.flags = consts::collision_switch_flags::PROPAGATE;
        let mut w = Writer::new();
        n.write(&mut w).unwrap();
        assert_eq!(NiCollisionSwitch::read(&mut Reader::new(&w.out)).unwrap(), n);
    }
}
