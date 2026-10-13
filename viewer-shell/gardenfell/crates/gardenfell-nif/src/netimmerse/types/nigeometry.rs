//! `NiGeometry`: a drawable object, pointing at its vertex data and, when skinned, its
//! skin instance. `tes3` nigeometry.rs.
//!
//! The geometry flag bits (`consts::geometry_flags::*`: compress vertices, normals, UV
//! sets, shadow) live in `base.flags`, the `NiAVObject` flag word.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiGeometry {
    pub base: NiAVObject,
    pub geometry_data: Link<NiGeometryData>,
    pub skin_instance: Link<NiSkinInstance>,
}

impl Block for NiGeometry {
    const NAME: &'static str = "NiGeometry";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiGeometry { base: NiAVObject::read(r)?, geometry_data: r.link()?, skin_instance: r.link()? })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)?;
        w.link(self.geometry_data);
        w.link(self.skin_instance);
        Ok(())
    }

    fn links(&mut self, f: &mut dyn FnMut(&mut i32)) {
        self.base.links(f);
        f(&mut self.geometry_data.index);
        f(&mut self.skin_instance.index);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_geometry_round_trips_with_its_two_links() {
        let mut g = NiGeometry::default();
        g.base.base.name = "Tri Grass".into();
        g.geometry_data = Link::new(3);
        g.skin_instance = Link::NONE;
        let mut w = Writer::new();
        g.write(&mut w).unwrap();
        assert_eq!(NiGeometry::read(&mut Reader::new(&w.out)).unwrap(), g);
        let e = NiGeometry::read(&mut Reader::new(&w.out[..w.out.len() - 2])).unwrap_err();
        assert!(e.at > 0, "{e}");
    }
}
