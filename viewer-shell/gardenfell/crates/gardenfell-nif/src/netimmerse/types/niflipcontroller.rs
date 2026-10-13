//! `NiFlipController`: cycles a texturing property's map through a list of source
//! textures, one per frame. `tes3` niflipcontroller.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiFlipController {
    pub base: NiTimeController,
    /// Which map of the texturing property is flipped (0 base, 1 dark, 2 detail, ...).
    /// `tes3` keeps it a plain word too.
    pub affected_map: u32,
    pub flip_start_time: f32,
    pub secs_per_frame: f32,
    pub textures: Vec<Link<NiSourceTexture>>,
}

impl Block for NiFlipController {
    const NAME: &'static str = "NiFlipController";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiFlipController {
            base: NiTimeController::read(r)?,
            affected_map: r.u32()?,
            flip_start_time: r.f32()?,
            secs_per_frame: r.f32()?,
            textures: r.links()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)?;
        w.u32(self.affected_map);
        w.f32(self.flip_start_time);
        w.f32(self.secs_per_frame);
        w.links(&self.textures);
        Ok(())
    }

    fn links(&mut self, f: &mut dyn FnMut(&mut i32)) {
        self.base.links(f);
        for l in &mut self.textures {
            f(&mut l.index);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_flip_controller_round_trips_and_a_bad_texture_count_is_refused() {
        let c = NiFlipController {
            affected_map: 0,
            flip_start_time: 0.0,
            secs_per_frame: 0.1,
            textures: vec![Link::new(2), Link::new(3), Link::NONE],
            ..Default::default()
        };
        let mut w = Writer::new();
        c.write(&mut w).unwrap();
        assert_eq!(NiFlipController::read(&mut Reader::new(&w.out)).unwrap(), c);
        let mut bad = w.out.clone();
        let at = 4 + 2 + 16 + 4 + 4 + 4 + 4;
        bad[at..at + 4].copy_from_slice(&1_000_000u32.to_le_bytes());
        let e = NiFlipController::read(&mut Reader::new(&bad)).unwrap_err();
        assert!(e.what.contains("1000000 items"), "{e}");
    }
}
