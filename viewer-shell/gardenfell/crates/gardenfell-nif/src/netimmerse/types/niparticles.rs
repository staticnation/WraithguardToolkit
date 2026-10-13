//! `NiParticles`: a particle system drawn as points or sprites, nothing of its own; the
//! particles are the `NiParticlesData` it links to and the motion is its controller's.
//! `tes3` niparticles.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiParticles {
    pub base: NiGeometry,
}

impl Block for NiParticles {
    const NAME: &'static str = "NiParticles";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiParticles { base: NiGeometry::read(r)? })
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
    fn particles_are_their_geometry_and_round_trip() {
        let mut p = NiParticles::default();
        p.base.base.base.name = "Particles".into();
        p.base.geometry_data = Link::new(1);
        let mut w = Writer::new();
        p.write(&mut w).unwrap();
        assert_eq!(NiParticles::read(&mut Reader::new(&w.out)).unwrap(), p);
    }
}
