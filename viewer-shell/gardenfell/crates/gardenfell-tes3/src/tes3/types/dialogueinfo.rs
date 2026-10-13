//! `INFO`: one response in a dialogue topic, with the conditions under which it is said.
//! `tes3` dialogueinfo.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DialogueInfo {
    pub flags: u32,
    pub id: String,
    pub prev_id: String,
    pub next_id: String,
    pub data: DialogueData,
    pub speaker_id: String,
    pub speaker_race: String,
    pub speaker_class: String,
    pub speaker_faction: String,
    pub speaker_cell: String,
    pub player_faction: String,
    pub sound_path: String,
    /// `NAME`: the response. The engine allows 512 characters, so it is written without a
    /// NUL terminator, as `tes3` and the Construction Set do.
    pub text: String,
    /// Which of `QSTN`, `QSTF` and `QSTR` the record carries, if any. Their one byte is not
    /// read; the round trip writes a 1.
    pub quest_state: Option<QuestState>,
    /// `SCVR` with the `FLTV` or `INTV` after it, one per condition, in file order.
    pub filters: Vec<Filter>,
    pub script_text: String,
}

/// `DATA`, 12 bytes with one padding byte at the end.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DialogueData {
    /// `consts::dialogue_type::*`, a `u32` here, so compare with a cast.
    pub dialogue_type: u32,
    pub disposition: i32,
    pub speaker_rank: i8,
    /// `consts::sex::*`.
    pub speaker_sex: i8,
    pub player_rank: i8,
}

/// One condition: a packed `SCVR`, then the `FLTV` or `INTV` that gives its value.
///
/// `SCVR` is the index as a digit byte, the type byte, the two-byte function, the comparison
/// byte, then the id as the rest of the subrecord with no terminator.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Filter {
    /// The slot, 0 to 5, converted from its digit as `tes3` does.
    pub index: u8,
    /// `consts::filter_type::*`.
    pub filter_type: u8,
    /// `consts::filter_function::*`.
    pub function: u16,
    /// `consts::filter_comparison::*`.
    pub comparison: u8,
    pub id: String,
    /// `Float(0.0)` when neither `FLTV` nor `INTV` follows, which the round trip then
    /// writes as an `FLTV`, as `tes3` does.
    pub value: FilterValue,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FilterValue {
    Float(f32),
    Integer(i32),
}

impl Default for FilterValue {
    fn default() -> Self {
        FilterValue::Float(0.0)
    }
}

/// The journal marker a journal entry's `INFO` carries, named for its subrecord.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum QuestState {
    /// `QSTN`: the quest's name entry.
    #[default]
    Name,
    /// `QSTF`: finishes the quest.
    Finished,
    /// `QSTR`: restarts it.
    Restart,
}

impl DialogueData {
    pub const SIZE: usize = 12;

    pub fn read(r: &mut Reader<'_>) -> Result<DialogueData> {
        let dialogue_type = r.u32()?;
        let disposition = r.i32()?;
        let speaker_rank = r.i8()?;
        let speaker_sex = r.i8()?;
        let player_rank = r.i8()?;
        r.skip(1)?;
        Ok(DialogueData { dialogue_type, disposition, speaker_rank, speaker_sex, player_rank })
    }

    pub fn write(&self, w: &mut Writer) {
        w.u32(self.dialogue_type);
        w.i32(self.disposition);
        w.i8(self.speaker_rank);
        w.i8(self.speaker_sex);
        w.i8(self.player_rank);
        w.bytes(&[0; 1]);
    }
}

impl Filter {
    /// Reads a condition from its `SCVR` subrecord and the `FLTV` or `INTV` that may come
    /// next, which the cursor is positioned just past the `SCVR` to find.
    pub fn read(rec: &Record, scvr: &Sub, it: &mut Subs<'_>) -> Result<Filter> {
        if scvr.data.len() < 5 {
            return Err(Error {
                at: offset_of(rec, scvr),
                what: format!(
                    "{}::SCVR is {} bytes, the layout says at least 5",
                    rec.tag_str(),
                    scvr.data.len()
                ),
            });
        }
        let mut r = reader(scvr);
        let digit = r.u8()?;
        let filter_type = r.u8()?;
        let function = r.u16()?;
        let comparison = r.u8()?;
        let id = r.string_rest();
        let Some(index) = digit.checked_sub(b'0') else {
            return Err(Error {
                at: offset_of(rec, scvr),
                what: format!("{}::SCVR index byte {digit:#04x} is not a digit", rec.tag_str()),
            });
        };
        let value = match it.peek_tag() {
            Some(b"FLTV") => FilterValue::Float(f32_of(rec, it.expect(b"FLTV")?)?),
            Some(b"INTV") => FilterValue::Integer(i32_of(rec, it.expect(b"INTV")?)?),
            _ => FilterValue::default(),
        };
        Ok(Filter { index, filter_type, function, comparison, id, value })
    }

    /// Writes `SCVR`, then `FLTV` or `INTV` for the value.
    pub fn write(&self, out: &mut Out) -> Result<()> {
        let Some(digit) = self.index.checked_add(b'0') else {
            return Err(Error {
                at: 0,
                what: format!("INFO::SCVR filter index {} is not a digit", self.index),
            });
        };
        out.sub(b"SCVR", |w| {
            w.u8(digit);
            w.u8(self.filter_type);
            w.u16(self.function);
            w.u8(self.comparison);
            bare_text(w, &self.id)
        })?;
        match self.value {
            FilterValue::Float(v) => out.f32(b"FLTV", v),
            FilterValue::Integer(v) => out.i32(b"INTV", v),
        }
        Ok(())
    }
}

/// Text with no NUL terminator, as the `SCVR` id and the `NAME` text are stored.
fn bare_text(w: &mut Writer, s: &str) -> Result<()> {
    w.cstring(s)?;
    w.out.pop();
    Ok(())
}

impl Typed for DialogueInfo {
    const TAG: Tag = *b"INFO";

    fn from_raw(rec: &Record) -> Result<Self> {
        check_tag(rec, &Self::TAG)?;
        let mut this = Self { flags: flags_of(rec), ..Default::default() };
        let mut it = Subs::new(rec);
        while let Some(s) = it.next() {
            match &s.tag {
                b"INAM" => this.id = text(s),
                b"PNAM" => this.prev_id = text(s),
                b"NNAM" => this.next_id = text(s),
                b"DATA" => this.data = DialogueData::read(&mut sized(rec, s, DialogueData::SIZE)?)?,
                b"ONAM" => this.speaker_id = text(s),
                b"RNAM" => this.speaker_race = text(s),
                b"CNAM" => this.speaker_class = text(s),
                b"FNAM" => this.speaker_faction = text(s),
                b"ANAM" => this.speaker_cell = text(s),
                b"DNAM" => this.player_faction = text(s),
                b"SNAM" => this.sound_path = text(s),
                b"NAME" => this.text = text(s),
                b"QSTN" => this.quest_state = Some(QuestState::Name),
                b"QSTF" => this.quest_state = Some(QuestState::Finished),
                b"QSTR" => this.quest_state = Some(QuestState::Restart),
                b"SCVR" => this.filters.push(Filter::read(rec, s, &mut it)?),
                b"BNAM" => this.script_text = text(s),
                b"DELE" => {}
                _ => return Err(unexpected(rec, s)),
            }
        }
        Ok(this)
    }

    fn to_raw(&self) -> Result<Record<'static>> {
        let mut out = Out::new(&Self::TAG, self.flags);
        out.text(b"INAM", &self.id)?;
        out.text(b"PNAM", &self.prev_id)?;
        out.text(b"NNAM", &self.next_id)?;
        out.sub(b"DATA", |w| {
            self.data.write(w);
            Ok(())
        })?;
        out.text_if(b"ONAM", &self.speaker_id)?;
        out.text_if(b"RNAM", &self.speaker_race)?;
        out.text_if(b"CNAM", &self.speaker_class)?;
        out.text_if(b"FNAM", &self.speaker_faction)?;
        out.text_if(b"ANAM", &self.speaker_cell)?;
        out.text_if(b"DNAM", &self.player_faction)?;
        out.text_if(b"SNAM", &self.sound_path)?;
        if !self.text.is_empty() {
            out.sub(b"NAME", |w| bare_text(w, &self.text))?;
        }
        match self.quest_state {
            Some(QuestState::Name) => out.u8(b"QSTN", 1),
            Some(QuestState::Finished) => out.u8(b"QSTF", 1),
            Some(QuestState::Restart) => out.u8(b"QSTR", 1),
            None => {}
        }
        for filter in &self.filters {
            filter.write(&mut out)?;
        }
        out.text_if(b"BNAM", &self.script_text)?;
        out.dele();
        Ok(out.finish())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scvr(index: u8, filter_type: u8, function: u16, comparison: u8, id: &str) -> Vec<u8> {
        let mut w = Writer::new();
        w.u8(index + b'0');
        w.u8(filter_type);
        w.u16(function);
        w.u8(comparison);
        w.bytes(id.as_bytes());
        w.into_bytes()
    }

    #[test]
    fn an_info_round_trips_with_its_filters_and_a_stray_value_is_refused() {
        let data = DialogueData {
            dialogue_type: consts::dialogue_type::TOPIC as u32,
            disposition: 30,
            speaker_rank: -1,
            speaker_sex: consts::sex::ANY,
            player_rank: -1,
        };
        let mut rec = Record::new(b"INFO");
        rec.push(b"INAM", b"1234567890\0".to_vec());
        rec.push(b"PNAM", b"\0".to_vec());
        rec.push(b"NNAM", b"2345678901\0".to_vec());
        let mut w = Writer::new();
        data.write(&mut w);
        assert_eq!(w.len(), DialogueData::SIZE);
        rec.push(b"DATA", w.into_bytes());
        rec.push(b"RNAM", b"Dark Elf\0".to_vec());
        rec.push(b"ANAM", b"Balmora\0".to_vec());
        rec.push(b"NAME", b"The grass is greener here.".to_vec());
        rec.push(b"QSTN", vec![1]);
        rec.push(
            b"SCVR",
            scvr(
                0,
                consts::filter_type::JOURNAL,
                consts::filter_function::JOURNAL_TYPE,
                consts::filter_comparison::GREATER_EQUAL,
                "A1_1_FindSpymaster",
            ),
        );
        rec.push(b"INTV", 10i32.to_le_bytes().to_vec());
        let (function, greater) = (consts::filter_function::PC_LEVEL, consts::filter_comparison::GREATER);
        rec.push(b"SCVR", scvr(1, consts::filter_type::FUNCTION, function, greater, ""));
        rec.push(b"FLTV", 5.0f32.to_le_bytes().to_vec());
        rec.push(b"BNAM", b"Journal A1_1_FindSpymaster 20\0".to_vec());

        let i = DialogueInfo::from_raw(&rec).unwrap();
        assert_eq!(i.data, data);
        assert_eq!(i.prev_id, "");
        assert_eq!(i.text, "The grass is greener here.");
        assert_eq!(i.quest_state, Some(QuestState::Name));
        assert_eq!(i.filters.len(), 2);
        assert_eq!(
            i.filters[0],
            Filter {
                index: 0,
                filter_type: consts::filter_type::JOURNAL,
                function: consts::filter_function::JOURNAL_TYPE,
                comparison: consts::filter_comparison::GREATER_EQUAL,
                id: "A1_1_FindSpymaster".into(),
                value: FilterValue::Integer(10),
            }
        );
        assert_eq!(i.filters[1].index, 1);
        assert!(i.filters[1].id.is_empty());
        assert_eq!(i.filters[1].value, FilterValue::Float(5.0));
        assert_eq!(i.to_raw().unwrap(), rec);

        let mut stray = rec.clone();
        stray.push(b"FLTV", 1.0f32.to_le_bytes().to_vec());
        let e = DialogueInfo::from_raw(&stray).unwrap_err();
        assert!(e.what.contains("INFO::FLTV"), "{e}");
    }

    #[test]
    fn a_filter_without_a_value_gets_a_zero_fltv_and_a_short_scvr_is_refused() {
        let mut rec = Record::new(b"INFO");
        rec.push(b"INAM", b"1\0".to_vec());
        rec.push(b"PNAM", b"\0".to_vec());
        rec.push(b"NNAM", b"\0".to_vec());
        rec.push(b"DATA", vec![0; 12]);
        let (function, equal) = (consts::filter_function::DEAD_TYPE, consts::filter_comparison::EQUAL);
        rec.push(b"SCVR", scvr(0, consts::filter_type::DEAD, function, equal, "fargoth"));
        let i = DialogueInfo::from_raw(&rec).unwrap();
        assert_eq!(i.filters[0].value, FilterValue::Float(0.0));
        let back = i.to_raw().unwrap();
        assert_eq!(back.subs.len(), 6);
        assert_eq!(back.subs[5].tag, *b"FLTV");
        assert_eq!(back.subs[..5], rec.subs[..]);

        rec.subs[4].data.to_mut().truncate(3);
        let e = DialogueInfo::from_raw(&rec).unwrap_err();
        assert!(e.what.contains("INFO::SCVR is 3 bytes"), "{e}");
        assert_eq!(e.at, 8 + 2 + 8 + 1 + 8 + 1 + 8 + 12 + 8);
    }
}
