//! `NiPosKey`: a position (or RGB color) track, one of three key shapes told apart by the
//! type word that follows the count (`consts::key_type::*`). Carried by `NiPosData`.
//! `tes3` niposkey.rs.
//!
//! The count comes first; when it is zero no type word follows, and an empty track is
//! written the same way whichever shape it holds.

use super::*;

#[derive(Clone, Debug, PartialEq)]
pub enum NiPosKey {
    Lin(Vec<NiLinPosKey>),
    Bez(Vec<NiBezPosKey>),
    Tcb(Vec<NiTcbPosKey>),
}

impl Default for NiPosKey {
    fn default() -> Self {
        NiPosKey::Lin(Vec::new())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NiLinPosKey {
    pub time: f32,
    pub value: [f32; 3],
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NiBezPosKey {
    pub time: f32,
    pub value: [f32; 3],
    pub in_tan: [f32; 3],
    pub out_tan: [f32; 3],
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NiTcbPosKey {
    pub time: f32,
    pub value: [f32; 3],
    pub tension: f32,
    pub continuity: f32,
    pub bias: f32,
}

impl NiPosKey {
    pub fn len(&self) -> usize {
        match self {
            NiPosKey::Lin(k) => k.len(),
            NiPosKey::Bez(k) => k.len(),
            NiPosKey::Tcb(k) => k.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn read(r: &mut Reader<'_>) -> Result<NiPosKey> {
        let n = r.count(16)?;
        if n == 0 {
            return Ok(NiPosKey::Lin(Vec::new()));
        }
        let at = r.pos();
        let kind = r.i32()?;
        Ok(match kind {
            consts::key_type::LIN => {
                NiPosKey::Lin(r.seq(n, |r| Ok(NiLinPosKey { time: r.f32()?, value: r.vec3()? }))?)
            }
            consts::key_type::BEZ => NiPosKey::Bez(r.seq(n, |r| {
                Ok(NiBezPosKey { time: r.f32()?, value: r.vec3()?, in_tan: r.vec3()?, out_tan: r.vec3()? })
            })?),
            consts::key_type::TCB => NiPosKey::Tcb(r.seq(n, |r| {
                Ok(NiTcbPosKey {
                    time: r.f32()?,
                    value: r.vec3()?,
                    tension: r.f32()?,
                    continuity: r.f32()?,
                    bias: r.f32()?,
                })
            })?),
            _ => {
                return Err(crate::bytes::Error {
                    at,
                    what: format!("position key type {kind} is not linear, Bezier or TCB"),
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
            NiPosKey::Lin(keys) => {
                w.i32(consts::key_type::LIN);
                for k in keys {
                    w.f32(k.time);
                    w.vec3(&k.value);
                }
            }
            NiPosKey::Bez(keys) => {
                w.i32(consts::key_type::BEZ);
                for k in keys {
                    w.f32(k.time);
                    w.vec3(&k.value);
                    w.vec3(&k.in_tan);
                    w.vec3(&k.out_tan);
                }
            }
            NiPosKey::Tcb(keys) => {
                w.i32(consts::key_type::TCB);
                for k in keys {
                    w.f32(k.time);
                    w.vec3(&k.value);
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
    fn each_key_shape_round_trips_and_an_unknown_type_is_refused() {
        for keys in [
            NiPosKey::Lin(vec![
                NiLinPosKey { time: 0.0, value: [0.0, 0.0, 0.0] },
                NiLinPosKey { time: 1.0, value: [1.0, 2.0, 3.0] },
            ]),
            NiPosKey::Bez(vec![NiBezPosKey {
                time: 0.5,
                value: [1.0; 3],
                in_tan: [0.1, 0.2, 0.3],
                out_tan: [-0.1, -0.2, -0.3],
            }]),
            NiPosKey::Tcb(vec![NiTcbPosKey { tension: 0.5, ..Default::default() }]),
        ] {
            let mut w = Writer::new();
            keys.write(&mut w).unwrap();
            assert_eq!(NiPosKey::read(&mut Reader::new(&w.out)).unwrap(), keys);
        }
        let mut w = Writer::new();
        NiPosKey::Tcb(Vec::new()).write(&mut w).unwrap();
        assert_eq!(w.out, vec![0, 0, 0, 0]);
        assert_eq!(NiPosKey::read(&mut Reader::new(&w.out)).unwrap(), NiPosKey::Lin(Vec::new()));
        let mut bad = vec![1u8, 0, 0, 0, 4, 0, 0, 0];
        bad.extend_from_slice(&[0u8; 16]);
        let e = NiPosKey::read(&mut Reader::new(&bad)).unwrap_err();
        assert!(e.what.contains("key type 4"), "{e}");
    }
}
