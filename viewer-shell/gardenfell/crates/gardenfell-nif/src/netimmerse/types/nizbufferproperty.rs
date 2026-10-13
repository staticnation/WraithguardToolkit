//! `NiZBufferProperty`: depth testing and writing, nothing beyond the base. `tes3`
//! nizbufferproperty.rs.
//!
//! The settings live in the base's flags (`consts::zbuffer_property_flags::*`): bit 0
//! depth test on, bit 1 depth write on, bits 2-5 the test function.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiZBufferProperty {
    pub base: NiProperty,
}

impl Block for NiZBufferProperty {
    const NAME: &'static str = "NiZBufferProperty";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiZBufferProperty { base: NiProperty::read(r)? })
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
    fn a_z_buffer_property_round_trips() {
        let p = NiZBufferProperty { base: NiProperty { flags: 0x000F, ..Default::default() } };
        let mut w = Writer::new();
        p.write(&mut w).unwrap();
        assert_eq!(NiZBufferProperty::read(&mut Reader::new(&w.out)).unwrap(), p);
    }
}
