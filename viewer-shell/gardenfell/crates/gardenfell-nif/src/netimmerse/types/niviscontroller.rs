//! `NiVisController`: shows and hides its target from an `NiVisData` track. `tes3`
//! niviscontroller.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiVisController {
    pub base: NiTimeController,
    pub data: Link<NiVisData>,
}

impl Block for NiVisController {
    const NAME: &'static str = "NiVisController";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiVisController { base: NiTimeController::read(r)?, data: r.link()? })
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
    fn a_vis_controller_round_trips() {
        let c = NiVisController { data: Link::new(12), ..Default::default() };
        let mut w = Writer::new();
        c.write(&mut w).unwrap();
        assert_eq!(NiVisController::read(&mut Reader::new(&w.out)).unwrap(), c);
    }
}
