//! `NiTriShape`: a triangle list mesh, the shape nearly every Morrowind model is made of.
//! Nothing of its own, its data is the `NiTriShapeData` it links to. `tes3` nitrishape.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiTriShape {
    pub base: NiTriBasedGeom,
}

impl Block for NiTriShape {
    const NAME: &'static str = "NiTriShape";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiTriShape { base: NiTriBasedGeom::read(r)? })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)
    }

    fn links(&mut self, f: &mut dyn FnMut(&mut i32)) {
        self.base.links(f);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tri_shape_is_its_geometry_and_round_trips() {
        let mut s = NiTriShape::default();
        s.base.base.base.base.name = "Tri Shape".into();
        s.base.base.base.properties = vec![Link::new(2), Link::new(5)];
        s.base.base.geometry_data = Link::new(7);
        let mut w = Writer::new();
        s.write(&mut w).unwrap();
        let back = NiTriShape::read(&mut Reader::new(&w.out)).unwrap();
        assert_eq!(back, s);
        // Name, flags, translation, rotation, scale, velocity, property count and two
        // links, the has-bound word, then the two geometry links.
        assert_eq!(w.out.len(), 4 + 9 + 4 + 4 + 2 + 12 + 36 + 4 + 12 + 4 + 8 + 4 + 4 + 4);
    }
}
