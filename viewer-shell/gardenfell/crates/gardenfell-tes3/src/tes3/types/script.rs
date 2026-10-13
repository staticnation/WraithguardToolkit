//! `SCPT`: a script. `tes3` script.rs.

use super::*;

/// A script record has no `NAME`: its id is the first 32 bytes of `SCHD`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Script {
    pub flags: u32,
    pub id: String,
    pub header: ScriptHeader,
    /// `SCVR`: the local variable names, NUL-separated, kept as the bytes they are.
    pub variables: Vec<u8>,
    /// `SCDT`: the compiled bytecode.
    pub bytecode: Vec<u8>,
    /// `SCTX`: the source text. The Construction Set writes it without a NUL, which the
    /// round trip adds, as `tes3` does.
    pub text: String,
}

/// The counts after the id in `SCHD`, 20 bytes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScriptHeader {
    pub num_shorts: u32,
    pub num_longs: u32,
    pub num_floats: u32,
    pub bytecode_length: u32,
    pub variables_length: u32,
}

impl ScriptHeader {
    pub const SIZE: usize = 20;

    pub fn read(r: &mut Reader<'_>) -> Result<ScriptHeader> {
        Ok(ScriptHeader {
            num_shorts: r.u32()?,
            num_longs: r.u32()?,
            num_floats: r.u32()?,
            bytecode_length: r.u32()?,
            variables_length: r.u32()?,
        })
    }

    pub fn write(&self, w: &mut Writer) {
        w.u32(self.num_shorts);
        w.u32(self.num_longs);
        w.u32(self.num_floats);
        w.u32(self.bytecode_length);
        w.u32(self.variables_length);
    }
}

impl Typed for Script {
    const TAG: Tag = *b"SCPT";

    fn from_raw(rec: &Record) -> Result<Self> {
        check_tag(rec, &Self::TAG)?;
        let mut this = Self { flags: flags_of(rec), ..Default::default() };
        for s in &rec.subs {
            match &s.tag {
                b"SCHD" => {
                    let mut r = sized(rec, s, 32 + ScriptHeader::SIZE)?;
                    this.id = r.fixed_string(32)?;
                    this.header = ScriptHeader::read(&mut r)?;
                }
                b"SCVR" => this.variables = s.data.to_vec(),
                b"SCDT" => this.bytecode = s.data.to_vec(),
                b"SCTX" => this.text = text(s),
                b"DELE" => {}
                _ => return Err(unexpected(rec, s)),
            }
        }
        Ok(this)
    }

    fn to_raw(&self) -> Result<Record<'static>> {
        let mut out = Out::new(&Self::TAG, self.flags);
        out.sub(b"SCHD", |w| {
            w.fixed_string(&self.id, 32)?;
            self.header.write(w);
            Ok(())
        })?;
        if !self.variables.is_empty() {
            out.bytes(b"SCVR", self.variables.clone());
        }
        if !self.bytecode.is_empty() {
            out.bytes(b"SCDT", self.bytecode.clone());
        }
        out.text_if(b"SCTX", &self.text)?;
        out.dele();
        Ok(out.finish())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_script_round_trips_and_a_short_schd_is_refused() {
        let header = ScriptHeader {
            num_shorts: 1,
            num_longs: 0,
            num_floats: 1,
            bytecode_length: 6,
            variables_length: 10,
        };
        let mut rec = Record::new(b"SCPT");
        let mut w = Writer::new();
        w.fixed_string("gf_grass", 32).unwrap();
        header.write(&mut w);
        assert_eq!(w.len(), 52);
        rec.push(b"SCHD", w.into_bytes());
        rec.push(b"SCVR", b"done\0timer\0".to_vec());
        rec.push(b"SCDT", vec![1, 0, 2, 0, 3, 0]);
        rec.push(b"SCTX", b"begin gf_grass\r\nend\0".to_vec());
        let s = Script::from_raw(&rec).unwrap();
        assert_eq!(s.id, "gf_grass");
        assert_eq!(s.header, header);
        assert_eq!(s.variables, b"done\0timer\0".to_vec());
        assert_eq!(s.text, "begin gf_grass\r\nend");
        assert_eq!(s.to_raw().unwrap(), rec);

        rec.subs[0].data.to_mut().truncate(51);
        let e = Script::from_raw(&rec).unwrap_err();
        assert!(e.what.contains("SCPT::SCHD is 51 bytes"), "{e}");
    }
}
