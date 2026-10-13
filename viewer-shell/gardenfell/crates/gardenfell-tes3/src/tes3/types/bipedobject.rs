//! `INDX` with its `BNAM` and `CNAM`: one body slot of a piece of armor or clothing and
//! the body parts worn in it, carried by `ARMO` and `CLOT`. `tes3` bipedobject.rs.

use super::*;

/// An `INDX` byte, then an optional `BNAM` (the male body part) and an optional `CNAM`
/// (the female one), in whichever order the file has them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BipedObject {
    /// `consts::biped_object_type::*`.
    pub biped_object_type: u8,
    pub male_bodypart: String,
    pub female_bodypart: String,
}

impl BipedObject {
    /// Reads the slot from its `INDX` subrecord and up to two name subrecords after it,
    /// which the cursor is positioned just past the `INDX` to find. A subrecord that is
    /// neither `BNAM` nor `CNAM` ends the slot and is left for the caller.
    pub fn read(rec: &Record, indx: &Sub, it: &mut Subs<'_>) -> Result<BipedObject> {
        let mut this = BipedObject { biped_object_type: sized(rec, indx, 1)?.u8()?, ..Default::default() };
        for _ in 0..2 {
            match it.peek_tag() {
                Some(b"BNAM") => {
                    if let Some(s) = it.next() {
                        this.male_bodypart = text(s);
                    }
                }
                Some(b"CNAM") => {
                    if let Some(s) = it.next() {
                        this.female_bodypart = text(s);
                    }
                }
                _ => break,
            }
        }
        Ok(this)
    }

    /// Writes `INDX`, then `BNAM` and `CNAM` when they are not empty.
    pub fn write(&self, out: &mut Out) -> Result<()> {
        out.u8(b"INDX", self.biped_object_type);
        out.text_if(b"BNAM", &self.male_bodypart)?;
        out.text_if(b"CNAM", &self.female_bodypart)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_slot_takes_its_names_in_either_order_and_stops_at_a_stranger() {
        let mut rec = Record::new(b"ARMO");
        rec.push(b"INDX", vec![consts::biped_object_type::CHEST]);
        rec.push(b"CNAM", b"a_f_chest\0".to_vec());
        rec.push(b"BNAM", b"a_m_chest\0".to_vec());
        rec.push(b"INDX", vec![consts::biped_object_type::TAIL]);
        rec.push(b"ENAM", b"ench\0".to_vec());
        let mut it = Subs::new(&rec);
        let first = it.next().unwrap();
        let b = BipedObject::read(&rec, first, &mut it).unwrap();
        assert_eq!(
            b,
            BipedObject {
                biped_object_type: consts::biped_object_type::CHEST,
                male_bodypart: "a_m_chest".into(),
                female_bodypart: "a_f_chest".into(),
            }
        );
        let second = it.next().unwrap();
        assert_eq!(second.tag, *b"INDX");
        let b2 = BipedObject::read(&rec, second, &mut it).unwrap();
        assert_eq!(b2.biped_object_type, consts::biped_object_type::TAIL);
        assert!(b2.male_bodypart.is_empty());
        assert_eq!(it.peek_tag(), Some(b"ENAM"));

        let mut out = Out::new(b"ARMO", 0);
        b.write(&mut out).unwrap();
        b2.write(&mut out).unwrap();
        let back = out.finish();
        assert_eq!(back.subs.len(), 4);
        assert_eq!(back.subs[0], rec.subs[0]);
        assert_eq!(back.subs[1], rec.subs[2]);
        assert_eq!(back.subs[2], rec.subs[1]);
        assert_eq!(back.subs[3], rec.subs[3]);

        let mut bad = Record::new(b"ARMO");
        bad.push(b"INDX", vec![1, 0]);
        let mut it = Subs::new(&bad);
        let s = it.next().unwrap();
        let e = BipedObject::read(&bad, s, &mut it).unwrap_err();
        assert!(e.what.contains("ARMO::INDX is 2 bytes"), "{e}");
    }
}
