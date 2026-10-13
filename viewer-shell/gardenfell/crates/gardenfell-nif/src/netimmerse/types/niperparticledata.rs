//! `NiPerParticleData`: one live particle's state as `NiParticleSystemController` saves it,
//! 40 bytes. `tes3` niperparticledata.rs.

use super::*;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NiPerParticleData {
    pub velocity: [f32; 3],
    pub rotation_axis: [f32; 3],
    pub age: f32,
    pub lifespan: f32,
    pub last_update: f32,
    pub generation: u16,
    /// The vertex in the particle data this particle draws as.
    pub index: u16,
}

impl NiPerParticleData {
    pub const SIZE: usize = 12 + 12 + 4 + 4 + 4 + 2 + 2;

    pub fn read(r: &mut Reader<'_>) -> Result<NiPerParticleData> {
        Ok(NiPerParticleData {
            velocity: r.vec3()?,
            rotation_axis: r.vec3()?,
            age: r.f32()?,
            lifespan: r.f32()?,
            last_update: r.f32()?,
            generation: r.u16()?,
            index: r.u16()?,
        })
    }

    pub fn write(&self, w: &mut Writer) {
        w.vec3(&self.velocity);
        w.vec3(&self.rotation_axis);
        w.f32(self.age);
        w.f32(self.lifespan);
        w.f32(self.last_update);
        w.u16(self.generation);
        w.u16(self.index);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_particle_is_forty_bytes_and_round_trips() {
        let p = NiPerParticleData {
            velocity: [0.0, 0.0, 1.0],
            rotation_axis: [1.0, 0.0, 0.0],
            age: 0.5,
            lifespan: 2.0,
            last_update: 0.5,
            generation: 1,
            index: 7,
        };
        let mut w = Writer::new();
        p.write(&mut w);
        assert_eq!(w.out.len(), NiPerParticleData::SIZE);
        assert_eq!(NiPerParticleData::SIZE, 40);
        assert_eq!(NiPerParticleData::read(&mut Reader::new(&w.out)).unwrap(), p);
    }
}
