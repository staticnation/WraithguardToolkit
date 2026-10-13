//! `NiDitherProperty`: dithering on or off, nothing beyond the base. `tes3`
//! niditherproperty.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiDitherProperty {
    pub base: NiProperty,
}

impl Block for NiDitherProperty {
    const NAME: &'static str = "NiDitherProperty";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiDitherProperty { base: NiProperty::read(r)? })
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
    fn a_dither_property_round_trips() {
        let p = NiDitherProperty { base: NiProperty { flags: 1, ..Default::default() } };
        let mut w = Writer::new();
        p.write(&mut w).unwrap();
        assert_eq!(NiDitherProperty::read(&mut Reader::new(&w.out)).unwrap(), p);
    }
}
