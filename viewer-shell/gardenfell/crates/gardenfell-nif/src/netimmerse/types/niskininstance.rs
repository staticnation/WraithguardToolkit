//! `NiSkinInstance`: what a skinned geometry hangs its skin on, the skin data, the skeleton
//! root and the bones in the order the skin data counts them. `tes3` niskininstance.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiSkinInstance {
    pub base: NiObject,
    pub data: Link<NiSkinData>,
    pub root: Link<NiAVObject>,
    pub bones: Vec<Link<NiAVObject>>,
}

impl Block for NiSkinInstance {
    const NAME: &'static str = "NiSkinInstance";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiSkinInstance { base: NiObject::read(r)?, data: r.link()?, root: r.link()?, bones: r.links()? })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)?;
        w.link(self.data);
        w.link(self.root);
        w.links(&self.bones);
        Ok(())
    }

    fn links(&mut self, f: &mut dyn FnMut(&mut i32)) {
        self.base.links(f);
        f(&mut self.data.index);
        f(&mut self.root.index);
        for l in &mut self.bones {
            f(&mut l.index);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_skin_instance_round_trips_and_a_bad_bone_count_is_refused() {
        let s = NiSkinInstance {
            data: Link::new(4),
            root: Link::new(0),
            bones: vec![Link::new(1), Link::new(2)],
            ..Default::default()
        };
        let mut w = Writer::new();
        s.write(&mut w).unwrap();
        assert_eq!(w.out.len(), 4 + 4 + 4 + 8);
        assert_eq!(NiSkinInstance::read(&mut Reader::new(&w.out)).unwrap(), s);
        let mut bad = w.out.clone();
        bad[8..12].copy_from_slice(&3u32.to_le_bytes());
        let e = NiSkinInstance::read(&mut Reader::new(&bad)).unwrap_err();
        assert!(e.what.contains("3 items of 4 bytes"), "{e}");
    }
}
