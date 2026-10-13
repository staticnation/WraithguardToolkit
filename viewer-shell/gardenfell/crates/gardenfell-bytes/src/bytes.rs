//! The primitives every TES3 and NIF layout is made of: a cursor that reads
//! little-endian numbers and single-byte strings and says exactly where it ran out, a
//! writer that puts them back, and the code-page functions, which since round 18gq read
//! and write the install's page (`codepage`): Windows-1252 unless the install says otherwise.
//!
//! Round 18gg (backend-plan part 3, step 3 of 4.6). The shape is `tes3`'s `bytes_io`
//! without its dependencies: `Reader::new(bytes)`, typed reads that return `Result`,
//! `Writer` over a `Vec<u8>`. Strings are the code page both ways, as the game and every
//! tool read them (`bytes_io` reader.rs:17, writer.rs:19): bytes 0x80-0x9F decode to the
//! printable characters the code page gives them rather than C1 controls, and a character
//! the page cannot hold is an error on the way out, never a `?`. Every byte decodes, so a
//! file never fails to read for its text.

use std::fmt;

/// What went wrong, and where in the buffer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    pub at: usize,
    pub what: String,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at byte {}", self.what, self.at)
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

/// A cursor over a byte slice. Every read checks its bounds and answers an [`Error`]
/// naming the position rather than a zero.
#[derive(Clone, Copy, Debug)]
pub struct Reader<'a> {
    b: &'a [u8],
    p: usize,
}

impl<'a> Reader<'a> {
    pub fn new(b: &'a [u8]) -> Reader<'a> {
        Reader { b, p: 0 }
    }

    /// The position, in bytes from the start of the slice.
    #[inline]
    pub fn pos(&self) -> usize {
        self.p
    }
    #[inline]
    pub fn len(&self) -> usize {
        self.b.len()
    }
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.b.is_empty()
    }
    #[inline]
    pub fn remaining(&self) -> usize {
        self.b.len() - self.p
    }
    #[inline]
    pub fn at_end(&self) -> bool {
        self.p >= self.b.len()
    }
    /// The whole slice.
    #[inline]
    pub fn all(&self) -> &'a [u8] {
        self.b
    }

    pub fn error<T>(&self, what: impl Into<String>) -> Result<T> {
        Err(Error { at: self.p, what: what.into() })
    }

    /// The next `n` bytes, or an error naming the shortfall.
    pub fn bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        match self.b.get(self.p..self.p + n) {
            Some(s) => {
                self.p += n;
                Ok(s)
            }
            None => self.error(format!("{} bytes wanted, {} left", n, self.remaining())),
        }
    }
    /// The rest of the slice.
    pub fn rest(&mut self) -> &'a [u8] {
        let s = &self.b[self.p..];
        self.p = self.b.len();
        s
    }
    pub fn skip(&mut self, n: usize) -> Result<()> {
        self.bytes(n).map(|_| ())
    }
    pub fn array<const N: usize>(&mut self) -> Result<[u8; N]> {
        let s = self.bytes(N)?;
        let mut out = [0u8; N];
        out.copy_from_slice(s);
        Ok(out)
    }

    pub fn u8(&mut self) -> Result<u8> {
        Ok(self.array::<1>()?[0])
    }
    pub fn i8(&mut self) -> Result<i8> {
        Ok(self.u8()? as i8)
    }
    pub fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.array()?))
    }
    pub fn i16(&mut self) -> Result<i16> {
        Ok(i16::from_le_bytes(self.array()?))
    }
    pub fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.array()?))
    }
    pub fn i32(&mut self) -> Result<i32> {
        Ok(i32::from_le_bytes(self.array()?))
    }
    pub fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(self.array()?))
    }
    pub fn i64(&mut self) -> Result<i64> {
        Ok(i64::from_le_bytes(self.array()?))
    }
    pub fn f32(&mut self) -> Result<f32> {
        Ok(f32::from_le_bytes(self.array()?))
    }
    pub fn f32s<const N: usize>(&mut self) -> Result<[f32; N]> {
        let mut out = [0f32; N];
        for v in out.iter_mut() {
            *v = self.f32()?;
        }
        Ok(out)
    }
    pub fn u32s<const N: usize>(&mut self) -> Result<[u32; N]> {
        let mut out = [0u32; N];
        for v in out.iter_mut() {
            *v = self.u32()?;
        }
        Ok(out)
    }
    pub fn i32s<const N: usize>(&mut self) -> Result<[i32; N]> {
        let mut out = [0i32; N];
        for v in out.iter_mut() {
            *v = self.i32()?;
        }
        Ok(out)
    }
    /// A four-byte tag.
    pub fn tag(&mut self) -> Result<[u8; 4]> {
        self.array::<4>()
    }

    /// A fixed-size string field: `n` bytes, text up to the first NUL, in the code page.
    pub fn fixed_string(&mut self, n: usize) -> Result<String> {
        Ok(decode(self.bytes(n)?))
    }
    /// The rest of the slice as a string, text up to the first NUL.
    pub fn string_rest(&mut self) -> String {
        decode(self.rest())
    }
    /// A length-prefixed string (a `u32` count, then that many bytes), as NIF stores them.
    pub fn sized_string(&mut self) -> Result<String> {
        let n = self.u32()? as usize;
        if n > 1 << 20 {
            return self.error(format!("a string of {n} bytes is not a string"));
        }
        Ok(decode(self.bytes(n)?))
    }
}

/// A growing buffer the layouts write into.
#[derive(Clone, Debug, Default)]
pub struct Writer {
    pub out: Vec<u8>,
}

impl Writer {
    pub fn new() -> Writer {
        Writer { out: Vec::new() }
    }
    pub fn with_capacity(n: usize) -> Writer {
        Writer { out: Vec::with_capacity(n) }
    }
    pub fn into_bytes(self) -> Vec<u8> {
        self.out
    }
    #[inline]
    pub fn len(&self) -> usize {
        self.out.len()
    }
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.out.is_empty()
    }

    pub fn bytes(&mut self, b: &[u8]) {
        self.out.extend_from_slice(b);
    }
    pub fn u8(&mut self, v: u8) {
        self.out.push(v);
    }
    pub fn i8(&mut self, v: i8) {
        self.out.push(v as u8);
    }
    pub fn u16(&mut self, v: u16) {
        self.out.extend_from_slice(&v.to_le_bytes());
    }
    pub fn i16(&mut self, v: i16) {
        self.out.extend_from_slice(&v.to_le_bytes());
    }
    pub fn u32(&mut self, v: u32) {
        self.out.extend_from_slice(&v.to_le_bytes());
    }
    pub fn i32(&mut self, v: i32) {
        self.out.extend_from_slice(&v.to_le_bytes());
    }
    pub fn u64(&mut self, v: u64) {
        self.out.extend_from_slice(&v.to_le_bytes());
    }
    pub fn i64(&mut self, v: i64) {
        self.out.extend_from_slice(&v.to_le_bytes());
    }
    pub fn f32(&mut self, v: f32) {
        self.out.extend_from_slice(&v.to_le_bytes());
    }
    pub fn f32s(&mut self, v: &[f32]) {
        for x in v {
            self.f32(*x);
        }
    }
    pub fn u32s(&mut self, v: &[u32]) {
        for x in v {
            self.u32(*x);
        }
    }
    pub fn i32s(&mut self, v: &[i32]) {
        for x in v {
            self.i32(*x);
        }
    }
    pub fn tag(&mut self, t: &[u8; 4]) {
        self.out.extend_from_slice(t);
    }

    /// A fixed-size string field: the text encoded, then NUL padding to `n` bytes. An
    /// error if the text does not fit, which `tes3` also refuses to write (string.rs:15-28).
    pub fn fixed_string(&mut self, s: &str, n: usize) -> Result<()> {
        let bytes = encode(s)
            .map_err(|c| Error { at: self.out.len(), what: no_byte(c) })?;
        if bytes.len() > n {
            return Err(Error { at: self.out.len(), what: format!("{:?} does not fit in {n} bytes", s) });
        }
        self.out.extend_from_slice(&bytes);
        let padded = self.out.len() + (n - bytes.len());
        self.out.resize(padded, 0);
        Ok(())
    }
    /// A string with a NUL terminator, as a record's `NAME`, `MODL` and friends store it.
    /// An empty string is one NUL, as the Construction Set writes it.
    pub fn cstring(&mut self, s: &str) -> Result<()> {
        let bytes = encode(s)
            .map_err(|c| Error { at: self.out.len(), what: no_byte(c) })?;
        self.out.extend_from_slice(&bytes);
        self.out.push(0);
        Ok(())
    }
    /// A length-prefixed string without a terminator, as NIF stores them.
    pub fn sized_string(&mut self, s: &str) -> Result<()> {
        let bytes = encode(s)
            .map_err(|c| Error { at: self.out.len(), what: no_byte(c) })?;
        self.u32(bytes.len() as u32);
        self.out.extend_from_slice(&bytes);
        Ok(())
    }
}

/// One byte of the code page in force ([`crate::codepage::current`]) as a character.
///
/// Round 18gq: until then this was Windows-1252 and nothing else. The table lives in
/// `codepage` now, one per page, and the page is the install's: Windows-1251 for a Russian
/// one, Windows-1250 for a Polish one, Windows-1252 for everything else and by default.
#[inline]
pub fn byte_to_char(b: u8) -> char {
    crate::codepage::current().byte_to_char(b)
}

/// One character as a byte of the code page in force, or `None` for a character the page
/// cannot hold.
#[inline]
pub fn char_to_byte(c: char) -> Option<u8> {
    crate::codepage::current().char_to_byte(c)
}

/// Bytes as text in the code page in force, stopping at the first NUL.
pub fn decode(b: &[u8]) -> String {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    let b = &b[..end];
    // Nearly every string in a plugin is ASCII, and ASCII is the same in every page, so
    // the common case is one copy and no table walk (round 18gh, where the typed reader
    // became the engine's and this was on every record's path).
    if b.is_ascii() {
        return String::from_utf8_lossy(b).into_owned();
    }
    crate::codepage::current().decode(b)
}

/// Text as bytes of the code page in force, or the first character that has no byte.
pub fn encode(s: &str) -> std::result::Result<Vec<u8>, char> {
    crate::codepage::current().encode(s)
}

/// The writer's complaint about a character the page in force cannot hold.
fn no_byte(c: char) -> String {
    format!("{c:?} has no {} byte", crate::codepage::current().name())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_byte_round_trips_through_the_code_page() {
        for b in 0u8..=255 {
            if b == 0 {
                continue;
            }
            let c = byte_to_char(b);
            assert_eq!(char_to_byte(c), Some(b), "byte {b:#04x} -> {c:?}");
        }
        // The default page is Windows-1252; `codepage`'s own tests cover the other two.
        assert_eq!(decode(b"caf\xE9\0junk"), "café");
        assert_eq!(decode(b"\x80 \x93x\x94"), "€ \u{201C}x\u{201D}");
        assert_eq!(encode("naïve"), Ok(b"na\xEFve".to_vec()));
        assert_eq!(encode("日"), Err('日'));
    }

    #[test]
    fn a_reader_says_where_it_ran_out() {
        let mut r = Reader::new(&[1, 0, 0, 0, 2]);
        assert_eq!(r.u32().unwrap(), 1);
        let e = r.u32().unwrap_err();
        assert_eq!(e.at, 4);
        assert!(e.what.contains("4 bytes wanted"));
    }

    #[test]
    fn a_writer_puts_back_what_a_reader_took() {
        let mut w = Writer::new();
        w.u16(7);
        w.f32(1.5);
        w.fixed_string("ab", 4).unwrap();
        w.cstring("xé").unwrap();
        w.sized_string("Ni").unwrap();
        let mut r = Reader::new(&w.out);
        assert_eq!(r.u16().unwrap(), 7);
        assert_eq!(r.f32().unwrap(), 1.5);
        assert_eq!(r.fixed_string(4).unwrap(), "ab");
        assert_eq!(r.bytes(3).unwrap(), b"x\xE9\0");
        assert_eq!(r.sized_string().unwrap(), "Ni");
        assert!(r.at_end());
        assert!(Writer::new().fixed_string("toolong", 4).is_err());
    }
}
