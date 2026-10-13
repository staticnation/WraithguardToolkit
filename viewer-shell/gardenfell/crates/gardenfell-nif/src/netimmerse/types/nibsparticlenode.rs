//! `NiBSParticleNode`: a Bethesda node that holds a particle system. Nothing of its own
//! beyond `NiNode`; its follow bit lives in the `NiAVObject` flag word
//! (`consts::bs_particle_node_flags::FOLLOW`, bit 7). `tes3` nibsparticlenode.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiBSParticleNode {
    pub base: NiNode,
}

impl Block for NiBSParticleNode {
    const NAME: &'static str = "NiBSParticleNode";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiBSParticleNode { base: NiNode::read(r)? })
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
    fn a_particle_node_round_trips() {
        let mut n = NiBSParticleNode::default();
        n.base.base.flags = consts::bs_particle_node_flags::FOLLOW;
        let mut w = Writer::new();
        n.write(&mut w).unwrap();
        assert_eq!(NiBSParticleNode::read(&mut Reader::new(&w.out)).unwrap(), n);
    }
}
