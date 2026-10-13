//! `CLAS`: a character class. `tes3` class.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Class {
    pub flags: u32,
    pub id: String,
    pub name: String,
    pub description: String,
    pub data: ClassData,
}

/// `CLDT`, 60 bytes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ClassData {
    /// `consts::attribute::*`.
    pub attribute1: i32,
    /// `consts::attribute::*`.
    pub attribute2: i32,
    /// `consts::specialization::*`.
    pub specialization: i32,
    /// `consts::skill::*`, as are the nine below.
    pub minor1: i32,
    pub major1: i32,
    pub minor2: i32,
    pub major2: i32,
    pub minor3: i32,
    pub major3: i32,
    pub minor4: i32,
    pub major4: i32,
    pub minor5: i32,
    pub major5: i32,
    /// `consts::class::*`.
    pub flags: u32,
    /// `consts::service::*`.
    pub services: u32,
}

impl ClassData {
    pub const SIZE: usize = 60;

    pub fn read(r: &mut Reader<'_>) -> Result<ClassData> {
        Ok(ClassData {
            attribute1: r.i32()?,
            attribute2: r.i32()?,
            specialization: r.i32()?,
            minor1: r.i32()?,
            major1: r.i32()?,
            minor2: r.i32()?,
            major2: r.i32()?,
            minor3: r.i32()?,
            major3: r.i32()?,
            minor4: r.i32()?,
            major4: r.i32()?,
            minor5: r.i32()?,
            major5: r.i32()?,
            flags: r.u32()?,
            services: r.u32()?,
        })
    }

    pub fn write(&self, w: &mut Writer) {
        w.i32(self.attribute1);
        w.i32(self.attribute2);
        w.i32(self.specialization);
        w.i32(self.minor1);
        w.i32(self.major1);
        w.i32(self.minor2);
        w.i32(self.major2);
        w.i32(self.minor3);
        w.i32(self.major3);
        w.i32(self.minor4);
        w.i32(self.major4);
        w.i32(self.minor5);
        w.i32(self.major5);
        w.u32(self.flags);
        w.u32(self.services);
    }
}

impl Typed for Class {
    const TAG: Tag = *b"CLAS";

    fn from_raw(rec: &Record) -> Result<Self> {
        check_tag(rec, &Self::TAG)?;
        let mut this = Self { flags: flags_of(rec), ..Default::default() };
        for s in &rec.subs {
            match &s.tag {
                b"NAME" => this.id = text(s),
                b"FNAM" => this.name = text(s),
                b"CLDT" => this.data = ClassData::read(&mut sized(rec, s, ClassData::SIZE)?)?,
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
        out.sub(b"CLDT", |w| {
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
    fn a_class_round_trips_and_a_short_cldt_is_refused() {
        let data = ClassData {
            attribute1: consts::attribute::STRENGTH,
            attribute2: consts::attribute::ENDURANCE,
            specialization: consts::specialization::COMBAT,
            minor1: consts::skill::ARMORER,
            major1: consts::skill::LONG_BLADE,
            minor2: consts::skill::BLOCK,
            major2: consts::skill::HEAVY_ARMOR,
            minor3: consts::skill::MEDIUM_ARMOR,
            major3: consts::skill::ATHLETICS,
            minor4: consts::skill::SPEAR,
            major4: consts::skill::BLUNT_WEAPON,
            minor5: consts::skill::AXE,
            major5: consts::skill::ARMORER,
            flags: consts::class::PLAYABLE,
            services: consts::service::BARTERS_WEAPONS | consts::service::OFFERS_REPAIRS,
        };
        let mut rec = Record::new(b"CLAS");
        rec.push(b"NAME", b"Warrior\0".to_vec());
        rec.push(b"FNAM", b"Warrior\0".to_vec());
        let mut w = Writer::new();
        data.write(&mut w);
        assert_eq!(w.len(), ClassData::SIZE);
        rec.push(b"CLDT", w.into_bytes());
        rec.push(b"DESC", b"Fighters.\0".to_vec());
        let c = Class::from_raw(&rec).unwrap();
        assert_eq!(c.data, data);
        assert_eq!(c.to_raw().unwrap(), rec);

        rec.subs[2].data.to_mut().pop();
        let e = Class::from_raw(&rec).unwrap_err();
        assert!(e.what.contains("CLAS::CLDT is 59 bytes"), "{e}");
    }
}
