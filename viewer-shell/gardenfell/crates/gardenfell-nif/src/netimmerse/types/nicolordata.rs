//! `NiColorData`: a block holding one color track. `tes3` nicolordata.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiColorData {
    pub base: NiObject,
    pub keys: NiColorKey,
}

impl Block for NiColorData {
    const NAME: &'static str = "NiColorData";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiColorData { base: NiObject::read(r)?, keys: NiColorKey::read(r)? })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)?;
        self.keys.write(w)
    }

    fn links(&mut self, f: &mut dyn FnMut(&mut i32)) {
        self.base.links(f);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_data_round_trips() {
        let d = NiColorData {
            keys: NiColorKey::Lin(vec![NiLinColKey { time: 0.5, value: [0.2, 0.4, 0.6, 1.0] }]),
            ..Default::default()
        };
        let mut w = Writer::new();
        d.write(&mut w).unwrap();
        assert_eq!(NiColorData::read(&mut Reader::new(&w.out)).unwrap(), d);
    }
}
