//! `NiVisData`: a visibility track, each key a time and one byte. No type word here,
//! unlike the other key tracks. `tes3` nivisdata.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiVisData {
    pub base: NiObject,
    pub keys: Vec<NiVisKey>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NiVisKey {
    pub time: f32,
    /// Non-zero for visible.
    pub value: u8,
}

impl Block for NiVisData {
    const NAME: &'static str = "NiVisData";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let base = NiObject::read(r)?;
        let keys = r.counted(5, |r| Ok(NiVisKey { time: r.f32()?, value: r.u8()? }))?;
        Ok(NiVisData { base, keys })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)?;
        w.count(self.keys.len());
        for k in &self.keys {
            w.f32(k.time);
            w.u8(k.value);
        }
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
    fn vis_data_round_trips_and_a_bad_count_is_refused() {
        let d = NiVisData {
            keys: vec![NiVisKey { time: 0.0, value: 1 }, NiVisKey { time: 0.5, value: 0 }],
            ..Default::default()
        };
        let mut w = Writer::new();
        d.write(&mut w).unwrap();
        assert_eq!(w.out.len(), 4 + 2 * 5);
        assert_eq!(NiVisData::read(&mut Reader::new(&w.out)).unwrap(), d);
        let mut bad = w.out.clone();
        bad[0..4].copy_from_slice(&1_000_000u32.to_le_bytes());
        let e = NiVisData::read(&mut Reader::new(&bad)).unwrap_err();
        assert!(e.what.contains("1000000 items"), "{e}");
    }
}
