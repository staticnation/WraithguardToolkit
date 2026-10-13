//! `NiParticleRotation`: a modifier that spins each particle about an axis, fixed or
//! random per particle. `tes3` niparticlerotation.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiParticleRotation {
    pub base: NiParticleModifier,
    /// One byte in the file.
    pub random_initial_axis: bool,
    pub initial_axis: [f32; 3],
    pub rotation_speed: f32,
}

impl Block for NiParticleRotation {
    const NAME: &'static str = "NiParticleRotation";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiParticleRotation {
            base: NiParticleModifier::read(r)?,
            random_initial_axis: r.bool_u8()?,
            initial_axis: r.vec3()?,
            rotation_speed: r.f32()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)?;
        w.bool_u8(self.random_initial_axis);
        w.vec3(&self.initial_axis);
        w.f32(self.rotation_speed);
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
    fn a_rotation_round_trips_with_its_one_byte_flag() {
        let m = NiParticleRotation {
            random_initial_axis: true,
            initial_axis: [0.0, 0.0, 1.0],
            rotation_speed: 3.0,
            ..Default::default()
        };
        let mut w = Writer::new();
        m.write(&mut w).unwrap();
        assert_eq!(w.out.len(), 8 + 1 + 12 + 4);
        assert_eq!(NiParticleRotation::read(&mut Reader::new(&w.out)).unwrap(), m);
    }
}
