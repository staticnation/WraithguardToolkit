//! `NiRotData`: a block holding one rotation track. `tes3` nirotdata.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiRotData {
    pub base: NiObject,
    pub keys: NiRotKey,
}

impl Block for NiRotData {
    const NAME: &'static str = "NiRotData";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiRotData { base: NiObject::read(r)?, keys: NiRotKey::read(r)? })
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
    fn rot_data_round_trips() {
        let d = NiRotData {
            keys: NiRotKey::Lin(vec![NiLinRotKey { time: 1.0, value: [0.0, 0.0, 1.0, 0.0] }]),
            ..Default::default()
        };
        let mut w = Writer::new();
        d.write(&mut w).unwrap();
        assert_eq!(w.out.len(), 4 + 4 + 20);
        assert_eq!(NiRotData::read(&mut Reader::new(&w.out)).unwrap(), d);
    }
}
