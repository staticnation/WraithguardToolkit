//! `NiPathController`: moves its target along a position track, banking into the turns,
//! with a float track for how far along it is. `tes3` nipathcontroller.rs.

use super::*;

#[derive(Clone, Debug, PartialEq)]
pub struct NiPathController {
    pub base: NiTimeController,
    /// `consts::bank_direction::*`: 1 positive, -1 negative.
    pub bank_direction: i32,
    pub max_bank_angle: f32,
    pub smoothing: f32,
    /// Which axis follows the path. `tes3` keeps it a plain word.
    pub follow_axis: u16,
    pub data: Link<NiPosData>,
    pub percentage_data: Link<NiFloatData>,
}

impl Default for NiPathController {
    fn default() -> Self {
        NiPathController {
            base: Default::default(),
            bank_direction: consts::bank_direction::POSITIVE,
            max_bank_angle: 0.0,
            smoothing: 0.0,
            follow_axis: 0,
            data: Link::NONE,
            percentage_data: Link::NONE,
        }
    }
}

impl Block for NiPathController {
    const NAME: &'static str = "NiPathController";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiPathController {
            base: NiTimeController::read(r)?,
            bank_direction: r.i32()?,
            max_bank_angle: r.f32()?,
            smoothing: r.f32()?,
            follow_axis: r.u16()?,
            data: r.link()?,
            percentage_data: r.link()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)?;
        w.i32(self.bank_direction);
        w.f32(self.max_bank_angle);
        w.f32(self.smoothing);
        w.u16(self.follow_axis);
        w.link(self.data);
        w.link(self.percentage_data);
        Ok(())
    }

    fn links(&mut self, f: &mut dyn FnMut(&mut i32)) {
        self.base.links(f);
        f(&mut self.data.index);
        f(&mut self.percentage_data.index);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_controller_round_trips() {
        let c = NiPathController {
            bank_direction: consts::bank_direction::NEGATIVE,
            max_bank_angle: 0.5,
            smoothing: 0.25,
            follow_axis: 2,
            data: Link::new(3),
            percentage_data: Link::new(4),
            ..Default::default()
        };
        let mut w = Writer::new();
        c.write(&mut w).unwrap();
        assert_eq!(w.out.len(), 4 + 2 + 16 + 4 + 4 + 4 + 4 + 2 + 4 + 4);
        assert_eq!(NiPathController::read(&mut Reader::new(&w.out)).unwrap(), c);
    }
}
