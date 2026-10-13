//! `NiDirectionalLight`: a light from one direction, the node's own, nothing of its own
//! beyond `NiLight`. `tes3` nidirectionallight.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiDirectionalLight {
    pub base: NiLight,
}

impl Block for NiDirectionalLight {
    const NAME: &'static str = "NiDirectionalLight";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiDirectionalLight { base: NiLight::read(r)? })
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
    fn a_directional_light_round_trips() {
        let mut l = NiDirectionalLight::default();
        l.base.diffuse_color = [1.0, 1.0, 0.9];
        l.base.base.affected_nodes = vec![1];
        let mut w = Writer::new();
        l.write(&mut w).unwrap();
        assert_eq!(NiDirectionalLight::read(&mut Reader::new(&w.out)).unwrap(), l);
    }
}
