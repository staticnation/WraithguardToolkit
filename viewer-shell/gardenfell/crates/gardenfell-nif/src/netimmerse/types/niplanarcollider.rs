//! `NiPlanarCollider`: a rectangle particles bounce off, given by its position, two axes
//! and size, with the plane it lies in. `tes3` niplanarcollider.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiPlanarCollider {
    pub base: NiParticleCollider,
    pub height: f32,
    pub width: f32,
    pub position: [f32; 3],
    pub x_axis: [f32; 3],
    pub y_axis: [f32; 3],
    pub normal: [f32; 3],
    /// The plane's distance term, `normal . position`.
    pub constant: f32,
}

impl Block for NiPlanarCollider {
    const NAME: &'static str = "NiPlanarCollider";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiPlanarCollider {
            base: NiParticleCollider::read(r)?,
            height: r.f32()?,
            width: r.f32()?,
            position: r.vec3()?,
            x_axis: r.vec3()?,
            y_axis: r.vec3()?,
            normal: r.vec3()?,
            constant: r.f32()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)?;
        w.f32(self.height);
        w.f32(self.width);
        w.vec3(&self.position);
        w.vec3(&self.x_axis);
        w.vec3(&self.y_axis);
        w.vec3(&self.normal);
        w.f32(self.constant);
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
    fn a_planar_collider_round_trips() {
        let c = NiPlanarCollider {
            height: 2.0,
            width: 4.0,
            x_axis: [1.0, 0.0, 0.0],
            y_axis: [0.0, 1.0, 0.0],
            normal: [0.0, 0.0, 1.0],
            constant: 0.0,
            ..Default::default()
        };
        let mut w = Writer::new();
        c.write(&mut w).unwrap();
        assert_eq!(w.out.len(), 12 + 4 + 4 + 4 * 12 + 4);
        assert_eq!(NiPlanarCollider::read(&mut Reader::new(&w.out)).unwrap(), c);
    }
}
