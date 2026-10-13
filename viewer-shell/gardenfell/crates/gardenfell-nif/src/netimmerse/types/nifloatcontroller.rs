//! `NiFloatController`: a controller that drives one float from an `NiFloatData` track.
//! The base of the alpha and roll controllers. `tes3` nifloatcontroller.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiFloatController {
    pub base: NiTimeController,
    pub data: Link<NiFloatData>,
}

impl Block for NiFloatController {
    const NAME: &'static str = "NiFloatController";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiFloatController { base: NiTimeController::read(r)?, data: r.link()? })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)?;
        w.link(self.data);
        Ok(())
    }

    fn links(&mut self, f: &mut dyn FnMut(&mut i32)) {
        self.base.links(f);
        f(&mut self.data.index);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_float_controller_round_trips() {
        let mut c = NiFloatController { data: Link::new(3), ..Default::default() };
        c.base.flags = 0x000C;
        c.base.stop_time = 2.5;
        let mut w = Writer::new();
        c.write(&mut w).unwrap();
        assert_eq!(w.out.len(), 4 + 2 + 16 + 4 + 4);
        assert_eq!(NiFloatController::read(&mut Reader::new(&w.out)).unwrap(), c);
    }
}
