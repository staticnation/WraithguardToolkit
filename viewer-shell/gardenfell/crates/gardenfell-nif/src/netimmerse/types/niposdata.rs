//! `NiPosData`: a block holding one position track, which the color controllers also use
//! for RGB. `tes3` niposdata.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiPosData {
    pub base: NiObject,
    pub keys: NiPosKey,
}

impl Block for NiPosData {
    const NAME: &'static str = "NiPosData";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiPosData { base: NiObject::read(r)?, keys: NiPosKey::read(r)? })
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
    fn pos_data_round_trips() {
        let d = NiPosData {
            keys: NiPosKey::Lin(vec![NiLinPosKey { time: 0.0, value: [1.0, 2.0, 3.0] }]),
            ..Default::default()
        };
        let mut w = Writer::new();
        d.write(&mut w).unwrap();
        assert_eq!(w.out.len(), 4 + 4 + 16);
        assert_eq!(NiPosData::read(&mut Reader::new(&w.out)).unwrap(), d);
    }
}
