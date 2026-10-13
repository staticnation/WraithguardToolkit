//! `NiStencilProperty`: the stencil test, its actions and which faces are drawn. `tes3`
//! nistencilproperty.rs.

use super::*;

#[derive(Clone, Debug, PartialEq)]
pub struct NiStencilProperty {
    pub base: NiProperty,
    /// One byte in the file.
    pub stencil_enabled: bool,
    /// `consts::stencil_test_function::*`
    pub stencil_function: i32,
    pub stencil_ref: u32,
    pub stencil_mask: u32,
    /// `consts::stencil_action::*`, when the stencil test fails.
    pub fail_action: i32,
    /// `consts::stencil_action::*`, when the stencil test passes and the depth test fails.
    pub pass_z_fail_action: i32,
    /// `consts::stencil_action::*`, when both tests pass.
    pub pass_action: i32,
    /// `consts::draw_mode::*`: which winding is drawn, `BOTH` for two-sided shapes.
    pub draw_mode: i32,
}

impl Default for NiStencilProperty {
    fn default() -> Self {
        NiStencilProperty {
            base: Default::default(),
            stencil_enabled: false,
            stencil_function: 0,
            stencil_ref: 0,
            stencil_mask: 0xFFFF_FFFF,
            fail_action: 0,
            pass_z_fail_action: 0,
            pass_action: 0,
            draw_mode: 0,
        }
    }
}

impl Block for NiStencilProperty {
    const NAME: &'static str = "NiStencilProperty";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiStencilProperty {
            base: NiProperty::read(r)?,
            stencil_enabled: r.bool_u8()?,
            stencil_function: r.i32()?,
            stencil_ref: r.u32()?,
            stencil_mask: r.u32()?,
            fail_action: r.i32()?,
            pass_z_fail_action: r.i32()?,
            pass_action: r.i32()?,
            draw_mode: r.i32()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)?;
        w.bool_u8(self.stencil_enabled);
        w.i32(self.stencil_function);
        w.u32(self.stencil_ref);
        w.u32(self.stencil_mask);
        w.i32(self.fail_action);
        w.i32(self.pass_z_fail_action);
        w.i32(self.pass_action);
        w.i32(self.draw_mode);
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
    fn a_stencil_property_round_trips_and_defaults_to_a_full_mask() {
        let s = NiStencilProperty { stencil_enabled: true, draw_mode: 3, ..Default::default() };
        assert_eq!(s.stencil_mask, 0xFFFF_FFFF);
        let mut w = Writer::new();
        s.write(&mut w).unwrap();
        assert_eq!(w.out.len(), 4 + 4 + 4 + 2 + 1 + 4 * 7);
        assert_eq!(NiStencilProperty::read(&mut Reader::new(&w.out)).unwrap(), s);
    }
}
