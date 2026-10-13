//! `REGN`: a region, its weather and ambient sounds. `tes3` region.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Region {
    pub flags: u32,
    pub id: String,
    pub name: String,
    pub weather_chances: WeatherChances,
    pub sleep_creature: String,
    /// `CNAM`: RGBA, the color on the world map.
    pub map_color: [u8; 4],
    /// `SNAM`, one per sound: a 32-byte fixed string and a one-byte chance.
    pub sounds: Vec<(String, u8)>,
}

/// `WEAT`: a percent chance per weather. 8 bytes in a Morrowind-only file, 10 with
/// Bloodmoon's snow and blizzard. `tes3` writes every region back as 10 bytes; here the
/// two Bloodmoon chances are `None` for a region that had 8, and the record comes back
/// at its own size.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WeatherChances {
    pub clear: u8,
    pub cloudy: u8,
    pub foggy: u8,
    pub overcast: u8,
    pub rain: u8,
    pub thunder: u8,
    pub ash: u8,
    pub blight: u8,
    /// Snow and blizzard, present in a 10-byte `WEAT` (Bloodmoon and later).
    pub snow: Option<u8>,
    pub blizzard: Option<u8>,
}

impl WeatherChances {
    /// Reads 8 or 10 bytes, whichever the reader holds.
    pub fn read(r: &mut Reader<'_>) -> Result<WeatherChances> {
        let mut this = WeatherChances {
            clear: r.u8()?,
            cloudy: r.u8()?,
            foggy: r.u8()?,
            overcast: r.u8()?,
            rain: r.u8()?,
            thunder: r.u8()?,
            ash: r.u8()?,
            blight: r.u8()?,
            snow: None,
            blizzard: None,
        };
        if !r.at_end() {
            this.snow = Some(r.u8()?);
            this.blizzard = Some(r.u8()?);
        }
        Ok(this)
    }

    pub fn write(&self, w: &mut Writer) {
        w.u8(self.clear);
        w.u8(self.cloudy);
        w.u8(self.foggy);
        w.u8(self.overcast);
        w.u8(self.rain);
        w.u8(self.thunder);
        w.u8(self.ash);
        w.u8(self.blight);
        if let (Some(snow), Some(blizzard)) = (self.snow, self.blizzard) {
            w.u8(snow);
            w.u8(blizzard);
        }
    }
}

impl Typed for Region {
    const TAG: Tag = *b"REGN";

    fn from_raw(rec: &Record) -> Result<Self> {
        check_tag(rec, &Self::TAG)?;
        let mut this = Self { flags: flags_of(rec), ..Default::default() };
        for s in &rec.subs {
            match &s.tag {
                b"NAME" => this.id = text(s),
                b"FNAM" => this.name = text(s),
                b"WEAT" => {
                    let want = if s.data.len() == 8 { 8 } else { 10 };
                    this.weather_chances = WeatherChances::read(&mut sized(rec, s, want)?)?;
                }
                b"BNAM" => this.sleep_creature = text(s),
                b"CNAM" => this.map_color = sized(rec, s, 4)?.array::<4>()?,
                b"SNAM" => {
                    let mut r = sized(rec, s, 33)?;
                    let sound = r.fixed_string(32)?;
                    let chance = r.u8()?;
                    this.sounds.push((sound, chance));
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
        out.sub(b"WEAT", |w| {
            self.weather_chances.write(w);
            Ok(())
        })?;
        out.text_if(b"BNAM", &self.sleep_creature)?;
        out.bytes(b"CNAM", self.map_color.to_vec());
        for (sound, chance) in &self.sounds {
            out.sub(b"SNAM", |w| {
                w.fixed_string(sound, 32)?;
                w.u8(*chance);
                Ok(())
            })?;
        }
        out.dele();
        Ok(out.finish())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_region_round_trips_and_an_eight_byte_weat_is_written_as_ten() {
        let mut rec = Record::new(b"REGN");
        rec.push(b"NAME", b"Grazelands Region\0".to_vec());
        rec.push(b"FNAM", b"Grazelands Region\0".to_vec());
        rec.push(b"WEAT", vec![50, 20, 10, 10, 5, 5, 0, 0, 0, 0]);
        rec.push(b"BNAM", b"ex_grazelands_sleep\0".to_vec());
        rec.push(b"CNAM", vec![120, 160, 60, 0]);
        let mut w = Writer::new();
        w.fixed_string("Wind Light", 32).unwrap();
        w.u8(40);
        rec.push(b"SNAM", w.into_bytes());
        let mut w = Writer::new();
        w.fixed_string("Birds", 32).unwrap();
        w.u8(10);
        rec.push(b"SNAM", w.into_bytes());
        let r = Region::from_raw(&rec).unwrap();
        assert_eq!(r.weather_chances.clear, 50);
        assert_eq!(r.map_color, [120, 160, 60, 0]);
        assert_eq!(r.sounds, vec![("Wind Light".to_string(), 40), ("Birds".to_string(), 10)]);
        assert_eq!(r.to_raw().unwrap(), rec);

        let mut short = rec.clone();
        short.subs[2].data.to_mut().truncate(8);
        let r = Region::from_raw(&short).unwrap();
        assert_eq!(r.weather_chances.snow, None);
        assert_eq!(r.to_raw().unwrap(), short);
    }

    #[test]
    fn a_nine_byte_weat_and_a_short_snam_are_refused() {
        let mut rec = Record::new(b"REGN");
        rec.push(b"NAME", b"x\0".to_vec());
        rec.push(b"WEAT", vec![0; 9]);
        let e = Region::from_raw(&rec).unwrap_err();
        assert!(e.what.contains("REGN::WEAT is 9 bytes"), "{e}");

        let mut rec = Record::new(b"REGN");
        rec.push(b"NAME", b"x\0".to_vec());
        rec.push(b"SNAM", vec![0; 32]);
        let e = Region::from_raw(&rec).unwrap_err();
        assert!(e.what.contains("REGN::SNAM is 32 bytes"), "{e}");
    }
}
