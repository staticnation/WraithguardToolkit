//! `NiAmbientLight`: a light with no position or direction, nothing of its own beyond
//! `NiLight`. `tes3` niambientlight.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiAmbientLight {
    pub base: NiLight,
}

impl Block for NiAmbientLight {
    const NAME: &'static str = "NiAmbientLight";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiAmbientLight { base: NiLight::read(r)? })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)
    }

    fn links(&mut self, f: &mut dyn FnMut(&mut i32)) {
        self.base.links(f);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_ambient_light_round_trips() {
        let mut l = NiAmbientLight::default();
        l.base.ambient_color = [0.2, 0.2, 0.3];
        let mut w = Writer::new();
        l.write(&mut w).unwrap();
        assert_eq!(NiAmbientLight::read(&mut Reader::new(&w.out)).unwrap(), l);
    }
}
