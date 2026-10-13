//! `FACT`: a faction. `tes3` faction.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Faction {
    pub flags: u32,
    pub id: String,
    pub name: String,
    /// `RNAM`, one per rank, each written as a 32-byte fixed string.
    pub rank_names: Vec<String>,
    /// `ANAM` and `INTV` pairs, in file order.
    pub reactions: Vec<FactionReaction>,
    pub data: FactionData,
}

/// `FADT`, 240 bytes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FactionData {
    /// `consts::attribute::*`.
    pub favored_attributes: [i32; 2],
    pub requirements: [FactionRequirement; 10],
    /// `consts::skill::*`.
    pub favored_skills: [i32; 7],
    /// `consts::faction::*`.
    pub flags: u32,
}

/// One rank's requirements within `FADT`, 20 bytes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FactionRequirement {
    pub attributes: [i32; 2],
    pub primary_skill: i32,
    pub favored_skill: i32,
    pub reputation: i32,
}

/// A reaction toward another faction: `ANAM` (the faction id) followed by `INTV`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FactionReaction {
    pub faction: String,
    pub reaction: i32,
}

impl FactionRequirement {
    pub const SIZE: usize = 20;

    pub fn read(r: &mut Reader<'_>) -> Result<FactionRequirement> {
        Ok(FactionRequirement {
            attributes: r.i32s::<2>()?,
            primary_skill: r.i32()?,
            favored_skill: r.i32()?,
            reputation: r.i32()?,
        })
    }

    pub fn write(&self, w: &mut Writer) {
        w.i32s(&self.attributes);
        w.i32(self.primary_skill);
        w.i32(self.favored_skill);
        w.i32(self.reputation);
    }
}

impl FactionData {
    pub const SIZE: usize = 240;

    pub fn read(r: &mut Reader<'_>) -> Result<FactionData> {
        let favored_attributes = r.i32s::<2>()?;
        let mut requirements: [FactionRequirement; 10] = Default::default();
        for q in requirements.iter_mut() {
            *q = FactionRequirement::read(r)?;
        }
        let favored_skills = r.i32s::<7>()?;
        let flags = r.u32()?;
        Ok(FactionData { favored_attributes, requirements, favored_skills, flags })
    }

    pub fn write(&self, w: &mut Writer) {
        w.i32s(&self.favored_attributes);
        for q in &self.requirements {
            q.write(w);
        }
        w.i32s(&self.favored_skills);
        w.u32(self.flags);
    }
}

impl Typed for Faction {
    const TAG: Tag = *b"FACT";

    fn from_raw(rec: &Record) -> Result<Self> {
        check_tag(rec, &Self::TAG)?;
        let mut this = Self { flags: flags_of(rec), ..Default::default() };
        let mut it = Subs::new(rec);
        while let Some(s) = it.next() {
            match &s.tag {
                b"NAME" => this.id = text(s),
                b"FNAM" => this.name = text(s),
                b"RNAM" => this.rank_names.push(text(s)),
                b"FADT" => this.data = FactionData::read(&mut sized(rec, s, FactionData::SIZE)?)?,
                b"ANAM" => {
                    let faction = text(s);
                    let i = it.expect(b"INTV")?;
                    let reaction = i32_of(rec, i)?;
                    this.reactions.push(FactionReaction { faction, reaction });
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
        out.text_if(b"FNAM", &self.name)?;
        for rank in &self.rank_names {
            out.sub(b"RNAM", |w| w.fixed_string(rank, 32))?;
        }
        out.sub(b"FADT", |w| {
            self.data.write(w);
            Ok(())
        })?;
        for reaction in &self.reactions {
            out.text(b"ANAM", &reaction.faction)?;
            out.i32(b"INTV", reaction.reaction);
        }
        out.dele();
        Ok(out.finish())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fadt() -> FactionData {
        let mut data = FactionData {
            favored_attributes: [consts::attribute::INTELLIGENCE, consts::attribute::WILLPOWER],
            favored_skills: [
                consts::skill::ALCHEMY,
                consts::skill::ENCHANT,
                consts::skill::ALTERATION,
                consts::skill::MYSTICISM,
                consts::skill::DESTRUCTION,
                consts::skill::NONE,
                consts::skill::NONE,
            ],
            flags: 0,
            ..Default::default()
        };
        for (i, q) in data.requirements.iter_mut().enumerate() {
            *q = FactionRequirement {
                attributes: [30 + i as i32, 30 + i as i32],
                primary_skill: 10 * i as i32,
                favored_skill: 5 * i as i32,
                reputation: i as i32,
            };
        }
        data
    }

    #[test]
    fn a_faction_round_trips_with_ranks_and_reactions() {
        let mut rec = Record::new(b"FACT");
        rec.push(b"NAME", b"Mages Guild\0".to_vec());
        rec.push(b"FNAM", b"Mages Guild\0".to_vec());
        let mut w = Writer::new();
        w.fixed_string("Associate", 32).unwrap();
        rec.push(b"RNAM", w.into_bytes());
        let mut w = Writer::new();
        w.fixed_string("Apprentice", 32).unwrap();
        rec.push(b"RNAM", w.into_bytes());
        let mut w = Writer::new();
        fadt().write(&mut w);
        assert_eq!(w.len(), FactionData::SIZE);
        rec.push(b"FADT", w.into_bytes());
        rec.push(b"ANAM", b"Fighters Guild\0".to_vec());
        rec.push(b"INTV", (-1i32).to_le_bytes().to_vec());
        rec.push(b"ANAM", b"Telvanni\0".to_vec());
        rec.push(b"INTV", (-3i32).to_le_bytes().to_vec());
        let f = Faction::from_raw(&rec).unwrap();
        assert_eq!(f.rank_names, vec!["Associate".to_string(), "Apprentice".to_string()]);
        assert_eq!(f.data, fadt());
        assert_eq!(f.reactions.len(), 2);
        assert_eq!(f.reactions[1], FactionReaction { faction: "Telvanni".into(), reaction: -3 });
        assert_eq!(f.to_raw().unwrap(), rec);
    }

    #[test]
    fn a_reaction_without_its_intv_is_refused() {
        let mut rec = Record::new(b"FACT");
        rec.push(b"NAME", b"Telvanni\0".to_vec());
        rec.push(b"ANAM", b"Hlaalu\0".to_vec());
        rec.push(b"ANAM", b"Redoran\0".to_vec());
        let e = Faction::from_raw(&rec).unwrap_err();
        assert!(e.what.contains("FACT::ANAM where INTV was expected"), "{e}");
    }
}
