//! `NiEmitterModifier`: the base of the emitter modifiers a particle system controller
//! links to, nothing of its own. `tes3` niemittermodifier.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NiEmitterModifier {
    pub base: NiObject,
}

impl Block for NiEmitterModifier {
    const NAME: &'static str = "NiEmitterModifier";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiEmitterModifier { base: NiObject::read(r)? })
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
    fn an_emitter_modifier_is_empty() {
        let mut w = Writer::new();
        NiEmitterModifier::default().write(&mut w).unwrap();
        assert!(w.out.is_empty());
        assert_eq!(NiEmitterModifier::read(&mut Reader::new(&[])).unwrap(), NiEmitterModifier::default());
    }
}
