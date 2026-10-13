//! `NiTriShapeData`: a triangle list and its shared-normal groups. `tes3` nitrishapedata.rs.
//!
//! The file holds a `u16` triangle count and a `u32` count of triangle points (three per
//! triangle); both come from `triangles.len()` on the way out, so a file whose two counts
//! disagree is the one case this does not write back exactly.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiTriShapeData {
    pub base: NiTriBasedGeomData,
    pub triangles: Vec<[u16; 3]>,
    /// Groups of vertex indices that share a normal.
    pub shared_normals: Vec<Vec<u16>>,
}

impl Block for NiTriShapeData {
    const NAME: &'static str = "NiTriShapeData";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let base = NiTriBasedGeomData::read(r)?;
        let _num_triangles = r.u16()?;
        let num_points = r.count(2)?;
        let triangles = r.seq(num_points / 3, |r| Ok([r.u16()?, r.u16()?, r.u16()?]))?;
        let num_groups = r.u16()? as usize;
        let shared_normals = r.seq(num_groups, |r| {
            let n = r.u16()? as usize;
            r.u16s_n(n)
        })?;
        Ok(NiTriShapeData { base, triangles, shared_normals })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)?;
        w.u16(self.triangles.len() as u16);
        w.count(self.triangles.len() * 3);
        for t in &self.triangles {
            w.u16s(t);
        }
        w.u16(self.shared_normals.len() as u16);
        for g in &self.shared_normals {
            w.u16(g.len() as u16);
            w.u16s(g);
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
    fn a_tri_shape_data_round_trips_and_a_bad_point_count_is_refused() {
        let mut d = NiTriShapeData::default();
        d.base.base.vertices = vec![[0.0; 3]; 4];
        d.triangles = vec![[0, 1, 2], [0, 2, 3]];
        d.shared_normals = vec![vec![0, 3], vec![1]];
        let mut w = Writer::new();
        d.write(&mut w).unwrap();
        assert_eq!(NiTriShapeData::read(&mut Reader::new(&w.out)).unwrap(), d);
        // Point the u32 count past the end.
        let mut bad = w.out.clone();
        let at = 2 + 4 + 4 + 16 + 4 + 2 + 4 + 4 * 12 + 2;
        bad[at..at + 4].copy_from_slice(&1_000_000u32.to_le_bytes());
        let e = NiTriShapeData::read(&mut Reader::new(&bad)).unwrap_err();
        assert!(e.what.contains("1000000 items"), "{e}");
    }
}
