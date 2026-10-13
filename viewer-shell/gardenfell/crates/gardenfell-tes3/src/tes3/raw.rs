//! A plugin as records of subrecords, exactly as the file holds them.
//!
//! Round 18gg. This is the layer that round-trips every file byte for byte: a record is
//! its tag, the two header dwords and its subrecords in file order, each subrecord its
//! tag and its bytes. Nothing is interpreted, so nothing can be lost - a subrecord no
//! reader knows, a duplicated one, a padding byte, the order a tool wrote them in all
//! come back as they went in. The typed layer ([`super::types`]) reads *from* a
//! [`Record`] and writes *to* one, so a typed object round-trips exactly where the file
//! was written the canonical way and the difference is measurable where it was not.
//!
//! Round 18gi: a subrecord read from a file borrows its bytes from the file's buffer
//! (`Cow::Borrowed`), and one built for writing owns them (`Cow::Owned`). Reading
//! Morrowind.esm made 1.5 million small allocations before this and makes none now; the
//! engine's plugin parse, which went through here since 18gh, had tripled in time for it.
//! A record that has to outlive its buffer takes [`Record::into_owned`].

use std::borrow::Cow;

use super::bytes::{Error, Reader, Result, Writer};

pub type Tag = [u8; 4];

/// One subrecord: its tag and its bytes, borrowed from the file or owned.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sub<'a> {
    pub tag: Tag,
    pub data: Cow<'a, [u8]>,
}

impl<'a> Sub<'a> {
    pub fn new(tag: &Tag, data: Vec<u8>) -> Sub<'static> {
        Sub { tag: *tag, data: Cow::Owned(data) }
    }
    pub fn borrowed(tag: &Tag, data: &'a [u8]) -> Sub<'a> {
        Sub { tag: *tag, data: Cow::Borrowed(data) }
    }
    pub fn reader(&self) -> Reader<'_> {
        Reader::new(&self.data)
    }
    pub fn tag_str(&self) -> String {
        tag_str(&self.tag)
    }
    /// The same subrecord with its bytes copied, free of the buffer it was read from.
    pub fn into_owned(self) -> Sub<'static> {
        Sub { tag: self.tag, data: Cow::Owned(self.data.into_owned()) }
    }
}

/// One top-level record: the 16-byte header's tag and two dwords, and its subrecords.
///
/// The second header dword is unused by every reader and written as zero by every tool
/// that has been looked at; it is kept because keeping it costs nothing and dropping it
/// would be the one way this layer could fail to round-trip a file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record<'a> {
    pub tag: Tag,
    /// The header dword at +8, not known to mean anything.
    pub unknown: u32,
    /// The header dword at +12: the record flags (`DELETED` 0x20, `PERSISTENT` 0x400,
    /// `IGNORED` 0x1000, `BLOCKED` 0x2000 - `tes3` flags.rs:270-276).
    pub flags: u32,
    pub subs: Vec<Sub<'a>>,
}

/// The record flag `tes3` and the Construction Set set alongside a `DELE` subrecord.
pub const FLAG_DELETED: u32 = 0x0020;
pub const FLAG_PERSISTENT: u32 = 0x0400;
pub const FLAG_IGNORED: u32 = 0x1000;
pub const FLAG_BLOCKED: u32 = 0x2000;

impl<'a> Record<'a> {
    pub fn new(tag: &Tag) -> Record<'static> {
        Record { tag: *tag, unknown: 0, flags: 0, subs: Vec::new() }
    }
    /// The same record with every subrecord's bytes copied, free of the buffer it was
    /// read from.
    pub fn into_owned(self) -> Record<'static> {
        Record {
            tag: self.tag,
            unknown: self.unknown,
            flags: self.flags,
            subs: self.subs.into_iter().map(Sub::into_owned).collect(),
        }
    }
    pub fn tag_str(&self) -> String {
        tag_str(&self.tag)
    }
    /// The first subrecord with this tag.
    pub fn sub(&self, tag: &Tag) -> Option<&Sub<'a>> {
        self.subs.iter().find(|s| &s.tag == tag)
    }
    /// Every subrecord with this tag, in file order.
    pub fn subs_tagged<'b>(&'b self, tag: &'b Tag) -> impl Iterator<Item = &'b Sub<'a>> + 'b {
        self.subs.iter().filter(move |s| &s.tag == tag)
    }
    pub fn has(&self, tag: &Tag) -> bool {
        self.sub(tag).is_some()
    }
    pub fn push(&mut self, tag: &Tag, data: Vec<u8>) {
        self.subs.push(Sub::new(tag, data));
    }
    /// A subrecord that borrows its bytes, for a record built over a buffer that outlives it.
    pub fn push_borrowed(&mut self, tag: &Tag, data: &'a [u8]) {
        self.subs.push(Sub::borrowed(tag, data));
    }
    /// The record's body size, as the header will state it.
    pub fn body_len(&self) -> usize {
        self.subs.iter().map(|s| 8 + s.data.len()).sum()
    }

    /// Reads one record at the cursor's position, borrowing the subrecords' bytes from
    /// the reader's buffer.
    pub fn read(r: &mut Reader<'a>) -> Result<Record<'a>> {
        let at = r.pos();
        let tag = r.tag()?;
        let size = r.u32()? as usize;
        let unknown = r.u32()?;
        let flags = r.u32()?;
        let body = r.bytes(size).map_err(|e| Error {
            at,
            what: format!("record {} claims {size} bytes: {}", tag_str(&tag), e.what),
        })?;
        let mut subs = Vec::new();
        let mut b = Reader::new(body);
        while !b.at_end() {
            let sat = at + 16 + b.pos();
            let stag =
                b.tag().map_err(|e| Error { at: sat, what: format!("subrecord header: {}", e.what) })?;
            let ssize =
                b.u32().map_err(|e| Error { at: sat, what: format!("subrecord header: {}", e.what) })?
                    as usize;
            let data = b.bytes(ssize).map_err(|e| Error {
                at: sat,
                what: format!("{}::{} claims {ssize} bytes: {}", tag_str(&tag), tag_str(&stag), e.what),
            })?;
            subs.push(Sub { tag: stag, data: Cow::Borrowed(data) });
        }
        Ok(Record { tag, unknown, flags, subs })
    }

    /// Writes the record, header and all.
    pub fn write(&self, w: &mut Writer) {
        w.tag(&self.tag);
        w.u32(self.body_len() as u32);
        w.u32(self.unknown);
        w.u32(self.flags);
        for s in &self.subs {
            w.tag(&s.tag);
            w.u32(s.data.len() as u32);
            w.bytes(&s.data);
        }
    }
}

/// Every record in a plugin, in file order, borrowing from `buf`. An error names the
/// record and the byte.
pub fn parse_plugin(buf: &[u8]) -> Result<Vec<Record<'_>>> {
    let mut r = Reader::new(buf);
    let mut out = Vec::new();
    while !r.at_end() {
        out.push(Record::read(&mut r)?);
    }
    Ok(out)
}

/// The records back as a file.
pub fn write_plugin(records: &[Record]) -> Vec<u8> {
    let mut w = Writer::with_capacity(records.iter().map(|r| 16 + r.body_len()).sum());
    for r in records {
        r.write(&mut w);
    }
    w.into_bytes()
}

pub fn tag_str(t: &Tag) -> String {
    t.iter().map(|&b| if b.is_ascii_graphic() || b == b' ' { b as char } else { '?' }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plugin_round_trips_byte_for_byte() {
        let mut rec = Record::new(b"STAT");
        rec.flags = FLAG_PERSISTENT;
        rec.unknown = 7;
        rec.push(b"NAME", b"rock\0".to_vec());
        rec.push(b"MODL", b"f\\rock.nif\0".to_vec());
        rec.push(b"ZZZZ", vec![1, 2, 3]); // a subrecord nobody knows
        rec.push(b"NAME", b"twice\0".to_vec()); // a duplicate
        let bytes = write_plugin(&[rec.clone(), rec.clone()]);
        let back = parse_plugin(&bytes).unwrap();
        assert_eq!(back, vec![rec.clone(), rec]);
        assert_eq!(write_plugin(&back), bytes);
    }

    #[test]
    fn a_truncated_record_is_named_not_panicked() {
        let mut rec = Record::new(b"STAT");
        rec.push(b"NAME", b"rock\0".to_vec());
        let mut bytes = write_plugin(&[rec]);
        bytes.truncate(bytes.len() - 2);
        let e = parse_plugin(&bytes).unwrap_err();
        assert!(e.what.contains("STAT"), "{e}");
        assert_eq!(e.at, 0);
    }
}
