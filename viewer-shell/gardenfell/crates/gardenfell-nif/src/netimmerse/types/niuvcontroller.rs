//! `NiUVController`: scrolls and scales one UV set from the four float tracks of an
//! `NiUVData`. `tes3` niuvcontroller.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiUVController {
    pub base: NiTimeController,
    pub texture_set: u16,
    pub data: Link<NiUVData>,
}

impl Block for NiUVController {
    const NAME: &'static str = "NiUVController";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiUVController { base: NiTimeController::read(r)?, texture_set: r.u16()?, data: r.link()? })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)?;
        w.u16(self.texture_set);
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
    fn a_uv_controller_round_trips() {
        let c = NiUVController { texture_set: 1, data: Link::new(8), ..Default::default() };
        let mut w = Writer::new();
        c.write(&mut w).unwrap();
        assert_eq!(w.out.len(), 4 + 2 + 16 + 4 + 2 + 4);
        assert_eq!(NiUVController::read(&mut Reader::new(&w.out)).unwrap(), c);
    }
}
