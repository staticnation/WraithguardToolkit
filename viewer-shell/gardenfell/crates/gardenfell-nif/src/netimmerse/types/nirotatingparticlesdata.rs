//! `NiRotatingParticlesData`: particle data with an optional rotation per particle.
//! `tes3` nirotatingparticlesdata.rs.
//!
//! A has-rotations word, then `vertices.len()` quaternions when it is set. `tes3` and the
//! engine's own reader (`nif.rs` `read_particle_data`) agree.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiRotatingParticlesData {
    pub base: NiParticlesData,
    /// Empty, or one `[w, x, y, z]` per vertex.
    pub rotations: Vec<[f32; 4]>,
}

impl Block for NiRotatingParticlesData {
    const NAME: &'static str = "NiRotatingParticlesData";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let base = NiParticlesData::read(r)?;
        let has_rotations = r.bool_u32()?;
        let n = if has_rotations { base.base.vertices.len() } else { 0 };
        let rotations = r.seq(n, |r| r.quat())?;
        Ok(NiRotatingParticlesData { base, rotations })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)?;
        w.bool_u32(!self.rotations.is_empty());
        for q in &self.rotations {
            w.quat(q);
        }
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
    fn rotating_particle_data_round_trips_and_is_cut_off_with_the_byte() {
        let mut d = NiRotatingParticlesData::default();
        d.base.base.vertices = vec![[0.0; 3]; 2];
        d.base.num_particles = 2;
        d.rotations = vec![[1.0, 0.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0]];
        let mut w = Writer::new();
        d.write(&mut w).unwrap();
        assert_eq!(NiRotatingParticlesData::read(&mut Reader::new(&w.out)).unwrap(), d);
        let e = NiRotatingParticlesData::read(&mut Reader::new(&w.out[..w.out.len() - 8])).unwrap_err();
        assert!(e.what.contains("4 bytes wanted"), "{e}");
    }
}
