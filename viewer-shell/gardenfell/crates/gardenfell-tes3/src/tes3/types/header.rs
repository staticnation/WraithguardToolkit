//! `TES3`: the file header. `tes3` header.rs.

use super::*;

#[derive(Clone, Debug, PartialEq)]
pub struct Header {
    pub flags: u32,
    pub version: f32,
    /// `consts::file_type::*`.
    pub file_type: u32,
    pub author: String,
    pub description: String,
    pub num_objects: u32,
    /// Each master's file name and size in bytes, a `MAST` followed by its `DATA`.
    pub masters: Vec<(String, u64)>,
}

impl Default for Header {
    fn default() -> Header {
        Header {
            flags: 0,
            version: 1.3,
            file_type: 0,
            author: String::new(),
            description: String::new(),
            num_objects: 0,
            masters: Vec::new(),
        }
    }
}

impl Typed for Header {
    const TAG: Tag = *b"TES3";

    fn from_raw(rec: &Record) -> Result<Self> {
        check_tag(rec, &Self::TAG)?;
        let mut this = Self { flags: flags_of(rec), ..Default::default() };
        let mut it = Subs::new(rec);
        while let Some(s) = it.next() {
            match &s.tag {
                b"HEDR" => {
                    let mut r = sized(rec, s, 300)?;
                    this.version = r.f32()?;
                    this.file_type = r.u32()?;
                    this.author = r.fixed_string(32)?;
                    this.description = r.fixed_string(256)?;
                    this.num_objects = r.u32()?;
                }
                b"MAST" => {
                    let name = text(s);
                    let d = it.expect(b"DATA")?;
                    let size = sized(rec, d, 8)?.u64()?;
                    this.masters.push((name, size));
                }
                _ => return Err(unexpected(rec, s)),
            }
        }
        Ok(this)
    }

    // The header has no DELE in tes3, so none is read or written here.
    fn to_raw(&self) -> Result<Record<'static>> {
        let mut out = Out::new(&Self::TAG, self.flags);
        out.sub(b"HEDR", |w| {
            w.f32(self.version);
            w.u32(self.file_type);
            w.fixed_string(&self.author, 32)?;
            w.fixed_string(&self.description, 256)?;
            w.u32(self.num_objects);
            Ok(())
        })?;
        for (name, size) in &self.masters {
            out.text(b"MAST", name)?;
            out.bytes(b"DATA", size.to_le_bytes().to_vec());
        }
        Ok(out.finish())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hedr(author: &str, description: &str, num_objects: u32) -> Vec<u8> {
        let mut w = Writer::new();
        w.f32(1.3);
        w.u32(consts::file_type::ESP);
        w.fixed_string(author, 32).unwrap();
        w.fixed_string(description, 256).unwrap();
        w.u32(num_objects);
        w.into_bytes()
    }

    #[test]
    fn a_header_with_two_masters_round_trips() {
        let mut rec = Record::new(b"TES3");
        rec.push(b"HEDR", hedr("Robin", "grass for the Grazelands", 1234));
        rec.push(b"MAST", b"Morrowind.esm\0".to_vec());
        rec.push(b"DATA", 79837557u64.to_le_bytes().to_vec());
        rec.push(b"MAST", b"Tribunal.esm\0".to_vec());
        rec.push(b"DATA", 4565686u64.to_le_bytes().to_vec());
        let h = Header::from_raw(&rec).unwrap();
        assert_eq!(h.version, 1.3);
        assert_eq!(h.author, "Robin");
        assert_eq!(h.num_objects, 1234);
        assert_eq!(
            h.masters,
            vec![("Morrowind.esm".to_string(), 79837557), ("Tribunal.esm".to_string(), 4565686)]
        );
        assert_eq!(h.to_raw().unwrap(), rec);
        assert_eq!(Header::default().version, 1.3);
    }

    #[test]
    fn a_master_without_its_data_and_a_short_hedr_are_refused() {
        let mut rec = Record::new(b"TES3");
        rec.push(b"HEDR", hedr("", "", 0));
        rec.push(b"MAST", b"Morrowind.esm\0".to_vec());
        rec.push(b"MAST", b"Tribunal.esm\0".to_vec());
        let e = Header::from_raw(&rec).unwrap_err();
        assert!(e.what.contains("TES3::MAST where DATA was expected"), "{e}");

        let mut rec = Record::new(b"TES3");
        rec.push(b"HEDR", vec![0; 299]);
        let e = Header::from_raw(&rec).unwrap_err();
        assert!(e.what.contains("TES3::HEDR is 299 bytes"), "{e}");
    }
}
