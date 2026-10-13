//! `ENAM`: one magic effect of a spell, an enchantment or a potion, carried by `SPEL`,
//! `ENCH` and `ALCH`. `tes3` effect.rs.

use super::*;

/// `ENAM`, 24 bytes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Effect {
    /// `consts::effect::*`, an `i16` here, so compare with a cast.
    pub magic_effect: i16,
    /// `consts::skill::*`, an `i8` here, so compare with a cast.
    pub skill: i8,
    /// `consts::attribute::*`, an `i8` here, so compare with a cast.
    pub attribute: i8,
    /// `consts::effect_range::*`.
    pub range: u32,
    pub area: u32,
    pub duration: u32,
    pub min_magnitude: u32,
    pub max_magnitude: u32,
}

impl Effect {
    pub const SIZE: usize = 24;

    pub fn read(r: &mut Reader<'_>) -> Result<Effect> {
        Ok(Effect {
            magic_effect: r.i16()?,
            skill: r.i8()?,
            attribute: r.i8()?,
            range: r.u32()?,
            area: r.u32()?,
            duration: r.u32()?,
            min_magnitude: r.u32()?,
            max_magnitude: r.u32()?,
        })
    }

    pub fn write(&self, w: &mut Writer) {
        w.i16(self.magic_effect);
        w.i8(self.skill);
        w.i8(self.attribute);
        w.u32(self.range);
        w.u32(self.area);
        w.u32(self.duration);
        w.u32(self.min_magnitude);
        w.u32(self.max_magnitude);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_effect_round_trips_and_a_short_one_says_where_it_ran_out() {
        let e = Effect {
            magic_effect: consts::effect::FORTIFY_SKILL as i16,
            skill: consts::skill::ATHLETICS as i8,
            attribute: consts::attribute::NONE as i8,
            range: consts::effect_range::ON_SELF,
            area: 0,
            duration: 60,
            min_magnitude: 5,
            max_magnitude: 10,
        };
        let mut w = Writer::new();
        e.write(&mut w);
        assert_eq!(w.len(), Effect::SIZE);
        assert_eq!(Effect::read(&mut Reader::new(&w.out)).unwrap(), e);

        let err = Effect::read(&mut Reader::new(&w.out[..20])).unwrap_err();
        assert_eq!(err.at, 20);
        assert!(err.what.contains("4 bytes wanted"), "{err}");
    }
}
