//! `NiObjectNET`: a named block with its extra data and controller chains. `tes3`
//! niobjectnet.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiObjectNET {
    pub base: NiObject,
    pub name: String,
    /// The first of the extra data chain; each `NiExtraData` names the next.
    pub extra_data: Link<NiExtraData>,
    /// The first of the controller chain; each `NiTimeController` names the next.
    pub controller: Link<NiTimeController>,
}

impl Block for NiObjectNET {
    const NAME: &'static str = "NiObjectNET";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiObjectNET {
            base: NiObject::read(r)?,
            name: r.string()?,
            extra_data: r.link()?,
            controller: r.link()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<()> {
        self.base.write(w)?;
        w.string(&self.name)?;
        w.link(self.extra_data);
        w.link(self.controller);
        Ok(())
    }

    fn links(&mut self, f: &mut dyn FnMut(&mut i32)) {
        self.base.links(f);
        f(&mut self.extra_data.index);
        f(&mut self.controller.index);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_named_block_round_trips() {
        let x = NiObjectNET { name: "Bip01".into(), extra_data: Link::new(4), ..Default::default() };
        let mut w = Writer::new();
        x.write(&mut w).unwrap();
        assert_eq!(w.out.len(), 4 + 5 + 4 + 4);
        assert_eq!(NiObjectNET::read(&mut Reader::new(&w.out)).unwrap(), x);
    }
}
