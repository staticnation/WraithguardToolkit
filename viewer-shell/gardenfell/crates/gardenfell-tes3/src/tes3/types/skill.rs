//! `SKIL`: a skill. `tes3` skill.rs.

use super::*;

/// A skill record has no `NAME`: `INDX` is its identity.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Skill {
    pub flags: u32,
    /// `consts::skill::*`: `INDX`, 4 bytes.
    pub skill_id: i32,
    pub data: SkillData,
    pub description: String,
}

/// `SKDT`, 24 bytes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SkillData {
    /// `consts::attribute::*`.
    pub governing_attribute: i32,
    /// `consts::specialization::*`.
    pub specialization: i32,
    /// Experience gained per use, one value for each of the skill's four actions.
    pub actions: [f32; 4],
}

impl SkillData {
    pub const SIZE: usize = 24;

    pub fn read(r: &mut Reader<'_>) -> Result<SkillData> {
        Ok(SkillData { governing_attribute: r.i32()?, specialization: r.i32()?, actions: r.f32s::<4>()? })
    }

    pub fn write(&self, w: &mut Writer) {
        w.i32(self.governing_attribute);
        w.i32(self.specialization);
        w.f32s(&self.actions);
    }
}

impl Typed for Skill {
    const TAG: Tag = *b"SKIL";

    fn from_raw(rec: &Record) -> Result<Self> {
        check_tag(rec, &Self::TAG)?;
        let mut this = Self { flags: flags_of(rec), ..Default::default() };
        for s in &rec.subs {
            match &s.tag {
                b"INDX" => this.skill_id = i32_of(rec, s)?,
                b"SKDT" => this.data = SkillData::read(&mut sized(rec, s, SkillData::SIZE)?)?,
                b"DESC" => this.description = text(s),
                b"DELE" => {}
                _ => return Err(unexpected(rec, s)),
            }
        }
        Ok(this)
    }

    fn to_raw(&self) -> Result<Record<'static>> {
        let mut out = Out::new(&Self::TAG, self.flags);
        out.i32(b"INDX", self.skill_id);
        out.sub(b"SKDT", |w| {
            self.data.write(w);
            Ok(())
        })?;
        out.text_if(b"DESC", &self.description)?;
        out.dele();
        Ok(out.finish())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_skill_round_trips_and_a_short_skdt_is_refused() {
        let data = SkillData {
            governing_attribute: consts::attribute::AGILITY,
            specialization: consts::specialization::COMBAT,
            actions: [2.5, 0.0, 0.0, 0.0],
        };
        let mut rec = Record::new(b"SKIL");
        rec.push(b"INDX", consts::skill::BLOCK.to_le_bytes().to_vec());
        let mut w = Writer::new();
        data.write(&mut w);
        assert_eq!(w.len(), SkillData::SIZE);
        rec.push(b"SKDT", w.into_bytes());
        rec.push(b"DESC", b"Blocking.\0".to_vec());
        let k = Skill::from_raw(&rec).unwrap();
        assert_eq!(k.skill_id, consts::skill::BLOCK);
        assert_eq!(k.data, data);
        assert_eq!(k.to_raw().unwrap(), rec);

        rec.subs[1].data.to_mut().truncate(23);
        let e = Skill::from_raw(&rec).unwrap_err();
        assert!(e.what.contains("SKIL::SKDT is 23 bytes"), "{e}");
    }
}
