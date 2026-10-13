//! `BOOK`: a book or scroll, text included. `tes3` book.rs.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Book {
    pub flags: u32,
    pub id: String,
    pub name: String,
    pub script: String,
    pub mesh: String,
    pub icon: String,
    pub enchanting: String,
    /// `TEXT`: the book's pages, as HTML-like markup.
    pub text: String,
    pub data: BookData,
}

/// `BKDT`, 20 bytes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BookData {
    pub weight: f32,
    pub value: u32,
    /// `consts::book_type::*`.
    pub book_type: u32,
    /// `consts::skill::*`, the skill the book teaches, `NONE` for none.
    pub skill: i32,
    pub enchantment: u32,
}

impl BookData {
    pub const SIZE: usize = 20;

    pub fn read(r: &mut Reader<'_>) -> Result<BookData> {
        Ok(BookData {
            weight: r.f32()?,
            value: r.u32()?,
            book_type: r.u32()?,
            skill: r.i32()?,
            enchantment: r.u32()?,
        })
    }

    pub fn write(&self, w: &mut Writer) {
        w.f32(self.weight);
        w.u32(self.value);
        w.u32(self.book_type);
        w.i32(self.skill);
        w.u32(self.enchantment);
    }
}

impl Typed for Book {
    const TAG: Tag = *b"BOOK";

    fn from_raw(rec: &Record) -> Result<Self> {
        check_tag(rec, &Self::TAG)?;
        let mut this = Self { flags: flags_of(rec), ..Default::default() };
        for s in &rec.subs {
            match &s.tag {
                b"NAME" => this.id = text(s),
                b"MODL" => this.mesh = text(s),
                b"FNAM" => this.name = text(s),
                b"BKDT" => this.data = BookData::read(&mut sized(rec, s, BookData::SIZE)?)?,
                b"SCRI" => this.script = text(s),
                b"ITEX" => this.icon = text(s),
                b"TEXT" => this.text = text(s),
                b"ENAM" => this.enchanting = text(s),
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
        out.sub(b"BKDT", |w| {
            self.data.write(w);
            Ok(())
        })?;
        out.text_if(b"SCRI", &self.script)?;
        out.text_if(b"ITEX", &self.icon)?;
        out.text_if(b"TEXT", &self.text)?;
        out.text_if(b"ENAM", &self.enchanting)?;
        out.dele();
        Ok(out.finish())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_book_round_trips_and_a_short_bkdt_is_refused() {
        let mut rec = Record::new(b"BOOK");
        rec.push(b"NAME", b"bk_abcs\0".to_vec());
        rec.push(b"MODL", b"m\\text_octavo_01.nif\0".to_vec());
        rec.push(b"FNAM", b"ABCs for Barbarians\0".to_vec());
        let data = BookData {
            weight: 3.0,
            value: 50,
            book_type: consts::book_type::BOOK,
            skill: consts::skill::SPEECHCRAFT,
            enchantment: 0,
        };
        let mut w = Writer::new();
        data.write(&mut w);
        rec.push(b"BKDT", w.into_bytes());
        rec.push(b"ITEX", b"m\\tx_octavo_01.dds\0".to_vec());
        rec.push(b"TEXT", b"<DIV ALIGN=\"CENTER\"><FONT COLOR=\"000000\">A is for Axe<BR>\0".to_vec());
        let b = Book::from_raw(&rec).unwrap();
        assert_eq!(b.data, data);
        assert!(b.text.starts_with("<DIV ALIGN"));
        assert_eq!(b.to_raw().unwrap(), rec);

        rec.subs[3].data.to_mut().truncate(16);
        let e = Book::from_raw(&rec).unwrap_err();
        assert!(e.what.contains("BOOK::BKDT is 16 bytes"), "{e}");
    }
}
