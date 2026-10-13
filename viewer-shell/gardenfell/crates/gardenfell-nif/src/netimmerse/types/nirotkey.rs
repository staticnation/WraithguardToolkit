//! `NiRotKey`: a rotation track, one of four key shapes told apart by the type word that
//! follows the count (`consts::key_type::*`): quaternion keys in the linear, Bezier and
//! TCB shapes, or three Euler angle tracks. Carried by `NiRotData`. `tes3` nirotkey.rs.
//!
//! The count comes first; when it is zero no type word follows, and an empty track is
//! written the same way whichever shape it holds. The Euler shape is one entry in the
//! file (its count is written as `len().min(1)`, as `tes3` does) holding an axis order
//! and three float tracks.

use super::*;

#[derive(Clone, Debug, PartialEq)]
pub enum NiRotKey {
    Lin(Vec<NiLinRotKey>),
    Bez(Vec<NiBezRotKey>),
    Tcb(Vec<NiTcbRotKey>),
    Euler(NiEulerRotKeys),
}

impl Default for NiRotKey {
    fn default() -> Self {
        NiRotKey::Lin(Vec::new())
    }
}

/// The identity quaternion as the file holds one, `[w, x, y, z]`.
const QUAT_IDENTITY: [f32; 4] = [1.0, 0.0, 0.0, 0.0];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NiLinRotKey {
    pub time: f32,
    /// `[w, x, y, z]`.
    pub value: [f32; 4],
}

impl Default for NiLinRotKey {
    fn default() -> Self {
        NiLinRotKey { time: 0.0, value: QUAT_IDENTITY }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NiBezRotKey {
    pub time: f32,
    /// `[w, x, y, z]`.
    pub value: [f32; 4],
}

impl Default for NiBezRotKey {
    fn default() -> Self {
        NiBezRotKey { time: 0.0, value: QUAT_IDENTITY }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NiTcbRotKey {
    pub time: f32,
    /// `[w, x, y, z]`.
    pub value: [f32; 4],
    pub tension: f32,
    pub continuity: f32,
    pub bias: f32,
}

impl Default for NiTcbRotKey {
    fn default() -> Self {
        NiTcbRotKey { time: 0.0, value: QUAT_IDENTITY, tension: 0.0, continuity: 0.0, bias: 0.0 }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiEulerRotKeys {
    /// `consts::axis_order::*`.
    pub axis_order: i32,
    /// One float track per axis, each a whole `NiFloatData` layout inline.
    pub axes: [NiFloatData; 3],
}

impl NiEulerRotKeys {
    /// The longest of the three axis tracks, which is what the file's count reflects
    /// (as zero or one).
    pub fn len(&self) -> usize {
        self.axes.iter().map(|a| a.keys.len()).max().unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn read(r: &mut Reader<'_>) -> Result<NiEulerRotKeys> {
        let axis_order = r.i32()?;
        let axes = [NiFloatData::read(r)?, NiFloatData::read(r)?, NiFloatData::read(r)?];
        Ok(NiEulerRotKeys { axis_order, axes })
    }

    pub fn write(&self, w: &mut Writer) -> Result<()> {
        w.i32(self.axis_order);
        for a in &self.axes {
            a.write(w)?;
        }
        Ok(())
    }
}

impl NiRotKey {
    /// The key count the file holds: the number of keys, or for the Euler shape one when
    /// any axis has a key and zero otherwise.
    pub fn len(&self) -> usize {
        match self {
            NiRotKey::Lin(k) => k.len(),
            NiRotKey::Bez(k) => k.len(),
            NiRotKey::Tcb(k) => k.len(),
            NiRotKey::Euler(e) => e.len().min(1),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn read(r: &mut Reader<'_>) -> Result<NiRotKey> {
        // The smallest entry is an Euler one: the axis order and three empty tracks.
        let n = r.count(16)?;
        if n == 0 {
            return Ok(NiRotKey::Lin(Vec::new()));
        }
        let at = r.pos();
        let kind = r.i32()?;
        Ok(match kind {
            consts::key_type::LIN => {
                NiRotKey::Lin(r.seq(n, |r| Ok(NiLinRotKey { time: r.f32()?, value: r.quat()? }))?)
            }
            consts::key_type::BEZ => {
                NiRotKey::Bez(r.seq(n, |r| Ok(NiBezRotKey { time: r.f32()?, value: r.quat()? }))?)
            }
            consts::key_type::TCB => NiRotKey::Tcb(r.seq(n, |r| {
                Ok(NiTcbRotKey {
                    time: r.f32()?,
                    value: r.quat()?,
                    tension: r.f32()?,
                    continuity: r.f32()?,
                    bias: r.f32()?,
                })
            })?),
            // One entry whatever the count says, as tes3 reads it.
            consts::key_type::EULER => NiRotKey::Euler(NiEulerRotKeys::read(r)?),
            _ => {
                return Err(crate::bytes::Error {
                    at,
                    what: format!("rotation key type {kind} is not linear, Bezier, TCB or Euler"),
                });
            }
        })
    }

    pub fn write(&self, w: &mut Writer) -> Result<()> {
        w.count(self.len());
        if self.is_empty() {
            return Ok(());
        }
        match self {
            NiRotKey::Lin(keys) => {
                w.i32(consts::key_type::LIN);
                for k in keys {
                    w.f32(k.time);
                    w.quat(&k.value);
                }
            }
            NiRotKey::Bez(keys) => {
                w.i32(consts::key_type::BEZ);
                for k in keys {
                    w.f32(k.time);
                    w.quat(&k.value);
                }
            }
            NiRotKey::Tcb(keys) => {
                w.i32(consts::key_type::TCB);
                for k in keys {
                    w.f32(k.time);
                    w.quat(&k.value);
                    w.f32(k.tension);
                    w.f32(k.continuity);
                    w.f32(k.bias);
                }
            }
            NiRotKey::Euler(e) => {
                w.i32(consts::key_type::EULER);
                e.write(w)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_key_shape_round_trips_and_an_unknown_type_is_refused() {
        let mut euler = NiEulerRotKeys { axis_order: consts::axis_order::ZYX, ..Default::default() };
        euler.axes[1].keys = NiFloatKey::Lin(vec![NiLinFloatKey { time: 0.0, value: 1.5 }]);
        assert_eq!(euler.len(), 1);
        for keys in [
            NiRotKey::Lin(vec![
                NiLinRotKey { time: 0.0, value: QUAT_IDENTITY },
                NiLinRotKey { time: 1.0, value: [0.0, 1.0, 0.0, 0.0] },
            ]),
            NiRotKey::Bez(vec![NiBezRotKey { time: 0.5, value: [0.7, 0.7, 0.0, 0.0] }]),
            NiRotKey::Tcb(vec![NiTcbRotKey { tension: 0.5, ..Default::default() }]),
            NiRotKey::Euler(euler),
        ] {
            let mut w = Writer::new();
            keys.write(&mut w).unwrap();
            assert_eq!(NiRotKey::read(&mut Reader::new(&w.out)).unwrap(), keys);
        }
        // An Euler shape with no keys on any axis writes as an empty track.
        let mut w = Writer::new();
        NiRotKey::Euler(NiEulerRotKeys::default()).write(&mut w).unwrap();
        assert_eq!(w.out, vec![0, 0, 0, 0]);
        assert_eq!(NiRotKey::read(&mut Reader::new(&w.out)).unwrap(), NiRotKey::Lin(Vec::new()));
        let mut bad = vec![1u8, 0, 0, 0, 5, 0, 0, 0];
        bad.extend_from_slice(&[0u8; 20]);
        let e = NiRotKey::read(&mut Reader::new(&bad)).unwrap_err();
        assert!(e.what.contains("key type 5"), "{e}");
    }
}
