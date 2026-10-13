//! `NiLight`: the base of every light, its dimmer and three colors. `tes3` nilight.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiLight {
    pub base: NiDynamicEffect,
    pub dimmer: f32,
    pub ambient_color: [f32; 3],
    pub diffuse_color: [f32; 3],
    pub specular_color: [f32; 3],
}

impl Block for NiLight {
    const NAME: &'static str = "NiLight";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiLight {
            base: NiDynamicEffect::read(r)?,
            dimmer: r.f32()?,
            ambient_color: r.vec3()?,
            diffuse_color: r.vec3()?,
            specular_color: r.vec3()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)?;
        w.f32(self.dimmer);
        w.vec3(&self.ambient_color);
        w.vec3(&self.diffuse_color);
        w.vec3(&self.specular_color);
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
    fn a_light_round_trips() {
        let l = NiLight {
            dimmer: 1.0,
            ambient_color: [0.1, 0.1, 0.1],
            diffuse_color: [1.0, 0.9, 0.8],
            specular_color: [0.0; 3],
            ..Default::default()
        };
        let mut w = Writer::new();
        l.write(&mut w).unwrap();
        assert_eq!(NiLight::read(&mut Reader::new(&w.out)).unwrap(), l);
    }
}
