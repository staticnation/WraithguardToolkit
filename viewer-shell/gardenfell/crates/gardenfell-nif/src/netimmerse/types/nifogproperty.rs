//! `NiFogProperty`: per-shape fog, a depth and a color. `tes3` nifogproperty.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiFogProperty {
    pub base: NiProperty,
    pub fog_depth: f32,
    /// RGB floats.
    pub fog_color: [f32; 3],
}

impl Block for NiFogProperty {
    const NAME: &'static str = "NiFogProperty";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiFogProperty { base: NiProperty::read(r)?, fog_depth: r.f32()?, fog_color: r.vec3()? })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)?;
        w.f32(self.fog_depth);
        w.vec3(&self.fog_color);
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
    fn a_fog_property_round_trips() {
        let p = NiFogProperty { fog_depth: 0.5, fog_color: [0.1, 0.2, 0.3], ..Default::default() };
        let mut w = Writer::new();
        p.write(&mut w).unwrap();
        assert_eq!(w.out.len(), 4 + 4 + 4 + 2 + 4 + 12);
        assert_eq!(NiFogProperty::read(&mut Reader::new(&w.out)).unwrap(), p);
    }
}
