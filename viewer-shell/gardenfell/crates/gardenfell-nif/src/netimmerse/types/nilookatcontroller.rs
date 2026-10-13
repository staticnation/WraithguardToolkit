//! `NiLookAtController`: turns its target to face another object. `tes3`
//! nilookatcontroller.rs.
//!
//! Its own flags live in `base.flags`: `consts::look_at_controller_flags::FLIP` (bit 4)
//! and the axis in bits 5-6 (`AXIS_MASK`, `AXIS_SHIFT`, values `consts::axis::*`).

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiLookAtController {
    pub base: NiTimeController,
    pub look_at: Link<NiAVObject>,
}

impl Block for NiLookAtController {
    const NAME: &'static str = "NiLookAtController";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiLookAtController { base: NiTimeController::read(r)?, look_at: r.link()? })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)?;
        w.link(self.look_at);
        Ok(())
    }

    fn links(&mut self, f: &mut dyn FnMut(&mut i32)) {
        self.base.links(f);
        f(&mut self.look_at.index);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_look_at_controller_round_trips() {
        let mut c = NiLookAtController { look_at: Link::new(11), ..Default::default() };
        use consts::look_at_controller_flags::{AXIS_SHIFT, FLIP};
        c.base.flags = FLIP | (2 << AXIS_SHIFT);
        let mut w = Writer::new();
        c.write(&mut w).unwrap();
        assert_eq!(NiLookAtController::read(&mut Reader::new(&w.out)).unwrap(), c);
    }
}
