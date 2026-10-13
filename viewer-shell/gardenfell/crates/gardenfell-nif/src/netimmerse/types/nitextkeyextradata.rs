//! `NiTextKeyExtraData`: the named moments of an animation, `start`, `stop` and the rest,
//! each a time and a string. `tes3` nitextkeyextradata.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiTextKeyExtraData {
    pub base: NiExtraData,
    pub keys: Vec<NiTextKey>,
}

impl Block for NiTextKeyExtraData {
    const NAME: &'static str = "NiTextKeyExtraData";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let base = NiExtraData::read(r)?;
        // A key is at least its time and an empty string's length word.
        let keys = r.counted(8, NiTextKey::read)?;
        Ok(NiTextKeyExtraData { base, keys })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)?;
        w.count(self.keys.len());
        for k in &self.keys {
            k.write(w)?;
        }
        Ok(())
    }

    fn links(&mut self, f: &mut dyn FnMut(&mut i32)) {
        self.base.links(f);
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiTextKey {
    pub time: f32,
    pub value: String,
}

impl NiTextKey {
    pub fn read(r: &mut Reader<'_>) -> Result<NiTextKey> {
        Ok(NiTextKey { time: r.f32()?, value: r.string()? })
    }

    pub fn write(&self, w: &mut Writer) -> Result<()> {
        w.f32(self.time);
        w.string(&self.value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_keys_round_trip_and_a_bad_key_count_is_refused() {
        let x = NiTextKeyExtraData {
            keys: vec![
                NiTextKey { time: 0.0, value: "start".into() },
                NiTextKey { time: 1.5, value: "stop".into() },
            ],
            ..Default::default()
        };
        let mut w = Writer::new();
        x.write(&mut w).unwrap();
        assert_eq!(NiTextKeyExtraData::read(&mut Reader::new(&w.out)).unwrap(), x);
        let mut bad = w.out.clone();
        bad[8..12].copy_from_slice(&1_000_000u32.to_le_bytes());
        let e = NiTextKeyExtraData::read(&mut Reader::new(&bad)).unwrap_err();
        assert!(e.what.contains("1000000 items"), "{e}");
    }
}
