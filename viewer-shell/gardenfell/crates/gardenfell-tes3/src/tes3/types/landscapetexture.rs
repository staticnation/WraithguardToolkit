//! `LTEX`: a landscape texture, the file a `LAND` texture index refers to. `tes3`
//! landscapetexture.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LandscapeTexture {
    pub flags: u32,
    pub id: String,
    /// `INTV`: the texture index, 4 bytes.
    pub index: u32,
    pub file_name: String,
}

impl Typed for LandscapeTexture {
    const TAG: Tag = *b"LTEX";

    fn from_raw(rec: &Record) -> Result<Self> {
        check_tag(rec, &Self::TAG)?;
        let mut this = Self { flags: flags_of(rec), ..Default::default() };
        for s in &rec.subs {
            match &s.tag {
                b"NAME" => this.id = text(s),
                b"INTV" => this.index = u32_of(rec, s)?,
                b"DATA" => this.file_name = text(s),
                b"DELE" => {}
                _ => return Err(unexpected(rec, s)),
            }
        }
        Ok(this)
    }

    fn to_raw(&self) -> Result<Record<'static>> {
        let mut out = Out::new(&Self::TAG, self.flags);
        out.text(b"NAME", &self.id)?;
        out.u32(b"INTV", self.index);
        out.text_if(b"DATA", &self.file_name)?;
        out.dele();
        Ok(out.finish())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_landscape_texture_round_trips_and_a_short_intv_is_refused() {
        let mut rec = Record::new(b"LTEX");
        rec.push(b"NAME", b"Tx_grass_01\0".to_vec());
        rec.push(b"INTV", 12u32.to_le_bytes().to_vec());
        rec.push(b"DATA", b"Tx_grass_01.tga\0".to_vec());
        let t = LandscapeTexture::from_raw(&rec).unwrap();
        assert_eq!(
            t,
            LandscapeTexture {
                flags: 0,
                id: "Tx_grass_01".into(),
                index: 12,
                file_name: "Tx_grass_01.tga".into()
            }
        );
        assert_eq!(t.to_raw().unwrap(), rec);

        rec.subs[1].data.to_mut().truncate(3);
        let e = LandscapeTexture::from_raw(&rec).unwrap_err();
        assert!(e.what.contains("LTEX::INTV is 3 bytes"), "{e}");
    }
}
