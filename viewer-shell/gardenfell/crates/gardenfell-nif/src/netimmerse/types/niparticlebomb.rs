//! `NiParticleBomb`: a modifier that pushes particles away from a point for a while.
//! `tes3` niparticlebomb.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiParticleBomb {
    pub base: NiParticleModifier,
    pub decay: f32,
    pub duration: f32,
    pub delta_v: f32,
    pub start_time: f32,
    /// `consts::decay_type::*`
    pub decay_type: i32,
    /// `consts::symmetry_type::*`
    pub symmetry_type: i32,
    pub position: [f32; 3],
    pub direction: [f32; 3],
}

impl Block for NiParticleBomb {
    const NAME: &'static str = "NiParticleBomb";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiParticleBomb {
            base: NiParticleModifier::read(r)?,
            decay: r.f32()?,
            duration: r.f32()?,
            delta_v: r.f32()?,
            start_time: r.f32()?,
            decay_type: r.i32()?,
            symmetry_type: r.i32()?,
            position: r.vec3()?,
            direction: r.vec3()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)?;
        w.f32(self.decay);
        w.f32(self.duration);
        w.f32(self.delta_v);
        w.f32(self.start_time);
        w.i32(self.decay_type);
        w.i32(self.symmetry_type);
        w.vec3(&self.position);
        w.vec3(&self.direction);
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
    fn a_bomb_round_trips_and_keeps_an_unknown_type_word() {
        let b = NiParticleBomb {
            decay: 0.5,
            duration: 2.0,
            delta_v: 100.0,
            decay_type: consts::decay_type::LINEAR,
            symmetry_type: 9,
            direction: [0.0, 0.0, 1.0],
            ..Default::default()
        };
        let mut w = Writer::new();
        b.write(&mut w).unwrap();
        assert_eq!(w.out.len(), 8 + 4 * 4 + 4 + 4 + 12 + 12);
        assert_eq!(NiParticleBomb::read(&mut Reader::new(&w.out)).unwrap(), b);
    }
}
