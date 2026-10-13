//! `NiTexture`: the base of every texture, a named block with nothing of its own. `tes3`
//! nitexture.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NiTexture {
    pub base: NiObjectNET,
}

impl Block for NiTexture {
    const NAME: &'static str = "NiTexture";

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(NiTexture { base: NiObjectNET::read(r)? })
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
    fn a_texture_round_trips() {
        let t = NiTexture { base: NiObjectNET { name: "tx".into(), ..Default::default() } };
        let mut w = Writer::new();
        t.write(&mut w).unwrap();
        assert_eq!(NiTexture::read(&mut Reader::new(&w.out)).unwrap(), t);
    }
}
