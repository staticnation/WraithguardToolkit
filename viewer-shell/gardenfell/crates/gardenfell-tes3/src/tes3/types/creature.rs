//! `CREA`: a creature. `tes3` creature.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Creature {
    pub flags: u32,
    pub id: String,
    pub name: String,
    pub script: String,
    pub mesh: String,
    /// `NPCO`, one per entry: a count (negative for a restocking item) and a 32-byte item id.
    pub inventory: Vec<(i32, String)>,
    /// `NPCS`, one per spell, each written as a 32-byte fixed string.
    pub spells: Vec<String>,
    /// `None` for an actor with no `AIDT` (Morrowind.esm has one), which `tes3` writes
    /// back with a zero `AIDT`.
    pub ai_data: Option<AiData>,
    pub ai_packages: Vec<AiPackage>,
    pub travel_destinations: Vec<TravelDestination>,
    /// `CNAM`: the sound generator creature to borrow sounds from.
    pub sound: String,
    /// `XSCL`, written only when it is not 1.0, as `tes3` does. `tes3` also clamps it to
    /// 0.5..2.0 on the way out; the file's value is kept here.
    pub scale: Option<f32>,
    /// The `FLAG` word less its blood type bits (10 to 12): the low byte is
    /// `consts::creature::*`, compared with a cast. `tes3` keeps only the known low bits;
    /// every other bit is kept here so the word comes back as it was.
    pub creature_flags: u32,
    /// Bits 10 to 12 of `FLAG`.
    pub blood_type: u8,
    pub data: CreatureData,
}

/// `NPDT`, 96 bytes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CreatureData {
    /// `consts::creature_type::*`.
    pub creature_type: u32,
    pub level: u32,
    pub strength: u32,
    pub intelligence: u32,
    pub willpower: u32,
    pub agility: u32,
    pub speed: u32,
    pub endurance: u32,
    pub personality: u32,
    pub luck: u32,
    pub health: u32,
    pub magicka: u32,
    pub fatigue: u32,
    pub soul: u32,
    pub combat: u32,
    pub magic: u32,
    pub stealth: u32,
    /// Minimum and maximum damage.
    pub attack1: (u32, u32),
    pub attack2: (u32, u32),
    pub attack3: (u32, u32),
    pub gold: u32,
}

impl CreatureData {
    pub const SIZE: usize = 96;

    pub fn read(r: &mut Reader<'_>) -> Result<CreatureData> {
        Ok(CreatureData {
            creature_type: r.u32()?,
            level: r.u32()?,
            strength: r.u32()?,
            intelligence: r.u32()?,
            willpower: r.u32()?,
            agility: r.u32()?,
            speed: r.u32()?,
            endurance: r.u32()?,
            personality: r.u32()?,
            luck: r.u32()?,
            health: r.u32()?,
            magicka: r.u32()?,
            fatigue: r.u32()?,
            soul: r.u32()?,
            combat: r.u32()?,
            magic: r.u32()?,
            stealth: r.u32()?,
            attack1: (r.u32()?, r.u32()?),
            attack2: (r.u32()?, r.u32()?),
            attack3: (r.u32()?, r.u32()?),
            gold: r.u32()?,
        })
    }

    pub fn write(&self, w: &mut Writer) {
        w.u32(self.creature_type);
        w.u32(self.level);
        w.u32(self.strength);
        w.u32(self.intelligence);
        w.u32(self.willpower);
        w.u32(self.agility);
        w.u32(self.speed);
        w.u32(self.endurance);
        w.u32(self.personality);
        w.u32(self.luck);
        w.u32(self.health);
        w.u32(self.magicka);
        w.u32(self.fatigue);
        w.u32(self.soul);
        w.u32(self.combat);
        w.u32(self.magic);
        w.u32(self.stealth);
        w.u32(self.attack1.0);
        w.u32(self.attack1.1);
        w.u32(self.attack2.0);
        w.u32(self.attack2.1);
        w.u32(self.attack3.0);
        w.u32(self.attack3.1);
        w.u32(self.gold);
    }
}

/// The three bits of `FLAG` that hold the blood type.
const BLOOD_TYPE_BITS: u32 = 0b111 << 10;

/// The `FLAG` word as the flags without the blood type bits, and the blood type.
fn unpack_flags(flags: u32) -> (u32, u8) {
    (flags & !BLOOD_TYPE_BITS, ((flags >> 10) & 0b111) as u8)
}

fn pack_flags(creature_flags: u32, blood_type: u8) -> u32 {
    (creature_flags & !BLOOD_TYPE_BITS) | (((blood_type & 0b111) as u32) << 10)
}

impl Typed for Creature {
    const TAG: Tag = *b"CREA";

    fn from_raw(rec: &Record) -> Result<Self> {
        check_tag(rec, &Self::TAG)?;
        let mut this = Self { flags: flags_of(rec), ..Default::default() };
        let mut it = Subs::new(rec);
        while let Some(s) = it.next() {
            match &s.tag {
                b"NAME" => this.id = text(s),
                b"MODL" => this.mesh = text(s),
                b"CNAM" => this.sound = text(s),
                b"FNAM" => this.name = text(s),
                b"SCRI" => this.script = text(s),
                b"NPDT" => this.data = CreatureData::read(&mut sized(rec, s, CreatureData::SIZE)?)?,
                b"FLAG" => (this.creature_flags, this.blood_type) = unpack_flags(u32_of(rec, s)?),
                b"XSCL" => this.scale = Some(f32_of(rec, s)?),
                b"NPCO" => {
                    let mut r = sized(rec, s, 36)?;
                    let count = r.i32()?;
                    let id = r.fixed_string(32)?;
                    this.inventory.push((count, id));
                }
                b"NPCS" => this.spells.push(text(s)),
                b"AIDT" => this.ai_data = Some(AiData::read(&mut sized(rec, s, AiData::SIZE)?)?),
                b"DODT" => this.travel_destinations.push(TravelDestination::read(rec, s, &mut it)?),
                b"AI_T" | b"AI_W" | b"AI_E" | b"AI_F" | b"AI_A" => {
                    this.ai_packages.push(AiPackage::read(rec, s, &mut it)?);
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
        out.text_if(b"MODL", &self.mesh)?;
        out.text_if(b"CNAM", &self.sound)?;
        out.text_if(b"FNAM", &self.name)?;
        out.text_if(b"SCRI", &self.script)?;
        out.sub(b"NPDT", |w| {
            self.data.write(w);
            Ok(())
        })?;
        out.u32(b"FLAG", pack_flags(self.creature_flags, self.blood_type));
        if let Some(scale) = self.scale {
            let scale_is_default = (scale - 1.0).abs() < 1e-6;
            if !scale_is_default {
                out.f32(b"XSCL", scale);
            }
        }
        for (count, id) in &self.inventory {
            out.sub(b"NPCO", |w| {
                w.i32(*count);
                w.fixed_string(id, 32)
            })?;
        }
        for spell in &self.spells {
            out.sub(b"NPCS", |w| w.fixed_string(spell, 32))?;
        }
        if let Some(ai) = &self.ai_data {
            out.sub(b"AIDT", |w| {
                ai.write(w);
                Ok(())
            })?;
        }
        for d in &self.travel_destinations {
            d.write(&mut out)?;
        }
        for p in &self.ai_packages {
            p.write(&mut out)?;
        }
        out.dele();
        Ok(out.finish())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_creature_round_trips_with_its_packages_and_a_short_npdt_is_refused() {
        let data = CreatureData {
            creature_type: consts::creature_type::NORMAL,
            level: 3,
            strength: 50,
            health: 60,
            attack1: (3, 9),
            attack2: (3, 9),
            attack3: (3, 9),
            gold: 0,
            ..Default::default()
        };
        let mut rec = Record::new(b"CREA");
        rec.push(b"NAME", b"nix-hound\0".to_vec());
        rec.push(b"MODL", b"r\\NixHound.NIF\0".to_vec());
        rec.push(b"CNAM", b"rat\0".to_vec());
        rec.push(b"FNAM", b"Nix-Hound\0".to_vec());
        let mut w = Writer::new();
        data.write(&mut w);
        assert_eq!(w.len(), CreatureData::SIZE);
        rec.push(b"NPDT", w.into_bytes());
        let flag = consts::creature::WALKS as u32 | consts::creature::RESPAWN as u32 | (2 << 10) | 0x8000;
        rec.push(b"FLAG", flag.to_le_bytes().to_vec());
        rec.push(b"XSCL", 1.3f32.to_le_bytes().to_vec());
        let mut w = Writer::new();
        w.i32(1);
        w.fixed_string("ingred_hound_meat_01", 32).unwrap();
        rec.push(b"NPCO", w.into_bytes());
        let mut w = Writer::new();
        w.fixed_string("hound_bite", 32).unwrap();
        rec.push(b"NPCS", w.into_bytes());
        let mut w = Writer::new();
        AiData { hello: 0, fight: 90, flee: 20, alarm: 0, services: 0 }.write(&mut w);
        rec.push(b"AIDT", w.into_bytes());
        let mut w = Writer::new();
        AiWanderPackage { distance: 1000, duration: 5, idle2: 50, idle3: 20, reset: 1, ..Default::default() }
            .write(&mut w);
        rec.push(b"AI_W", w.into_bytes());
        let mut w = Writer::new();
        AiFollowPackage {
            location: [0.0; 3],
            duration: 0,
            target: "player".into(),
            reset: 1,
            cell: String::new(),
        }
        .write(&mut w)
        .unwrap();
        rec.push(b"AI_F", w.into_bytes());
        rec.push(b"CNDT", b"Seyda Neen\0".to_vec());

        let c = Creature::from_raw(&rec).unwrap();
        assert_eq!(c.data, data);
        assert_eq!(
            c.creature_flags,
            consts::creature::WALKS as u32 | consts::creature::RESPAWN as u32 | 0x8000
        );
        assert_eq!(c.blood_type, 2);
        assert_eq!(c.scale, Some(1.3));
        assert_eq!(c.inventory, vec![(1, "ingred_hound_meat_01".to_string())]);
        assert_eq!(c.spells, vec!["hound_bite".to_string()]);
        assert_eq!(c.ai_data.as_ref().unwrap().fight, 90);
        assert_eq!(c.ai_packages.len(), 2);
        assert!(matches!(&c.ai_packages[1], AiPackage::Follow(p) if p.cell == "Seyda Neen"));
        assert_eq!(c.to_raw().unwrap(), rec);

        rec.subs[4].data.to_mut().truncate(95);
        let e = Creature::from_raw(&rec).unwrap_err();
        assert!(e.what.contains("CREA::NPDT is 95 bytes"), "{e}");
    }

    #[test]
    fn a_unit_scale_is_not_written_and_a_stray_cndt_is_refused() {
        let mut rec = Record::new(b"CREA");
        rec.push(b"NAME", b"x\0".to_vec());
        rec.push(b"NPDT", vec![0; 96]);
        rec.push(b"FLAG", vec![0; 4]);
        rec.push(b"AIDT", vec![0; 12]);
        let mut c = Creature::from_raw(&rec).unwrap();
        c.scale = Some(1.0);
        assert_eq!(c.to_raw().unwrap(), rec);

        rec.push(b"CNDT", b"nowhere\0".to_vec());
        let e = Creature::from_raw(&rec).unwrap_err();
        assert!(e.what.contains("CREA::CNDT"), "{e}");
    }
}
