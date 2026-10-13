//! `NiKeyframeData`: a transform animation, three tracks inline: rotations, translations
//! and scales, each the whole layout of its own data block. `tes3` nikeyframedata.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiKeyframeData {
    pub base: NiObject,
    pub rotations: NiRotData,
    pub translations: NiPosData,
    pub scales: NiFloatData,
}

impl Block for NiKeyframeData {
    const NAME: &'static str = "NiKeyframeData";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiKeyframeData {
            base: NiObject::read(r)?,
            rotations: NiRotData::read(r)?,
            translations: NiPosData::read(r)?,
            scales: NiFloatData::read(r)?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)?;
        self.rotations.write(w)?;
        self.translations.write(w)?;
        self.scales.write(w)
    }

    fn links(&mut self, f: &mut dyn FnMut(&mut i32)) {
        self.base.links(f);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyframe_data_round_trips_and_a_bad_track_type_is_refused() {
        let mut d = NiKeyframeData::default();
        d.rotations.keys = NiRotKey::Lin(vec![NiLinRotKey::default()]);
        d.translations.keys = NiPosKey::Bez(vec![NiBezPosKey { time: 0.0, ..Default::default() }]);
        d.scales.keys = NiFloatKey::Tcb(vec![NiTcbFloatKey { value: 1.0, ..Default::default() }]);
        let mut w = Writer::new();
        d.write(&mut w).unwrap();
        assert_eq!(w.out.len(), (4 + 4 + 20) + (4 + 4 + 40) + (4 + 4 + 20));
        assert_eq!(NiKeyframeData::read(&mut Reader::new(&w.out)).unwrap(), d);
        // The empty default is three zero counts.
        let mut w = Writer::new();
        NiKeyframeData::default().write(&mut w).unwrap();
        assert_eq!(w.out, vec![0u8; 12]);
        // A translation track with a type word the layout does not have.
        let mut bad = vec![0u8; 4];
        bad.extend_from_slice(&[1, 0, 0, 0, 9, 0, 0, 0]);
        bad.extend_from_slice(&[0u8; 16]);
        bad.extend_from_slice(&[0u8; 4]);
        let e = NiKeyframeData::read(&mut Reader::new(&bad)).unwrap_err();
        assert!(e.what.contains("position key type 9"), "{e}");
    }
}
