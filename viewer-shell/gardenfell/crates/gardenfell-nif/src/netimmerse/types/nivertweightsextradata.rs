//! `NiVertWeightsExtraData`: one weight per vertex, hung on a shape by old exporters.
//! `tes3` nivertweightsextradata.rs.
//!
//! The count is a `u16`, not the format's usual `u32`, so it is checked against the bytes
//! left by hand rather than through `count`.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiVertWeightsExtraData {
    pub base: NiExtraData,
    pub weights: Vec<f32>,
}

impl Block for NiVertWeightsExtraData {
    const NAME: &'static str = "NiVertWeightsExtraData";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let base = NiExtraData::read(r)?;
        let at = r.pos();
        let num_weights = r.u16()? as usize;
        if num_weights * 4 > r.remaining() {
            return Err(crate::bytes::Error {
                at,
                what: format!("a count of {num_weights} items of 4 bytes with {} bytes left", r.remaining()),
            });
        }
        let weights = r.f32s_n(num_weights)?;
        Ok(NiVertWeightsExtraData { base, weights })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)?;
        w.u16(self.weights.len() as u16);
        w.f32s(&self.weights);
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
    fn vertex_weights_round_trip_and_a_bad_count_is_refused() {
        let x = NiVertWeightsExtraData { weights: vec![1.0, 0.5, 0.25], ..Default::default() };
        let mut w = Writer::new();
        x.write(&mut w).unwrap();
        assert_eq!(w.out.len(), 8 + 2 + 12);
        assert_eq!(NiVertWeightsExtraData::read(&mut Reader::new(&w.out)).unwrap(), x);
        let mut bad = w.out.clone();
        bad[8..10].copy_from_slice(&4u16.to_le_bytes());
        let e = NiVertWeightsExtraData::read(&mut Reader::new(&bad)).unwrap_err();
        assert!(e.what.contains("4 items"), "{e}");
        assert_eq!(e.at, 8);
    }
}
