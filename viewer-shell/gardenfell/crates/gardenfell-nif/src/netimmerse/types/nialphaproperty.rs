//! `NiAlphaProperty`: alpha blending and alpha testing, with the test threshold. `tes3`
//! nialphaproperty.rs.
//!
//! The settings live in the base's flags (`consts::alpha_property_flags::*`): bit 0
//! blending on, bits 1-4 the source blend function, bits 5-8 the destination blend
//! function, bit 9 testing on, bits 10-12 the test function.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiAlphaProperty {
    pub base: NiProperty,
    /// The alpha value a test compares against, 0 to 255.
    pub test_ref: u8,
}

impl Block for NiAlphaProperty {
    const NAME: &'static str = "NiAlphaProperty";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiAlphaProperty { base: NiProperty::read(r)?, test_ref: r.u8()? })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)?;
        w.u8(self.test_ref);
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
    fn an_alpha_property_round_trips() {
        let mut a = NiAlphaProperty::default();
        a.base.flags = 0x00ED;
        a.test_ref = 128;
        let mut w = Writer::new();
        a.write(&mut w).unwrap();
        assert_eq!(w.out.len(), 4 + 4 + 4 + 2 + 1);
        assert_eq!(NiAlphaProperty::read(&mut Reader::new(&w.out)).unwrap(), a);
    }
}
