//! `NiParticleColorModifier`: a modifier that colors each particle by its age from a color
//! track. `tes3` niparticlecolormodifier.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiParticleColorModifier {
    pub base: NiParticleModifier,
    pub color_data: Link<NiColorData>,
}

impl Block for NiParticleColorModifier {
    const NAME: &'static str = "NiParticleColorModifier";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiParticleColorModifier { base: NiParticleModifier::read(r)?, color_data: r.link()? })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)?;
        w.link(self.color_data);
        Ok(())
    }

    fn links(&mut self, f: &mut dyn FnMut(&mut i32)) {
        self.base.links(f);
        f(&mut self.color_data.index);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_color_modifier_round_trips() {
        let m = NiParticleColorModifier { color_data: Link::new(11), ..Default::default() };
        let mut w = Writer::new();
        m.write(&mut w).unwrap();
        assert_eq!(w.out.len(), 12);
        assert_eq!(NiParticleColorModifier::read(&mut Reader::new(&w.out)).unwrap(), m);
    }
}
