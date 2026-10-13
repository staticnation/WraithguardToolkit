//! `NiGravity`: a modifier that pulls particles along a direction or toward a point.
//! `tes3` nigravity.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiGravity {
    pub base: NiParticleModifier,
    pub decay: f32,
    pub strength: f32,
    /// `consts::force_type::*`: planar pulls along `direction`, spherical toward `position`.
    pub force_type: i32,
    pub position: [f32; 3],
    pub direction: [f32; 3],
}

impl Block for NiGravity {
    const NAME: &'static str = "NiGravity";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiGravity {
            base: NiParticleModifier::read(r)?,
            decay: r.f32()?,
            strength: r.f32()?,
            force_type: r.i32()?,
            position: r.vec3()?,
            direction: r.vec3()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)?;
        w.f32(self.decay);
        w.f32(self.strength);
        w.i32(self.force_type);
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
    fn gravity_round_trips() {
        let g = NiGravity {
            strength: -9.8,
            force_type: consts::force_type::PLANAR,
            direction: [0.0, 0.0, -1.0],
            ..Default::default()
        };
        let mut w = Writer::new();
        g.write(&mut w).unwrap();
        assert_eq!(w.out.len(), 8 + 4 + 4 + 4 + 12 + 12);
        assert_eq!(NiGravity::read(&mut Reader::new(&w.out)).unwrap(), g);
    }
}
