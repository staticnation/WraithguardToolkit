//! `NiWireframeProperty`: wireframe drawing on or off, nothing beyond the base. `tes3`
//! niwireframeproperty.rs.
//!
//! The setting is bit 0 of the base's flags (`consts::wireframe_property_flags::*`).

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiWireframeProperty {
    pub base: NiProperty,
}

impl Block for NiWireframeProperty {
    const NAME: &'static str = "NiWireframeProperty";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiWireframeProperty { base: NiProperty::read(r)? })
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
    fn a_wireframe_property_round_trips() {
        let p = NiWireframeProperty { base: NiProperty { flags: 1, ..Default::default() } };
        let mut w = Writer::new();
        p.write(&mut w).unwrap();
        assert_eq!(NiWireframeProperty::read(&mut Reader::new(&w.out)).unwrap(), p);
    }
}
