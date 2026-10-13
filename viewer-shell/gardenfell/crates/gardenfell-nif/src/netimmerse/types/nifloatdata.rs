//! `NiFloatData`: a block holding one float track. `tes3` nifloatdata.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiFloatData {
    pub base: NiObject,
    pub keys: NiFloatKey,
}

impl Block for NiFloatData {
    const NAME: &'static str = "NiFloatData";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiFloatData { base: NiObject::read(r)?, keys: NiFloatKey::read(r)? })
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
    fn float_data_round_trips() {
        let d = NiFloatData {
            keys: NiFloatKey::Lin(vec![
                NiLinFloatKey { time: 0.0, value: 0.0 },
                NiLinFloatKey { time: 1.0, value: 1.0 },
            ]),
            ..Default::default()
        };
        let mut w = Writer::new();
        d.write(&mut w).unwrap();
        assert_eq!(w.out.len(), 4 + 4 + 2 * 8);
        assert_eq!(NiFloatData::read(&mut Reader::new(&w.out)).unwrap(), d);
    }
}
