//! `NiParticleGrowFade`: a modifier that grows a particle in after birth and fades it out
//! before death. `tes3` niparticlegrowfade.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiParticleGrowFade {
    pub base: NiParticleModifier,
    pub grow_time: f32,
    pub fade_time: f32,
}

impl Block for NiParticleGrowFade {
    const NAME: &'static str = "NiParticleGrowFade";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiParticleGrowFade {
            base: NiParticleModifier::read(r)?,
            grow_time: r.f32()?,
            fade_time: r.f32()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)?;
        w.f32(self.grow_time);
        w.f32(self.fade_time);
        Ok(())
    }

    fn links(&mut self, f: &mut dyn FnMut(&mut i32)) {
        self.base.links(f);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grow_fade_round_trips() {
        let m = NiParticleGrowFade { grow_time: 0.2, fade_time: 1.5, ..Default::default() };
        let mut w = Writer::new();
        m.write(&mut w).unwrap();
        assert_eq!(w.out.len(), 16);
        assert_eq!(NiParticleGrowFade::read(&mut Reader::new(&w.out)).unwrap(), m);
    }
}
