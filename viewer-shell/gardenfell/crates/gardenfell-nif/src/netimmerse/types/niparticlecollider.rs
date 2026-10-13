//! `NiParticleCollider`: the base of the shapes particles bounce off, with how much of
//! their speed they keep. `tes3` niparticlecollider.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiParticleCollider {
    pub base: NiParticleModifier,
    pub bounce: f32,
}

impl Block for NiParticleCollider {
    const NAME: &'static str = "NiParticleCollider";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiParticleCollider { base: NiParticleModifier::read(r)?, bounce: r.f32()? })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)?;
        w.f32(self.bounce);
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
    fn a_collider_round_trips() {
        let c = NiParticleCollider { bounce: 0.75, ..Default::default() };
        let mut w = Writer::new();
        c.write(&mut w).unwrap();
        assert_eq!(w.out.len(), 12);
        assert_eq!(NiParticleCollider::read(&mut Reader::new(&w.out)).unwrap(), c);
    }
}
