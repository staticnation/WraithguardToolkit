//! `NiTriStripsData`: triangle strips, every strip's length and then all their indices
//! one after another. `tes3` nitristripsdata.rs.
//!
//! The triangle count is the file's own word and not derived: a strip of `n` indices
//! makes `n - 2` triangles, but the file carries the total and we keep what it says.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiTriStripsData {
    pub base: NiTriBasedGeomData,
    pub num_triangles: u16,
    /// One length per strip, a `u16` count of them in the file.
    pub strip_lengths: Vec<u16>,
    /// Every strip's indices in order, `strip_lengths` summed long.
    pub strips: Vec<u16>,
}

impl Block for NiTriStripsData {
    const NAME: &'static str = "NiTriStripsData";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let base = NiTriBasedGeomData::read(r)?;
        let num_triangles = r.u16()?;
        let num_strip_lengths = r.u16()? as usize;
        let strip_lengths = r.u16s_n(num_strip_lengths)?;
        let total: usize = strip_lengths.iter().map(|n| *n as usize).sum();
        let strips = r.u16s_n(total)?;
        Ok(NiTriStripsData { base, num_triangles, strip_lengths, strips })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)?;
        w.u16(self.num_triangles);
        w.u16(self.strip_lengths.len() as u16);
        w.u16s(&self.strip_lengths);
        w.u16s(&self.strips);
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
    fn strips_round_trip_and_a_length_past_the_end_is_refused() {
        let mut d = NiTriStripsData::default();
        d.base.base.vertices = vec![[0.0; 3]; 5];
        d.num_triangles = 4;
        d.strip_lengths = vec![4, 3];
        d.strips = vec![0, 1, 2, 3, 2, 3, 4];
        let mut w = Writer::new();
        d.write(&mut w).unwrap();
        assert_eq!(NiTriStripsData::read(&mut Reader::new(&w.out)).unwrap(), d);
        // The first strip length claims more indices than the file has.
        let mut bad = w.out.clone();
        let at = 2 + 4 + 5 * 12 + 4 + 16 + 4 + 2 + 4 + 2 + 2;
        bad[at..at + 2].copy_from_slice(&60000u16.to_le_bytes());
        let e = NiTriStripsData::read(&mut Reader::new(&bad)).unwrap_err();
        assert!(e.what.contains("bytes wanted"), "{e}");
    }
}
