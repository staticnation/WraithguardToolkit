//! `NiLODNode`: a switch node that picks a child by distance from `lod_center`, one
//! near-far range per child. `tes3` nilodnode.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiLODNode {
    pub base: NiSwitchNode,
    pub lod_center: [f32; 3],
    /// One `[near, far]` per child, in the children's order.
    pub lod_levels: Vec<[f32; 2]>,
}

impl Block for NiLODNode {
    const NAME: &'static str = "NiLODNode";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let base = NiSwitchNode::read(r)?;
        let lod_center = r.vec3()?;
        let num_lod_levels = r.count(8)?;
        let lod_levels = r.vec2s_n(num_lod_levels)?;
        Ok(NiLODNode { base, lod_center, lod_levels })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)?;
        w.vec3(&self.lod_center);
        w.count(self.lod_levels.len());
        w.vec2s(&self.lod_levels);
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
    fn a_lod_node_round_trips_and_a_bad_level_count_is_refused() {
        let mut n = NiLODNode {
            lod_center: [1.0, 2.0, 3.0],
            lod_levels: vec![[0.0, 100.0], [100.0, 500.0], [500.0, 10000.0]],
            ..Default::default()
        };
        n.base.base.children = vec![Link::new(1), Link::new(2), Link::new(3)];
        let mut w = Writer::new();
        n.write(&mut w).unwrap();
        assert_eq!(NiLODNode::read(&mut Reader::new(&w.out)).unwrap(), n);
        // The level count is the u32 before the three 8-byte ranges.
        let mut bad = w.out.clone();
        let at = bad.len() - 3 * 8 - 4;
        bad[at..at + 4].copy_from_slice(&1_000_000u32.to_le_bytes());
        let e = NiLODNode::read(&mut Reader::new(&bad)).unwrap_err();
        assert!(e.what.contains("1000000 items"), "{e}");
    }
}
