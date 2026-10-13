//! `NiKeyframeManager`: the controller that holds a model's named animation sequences.
//! `tes3` nikeyframemanager.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiKeyframeManager {
    pub base: NiTimeController,
    pub sequences: Vec<NiSequence>,
}

impl Block for NiKeyframeManager {
    const NAME: &'static str = "NiKeyframeManager";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let base = NiTimeController::read(r)?;
        let sequences = r.counted(NiSequence::MIN_SIZE, NiSequence::read)?;
        Ok(NiKeyframeManager { base, sequences })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)?;
        w.count(self.sequences.len());
        for s in &self.sequences {
            s.write(w)?;
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
    fn a_keyframe_manager_round_trips_and_a_bad_sequence_count_is_refused() {
        let m = NiKeyframeManager {
            sequences: vec![
                NiSequence { sequence_name: "Idle".into(), ..Default::default() },
                NiSequence {
                    sequence_name: "Run".into(),
                    sequence_target: SequenceTarget::External("base_anim.kf".into()),
                    name_controller_pairs: vec![("Bip01".into(), 2)],
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let mut w = Writer::new();
        m.write(&mut w).unwrap();
        assert_eq!(NiKeyframeManager::read(&mut Reader::new(&w.out)).unwrap(), m);
        let mut bad = w.out.clone();
        let at = 4 + 2 + 16 + 4;
        bad[at..at + 4].copy_from_slice(&1_000_000u32.to_le_bytes());
        let e = NiKeyframeManager::read(&mut Reader::new(&bad)).unwrap_err();
        assert!(e.what.contains("1000000 items"), "{e}");
    }
}
