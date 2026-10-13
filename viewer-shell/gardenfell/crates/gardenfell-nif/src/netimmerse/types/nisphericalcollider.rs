//! `NiSphericalCollider`: a sphere particles bounce off. `tes3` nisphericalcollider.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiSphericalCollider {
    pub base: NiParticleCollider,
    pub radius: f32,
    pub position: [f32; 3],
}

impl Block for NiSphericalCollider {
    const NAME: &'static str = "NiSphericalCollider";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiSphericalCollider { base: NiParticleCollider::read(r)?, radius: r.f32()?, position: r.vec3()? })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)?;
        w.f32(self.radius);
        w.vec3(&self.position);
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
    fn a_spherical_collider_round_trips() {
        let c = NiSphericalCollider { radius: 50.0, position: [1.0, 2.0, 3.0], ..Default::default() };
        let mut w = Writer::new();
        c.write(&mut w).unwrap();
        assert_eq!(w.out.len(), 12 + 4 + 12);
        assert_eq!(NiSphericalCollider::read(&mut Reader::new(&w.out)).unwrap(), c);
    }
}
