//! `NiKeyframeController`: animates its target's transform from an `NiKeyframeData`.
//! `tes3` nikeyframecontroller.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiKeyframeController {
    pub base: NiTimeController,
    pub data: Link<NiKeyframeData>,
}

impl Block for NiKeyframeController {
    const NAME: &'static str = "NiKeyframeController";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiKeyframeController { base: NiTimeController::read(r)?, data: r.link()? })
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
    fn a_keyframe_controller_round_trips() {
        let mut c = NiKeyframeController { data: Link::new(5), ..Default::default() };
        c.base.target = Link::new(0);
        let mut w = Writer::new();
        c.write(&mut w).unwrap();
        assert_eq!(NiKeyframeController::read(&mut Reader::new(&w.out)).unwrap(), c);
    }
}
