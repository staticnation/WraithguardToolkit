//! `NiSpecularProperty`: specular highlights on or off, nothing beyond the base. `tes3`
//! nispecularproperty.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiSpecularProperty {
    pub base: NiProperty,
}

impl Block for NiSpecularProperty {
    const NAME: &'static str = "NiSpecularProperty";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiSpecularProperty { base: NiProperty::read(r)? })
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
    fn a_specular_property_round_trips() {
        let p = NiSpecularProperty { base: NiProperty { flags: 1, ..Default::default() } };
        let mut w = Writer::new();
        p.write(&mut w).unwrap();
        assert_eq!(NiSpecularProperty::read(&mut Reader::new(&w.out)).unwrap(), p);
    }
}
