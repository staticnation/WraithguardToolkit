//! `NiSkinData`: the bind-pose transforms of a skinned mesh, the skin's own and one per
//! bone with that bone's vertex weights. `tes3` niskindata.rs.
//!
//! The file puts the bone count before the skin partition link, so the count is read,
//! then the link, then the bones. A skin transform is rotation, translation, scale, the
//! other order from an `NiAVObject`'s.

use super::*;

#[derive(Clone, Debug, PartialEq)]
pub struct NiSkinData {
    pub base: NiObject,
    /// The file's 3x3, row by row.
    pub rotation: [[f32; 3]; 3],
    pub translation: [f32; 3],
    pub scale: f32,
    pub skin_partition: Link<NiSkinPartition>,
    /// One per bone, in the order of the skin instance's bone links.
    pub bone_data: Vec<BoneData>,
}

impl Default for NiSkinData {
    fn default() -> Self {
        NiSkinData {
            base: NiObject,
            rotation: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            translation: [0.0; 3],
            scale: 1.0,
            skin_partition: Link::NONE,
            bone_data: Vec::new(),
        }
    }
}

impl Block for NiSkinData {
    const NAME: &'static str = "NiSkinData";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let base = NiObject::read(r)?;
        let rotation = r.mat3()?;
        let translation = r.vec3()?;
        let scale = r.f32()?;
        let num_bone_data = r.count(BoneData::MIN_SIZE)?;
        let skin_partition = r.link()?;
        let bone_data = r.seq(num_bone_data, BoneData::read)?;
        Ok(NiSkinData { base, rotation, translation, scale, skin_partition, bone_data })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)?;
        w.mat3(&self.rotation);
        w.vec3(&self.translation);
        w.f32(self.scale);
        w.count(self.bone_data.len());
        w.link(self.skin_partition);
        for b in &self.bone_data {
            b.write(w);
        }
        Ok(())
    }

    fn links(&mut self, f: &mut dyn FnMut(&mut i32)) {
        self.base.links(f);
        f(&mut self.skin_partition.index);
    }
}

/// One bone's bind-pose transform, bound and vertex weights, as `NiSkinData` holds it.
#[derive(Clone, Debug, PartialEq)]
pub struct BoneData {
    /// The file's 3x3, row by row.
    pub rotation: [[f32; 3]; 3],
    pub translation: [f32; 3],
    pub scale: f32,
    pub bound: NiBound,
    /// Vertex index and weight pairs, a `u16` count of them in the file.
    pub vertex_weights: Vec<(u16, f32)>,
}

impl Default for BoneData {
    fn default() -> Self {
        BoneData {
            rotation: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            translation: [0.0; 3],
            scale: 1.0,
            bound: NiBound::default(),
            vertex_weights: Vec::new(),
        }
    }
}

impl BoneData {
    /// A bone with no weights: the transform, the bound and the weight count.
    pub const MIN_SIZE: usize = 36 + 12 + 4 + NiBound::SIZE + 2;

    pub fn read(r: &mut Reader<'_>) -> Result<BoneData> {
        let rotation = r.mat3()?;
        let translation = r.vec3()?;
        let scale = r.f32()?;
        let bound = NiBound::read(r)?;
        let num_vertex_weights = r.u16()? as usize;
        let vertex_weights = r.seq(num_vertex_weights, |r| Ok((r.u16()?, r.f32()?)))?;
        Ok(BoneData { rotation, translation, scale, bound, vertex_weights })
    }

    pub fn write(&self, w: &mut Writer) {
        w.mat3(&self.rotation);
        w.vec3(&self.translation);
        w.f32(self.scale);
        self.bound.write(w);
        w.u16(self.vertex_weights.len() as u16);
        for (index, weight) in &self.vertex_weights {
            w.u16(*index);
            w.f32(*weight);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skin_data_round_trips_and_a_bone_count_past_the_end_is_refused() {
        let s = NiSkinData {
            translation: [0.0, 0.0, 10.0],
            skin_partition: Link::new(9),
            bone_data: vec![
                BoneData {
                    translation: [1.0, 2.0, 3.0],
                    bound: NiBound { center: [1.0, 2.0, 3.0], radius: 0.5 },
                    vertex_weights: vec![(0, 1.0), (1, 0.25)],
                    ..Default::default()
                },
                BoneData::default(),
            ],
            ..Default::default()
        };
        let mut w = Writer::new();
        s.write(&mut w).unwrap();
        let back = NiSkinData::read(&mut Reader::new(&w.out)).unwrap();
        assert_eq!(back, s);
        assert_eq!(w.out.len(), 36 + 12 + 4 + 4 + 4 + 2 * BoneData::MIN_SIZE + 2 * 6);
        let mut bad = w.out.clone();
        bad[52..56].copy_from_slice(&1000u32.to_le_bytes());
        let e = NiSkinData::read(&mut Reader::new(&bad)).unwrap_err();
        assert!(e.what.contains("1000 items"), "{e}");
    }
}
