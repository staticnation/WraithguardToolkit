//! `NiSpotLight`: a point light narrowed to a cone. `tes3` nispotlight.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiSpotLight {
    pub base: NiPointLight,
    /// Half the cone's opening, in degrees.
    pub outer_spot_angle: f32,
    /// How sharply the light falls off toward the cone's edge.
    pub exponent: f32,
}

impl Block for NiSpotLight {
    const NAME: &'static str = "NiSpotLight";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiSpotLight { base: NiPointLight::read(r)?, outer_spot_angle: r.f32()?, exponent: r.f32()? })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)?;
        w.f32(self.outer_spot_angle);
        w.f32(self.exponent);
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
    fn a_spot_light_round_trips() {
        let mut l = NiSpotLight { outer_spot_angle: 45.0, exponent: 2.0, ..Default::default() };
        l.base.linear_attenuation = 0.1;
        let mut w = Writer::new();
        l.write(&mut w).unwrap();
        assert_eq!(NiSpotLight::read(&mut Reader::new(&w.out)).unwrap(), l);
    }
}
