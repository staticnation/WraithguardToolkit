//! `NiAutoNormalParticlesData`: particle data for `NiAutoNormalParticles`, nothing of its
//! own. `tes3` niautonormalparticlesdata.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiAutoNormalParticlesData {
    pub base: NiParticlesData,
}

impl Block for NiAutoNormalParticlesData {
    const NAME: &'static str = "NiAutoNormalParticlesData";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiAutoNormalParticlesData { base: NiParticlesData::read(r)? })
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
    fn auto_normal_particle_data_is_its_base_and_round_trips() {
        let mut d = NiAutoNormalParticlesData::default();
        d.base.base.vertices = vec![[1.0, 2.0, 3.0]];
        d.base.num_particles = 1;
        d.base.particle_radius = 0.5;
        let mut w = Writer::new();
        d.write(&mut w).unwrap();
        assert_eq!(NiAutoNormalParticlesData::read(&mut Reader::new(&w.out)).unwrap(), d);
    }
}
