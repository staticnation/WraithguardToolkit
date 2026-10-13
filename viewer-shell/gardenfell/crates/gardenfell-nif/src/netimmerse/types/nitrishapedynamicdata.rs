//! `NiTriShapeDynamicData`: triangle data whose vertex and triangle arrays are sized for a
//! maximum, with the counts in use trailing. `tes3` nitrishapedynamicdata.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiTriShapeDynamicData {
    pub base: NiTriShapeData,
    pub active_vertices: u16,
    pub active_triangles: u16,
}

impl Block for NiTriShapeDynamicData {
    const NAME: &'static str = "NiTriShapeDynamicData";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiTriShapeDynamicData {
            base: NiTriShapeData::read(r)?,
            active_vertices: r.u16()?,
            active_triangles: r.u16()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)?;
        w.u16(self.active_vertices);
        w.u16(self.active_triangles);
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
    fn dynamic_data_round_trips_with_its_active_counts() {
        let mut d = NiTriShapeDynamicData::default();
        d.base.base.base.vertices = vec![[0.0; 3]; 3];
        d.base.triangles = vec![[0, 1, 2]];
        d.active_vertices = 3;
        d.active_triangles = 1;
        let mut w = Writer::new();
        d.write(&mut w).unwrap();
        assert_eq!(NiTriShapeDynamicData::read(&mut Reader::new(&w.out)).unwrap(), d);
        let e = NiTriShapeDynamicData::read(&mut Reader::new(&w.out[..w.out.len() - 1])).unwrap_err();
        assert!(e.what.contains("2 bytes wanted"), "{e}");
    }
}
