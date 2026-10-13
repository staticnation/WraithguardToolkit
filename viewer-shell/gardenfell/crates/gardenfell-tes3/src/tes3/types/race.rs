//! `RACE`: a race. `tes3` race.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Race {
    pub flags: u32,
    pub id: String,
    pub name: String,
    /// `NPCS`, one per spell, each written as a 32-byte fixed string.
    pub spells: Vec<String>,
    pub description: String,
    pub data: RaceData,
}

/// `RADT`, 140 bytes. Each attribute is a male and a female value.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RaceData {
    pub skill_bonuses: SkillBonuses,
    pub strength: [i32; 2],
    pub intelligence: [i32; 2],
    pub willpower: [i32; 2],
    pub agility: [i32; 2],
    pub speed: [i32; 2],
    pub endurance: [i32; 2],
    pub personality: [i32; 2],
    pub luck: [i32; 2],
    pub height: [f32; 2],
    pub weight: [f32; 2],
    /// `consts::race::*`.
    pub flags: u32,
}

/// Seven skill and bonus pairs within `RADT`, 56 bytes. Each `skill_n` is `consts::skill::*`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SkillBonuses {
    pub skill_0: i32,
    pub bonus_0: i32,
    pub skill_1: i32,
    pub bonus_1: i32,
    pub skill_2: i32,
    pub bonus_2: i32,
    pub skill_3: i32,
    pub bonus_3: i32,
    pub skill_4: i32,
    pub bonus_4: i32,
    pub skill_5: i32,
    pub bonus_5: i32,
    pub skill_6: i32,
    pub bonus_6: i32,
}

impl SkillBonuses {
    pub const SIZE: usize = 56;

    pub fn read(r: &mut Reader<'_>) -> Result<SkillBonuses> {
        Ok(SkillBonuses {
            skill_0: r.i32()?,
            bonus_0: r.i32()?,
            skill_1: r.i32()?,
            bonus_1: r.i32()?,
            skill_2: r.i32()?,
            bonus_2: r.i32()?,
            skill_3: r.i32()?,
            bonus_3: r.i32()?,
            skill_4: r.i32()?,
            bonus_4: r.i32()?,
            skill_5: r.i32()?,
            bonus_5: r.i32()?,
            skill_6: r.i32()?,
            bonus_6: r.i32()?,
        })
    }

    pub fn write(&self, w: &mut Writer) {
        w.i32(self.skill_0);
        w.i32(self.bonus_0);
        w.i32(self.skill_1);
        w.i32(self.bonus_1);
        w.i32(self.skill_2);
        w.i32(self.bonus_2);
        w.i32(self.skill_3);
        w.i32(self.bonus_3);
        w.i32(self.skill_4);
        w.i32(self.bonus_4);
        w.i32(self.skill_5);
        w.i32(self.bonus_5);
        w.i32(self.skill_6);
        w.i32(self.bonus_6);
    }
}

impl RaceData {
    pub const SIZE: usize = 140;

    pub fn read(r: &mut Reader<'_>) -> Result<RaceData> {
        Ok(RaceData {
            skill_bonuses: SkillBonuses::read(r)?,
            strength: r.i32s::<2>()?,
            intelligence: r.i32s::<2>()?,
            willpower: r.i32s::<2>()?,
            agility: r.i32s::<2>()?,
            speed: r.i32s::<2>()?,
            endurance: r.i32s::<2>()?,
            personality: r.i32s::<2>()?,
            luck: r.i32s::<2>()?,
            height: r.f32s::<2>()?,
            weight: r.f32s::<2>()?,
            flags: r.u32()?,
        })
    }

    pub fn write(&self, w: &mut Writer) {
        self.skill_bonuses.write(w);
        w.i32s(&self.strength);
        w.i32s(&self.intelligence);
        w.i32s(&self.willpower);
        w.i32s(&self.agility);
        w.i32s(&self.speed);
        w.i32s(&self.endurance);
        w.i32s(&self.personality);
        w.i32s(&self.luck);
        w.f32s(&self.height);
        w.f32s(&self.weight);
        w.u32(self.flags);
    }
}

impl Typed for Race {
    const TAG: Tag = *b"RACE";

    fn from_raw(rec: &Record) -> Result<Self> {
        check_tag(rec, &Self::TAG)?;
        let mut this = Self { flags: flags_of(rec), ..Default::default() };
        for s in &rec.subs {
            match &s.tag {
                b"NAME" => this.id = text(s),
                b"FNAM" => this.name = text(s),
                b"RADT" => this.data = RaceData::read(&mut sized(rec, s, RaceData::SIZE)?)?,
                b"NPCS" => this.spells.push(text(s)),
                b"DESC" => this.description = text(s),
                b"DELE" => {}
                _ => return Err(unexpected(rec, s)),
            }
        }
        Ok(this)
    }

    fn to_raw(&self) -> Result<Record<'static>> {
        let mut out = Out::new(&Self::TAG, self.flags);
        out.text(b"NAME", &self.id)?;
        out.text_if(b"FNAM", &self.name)?;
        out.sub(b"RADT", |w| {
            self.data.write(w);
            Ok(())
        })?;
        for spell in &self.spells {
            out.sub(b"NPCS", |w| w.fixed_string(spell, 32))?;
        }
        out.text_if(b"DESC", &self.description)?;
        out.dele();
        Ok(out.finish())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_race_round_trips_and_a_short_radt_is_refused() {
        let data = RaceData {
            skill_bonuses: SkillBonuses {
                skill_0: consts::skill::LONG_BLADE,
                bonus_0: 5,
                skill_1: consts::skill::ATHLETICS,
                bonus_1: 10,
                skill_2: consts::skill::NONE,
                ..Default::default()
            },
            strength: [40, 30],
            intelligence: [40, 40],
            willpower: [30, 30],
            agility: [40, 40],
            speed: [40, 40],
            endurance: [40, 40],
            personality: [40, 40],
            luck: [40, 40],
            height: [1.0, 1.0],
            weight: [1.0, 0.9],
            flags: consts::race::PLAYABLE,
        };
        let mut rec = Record::new(b"RACE");
        rec.push(b"NAME", b"Dark Elf\0".to_vec());
        rec.push(b"FNAM", b"Dark Elf\0".to_vec());
        let mut w = Writer::new();
        data.write(&mut w);
        assert_eq!(w.len(), RaceData::SIZE);
        rec.push(b"RADT", w.into_bytes());
        let mut w = Writer::new();
        w.fixed_string("ancestor guardian", 32).unwrap();
        rec.push(b"NPCS", w.into_bytes());
        rec.push(b"DESC", b"Dunmer.\0".to_vec());
        let r = Race::from_raw(&rec).unwrap();
        assert_eq!(r.data, data);
        assert_eq!(r.spells, vec!["ancestor guardian".to_string()]);
        assert_eq!(r.to_raw().unwrap(), rec);

        rec.subs[2].data.to_mut().truncate(100);
        let e = Race::from_raw(&rec).unwrap_err();
        assert!(e.what.contains("RACE::RADT is 100 bytes"), "{e}");
    }
}
