//! `NiNode`: a scene graph node with children and dynamic effects. `tes3` ninode.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiNode {
    pub base: NiAVObject,
    pub children: Vec<Link<NiAVObject>>,
    pub effects: Vec<Link<NiDynamicEffect>>,
}

impl Block for NiNode {
    const NAME: &'static str = "NiNode";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiNode { base: NiAVObject::read(r)?, children: r.links()?, effects: r.links()? })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)?;
        w.links(&self.children);
        w.links(&self.effects);
        Ok(())
    }

    fn links(&mut self, f: &mut dyn FnMut(&mut i32)) {
        self.base.links(f);
        for l in &mut self.children {
            f(&mut l.index);
        }
        for l in &mut self.effects {
            f(&mut l.index);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_node_round_trips_with_its_bounding_volume() {
        let mut n = NiNode::default();
        n.base.base.name = "root".into();
        n.base.translation = [1.0, 2.0, 3.0];
        n.base.bounding_volume = Some(NiBoundingVolume::Sphere(NiSphereBV { center: [0.0; 3], radius: 5.0 }));
        n.children = vec![Link::new(1), Link::NONE];
        let mut w = Writer::new();
        n.write(&mut w).unwrap();
        let back = NiNode::read(&mut Reader::new(&w.out)).unwrap();
        assert_eq!(back, n);
        // A truncated block is refused with the byte, not read as zeros.
        let e = NiNode::read(&mut Reader::new(&w.out[..w.out.len() - 3])).unwrap_err();
        assert!(e.at > 0, "{e}");
    }
}
