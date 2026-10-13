//! `AIDT`: an actor's AI ratings and the services it offers, carried by `NPC_` and
//! `CREA`. `tes3` aidata.rs.

use super::*;

/// `AIDT`, 12 bytes: the four ratings, three padding bytes, then the service flags.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AiData {
    pub hello: i16,
    pub fight: i8,
    pub flee: i8,
    pub alarm: i8,
    /// `consts::service::*`.
    pub services: u32,
}

impl AiData {
    pub const SIZE: usize = 12;

    pub fn read(r: &mut Reader<'_>) -> Result<AiData> {
        let hello = r.i16()?;
        let fight = r.i8()?;
        let flee = r.i8()?;
        let alarm = r.i8()?;
        r.skip(3)?;
        let services = r.u32()?;
        Ok(AiData { hello, fight, flee, alarm, services })
    }

    pub fn write(&self, w: &mut Writer) {
        w.i16(self.hello);
        w.i8(self.fight);
        w.i8(self.flee);
        w.i8(self.alarm);
        w.bytes(&[0; 3]);
        w.u32(self.services);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ai_data_round_trips_and_a_short_one_says_where_it_ran_out() {
        let a = AiData {
            hello: 30,
            fight: 30,
            flee: 20,
            alarm: -10,
            services: consts::service::BARTERS_INGREDIENTS | consts::service::OFFERS_TRAINING,
        };
        let mut w = Writer::new();
        a.write(&mut w);
        assert_eq!(w.len(), AiData::SIZE);
        assert_eq!(&w.out[5..8], &[0, 0, 0]);
        assert_eq!(AiData::read(&mut Reader::new(&w.out)).unwrap(), a);

        let err = AiData::read(&mut Reader::new(&w.out[..8])).unwrap_err();
        assert_eq!(err.at, 8);
        assert!(err.what.contains("4 bytes wanted"), "{err}");
    }
}
