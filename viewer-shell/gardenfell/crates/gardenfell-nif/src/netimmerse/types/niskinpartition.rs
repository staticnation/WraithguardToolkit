//! `NiSkinPartition`: a skinned mesh split into partitions that each use few enough bones
//! for the hardware, every partition with its own vertex, weight and triangle lists.
//! `tes3` niskinpartition.rs.
//!
//! A partition's five `u16` counts come first, then the arrays they size. The weights and
//! the optional bone palette are `vertex_indices.len() * num_bones_per_vertex` long; both
//! are written as held, as `tes3` does, so a mismatched struct writes a file that does not
//! read back.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiSkinPartition {
    pub base: NiObject,
    pub partitions: Vec<Partition>,
}

impl Block for NiSkinPartition {
    const NAME: &'static str = "NiSkinPartition";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let base = NiObject::read(r)?;
        let partitions = r.counted(Partition::MIN_SIZE, Partition::read)?;
        Ok(NiSkinPartition { base, partitions })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)?;
        w.count(self.partitions.len());
        for p in &self.partitions {
            p.write(w);
        }
        Ok(())
    }

    fn links(&mut self, f: &mut dyn FnMut(&mut i32)) {
        self.base.links(f);
    }
}

/// One partition of an `NiSkinPartition`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Partition {
    pub num_bones_per_vertex: u16,
    /// Indices into the skin instance's bone list.
    pub bones: Vec<u16>,
    /// Indices into the geometry's vertices.
    pub vertex_indices: Vec<u16>,
    /// `num_bones_per_vertex` weights per vertex, in vertex order.
    pub weights: Vec<f32>,
    pub triangles: Vec<[u16; 3]>,
    pub strip_lengths: Vec<u16>,
    /// Every strip's indices in order, `strip_lengths` summed long.
    pub strips: Vec<u16>,
    /// `num_bones_per_vertex` bone slots per vertex when the has-palette byte is set.
    pub bone_palette: Option<Vec<u8>>,
}

impl Partition {
    /// A partition with nothing in it: the five counts and the has-palette byte.
    pub const MIN_SIZE: usize = 5 * 2 + 1;

    pub fn read(r: &mut Reader<'_>) -> Result<Partition> {
        let num_vertices = r.u16()? as usize;
        let num_triangles = r.u16()? as usize;
        let num_bones = r.u16()? as usize;
        let num_strip_lengths = r.u16()? as usize;
        let num_bones_per_vertex = r.u16()?;
        let num_weights = num_vertices * num_bones_per_vertex as usize;
        let bones = r.u16s_n(num_bones)?;
        let vertex_indices = r.u16s_n(num_vertices)?;
        let weights = r.f32s_n(num_weights)?;
        let triangles = r.seq(num_triangles, |r| Ok([r.u16()?, r.u16()?, r.u16()?]))?;
        let strip_lengths = r.u16s_n(num_strip_lengths)?;
        let total: usize = strip_lengths.iter().map(|n| *n as usize).sum();
        let strips = r.u16s_n(total)?;
        let bone_palette = if r.bool_u8()? { Some(r.bytes(num_weights)?.to_vec()) } else { None };
        Ok(Partition {
            num_bones_per_vertex,
            bones,
            vertex_indices,
            weights,
            triangles,
            strip_lengths,
            strips,
            bone_palette,
        })
    }

    pub fn write(&self, w: &mut Writer) {
        w.u16(self.vertex_indices.len() as u16);
        w.u16(self.triangles.len() as u16);
        w.u16(self.bones.len() as u16);
        w.u16(self.strip_lengths.len() as u16);
        w.u16(self.num_bones_per_vertex);
        w.u16s(&self.bones);
        w.u16s(&self.vertex_indices);
        w.f32s(&self.weights);
        for t in &self.triangles {
            w.u16s(t);
        }
        w.u16s(&self.strip_lengths);
        w.u16s(&self.strips);
        w.bool_u8(self.bone_palette.is_some());
        if let Some(p) = &self.bone_palette {
            w.bytes(p);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_skin_partition_round_trips_and_a_bad_partition_count_is_refused() {
        let s = NiSkinPartition {
            partitions: vec![
                Partition {
                    num_bones_per_vertex: 2,
                    bones: vec![0, 3],
                    vertex_indices: vec![0, 1, 2],
                    weights: vec![1.0, 0.0, 0.5, 0.5, 0.0, 1.0],
                    triangles: vec![[0, 1, 2]],
                    strip_lengths: Vec::new(),
                    strips: Vec::new(),
                    bone_palette: Some(vec![0, 1, 0, 1, 0, 1]),
                },
                Partition {
                    num_bones_per_vertex: 1,
                    strip_lengths: vec![3],
                    strips: vec![0, 1, 2],
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let mut w = Writer::new();
        s.write(&mut w).unwrap();
        assert_eq!(NiSkinPartition::read(&mut Reader::new(&w.out)).unwrap(), s);
        let mut bad = w.out.clone();
        bad[0..4].copy_from_slice(&1_000_000u32.to_le_bytes());
        let e = NiSkinPartition::read(&mut Reader::new(&bad)).unwrap_err();
        assert!(e.what.contains("1000000 items of 11 bytes"), "{e}");
    }
}
