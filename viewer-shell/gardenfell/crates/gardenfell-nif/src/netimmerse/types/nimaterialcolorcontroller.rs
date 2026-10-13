//! `NiMaterialColorController`: animates a material color from an `NiPosData` track, the
//! three floats of a position read as RGB. `tes3` nimaterialcolorcontroller.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiMaterialColorController {
    pub base: NiTimeController,
    pub data: Link<NiPosData>,
}

impl Block for NiMaterialColorController {
    const NAME: &'static str = "NiMaterialColorController";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiMaterialColorController { base: NiTimeController::read(r)?, data: r.link()? })
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
    fn a_material_color_controller_round_trips() {
        let c = NiMaterialColorController { data: Link::new(6), ..Default::default() };
        let mut w = Writer::new();
        c.write(&mut w).unwrap();
        assert_eq!(NiMaterialColorController::read(&mut Reader::new(&w.out)).unwrap(), c);
    }
}
