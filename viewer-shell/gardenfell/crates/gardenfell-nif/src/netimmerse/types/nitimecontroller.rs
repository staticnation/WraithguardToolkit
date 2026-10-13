//! `NiTimeController`: the base of every animation controller, with the chain link and
//! the timing. `tes3` nitimecontroller.rs.

use super::*;

#[derive(Clone, Debug, PartialEq)]
pub struct NiTimeController {
    pub base: NiObject,
    pub next: Link<NiTimeController>,
    /// `consts::controller_flags::*`: bits 1-2 the cycle type, bit 3 active.
    pub flags: u16,
    pub frequency: f32,
    pub phase: f32,
    pub start_time: f32,
    pub stop_time: f32,
    pub target: Link<NiObjectNET>,
}

impl Default for NiTimeController {
    fn default() -> Self {
        NiTimeController {
            base: NiObject,
            next: Link::NONE,
            flags: 0,
            frequency: 1.0,
            phase: 0.0,
            start_time: 0.0,
            stop_time: 0.0,
            target: Link::NONE,
        }
    }
}

impl Block for NiTimeController {
    const NAME: &'static str = "NiTimeController";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiTimeController {
            base: NiObject::read(r)?,
            next: r.link()?,
            flags: r.u16()?,
            frequency: r.f32()?,
            phase: r.f32()?,
            start_time: r.f32()?,
            stop_time: r.f32()?,
            target: r.link()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)?;
        w.link(self.next);
        w.u16(self.flags);
        w.f32(self.frequency);
        w.f32(self.phase);
        w.f32(self.start_time);
        w.f32(self.stop_time);
        w.link(self.target);
        Ok(())
    }

    fn links(&mut self, f: &mut dyn FnMut(&mut i32)) {
        self.base.links(f);
        f(&mut self.next.index);
        f(&mut self.target.index);
    }
}
