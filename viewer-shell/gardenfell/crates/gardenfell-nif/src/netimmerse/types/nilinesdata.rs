//! `NiLinesData`: line geometry, one connectivity byte per vertex saying whether a
//! segment runs from it to the next. `tes3` nilinesdata.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiLinesData {
    pub base: NiGeometryData,
    /// One byte per vertex, no count of its own in the file.
    pub vertex_connectivity_flags: Vec<u8>,
}

impl Block for NiLinesData {
    const NAME: &'static str = "NiLinesData";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let base = NiGeometryData::read(r)?;
        let vertex_connectivity_flags = r.bytes(base.vertices.len())?.to_vec();
        Ok(NiLinesData { base, vertex_connectivity_flags })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)?;
        w.bytes(&self.vertex_connectivity_flags);
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
    fn lines_data_round_trips_and_is_cut_off_with_the_byte() {
        let mut d = NiLinesData::default();
        d.base.vertices = vec![[0.0; 3], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0]];
        d.vertex_connectivity_flags = vec![1, 1, 0];
        let mut w = Writer::new();
        d.write(&mut w).unwrap();
        assert_eq!(NiLinesData::read(&mut Reader::new(&w.out)).unwrap(), d);
        let e = NiLinesData::read(&mut Reader::new(&w.out[..w.out.len() - 1])).unwrap_err();
        assert!(e.what.contains("3 bytes wanted, 2 left"), "{e}");
    }
}
