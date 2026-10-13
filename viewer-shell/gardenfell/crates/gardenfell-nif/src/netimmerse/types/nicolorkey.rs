//! `NiColorKey`: an RGBA color track. `tes3` nicolorkey.rs has only the linear shape;
//! nif.xml's `KeyGroup<Color4>` and Gamebryo's `NiBezColorKey` and `NiTCBColorKey` have the
//! other two, and round 18bg read them (two tangents of a color each for Bezier, tension,
//! continuity and bias for TCB), so they are here too (18gh). The type word
//! (`consts::key_type::*`) follows the count. Carried by `NiColorData`.
//!
//! The count comes first; when it is zero no type word follows.

use super::*;

#[derive(Clone, Debug, PartialEq)]
pub enum NiColorKey {
    Lin(Vec<NiLinColKey>),
    Bez(Vec<NiBezColKey>),
    Tcb(Vec<NiTcbColKey>),
}

impl Default for NiColorKey {
    fn default() -> Self {
        NiColorKey::Lin(Vec::new())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NiLinColKey {
    pub time: f32,
    /// RGBA.
    pub value: [f32; 4],
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NiBezColKey {
    pub time: f32,
    pub value: [f32; 4],
    pub in_tan: [f32; 4],
    pub out_tan: [f32; 4],
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NiTcbColKey {
    pub time: f32,
    pub value: [f32; 4],
    pub tension: f32,
    pub continuity: f32,
    pub bias: f32,
}

impl NiColorKey {
    pub fn len(&self) -> usize {
        match self {
            NiColorKey::Lin(k) => k.len(),
            NiColorKey::Bez(k) => k.len(),
            NiColorKey::Tcb(k) => k.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn read(r: &mut Reader<'_>) -> Result<NiColorKey> {
        let n = r.count(20)?;
        if n == 0 {
            return Ok(NiColorKey::Lin(Vec::new()));
        }
        let at = r.pos();
        let kind = r.i32()?;
        Ok(match kind {
            consts::key_type::LIN => {
                NiColorKey::Lin(r.seq(n, |r| Ok(NiLinColKey { time: r.f32()?, value: r.vec4()? }))?)
            }
            consts::key_type::BEZ => NiColorKey::Bez(r.seq(n, |r| {
                Ok(NiBezColKey { time: r.f32()?, value: r.vec4()?, in_tan: r.vec4()?, out_tan: r.vec4()? })
            })?),
            consts::key_type::TCB => NiColorKey::Tcb(r.seq(n, |r| {
                Ok(NiTcbColKey {
                    time: r.f32()?,
                    value: r.vec4()?,
                    tension: r.f32()?,
                    continuity: r.f32()?,
                    bias: r.f32()?,
                })
            })?),
            _ => {
                return Err(crate::bytes::Error {
                    at,
                    what: format!("color key type {kind} is not linear, Bezier or TCB"),
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
            NiColorKey::Lin(keys) => {
                w.i32(consts::key_type::LIN);
                for k in keys {
                    w.f32(k.time);
                    w.vec4(&k.value);
                }
            }
            NiColorKey::Bez(keys) => {
                w.i32(consts::key_type::BEZ);
                for k in keys {
                    w.f32(k.time);
                    w.vec4(&k.value);
                    w.vec4(&k.in_tan);
                    w.vec4(&k.out_tan);
                }
            }
            NiColorKey::Tcb(keys) => {
                w.i32(consts::key_type::TCB);
                for k in keys {
                    w.f32(k.time);
                    w.vec4(&k.value);
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
    fn a_color_track_round_trips_and_another_key_shape_is_refused() {
        let keys = NiColorKey::Lin(vec![
            NiLinColKey { time: 0.0, value: [1.0, 0.0, 0.0, 1.0] },
            NiLinColKey { time: 2.0, value: [0.0, 0.0, 1.0, 0.5] },
        ]);
        let mut w = Writer::new();
        keys.write(&mut w).unwrap();
        assert_eq!(w.out.len(), 4 + 4 + 2 * 20);
        assert_eq!(NiColorKey::read(&mut Reader::new(&w.out)).unwrap(), keys);
        let mut w = Writer::new();
        NiColorKey::default().write(&mut w).unwrap();
        assert_eq!(w.out, vec![0, 0, 0, 0]);
        for keys in [
            NiColorKey::Bez(vec![NiBezColKey { time: 1.0, value: [1.0; 4], in_tan: [0.5; 4], out_tan: [0.25; 4] }]),
            NiColorKey::Tcb(vec![NiTcbColKey { time: 1.0, value: [1.0; 4], tension: 0.1, continuity: 0.2, bias: 0.3 }]),
        ] {
            let mut w = Writer::new();
            keys.write(&mut w).unwrap();
            assert_eq!(NiColorKey::read(&mut Reader::new(&w.out)).unwrap(), keys);
        }
        // Type 4 is not a key shape the format has.
        let mut bad = vec![1u8, 0, 0, 0, 4, 0, 0, 0];
        bad.extend_from_slice(&[0u8; 20]);
        let e = NiColorKey::read(&mut Reader::new(&bad)).unwrap_err();
        assert!(e.what.contains("key type 4"), "{e}");
    }
}
