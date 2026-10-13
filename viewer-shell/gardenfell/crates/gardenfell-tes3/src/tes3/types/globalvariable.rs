//! `GLOB`: a global variable. `tes3` globalvariable.rs.

use super::*;

/// The value is kept as the file holds it: the type letter and the `f32`. `tes3` folds the
/// two into a `GlobalValue` enum, which turns a NaN into zero, truncates a fractional long
/// or short, and refuses to save a long the `f32` cannot hold exactly. None of that is lost
/// here, so what came in goes out.
#[derive(Clone, Debug, PartialEq)]
pub struct GlobalVariable {
    pub flags: u32,
    pub id: String,
    /// `consts::global_type::*`: `FNAM`, one byte.
    pub global_type: u8,
    /// `FLTV`: the value, an `f32` whatever the type letter says.
    pub value: f32,
}

impl Default for GlobalVariable {
    fn default() -> GlobalVariable {
        GlobalVariable { flags: 0, id: String::new(), global_type: consts::global_type::FLOAT, value: 0.0 }
    }
}

impl Typed for GlobalVariable {
    const TAG: Tag = *b"GLOB";

    fn from_raw(rec: &Record) -> Result<Self> {
        check_tag(rec, &Self::TAG)?;
        let mut this = Self { flags: flags_of(rec), ..Default::default() };
        let mut it = Subs::new(rec);
        while let Some(s) = it.next() {
            match &s.tag {
                b"NAME" => this.id = text(s),
                b"FNAM" => {
                    this.global_type = u8_of(rec, s)?;
                    let f = it.expect(b"FLTV")?;
                    this.value = f32_of(rec, f)?;
                }
                b"DELE" => {}
                _ => return Err(unexpected(rec, s)),
            }
        }
        Ok(this)
    }

    fn to_raw(&self) -> Result<Record<'static>> {
        let mut out = Out::new(&Self::TAG, self.flags);
        out.text(b"NAME", &self.id)?;
        out.u8(b"FNAM", self.global_type);
        out.f32(b"FLTV", self.value);
        out.dele();
        Ok(out.finish())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_global_round_trips_with_its_type_letter() {
        let mut rec = Record::new(b"GLOB");
        rec.push(b"NAME", b"GameHour\0".to_vec());
        rec.push(b"FNAM", vec![b'f']);
        rec.push(b"FLTV", 9.5f32.to_le_bytes().to_vec());
        let g = GlobalVariable::from_raw(&rec).unwrap();
        assert_eq!(
            g,
            GlobalVariable {
                flags: 0,
                id: "GameHour".into(),
                global_type: consts::global_type::FLOAT,
                value: 9.5
            }
        );
        assert_eq!(g.to_raw().unwrap(), rec);

        // A short holding a NaN (Morrowind.esm's "ratskilled") keeps its bits, where tes3
        // writes back zero.
        let mut rec = Record::new(b"GLOB");
        rec.push(b"NAME", b"ratskilled\0".to_vec());
        rec.push(b"FNAM", vec![b's']);
        rec.push(b"FLTV", 0x7FC0_0000u32.to_le_bytes().to_vec());
        let g = GlobalVariable::from_raw(&rec).unwrap();
        assert_eq!(g.global_type, consts::global_type::SHORT);
        assert!(g.value.is_nan());
        assert_eq!(g.to_raw().unwrap(), rec);
    }

    #[test]
    fn an_fnam_without_its_fltv_is_refused() {
        let mut rec = Record::new(b"GLOB");
        rec.push(b"NAME", b"Day\0".to_vec());
        rec.push(b"FNAM", vec![b'l']);
        rec.push(b"NAME", b"Day\0".to_vec());
        let e = GlobalVariable::from_raw(&rec).unwrap_err();
        assert!(e.what.contains("GLOB::NAME where FLTV was expected"), "{e}");

        let mut rec = Record::new(b"GLOB");
        rec.push(b"NAME", b"Day\0".to_vec());
        rec.push(b"FNAM", vec![b'l']);
        let e = GlobalVariable::from_raw(&rec).unwrap_err();
        assert!(e.what.contains("ended where FLTV was expected"), "{e}");
    }
}
