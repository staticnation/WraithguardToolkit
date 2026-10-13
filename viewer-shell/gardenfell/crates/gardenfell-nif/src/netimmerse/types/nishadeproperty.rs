//! `NiShadeProperty`: smooth or flat shading, nothing beyond the base. `tes3`
//! nishadeproperty.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiShadeProperty {
    pub base: NiProperty,
}

impl Block for NiShadeProperty {
    const NAME: &'static str = "NiShadeProperty";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiShadeProperty { base: NiProperty::read(r)? })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)
    }

    fn links(&mut self, f: &mut dyn FnMut(&mut i32)) {
        self.base.links(f);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shade_property_round_trips() {
        let p = NiShadeProperty { base: NiProperty { flags: 1, ..Default::default() } };
        let mut w = Writer::new();
        p.write(&mut w).unwrap();
        assert_eq!(NiShadeProperty::read(&mut Reader::new(&w.out)).unwrap(), p);
    }
}
