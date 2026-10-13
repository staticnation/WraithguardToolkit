//! `NPC_`: a non-player character. `tes3` npc.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Npc {
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
    pub race: String,
    pub class: String,
    pub faction: String,
    pub head: String,
    pub hair: String,
    /// The `FLAG` word less its blood type bits (10 to 12): the low byte is
    /// `consts::npc::*`, compared with a cast. `tes3` keeps only the known low bits; every
    /// other bit is kept here so the word comes back as it was.
    pub npc_flags: u32,
    /// Bits 10 to 12 of `FLAG`.
    pub blood_type: u8,
    pub data: NpcData,
}

/// `NPDT`, 52 bytes with the stats or 12 without them, which is the form the Construction
/// Set writes for an auto-calculated NPC.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NpcData {
    pub level: i16,
    /// Present in the 52-byte form only.
    pub stats: Option<NpcStats>,
    pub disposition: i8,
    pub reputation: i8,
    pub rank: i8,
    pub gold: u32,
}

/// The stats inside a 52-byte `NPDT`, 42 bytes with one padding byte after the skills.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NpcStats {
    /// In `consts::attribute::*` order.
    pub attributes: [u8; 8],
    /// In `consts::skill::*` order.
    pub skills: [u8; 27],
    pub health: u16,
    pub magicka: u16,
    pub fatigue: u16,
}

impl NpcStats {
    pub const SIZE: usize = 42;

    pub fn read(r: &mut Reader<'_>) -> Result<NpcStats> {
        let attributes = r.array::<8>()?;
        let skills = r.array::<27>()?;
        r.skip(1)?;
        let health = r.u16()?;
        let magicka = r.u16()?;
        let fatigue = r.u16()?;
        Ok(NpcStats { attributes, skills, health, magicka, fatigue })
    }

    pub fn write(&self, w: &mut Writer) {
        w.bytes(&self.attributes);
        w.bytes(&self.skills);
        w.bytes(&[0; 1]);
        w.u16(self.health);
        w.u16(self.magicka);
        w.u16(self.fatigue);
    }
}

impl NpcData {
    /// The size with the stats.
    pub const FULL_SIZE: usize = 52;
    /// The size without them.
    pub const AUTO_CALC_SIZE: usize = 12;

    /// Reads whichever form the reader's length says it holds: the stats are present when
    /// it is `FULL_SIZE`, and the padding after the rank is one byte then, three otherwise.
    pub fn read(r: &mut Reader<'_>) -> Result<NpcData> {
        let level = r.i16()?;
        let stats = if r.len() == Self::FULL_SIZE { Some(NpcStats::read(r)?) } else { None };
        let disposition = r.i8()?;
        let reputation = r.i8()?;
        let rank = r.i8()?;
        r.skip(if stats.is_some() { 1 } else { 3 })?;
        let gold = r.u32()?;
        Ok(NpcData { level, stats, disposition, reputation, rank, gold })
    }

    pub fn write(&self, w: &mut Writer) {
        w.i16(self.level);
        if let Some(stats) = &self.stats {
            stats.write(w);
        }
        w.i8(self.disposition);
        w.i8(self.reputation);
        w.i8(self.rank);
        let padding: &[u8] = if self.stats.is_some() { &[0; 1] } else { &[0; 3] };
        w.bytes(padding);
        w.u32(self.gold);
    }
}

/// The three bits of `FLAG` that hold the blood type.
const BLOOD_TYPE_BITS: u32 = 0b111 << 10;

/// The `FLAG` word as the flags without the blood type bits, and the blood type.
fn unpack_flags(flags: u32) -> (u32, u8) {
    (flags & !BLOOD_TYPE_BITS, ((flags >> 10) & 0b111) as u8)
}

fn pack_flags(npc_flags: u32, blood_type: u8) -> u32 {
    (npc_flags & !BLOOD_TYPE_BITS) | (((blood_type & 0b111) as u32) << 10)
}

impl Typed for Npc {
    const TAG: Tag = *b"NPC_";

    fn from_raw(rec: &Record) -> Result<Self> {
        check_tag(rec, &Self::TAG)?;
        let mut this = Self { flags: flags_of(rec), ..Default::default() };
        let mut it = Subs::new(rec);
        while let Some(s) = it.next() {
            match &s.tag {
                b"NAME" => this.id = text(s),
                b"MODL" => this.mesh = text(s),
                b"FNAM" => this.name = text(s),
                b"RNAM" => this.race = text(s),
                b"CNAM" => this.class = text(s),
                b"ANAM" => this.faction = text(s),
                b"BNAM" => this.head = text(s),
                b"KNAM" => this.hair = text(s),
                b"SCRI" => this.script = text(s),
                b"NPDT" => {
                    let want = if s.data.len() == NpcData::AUTO_CALC_SIZE {
                        NpcData::AUTO_CALC_SIZE
                    } else {
                        NpcData::FULL_SIZE
                    };
                    this.data = NpcData::read(&mut sized(rec, s, want)?)?;
                }
                b"FLAG" => (this.npc_flags, this.blood_type) = unpack_flags(u32_of(rec, s)?),
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
        out.text_if(b"FNAM", &self.name)?;
        out.text_if(b"RNAM", &self.race)?;
        out.text_if(b"CNAM", &self.class)?;
        out.text_if(b"ANAM", &self.faction)?;
        out.text_if(b"BNAM", &self.head)?;
        out.text_if(b"KNAM", &self.hair)?;
        out.text_if(b"SCRI", &self.script)?;
        out.sub(b"NPDT", |w| {
            self.data.write(w);
            Ok(())
        })?;
        out.u32(b"FLAG", pack_flags(self.npc_flags, self.blood_type));
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
    fn an_npc_with_stats_round_trips_and_a_wrong_npdt_is_refused() {
        let mut stats = NpcStats { health: 80, magicka: 90, fatigue: 200, ..Default::default() };
        stats.attributes = [40, 50, 50, 40, 40, 40, 40, 40];
        stats.skills[consts::skill::ALCHEMY as usize] = 70;
        let data =
            NpcData { level: 12, stats: Some(stats), disposition: 50, reputation: 5, rank: 2, gold: 100 };
        let mut rec = Record::new(b"NPC_");
        rec.push(b"NAME", b"ajira\0".to_vec());
        rec.push(b"FNAM", b"Ajira\0".to_vec());
        rec.push(b"RNAM", b"Khajiit\0".to_vec());
        rec.push(b"CNAM", b"Alchemist\0".to_vec());
        rec.push(b"ANAM", b"Mages Guild\0".to_vec());
        rec.push(b"BNAM", b"b_n_khajiit_f_head_01\0".to_vec());
        rec.push(b"KNAM", b"b_n_khajiit_f_hair01\0".to_vec());
        rec.push(b"SCRI", b"ajiraScript\0".to_vec());
        let mut w = Writer::new();
        data.write(&mut w);
        assert_eq!(w.len(), NpcData::FULL_SIZE);
        rec.push(b"NPDT", w.into_bytes());
        let flag = consts::npc::FEMALE as u32 | consts::npc::IS_BASE as u32 | (1 << 10);
        rec.push(b"FLAG", flag.to_le_bytes().to_vec());
        let mut w = Writer::new();
        w.i32(1);
        w.fixed_string("common_robe_02", 32).unwrap();
        rec.push(b"NPCO", w.into_bytes());
        let mut w = Writer::new();
        w.fixed_string("fire bite", 32).unwrap();
        rec.push(b"NPCS", w.into_bytes());
        let mut w = Writer::new();
        AiData { hello: 30, fight: 30, flee: 30, alarm: 0, services: consts::service::BARTERS_INGREDIENTS }
            .write(&mut w);
        rec.push(b"AIDT", w.into_bytes());
        let mut w = Writer::new();
        w.f32s(&[-3.0, 2.0, 1.0]);
        w.f32s(&[0.0, 0.0, 0.0]);
        rec.push(b"DODT", w.into_bytes());
        rec.push(b"DNAM", b"Balmora, Guild of Mages\0".to_vec());
        let mut w = Writer::new();
        AiWanderPackage { distance: 128, duration: 5, idle2: 60, idle3: 20, reset: 1, ..Default::default() }
            .write(&mut w);
        rec.push(b"AI_W", w.into_bytes());

        let n = Npc::from_raw(&rec).unwrap();
        assert_eq!(n.data, data);
        assert_eq!(n.race, "Khajiit");
        assert_eq!(n.npc_flags, consts::npc::FEMALE as u32 | consts::npc::IS_BASE as u32);
        assert_eq!(n.blood_type, 1);
        assert_eq!(n.travel_destinations.len(), 1);
        assert_eq!(n.travel_destinations[0].cell, "Balmora, Guild of Mages");
        assert_eq!(n.ai_packages.len(), 1);
        assert_eq!(n.to_raw().unwrap(), rec);

        rec.subs[8].data.to_mut().truncate(13);
        let e = Npc::from_raw(&rec).unwrap_err();
        assert!(e.what.contains("NPC_::NPDT is 13 bytes"), "{e}");
    }

    #[test]
    fn an_auto_calculated_npc_has_a_twelve_byte_npdt() {
        let data = NpcData { level: 5, stats: None, disposition: 50, reputation: 0, rank: 0, gold: 0 };
        let mut rec = Record::new(b"NPC_");
        rec.push(b"NAME", b"guard\0".to_vec());
        let mut w = Writer::new();
        data.write(&mut w);
        assert_eq!(w.len(), NpcData::AUTO_CALC_SIZE);
        rec.push(b"NPDT", w.into_bytes());
        rec.push(b"FLAG", (consts::npc::AUTO_CALCULATE as u32).to_le_bytes().to_vec());
        rec.push(b"AIDT", vec![0; 12]);
        let n = Npc::from_raw(&rec).unwrap();
        assert_eq!(n.data, data);
        assert_eq!(n.npc_flags, consts::npc::AUTO_CALCULATE as u32);
        assert_eq!(n.to_raw().unwrap(), rec);
    }
}
