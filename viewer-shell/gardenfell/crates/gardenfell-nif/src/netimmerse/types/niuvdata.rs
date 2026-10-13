//! `NiUVData`: four float tracks for a UV set, offset then tiling in U and V, each the
//! whole layout of an `NiFloatData` inline. `tes3` niuvdata.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiUVData {
    pub base: NiObject,
    pub u_offset_data: NiFloatData,
    pub v_offset_data: NiFloatData,
    pub u_tiling_data: NiFloatData,
    pub v_tiling_data: NiFloatData,
}

impl Block for NiUVData {
    const NAME: &'static str = "NiUVData";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiUVData {
            base: NiObject::read(r)?,
            u_offset_data: NiFloatData::read(r)?,
            v_offset_data: NiFloatData::read(r)?,
            u_tiling_data: NiFloatData::read(r)?,
            v_tiling_data: NiFloatData::read(r)?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)?;
        self.u_offset_data.write(w)?;
        self.v_offset_data.write(w)?;
        self.u_tiling_data.write(w)?;
        self.v_tiling_data.write(w)
    }

    fn links(&mut self, f: &mut dyn FnMut(&mut i32)) {
        self.base.links(f);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uv_data_round_trips() {
        let mut d = NiUVData::default();
        d.u_offset_data.keys = NiFloatKey::Lin(vec![
            NiLinFloatKey { time: 0.0, value: 0.0 },
            NiLinFloatKey { time: 1.0, value: 1.0 },
        ]);
        d.v_tiling_data.keys = NiFloatKey::Lin(vec![NiLinFloatKey { time: 0.0, value: 2.0 }]);
        let mut w = Writer::new();
        d.write(&mut w).unwrap();
        assert_eq!(w.out.len(), (4 + 4 + 16) + 4 + 4 + (4 + 4 + 8));
        assert_eq!(NiUVData::read(&mut Reader::new(&w.out)).unwrap(), d);
    }
}
