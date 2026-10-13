//! `NiFloatKey`: a float animation track, one of three key shapes told apart by the type
//! word that follows the count (`consts::key_type::*`). Carried by `NiFloatData`.
//! `tes3` nifloatkey.rs.
//!
//! The count comes first; when it is zero no type word follows, and an empty track is
//! written the same way whichever shape it holds.

use super::*;

#[derive(Clone, Debug, PartialEq)]
pub enum NiFloatKey {
    Lin(Vec<NiLinFloatKey>),
    Bez(Vec<NiBezFloatKey>),
    Tcb(Vec<NiTcbFloatKey>),
}

impl Default for NiFloatKey {
    fn default() -> Self {
        NiFloatKey::Lin(Vec::new())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NiLinFloatKey {
    pub time: f32,
    pub value: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NiBezFloatKey {
    pub time: f32,
    pub value: f32,
    pub in_tan: f32,
    pub out_tan: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NiTcbFloatKey {
    pub time: f32,
    pub value: f32,
    pub tension: f32,
    pub continuity: f32,
    pub bias: f32,
}

impl NiFloatKey {
    pub fn len(&self) -> usize {
        match self {
            NiFloatKey::Lin(k) => k.len(),
            NiFloatKey::Bez(k) => k.len(),
            NiFloatKey::Tcb(k) => k.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn read(r: &mut Reader<'_>) -> Result<NiFloatKey> {
        let n = r.count(8)?;
        if n == 0 {
            return Ok(NiFloatKey::Lin(Vec::new()));
        }
        let at = r.pos();
        let kind = r.i32()?;
        Ok(match kind {
            consts::key_type::LIN => {
                NiFloatKey::Lin(r.seq(n, |r| Ok(NiLinFloatKey { time: r.f32()?, value: r.f32()? }))?)
            }
            consts::key_type::BEZ => NiFloatKey::Bez(r.seq(n, |r| {
                Ok(NiBezFloatKey { time: r.f32()?, value: r.f32()?, in_tan: r.f32()?, out_tan: r.f32()? })
            })?),
            consts::key_type::TCB => NiFloatKey::Tcb(r.seq(n, |r| {
                Ok(NiTcbFloatKey {
                    time: r.f32()?,
                    value: r.f32()?,
                    tension: r.f32()?,
                    continuity: r.f32()?,
                    bias: r.f32()?,
                })
            })?),
            _ => {
                return Err(crate::bytes::Error {
                    at,
                    what: format!("float key type {kind} is not linear, Bezier or TCB"),
                })
            }
        })
    }

    pub fn write(&self, w: &mut Writer) -> Result<()> {
        w.count(self.len());
        if self.is_empty() {
            return Ok(());
        }
        match self {
            NiFloatKey::Lin(keys) => {
                w.i32(consts::key_type::LIN);
                for k in keys {
                    w.f32(k.time);
                    w.f32(k.value);
                }
            }
            NiFloatKey::Bez(keys) => {
                w.i32(consts::key_type::BEZ);
                for k in keys {
                    w.f32(k.time);
                    w.f32(k.value);
                    w.f32(k.in_tan);
                    w.f32(k.out_tan);
                }
            }
            NiFloatKey::Tcb(keys) => {
                w.i32(consts::key_type::TCB);
                for k in keys {
                    w.f32(k.time);
                    w.f32(k.value);
                    w.f32(k.tension);
                    w.f32(k.continuity);
                    w.f32(k.bias);
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_key_shape_round_trips_and_an_empty_track_has_no_type_word() {
        for keys in [
            NiFloatKey::Lin(vec![
                NiLinFloatKey { time: 0.0, value: 1.0 },
                NiLinFloatKey { time: 1.0, value: 0.0 },
            ]),
            NiFloatKey::Bez(vec![NiBezFloatKey { time: 0.5, value: 2.0, in_tan: 0.1, out_tan: -0.1 }]),
            NiFloatKey::Tcb(vec![NiTcbFloatKey {
                time: 0.0,
                value: 1.0,
                tension: 0.0,
                continuity: 0.0,
                bias: 0.0,
            }]),
        ] {
            let mut w = Writer::new();
            keys.write(&mut w).unwrap();
            assert_eq!(NiFloatKey::read(&mut Reader::new(&w.out)).unwrap(), keys);
        }
        let mut w = Writer::new();
        NiFloatKey::Bez(Vec::new()).write(&mut w).unwrap();
        assert_eq!(w.out, vec![0, 0, 0, 0]);
        assert_eq!(NiFloatKey::read(&mut Reader::new(&w.out)).unwrap(), NiFloatKey::Lin(Vec::new()));
        let bad = [1u8, 0, 0, 0, 9, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let e = NiFloatKey::read(&mut Reader::new(&bad)).unwrap_err();
        assert!(e.what.contains("key type 9"), "{e}");
    }
}
