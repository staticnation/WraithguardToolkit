//! `GMST`: a game setting. `tes3` gamesetting.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct GameSetting {
    pub flags: u32,
    pub id: String,
    /// `None` for a setting with a `NAME` and no value subrecord, which Morrowind.esm
    /// has over a hundred of (`tes3` writes those back as a float zero).
    pub value: Option<GameSettingValue>,
}

/// The one value subrecord a setting carries: `FLTV`, `INTV` or `STRV`.
#[derive(Clone, Debug, PartialEq)]
pub enum GameSettingValue {
    Float(f32),
    Integer(i32),
    String(String),
}

impl Default for GameSettingValue {
    fn default() -> GameSettingValue {
        GameSettingValue::Float(0.0)
    }
}

impl Typed for GameSetting {
    const TAG: Tag = *b"GMST";

    fn from_raw(rec: &Record) -> Result<Self> {
        check_tag(rec, &Self::TAG)?;
        let mut this = Self { flags: flags_of(rec), ..Default::default() };
        for s in &rec.subs {
            match &s.tag {
                b"NAME" => this.id = text(s),
                b"STRV" => this.value = Some(GameSettingValue::String(text(s))),
                b"FLTV" => this.value = Some(GameSettingValue::Float(f32_of(rec, s)?)),
                b"INTV" => this.value = Some(GameSettingValue::Integer(i32_of(rec, s)?)),
                _ => return Err(unexpected(rec, s)),
            }
        }
        Ok(this)
    }

    // A game setting has no DELE in tes3, so none is read or written here.
    fn to_raw(&self) -> Result<Record<'static>> {
        let mut out = Out::new(&Self::TAG, self.flags);
        out.text(b"NAME", &self.id)?;
        match &self.value {
            Some(GameSettingValue::String(v)) => out.text(b"STRV", v)?,
            Some(GameSettingValue::Float(v)) => out.f32(b"FLTV", *v),
            Some(GameSettingValue::Integer(v)) => out.i32(b"INTV", *v),
            None => {}
        }
        Ok(out.finish())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_kind_of_setting_round_trips() {
        let mut rec = Record::new(b"GMST");
        rec.push(b"NAME", b"fJumpAcrobaticsBase\0".to_vec());
        rec.push(b"FLTV", 128.0f32.to_le_bytes().to_vec());
        let g = GameSetting::from_raw(&rec).unwrap();
        assert_eq!(g.value, Some(GameSettingValue::Float(128.0)));
        assert_eq!(g.to_raw().unwrap(), rec);

        let mut rec = Record::new(b"GMST");
        rec.push(b"NAME", b"iMaxActivateDist\0".to_vec());
        rec.push(b"INTV", 192i32.to_le_bytes().to_vec());
        let g = GameSetting::from_raw(&rec).unwrap();
        assert_eq!(g.value, Some(GameSettingValue::Integer(192)));
        assert_eq!(g.to_raw().unwrap(), rec);

        let mut rec = Record::new(b"GMST");
        rec.push(b"NAME", b"sOK\0".to_vec());
        rec.push(b"STRV", b"OK\0".to_vec());
        let g = GameSetting::from_raw(&rec).unwrap();
        assert_eq!(g.value, Some(GameSettingValue::String("OK".into())));
        assert_eq!(g.to_raw().unwrap(), rec);
    }

    #[test]
    fn a_short_fltv_is_refused() {
        let mut rec = Record::new(b"GMST");
        rec.push(b"NAME", b"fX\0".to_vec());
        rec.push(b"FLTV", vec![0, 0]);
        let e = GameSetting::from_raw(&rec).unwrap_err();
        assert!(e.what.contains("GMST::FLTV is 2 bytes"), "{e}");
    }
}
