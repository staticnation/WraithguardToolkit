//! `NiSequence`: one named animation sequence of an `NiKeyframeManager`, naming its target
//! and the controllers it drives. Not a block of its own. `tes3` nisequence.rs.
//!
//! The target is told apart by a one-byte has-external word: when set, the name of an
//! external file follows, otherwise two words `tes3` leaves unnamed.

use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NiSequence {
    pub base: NiObject,
    pub sequence_name: String,
    pub sequence_target: SequenceTarget,
    /// Each a controller's name and a block index, as the file holds them.
    pub name_controller_pairs: Vec<(String, i32)>,
}

impl Default for NiSequence {
    fn default() -> Self {
        NiSequence {
            base: NiObject,
            sequence_name: String::new(),
            sequence_target: SequenceTarget::Internal(0, 0),
            name_controller_pairs: Vec::new(),
        }
    }
}

/// Where a sequence's keyframes live: an external file by name, or inside this file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SequenceTarget {
    External(String),
    Internal(i32, i32),
}

impl Default for SequenceTarget {
    fn default() -> Self {
        SequenceTarget::External(String::new())
    }
}

impl SequenceTarget {
    pub fn read(r: &mut Reader<'_>) -> Result<SequenceTarget> {
        if r.bool_u8()? {
            Ok(SequenceTarget::External(r.string()?))
        } else {
            Ok(SequenceTarget::Internal(r.i32()?, r.i32()?))
        }
    }

    pub fn write(&self, w: &mut Writer) -> Result<()> {
        match self {
            SequenceTarget::External(file_name) => {
                w.bool_u8(true);
                w.string(file_name)?;
            }
            SequenceTarget::Internal(a, b) => {
                w.bool_u8(false);
                w.i32(*a);
                w.i32(*b);
            }
        }
        Ok(())
    }
}

impl NiSequence {
    /// The smallest a sequence can be: an empty name, the has-external byte, an empty
    /// external name and an empty pair count. An internal target is four bytes longer.
    pub const MIN_SIZE: usize = 4 + 1 + 4 + 4;

    pub fn read(r: &mut Reader<'_>) -> Result<NiSequence> {
        let base = NiObject::read(r)?;
        let sequence_name = r.string()?;
        let sequence_target = SequenceTarget::read(r)?;
        // A pair is at least an empty name and its index.
        let name_controller_pairs = r.counted(8, |r| Ok((r.string()?, r.i32()?)))?;
        Ok(NiSequence { base, sequence_name, sequence_target, name_controller_pairs })
    }

    pub fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)?;
        w.string(&self.sequence_name)?;
        self.sequence_target.write(w)?;
        w.count(self.name_controller_pairs.len());
        for (name, controller) in &self.name_controller_pairs {
            w.string(name)?;
            w.i32(*controller);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_target_shapes_round_trip_and_a_bad_pair_count_is_refused() {
        let internal = NiSequence {
            sequence_name: "Idle".into(),
            sequence_target: SequenceTarget::Internal(0, 1),
            name_controller_pairs: vec![("Bip01".into(), 4), ("Bip01 Head".into(), 7)],
            ..Default::default()
        };
        let external = NiSequence {
            sequence_name: "Walk".into(),
            sequence_target: SequenceTarget::External("xbase_anim.kf".into()),
            ..Default::default()
        };
        for s in [&internal, &external] {
            let mut w = Writer::new();
            s.write(&mut w).unwrap();
            assert_eq!(&NiSequence::read(&mut Reader::new(&w.out)).unwrap(), s);
        }
        let mut w = Writer::new();
        NiSequence::default().write(&mut w).unwrap();
        assert_eq!(w.out.len(), NiSequence::MIN_SIZE + 4);
        let mut bad = w.out.clone();
        let at = 4 + 1 + 8;
        bad[at..at + 4].copy_from_slice(&1_000_000u32.to_le_bytes());
        let e = NiSequence::read(&mut Reader::new(&bad)).unwrap_err();
        assert!(e.what.contains("1000000 items"), "{e}");
    }
}
