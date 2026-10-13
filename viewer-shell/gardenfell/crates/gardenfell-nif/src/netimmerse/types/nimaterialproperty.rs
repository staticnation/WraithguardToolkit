//! `NiMaterialProperty`: the lighting material, four colors, a shininess and an alpha.
//! `tes3` nimaterialproperty.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiMaterialProperty {
    pub base: NiProperty,
    pub ambient_color: [f32; 3],
    pub diffuse_color: [f32; 3],
    pub specular_color: [f32; 3],
    pub emissive_color: [f32; 3],
    pub shine: f32,
    pub alpha: f32,
}

impl Block for NiMaterialProperty {
    const NAME: &'static str = "NiMaterialProperty";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiMaterialProperty {
            base: NiProperty::read(r)?,
            ambient_color: r.vec3()?,
            diffuse_color: r.vec3()?,
            specular_color: r.vec3()?,
            emissive_color: r.vec3()?,
            shine: r.f32()?,
            alpha: r.f32()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)?;
        w.vec3(&self.ambient_color);
        w.vec3(&self.diffuse_color);
        w.vec3(&self.specular_color);
        w.vec3(&self.emissive_color);
        w.f32(self.shine);
        w.f32(self.alpha);
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
    fn a_material_property_round_trips() {
        let m = NiMaterialProperty {
            ambient_color: [1.0; 3],
            diffuse_color: [1.0; 3],
            specular_color: [0.0; 3],
            emissive_color: [0.1, 0.2, 0.3],
            shine: 10.0,
            alpha: 1.0,
            ..Default::default()
        };
        let mut w = Writer::new();
        m.write(&mut w).unwrap();
        assert_eq!(w.out.len(), 4 + 4 + 4 + 2 + 4 * 12 + 4 + 4);
        assert_eq!(NiMaterialProperty::read(&mut Reader::new(&w.out)).unwrap(), m);
    }
}
