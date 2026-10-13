//! `NiSortAdjustNode`: a node that changes how the renderer sorts what is under it.
//! `tes3` nisortadjustnode.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiSortAdjustNode {
    pub base: NiNode,
    /// `consts::sorting_mode::*`.
    pub sorting_mode: i32,
    pub sub_sorter: Link<NiAccumulator>,
}

impl Block for NiSortAdjustNode {
    const NAME: &'static str = "NiSortAdjustNode";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiSortAdjustNode { base: NiNode::read(r)?, sorting_mode: r.i32()?, sub_sorter: r.link()? })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)?;
        w.i32(self.sorting_mode);
        w.link(self.sub_sorter);
        Ok(())
    }

    fn links(&mut self, f: &mut dyn FnMut(&mut i32)) {
        self.base.links(f);
        f(&mut self.sub_sorter.index);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sort_adjust_node_round_trips_and_keeps_an_unknown_mode() {
        let n = NiSortAdjustNode {
            sorting_mode: consts::sorting_mode::GROUPED,
            sub_sorter: Link::new(7),
            ..Default::default()
        };
        let mut w = Writer::new();
        n.write(&mut w).unwrap();
        assert_eq!(NiSortAdjustNode::read(&mut Reader::new(&w.out)).unwrap(), n);
        let odd = NiSortAdjustNode { sorting_mode: 99, ..Default::default() };
        let mut w = Writer::new();
        odd.write(&mut w).unwrap();
        assert_eq!(NiSortAdjustNode::read(&mut Reader::new(&w.out)).unwrap().sorting_mode, 99);
    }
}
