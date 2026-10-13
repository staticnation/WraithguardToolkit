//! `NiGeometryData`: the vertex arrays every shape and particle system draws from.
//! `tes3` nigeometrydata.rs.
//!
//! The layout: a `u16` vertex count, then four has-flag words each followed by its array
//! when set (positions, normals, after the bound the colors, then a `u16` UV set count
//! and the sets). Every array is `vertices.len()` long, so the has-flags are the lengths
//! here: an empty array is written as "has none".

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiGeometryData {
    pub base: NiObject,
    pub vertices: Vec<[f32; 3]>,
    /// Empty, or one per vertex.
    pub normals: Vec<[f32; 3]>,
    pub bound: NiBound,
    /// Empty, or one RGBA per vertex.
    pub vertex_colors: Vec<[f32; 4]>,
    /// The UV sets one after another, each `vertices.len()` long.
    pub uv_sets: Vec<[f32; 2]>,
}

impl Block for NiGeometryData {
    const NAME: &'static str = "NiGeometryData";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let base = NiObject::read(r)?;
        let num_vertices = r.u16()? as usize;
        let has_vertices = r.bool_u32()?;
        let vertices = r.vec3s_n(if has_vertices { num_vertices } else { 0 })?;
        let has_normals = r.bool_u32()?;
        let normals = r.vec3s_n(if has_normals { num_vertices } else { 0 })?;
        let bound = NiBound::read(r)?;
        let has_colors = r.bool_u32()?;
        let vertex_colors = r.vec4s_n(if has_colors { num_vertices } else { 0 })?;
        let num_uv_sets = r.u16()? as usize;
        let has_uv = r.bool_u32()?;
        let uv_sets = r.vec2s_n(if has_uv { num_vertices * num_uv_sets } else { 0 })?;
        Ok(NiGeometryData { base, vertices, normals, bound, vertex_colors, uv_sets })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)?;
        w.u16(self.vertices.len() as u16);
        w.bool_u32(!self.vertices.is_empty());
        w.vec3s(&self.vertices);
        w.bool_u32(!self.normals.is_empty());
        w.vec3s(&self.normals);
        self.bound.write(w);
        w.bool_u32(!self.vertex_colors.is_empty());
        w.vec4s(&self.vertex_colors);
        w.u16(self.num_uv_sets() as u16);
        w.bool_u32(!self.uv_sets.is_empty());
        w.vec2s(&self.uv_sets);
        Ok(())
    }

    fn links(&mut self, f: &mut dyn FnMut(&mut i32)) {
        self.base.links(f);
    }
}

impl NiGeometryData {
    pub fn num_uv_sets(&self) -> usize {
        if self.vertices.is_empty() {
            0
        } else {
            self.uv_sets.len() / self.vertices.len()
        }
    }

    pub fn uv_set(&self, index: usize) -> Option<&[[f32; 2]]> {
        let start = index * self.vertices.len();
        self.uv_sets.get(start..start + self.vertices.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn geometry_data_round_trips_with_and_without_its_arrays() {
        let g = NiGeometryData {
            vertices: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            normals: vec![[0.0, 0.0, 1.0]; 3],
            bound: NiBound { center: [0.5, 0.5, 0.0], radius: 0.7 },
            vertex_colors: Vec::new(),
            uv_sets: vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
            ..Default::default()
        };
        assert_eq!(g.num_uv_sets(), 2);
        let mut w = Writer::new();
        g.write(&mut w).unwrap();
        assert_eq!(NiGeometryData::read(&mut Reader::new(&w.out)).unwrap(), g);
        let empty = NiGeometryData::default();
        let mut w = Writer::new();
        empty.write(&mut w).unwrap();
        assert_eq!(w.out.len(), 2 + 4 + 4 + 16 + 4 + 2 + 4);
        assert_eq!(NiGeometryData::read(&mut Reader::new(&w.out)).unwrap(), empty);
    }
}
