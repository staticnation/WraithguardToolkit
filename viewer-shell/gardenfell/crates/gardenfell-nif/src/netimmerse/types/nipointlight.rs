//! `NiPointLight`: a light at the node's position that falls off with distance. `tes3`
//! nipointlight.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiPointLight {
    pub base: NiLight,
    pub constant_attenuation: f32,
    pub linear_attenuation: f32,
    pub quadratic_attenuation: f32,
}

impl Block for NiPointLight {
    const NAME: &'static str = "NiPointLight";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiPointLight {
            base: NiLight::read(r)?,
            constant_attenuation: r.f32()?,
            linear_attenuation: r.f32()?,
            quadratic_attenuation: r.f32()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)?;
        w.f32(self.constant_attenuation);
        w.f32(self.linear_attenuation);
        w.f32(self.quadratic_attenuation);
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
    fn a_point_light_round_trips() {
        let l = NiPointLight {
            constant_attenuation: 1.0,
            linear_attenuation: 0.05,
            quadratic_attenuation: 0.001,
            ..Default::default()
        };
        let mut w = Writer::new();
        l.write(&mut w).unwrap();
        assert_eq!(NiPointLight::read(&mut Reader::new(&w.out)).unwrap(), l);
    }
}
