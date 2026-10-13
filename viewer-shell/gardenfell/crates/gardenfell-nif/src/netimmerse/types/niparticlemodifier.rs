//! `NiParticleModifier`: the base of the chain of effects a particle system controller
//! applies each frame, every one naming the next and its controller. `tes3`
//! niparticlemodifier.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiParticleModifier {
    pub base: NiObject,
    pub next: Link<NiParticleModifier>,
    pub controller: Link<NiParticleSystemController>,
}

impl Block for NiParticleModifier {
    const NAME: &'static str = "NiParticleModifier";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiParticleModifier { base: NiObject::read(r)?, next: r.link()?, controller: r.link()? })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)?;
        w.link(self.next);
        w.link(self.controller);
        Ok(())
    }

    fn links(&mut self, f: &mut dyn FnMut(&mut i32)) {
        self.base.links(f);
        f(&mut self.next.index);
        f(&mut self.controller.index);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_modifier_is_two_links_and_round_trips() {
        let m = NiParticleModifier { next: Link::new(8), controller: Link::new(3), ..Default::default() };
        let mut w = Writer::new();
        m.write(&mut w).unwrap();
        assert_eq!(w.out.len(), 8);
        assert_eq!(NiParticleModifier::read(&mut Reader::new(&w.out)).unwrap(), m);
    }
}
