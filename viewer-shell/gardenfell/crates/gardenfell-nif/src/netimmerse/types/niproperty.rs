//! `NiProperty`: the base of every render state a shape carries, a named block with a
//! flags word. `tes3` niproperty.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiProperty {
    pub base: NiObjectNET,
    /// What the bits mean depends on the property: `consts::alpha_property_flags::*`,
    /// `consts::wireframe_property_flags::*`, `consts::zbuffer_property_flags::*`.
    pub flags: u16,
}

impl Block for NiProperty {
    const NAME: &'static str = "NiProperty";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiProperty { base: NiObjectNET::read(r)?, flags: r.u16()? })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)?;
        w.u16(self.flags);
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
    fn a_property_round_trips_with_its_flags() {
        let p = NiProperty { base: NiObjectNET { name: "p".into(), ..Default::default() }, flags: 0x1234 };
        let mut w = Writer::new();
        p.write(&mut w).unwrap();
        assert_eq!(w.out.len(), 4 + 1 + 4 + 4 + 2);
        assert_eq!(NiProperty::read(&mut Reader::new(&w.out)).unwrap(), p);
    }
}
