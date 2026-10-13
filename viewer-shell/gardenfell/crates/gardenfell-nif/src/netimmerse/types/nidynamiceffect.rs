//! `NiDynamicEffect`: the base of lights and texture effects, with the list of nodes it
//! affects. `tes3` nidynamiceffect.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiDynamicEffect {
    pub base: NiAVObject,
    /// Links in form, but `tes3` marks them "invalid links" and keeps the raw integers,
    /// since the values in the game's files do not resolve to blocks. Kept the same here.
    pub affected_nodes: Vec<i32>,
}

impl Block for NiDynamicEffect {
    const NAME: &'static str = "NiDynamicEffect";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiDynamicEffect { base: NiAVObject::read(r)?, affected_nodes: r.counted(4, |r| r.i32())? })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)?;
        w.count(self.affected_nodes.len());
        w.i32s(&self.affected_nodes);
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
    fn a_dynamic_effect_round_trips_and_a_bad_node_count_is_refused() {
        let e = NiDynamicEffect { affected_nodes: vec![3, -1, 0x0040_0000], ..Default::default() };
        let mut w = Writer::new();
        e.write(&mut w).unwrap();
        assert_eq!(NiDynamicEffect::read(&mut Reader::new(&w.out)).unwrap(), e);
        let mut bad = w.out.clone();
        let at = bad.len() - 3 * 4 - 4;
        bad[at..at + 4].copy_from_slice(&1_000_000u32.to_le_bytes());
        let err = NiDynamicEffect::read(&mut Reader::new(&bad)).unwrap_err();
        assert!(err.what.contains("1000000 items"), "{err}");
    }
}
