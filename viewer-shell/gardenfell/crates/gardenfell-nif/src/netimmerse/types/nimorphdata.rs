//! `NiMorphData`: the targets of a morph animation, each a float track of weights and one
//! position per vertex of the geometry. `tes3` nimorphdata.rs.
//!
//! The layout: a `u32` target count, a `u32` vertex count shared by every target, one
//! byte for relative targets, then the targets. Unlike the key tracks elsewhere, a
//! target's type word (`consts::key_type::*`) is always present, even when the key count
//! is zero. With zero keys any type word is accepted and the track is an empty linear
//! one, as `tes3` reads it, and it is written back as linear.

use super::*;

#[derive(Clone, Debug, PartialEq)]
pub struct NiMorphData {
    pub base: NiObject,
    /// One byte in the file. The game's files set it.
    pub relative_targets: bool,
    /// Every target has the same number of vertices, written as the shared count.
    pub targets: Vec<MorphTarget>,
}

impl Default for NiMorphData {
    fn default() -> Self {
        NiMorphData { base: NiObject, relative_targets: true, targets: Vec::new() }
    }
}

/// One morph target: its weight track and its vertex positions. Not a block of its own.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MorphTarget {
    pub keys: NiFloatKey,
    pub vertices: Vec<[f32; 3]>,
}

impl MorphTarget {
    /// Reads a target whose vertex count the parent has already read.
    pub fn read(r: &mut Reader<'_>, num_vertices: usize) -> Result<MorphTarget> {
        let n = r.count(8)?;
        let at = r.pos();
        let kind = r.i32()?;
        let keys = match kind {
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
            // Allowed only when there are no keys, as tes3 has it.
            _ if n == 0 => NiFloatKey::Lin(Vec::new()),
            _ => {
                return Err(crate::bytes::Error {
                    at,
                    what: format!("morph target key type {kind} is not linear, Bezier or TCB"),
                });
            }
        };
        let vertices = r.vec3s_n(num_vertices)?;
        Ok(MorphTarget { keys, vertices })
    }

    pub fn write(&self, w: &mut Writer) -> Result<()> {
        w.count(self.keys.len());
        match &self.keys {
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
        w.vec3s(&self.vertices);
        Ok(())
    }
}

impl Block for NiMorphData {
    const NAME: &'static str = "NiMorphData";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let base = NiObject::read(r)?;
        // A target is at least its key count and type word.
        let num_targets = r.count(8)?;
        let num_vertices = r.u32()? as usize;
        if num_targets.saturating_mul(8 + num_vertices.saturating_mul(12)) > r.remaining() {
            return r.error(format!(
                "{num_targets} morph targets of {num_vertices} vertices with {} bytes left",
                r.remaining()
            ));
        }
        let relative_targets = r.bool_u8()?;
        let targets = r.seq(num_targets, |r| MorphTarget::read(r, num_vertices))?;
        Ok(NiMorphData { base, relative_targets, targets })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)?;
        w.count(self.targets.len());
        w.count(self.targets.first().map_or(0, |t| t.vertices.len()));
        w.bool_u8(self.relative_targets);
        for t in &self.targets {
            t.write(w)?;
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
    fn morph_data_round_trips_and_a_bad_vertex_count_is_refused() {
        let d = NiMorphData {
            relative_targets: true,
            targets: vec![
                MorphTarget { keys: NiFloatKey::Lin(Vec::new()), vertices: vec![[0.0; 3], [1.0; 3]] },
                MorphTarget {
                    keys: NiFloatKey::Lin(vec![
                        NiLinFloatKey { time: 0.0, value: 0.0 },
                        NiLinFloatKey { time: 1.0, value: 1.0 },
                    ]),
                    vertices: vec![[0.5; 3], [2.0; 3]],
                },
            ],
            ..Default::default()
        };
        let mut w = Writer::new();
        d.write(&mut w).unwrap();
        assert_eq!(w.out.len(), 4 + 4 + 1 + (4 + 4 + 24) + (4 + 4 + 16 + 24));
        assert_eq!(NiMorphData::read(&mut Reader::new(&w.out)).unwrap(), d);
        // A zero-key target with a type word of 0 reads as an empty linear track.
        let mut tolerant = w.out.clone();
        tolerant[13..17].copy_from_slice(&0i32.to_le_bytes());
        assert_eq!(NiMorphData::read(&mut Reader::new(&tolerant)).unwrap(), d);
        // A vertex count no target could hold.
        let mut bad = w.out.clone();
        bad[4..8].copy_from_slice(&1_000_000u32.to_le_bytes());
        let e = NiMorphData::read(&mut Reader::new(&bad)).unwrap_err();
        assert!(e.what.contains("1000000 vertices"), "{e}");
        // A key type the layout does not have, on a target with keys.
        let mut bad = w.out.clone();
        bad[45..49].copy_from_slice(&7i32.to_le_bytes());
        let e = NiMorphData::read(&mut Reader::new(&bad)).unwrap_err();
        assert!(e.what.contains("key type 7"), "{e}");
    }
}
