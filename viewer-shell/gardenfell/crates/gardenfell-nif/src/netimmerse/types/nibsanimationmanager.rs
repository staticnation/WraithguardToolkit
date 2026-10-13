//! `NiBSAnimationManager`: a Bethesda node type with nothing of its own beyond `NiNode`.
//! `tes3` nibsanimationmanager.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiBSAnimationManager {
    pub base: NiNode,
}

impl Block for NiBSAnimationManager {
    const NAME: &'static str = "NiBSAnimationManager";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiBSAnimationManager { base: NiNode::read(r)? })
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
    fn an_animation_manager_round_trips() {
        let mut n = NiBSAnimationManager::default();
        n.base.base.base.name = "AnimationManager".into();
        n.base.children = vec![Link::new(2)];
        let mut w = Writer::new();
        n.write(&mut w).unwrap();
        assert_eq!(NiBSAnimationManager::read(&mut Reader::new(&w.out)).unwrap(), n);
    }
}
