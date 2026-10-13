//! `NiLightColorController`: animates a light's color from an `NiPosData` track, the
//! three floats of a position read as RGB. `tes3` nilightcolorcontroller.rs.
//!
//! Its own flag lives in `base.flags`: `consts::light_color_controller_flags::AMBIENT`
//! (bit 4) picks the ambient color over the diffuse.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiLightColorController {
    pub base: NiTimeController,
    pub data: Link<NiPosData>,
}

impl Block for NiLightColorController {
    const NAME: &'static str = "NiLightColorController";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiLightColorController { base: NiTimeController::read(r)?, data: r.link()? })
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
    fn a_light_color_controller_round_trips() {
        let mut c = NiLightColorController { data: Link::new(2), ..Default::default() };
        c.base.flags = consts::light_color_controller_flags::AMBIENT;
        let mut w = Writer::new();
        c.write(&mut w).unwrap();
        assert_eq!(NiLightColorController::read(&mut Reader::new(&w.out)).unwrap(), c);
    }
}
