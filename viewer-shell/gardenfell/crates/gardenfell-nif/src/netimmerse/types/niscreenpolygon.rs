//! `NiScreenPolygon`: a polygon drawn in screen space, with its optional UVs and colors
//! and the render states it draws with. `tes3` niscreenpolygon.rs.
//!
//! The layout: a `u16` vertex count and the vertices, then two has-flag words each
//! followed by an array of `vertices.len()` when set, then a `u32` count of property
//! states. As in `NiGeometryData`, the has-flags are the array lengths here.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiScreenPolygon {
    pub base: NiObject,
    pub vertices: Vec<[f32; 3]>,
    /// Empty, or one per vertex.
    pub uv_coords: Vec<[f32; 2]>,
    /// Empty, or one RGBA per vertex.
    pub vertex_colors: Vec<[f32; 4]>,
    pub property_states: Vec<i32>,
}

impl Block for NiScreenPolygon {
    const NAME: &'static str = "NiScreenPolygon";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let base = NiObject::read(r)?;
        let num_vertices = r.u16()? as usize;
        let vertices = r.vec3s_n(num_vertices)?;
        let has_uv_coords = r.bool_u32()?;
        let uv_coords = r.vec2s_n(if has_uv_coords { num_vertices } else { 0 })?;
        let has_vertex_colors = r.bool_u32()?;
        let vertex_colors = r.vec4s_n(if has_vertex_colors { num_vertices } else { 0 })?;
        let property_states = r.counted(4, |r| r.i32())?;
        Ok(NiScreenPolygon { base, vertices, uv_coords, vertex_colors, property_states })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)?;
        w.u16(self.vertices.len() as u16);
        w.vec3s(&self.vertices);
        w.bool_u32(!self.uv_coords.is_empty());
        w.vec2s(&self.uv_coords);
        w.bool_u32(!self.vertex_colors.is_empty());
        w.vec4s(&self.vertex_colors);
        w.count(self.property_states.len());
        w.i32s(&self.property_states);
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
    fn a_screen_polygon_round_trips_and_a_bad_state_count_is_refused() {
        let p = NiScreenPolygon {
            vertices: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0]],
            uv_coords: vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]],
            vertex_colors: Vec::new(),
            property_states: vec![3, 4],
            ..Default::default()
        };
        let mut w = Writer::new();
        p.write(&mut w).unwrap();
        let at = 2 + 3 * 12 + 4 + 3 * 8 + 4;
        assert_eq!(w.out.len(), at + 4 + 8);
        assert_eq!(NiScreenPolygon::read(&mut Reader::new(&w.out)).unwrap(), p);
        let mut bad = w.out.clone();
        bad[at..at + 4].copy_from_slice(&1_000_000u32.to_le_bytes());
        let e = NiScreenPolygon::read(&mut Reader::new(&bad)).unwrap_err();
        assert!(e.what.contains("1000000 items"), "{e}");
    }
}
