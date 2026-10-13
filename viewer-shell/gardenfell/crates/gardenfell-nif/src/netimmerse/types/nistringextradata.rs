//! `NiStringExtraData`: one string hung on a block, how Morrowind's meshes carry their
//! `NCO`, `NCC`, `MRK` and `sgoKeep` markers. `tes3` nistringextradata.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiStringExtraData {
    pub base: NiExtraData,
    pub value: String,
}

impl Block for NiStringExtraData {
    const NAME: &'static str = "NiStringExtraData";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiStringExtraData { base: NiExtraData::read(r)?, value: r.string()? })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)?;
        w.string(&self.value)?;
        Ok(())
    }

    fn links(&mut self, f: &mut dyn FnMut(&mut i32)) {
        self.base.links(f);
    }
}

impl NiStringExtraData {
    /// Whether the string starts with `prefix`, ASCII case aside, as the game checks its
    /// markers.
    pub fn starts_with_ignore_ascii_case(&self, prefix: &str) -> bool {
        self.value.len() >= prefix.len()
            && self.value.bytes().zip(prefix.bytes()).all(|(a, b)| a.eq_ignore_ascii_case(&b))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_extra_data_round_trips_and_a_bad_length_is_refused() {
        let x = NiStringExtraData { value: "NCO".into(), ..Default::default() };
        let mut w = Writer::new();
        x.write(&mut w).unwrap();
        assert_eq!(w.out.len(), 8 + 4 + 3);
        assert_eq!(NiStringExtraData::read(&mut Reader::new(&w.out)).unwrap(), x);
        assert!(x.starts_with_ignore_ascii_case("nc"));
        assert!(!x.starts_with_ignore_ascii_case("NCOX"));
        let mut bad = w.out.clone();
        bad[8..12].copy_from_slice(&100u32.to_le_bytes());
        let e = NiStringExtraData::read(&mut Reader::new(&bad)).unwrap_err();
        assert!(e.what.contains("100 bytes wanted"), "{e}");
    }
}
