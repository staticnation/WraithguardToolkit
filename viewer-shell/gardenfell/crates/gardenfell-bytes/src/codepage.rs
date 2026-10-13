//! The code page a game's text is written in: Windows-1252 for the English, German, French,
//! Italian and Spanish editions, Windows-1251 for the Russian one, Windows-1250 for the
//! Polish and Czech ones. Round 18gq.
//!
//! Morrowind stores every string as one byte per character and never says which page the
//! bytes are in; the game simply uses the system's. Until this round the engine decoded
//! every byte as Windows-1252 and read `Morrowind.ini` through `read_to_string`, which
//! refuses anything that is not UTF-8, so a Russian install did not connect at all (the ini
//! carries Cyrillic in its `[Question N]` and `[Level Up]` sections) and would have shown
//! its cell names as `Áàëìîðà` if it had.
//!
//! One page is in force at a time, for the whole process, because the plugins, the archives
//! and the meshes of one install are all in the same one and the record readers run on
//! every thread of the pool. [`set`] is called once per connect, from the layout's answer
//! ([`sniff`] over the ini, `encoding=` in `openmw.cfg`, or the command line's `--encoding`);
//! [`current`] is what [`crate::bytes::decode`] and [`crate::bytes::encode`] consult. The
//! default is Windows-1252, so nothing changes for an install that never names another.
//!
//! The three tables map the 128 high bytes to their code points. A slot the page leaves
//! undefined decodes to the C1 control of the same value and encodes back the same way, as
//! `encoding_rs` and every Windows API do, so no byte is ever lost through a round trip.

use std::path::Path;
use std::sync::atomic::{AtomicU8, Ordering};

/// A single-byte Windows code page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CodePage {
    /// Western European: the English and most other editions.
    #[default]
    Win1252,
    /// Cyrillic: the Russian edition.
    Win1251,
    /// Central European: the Polish and Czech editions.
    Win1250,
}

/// Windows-1252, 0x80 to 0xFF.
const TABLE_1252: [u16; 128] = [
    0x20AC, 0x0081, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160, 0x2039, 0x0152,
    0x008D, 0x017D, 0x008F, 0x0090, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014, 0x02DC, 0x2122,
    0x0161, 0x203A, 0x0153, 0x009D, 0x017E, 0x0178, 0x00A0, 0x00A1, 0x00A2, 0x00A3, 0x00A4, 0x00A5, 0x00A6,
    0x00A7, 0x00A8, 0x00A9, 0x00AA, 0x00AB, 0x00AC, 0x00AD, 0x00AE, 0x00AF, 0x00B0, 0x00B1, 0x00B2, 0x00B3,
    0x00B4, 0x00B5, 0x00B6, 0x00B7, 0x00B8, 0x00B9, 0x00BA, 0x00BB, 0x00BC, 0x00BD, 0x00BE, 0x00BF, 0x00C0,
    0x00C1, 0x00C2, 0x00C3, 0x00C4, 0x00C5, 0x00C6, 0x00C7, 0x00C8, 0x00C9, 0x00CA, 0x00CB, 0x00CC, 0x00CD,
    0x00CE, 0x00CF, 0x00D0, 0x00D1, 0x00D2, 0x00D3, 0x00D4, 0x00D5, 0x00D6, 0x00D7, 0x00D8, 0x00D9, 0x00DA,
    0x00DB, 0x00DC, 0x00DD, 0x00DE, 0x00DF, 0x00E0, 0x00E1, 0x00E2, 0x00E3, 0x00E4, 0x00E5, 0x00E6, 0x00E7,
    0x00E8, 0x00E9, 0x00EA, 0x00EB, 0x00EC, 0x00ED, 0x00EE, 0x00EF, 0x00F0, 0x00F1, 0x00F2, 0x00F3, 0x00F4,
    0x00F5, 0x00F6, 0x00F7, 0x00F8, 0x00F9, 0x00FA, 0x00FB, 0x00FC, 0x00FD, 0x00FE, 0x00FF,
];

/// Windows-1251, 0x80 to 0xFF.
const TABLE_1251: [u16; 128] = [
    0x0402, 0x0403, 0x201A, 0x0453, 0x201E, 0x2026, 0x2020, 0x2021, 0x20AC, 0x2030, 0x0409, 0x2039, 0x040A,
    0x040C, 0x040B, 0x040F, 0x0452, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014, 0x0098, 0x2122,
    0x0459, 0x203A, 0x045A, 0x045C, 0x045B, 0x045F, 0x00A0, 0x040E, 0x045E, 0x0408, 0x00A4, 0x0490, 0x00A6,
    0x00A7, 0x0401, 0x00A9, 0x0404, 0x00AB, 0x00AC, 0x00AD, 0x00AE, 0x0407, 0x00B0, 0x00B1, 0x0406, 0x0456,
    0x0491, 0x00B5, 0x00B6, 0x00B7, 0x0451, 0x2116, 0x0454, 0x00BB, 0x0458, 0x0405, 0x0455, 0x0457, 0x0410,
    0x0411, 0x0412, 0x0413, 0x0414, 0x0415, 0x0416, 0x0417, 0x0418, 0x0419, 0x041A, 0x041B, 0x041C, 0x041D,
    0x041E, 0x041F, 0x0420, 0x0421, 0x0422, 0x0423, 0x0424, 0x0425, 0x0426, 0x0427, 0x0428, 0x0429, 0x042A,
    0x042B, 0x042C, 0x042D, 0x042E, 0x042F, 0x0430, 0x0431, 0x0432, 0x0433, 0x0434, 0x0435, 0x0436, 0x0437,
    0x0438, 0x0439, 0x043A, 0x043B, 0x043C, 0x043D, 0x043E, 0x043F, 0x0440, 0x0441, 0x0442, 0x0443, 0x0444,
    0x0445, 0x0446, 0x0447, 0x0448, 0x0449, 0x044A, 0x044B, 0x044C, 0x044D, 0x044E, 0x044F,
];

/// Windows-1250, 0x80 to 0xFF.
const TABLE_1250: [u16; 128] = [
    0x20AC, 0x0081, 0x201A, 0x0083, 0x201E, 0x2026, 0x2020, 0x2021, 0x0088, 0x2030, 0x0160, 0x2039, 0x015A,
    0x0164, 0x017D, 0x0179, 0x0090, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014, 0x0098, 0x2122,
    0x0161, 0x203A, 0x015B, 0x0165, 0x017E, 0x017A, 0x00A0, 0x02C7, 0x02D8, 0x0141, 0x00A4, 0x0104, 0x00A6,
    0x00A7, 0x00A8, 0x00A9, 0x015E, 0x00AB, 0x00AC, 0x00AD, 0x00AE, 0x017B, 0x00B0, 0x00B1, 0x02DB, 0x0142,
    0x00B4, 0x00B5, 0x00B6, 0x00B7, 0x00B8, 0x0105, 0x015F, 0x00BB, 0x013D, 0x02DD, 0x013E, 0x017C, 0x0154,
    0x00C1, 0x00C2, 0x0102, 0x00C4, 0x0139, 0x0106, 0x00C7, 0x010C, 0x00C9, 0x0118, 0x00CB, 0x011A, 0x00CD,
    0x00CE, 0x010E, 0x0110, 0x0143, 0x0147, 0x00D3, 0x00D4, 0x0150, 0x00D6, 0x00D7, 0x0158, 0x016E, 0x00DA,
    0x0170, 0x00DC, 0x00DD, 0x0162, 0x00DF, 0x0155, 0x00E1, 0x00E2, 0x0103, 0x00E4, 0x013A, 0x0107, 0x00E7,
    0x010D, 0x00E9, 0x0119, 0x00EB, 0x011B, 0x00ED, 0x00EE, 0x010F, 0x0111, 0x0144, 0x0148, 0x00F3, 0x00F4,
    0x0151, 0x00F6, 0x00F7, 0x0159, 0x016F, 0x00FA, 0x0171, 0x00FC, 0x00FD, 0x0163, 0x02D9,
];

impl CodePage {
    /// Every page, in the order the names below list them.
    pub const ALL: [CodePage; 3] = [CodePage::Win1252, CodePage::Win1251, CodePage::Win1250];

    fn table(self) -> &'static [u16; 128] {
        match self {
            CodePage::Win1252 => &TABLE_1252,
            CodePage::Win1251 => &TABLE_1251,
            CodePage::Win1250 => &TABLE_1250,
        }
    }

    /// The page's usual name, for the log: `Windows-1251`.
    pub fn name(self) -> &'static str {
        match self {
            CodePage::Win1252 => "Windows-1252",
            CodePage::Win1251 => "Windows-1251",
            CodePage::Win1250 => "Windows-1250",
        }
    }

    /// The name OpenMW's `encoding=` uses: `win1251`.
    pub fn openmw_name(self) -> &'static str {
        match self {
            CodePage::Win1252 => "win1252",
            CodePage::Win1251 => "win1251",
            CodePage::Win1250 => "win1250",
        }
    }

    /// A page by any of the names people write: `win1251`, `cp1251`, `windows-1251`, `1251`,
    /// case and the separator not mattering. `None` for a name that is not one of the three.
    pub fn from_name(s: &str) -> Option<CodePage> {
        let digits: String = s.chars().filter(|c| c.is_ascii_digit()).collect();
        let letters: String =
            s.chars().filter(|c| c.is_ascii_alphabetic()).collect::<String>().to_ascii_lowercase();
        if !matches!(letters.as_str(), "" | "win" | "cp" | "windows") {
            return None;
        }
        match digits.as_str() {
            "1252" => Some(CodePage::Win1252),
            "1251" => Some(CodePage::Win1251),
            "1250" => Some(CodePage::Win1250),
            _ => None,
        }
    }

    /// One byte as a character.
    #[inline]
    pub fn byte_to_char(self, b: u8) -> char {
        if b < 0x80 {
            return b as char;
        }
        char::from_u32(self.table()[(b - 0x80) as usize] as u32).unwrap_or('\u{FFFD}')
    }

    /// One character as a byte, or `None` for a character the page cannot hold.
    #[inline]
    pub fn char_to_byte(self, c: char) -> Option<u8> {
        let u = c as u32;
        if u < 0x80 {
            return Some(u as u8);
        }
        // Windows-1252 keeps Latin-1 in place above 0x9F, which is most of what is asked.
        if self == CodePage::Win1252 && (0xA0..=0xFF).contains(&u) {
            return Some(u as u8);
        }
        if u > 0xFFFF {
            return None;
        }
        self.table().iter().position(|h| *h as u32 == u).map(|i| 0x80 + i as u8)
    }

    /// Bytes as text, every byte one character.
    pub fn decode(self, b: &[u8]) -> String {
        if b.is_ascii() {
            return String::from_utf8_lossy(b).into_owned();
        }
        let mut out = String::with_capacity(b.len() + 4);
        out.extend(b.iter().map(|&c| self.byte_to_char(c)));
        out
    }

    /// Text as bytes, or the first character that has no byte in this page.
    pub fn encode(self, s: &str) -> Result<Vec<u8>, char> {
        s.chars().map(|c| self.char_to_byte(c).ok_or(c)).collect()
    }
}

/// The page in force: 0 is Windows-1252, the default, so a process that never calls [`set`]
/// reads as it always did.
static CURRENT: AtomicU8 = AtomicU8::new(0);

fn index(p: CodePage) -> u8 {
    match p {
        CodePage::Win1252 => 0,
        CodePage::Win1251 => 1,
        CodePage::Win1250 => 2,
    }
}

/// The code page every record, archive and mesh string is read and written in right now.
#[inline]
pub fn current() -> CodePage {
    match CURRENT.load(Ordering::Relaxed) {
        1 => CodePage::Win1251,
        2 => CodePage::Win1250,
        _ => CodePage::Win1252,
    }
}

/// Makes `p` the page in force, for the whole process. Called once per connect.
pub fn set(p: CodePage) {
    CURRENT.store(index(p), Ordering::Relaxed);
}

/// A guess at the page a text file is written in, from its bytes.
///
/// Cyrillic text is nearly all high bytes, in runs the length of a word, so when most high
/// bytes are followed by another high byte the file is Windows-1251. Western text has its
/// accented letters one at a time between ASCII ones. Polish and Czech text sits between,
/// and is told apart by the letters that are its own: `ą ł ś ź Ś Ź Ą Ż ř ť Ť Ř` sit on bytes
/// that in Windows-1252 are superscripts, currency signs and undefined slots, which an ini
/// never carries, so three of them are enough. Anything else is Windows-1252, the default.
/// Fewer than eight high bytes is too little to go on and also answers Windows-1252.
pub fn sniff(bytes: &[u8]) -> CodePage {
    let mut high = 0usize;
    let mut runs = 0usize;
    let mut central = 0usize;
    for (i, &b) in bytes.iter().enumerate() {
        if b < 0x80 {
            continue;
        }
        high += 1;
        if bytes.get(i + 1).map(|n| *n >= 0x80).unwrap_or(false) {
            runs += 1;
        }
        if matches!(b, 0xB9 | 0xB3 | 0x9C | 0x9F | 0x8C | 0x8F | 0xA5 | 0xAF | 0xF8 | 0x9D | 0x8D | 0xD8) {
            central += 1;
        }
    }
    if high < 8 {
        return CodePage::Win1252;
    }
    if runs * 2 > high {
        return CodePage::Win1251;
    }
    if central >= 3 {
        return CodePage::Win1250;
    }
    CodePage::Win1252
}

/// Bytes as text: UTF-8 when they are UTF-8 (a BOM dropped), otherwise decoded with the page
/// [`sniff`] names. Nothing is lost either way, which `String::from_utf8_lossy` cannot say.
pub fn text_of(bytes: &[u8]) -> String {
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => sniff(bytes).decode(bytes),
    }
}

/// A text file read whole, as [`text_of`] reads it. The replacement for `read_to_string`
/// on every ini and cfg an install hands over: `read_to_string` refuses a file that is not
/// UTF-8, and the Russian edition's `Morrowind.ini` is not.
pub fn read_text(path: &Path) -> std::io::Result<String> {
    std::fs::read(path).map(|b| text_of(&b))
}

/// The page a file is written in, when it is not UTF-8: [`sniff`] over its bytes, and
/// Windows-1252 for a file that is UTF-8 or cannot be read.
pub fn sniff_file(path: &Path) -> CodePage {
    match std::fs::read(path) {
        Ok(b) if std::str::from_utf8(&b).is_err() => sniff(&b),
        _ => CodePage::Win1252,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_byte_round_trips_through_every_page() {
        for p in CodePage::ALL {
            for b in 1u8..=255 {
                let c = p.byte_to_char(b);
                assert_eq!(p.char_to_byte(c), Some(b), "{} byte {b:#04x} -> {c:?}", p.name());
            }
        }
    }

    #[test]
    fn the_pages_spell_their_own_letters() {
        assert_eq!(CodePage::Win1251.decode(b"\xC1\xE0\xEB\xEC\xEE\xF0\xE0"), "Балмора");
        assert_eq!(CodePage::Win1252.decode(b"\xC1\xE0\xEB\xEC\xEE\xF0\xE0"), "Áàëìîðà");
        assert_eq!(CodePage::Win1250.decode(b"Ska\xB3y"), "Skały");
        assert_eq!(CodePage::Win1251.encode("Балмора"), Ok(b"\xC1\xE0\xEB\xEC\xEE\xF0\xE0".to_vec()));
        assert_eq!(CodePage::Win1252.encode("Балмора"), Err('Б'));
        assert_eq!(CodePage::Win1250.encode("é"), Ok(vec![0xE9]));
        assert_eq!(CodePage::Win1251.encode("é"), Err('é'));
        assert_eq!(CodePage::Win1252.encode("日"), Err('日'));
    }

    #[test]
    fn names_are_read_the_ways_people_write_them() {
        for s in ["win1251", "WIN1251", "cp1251", "Windows-1251", "windows_1251", "1251"] {
            assert_eq!(CodePage::from_name(s), Some(CodePage::Win1251), "{s}");
        }
        assert_eq!(CodePage::from_name("win1250"), Some(CodePage::Win1250));
        assert_eq!(CodePage::from_name("win1252"), Some(CodePage::Win1252));
        assert_eq!(CodePage::from_name("utf8"), None);
        assert_eq!(CodePage::from_name("1253"), None);
        assert_eq!(CodePage::from_name("koi8-r"), None);
        assert_eq!(CodePage::Win1251.openmw_name(), "win1251");
    }

    #[test]
    fn sniffing_tells_the_three_apart() {
        // A Russian question as the game's ini carries it.
        let ru =
            CodePage::Win1251.encode("Question0=Вы встречаете старого друга. Что вы делаете?\r\n").unwrap();
        assert_eq!(sniff(&ru), CodePage::Win1251);
        // German and French, each a word at a time between ASCII ones.
        let de = CodePage::Win1252
            .encode("Question0=Ihr trefft einen alten Freund, der über die Straße läuft. Fühlt ihr euch wohl? Größe, Mühe, Äußerung\r\n")
            .unwrap();
        assert_eq!(sniff(&de), CodePage::Win1252);
        let fr = CodePage::Win1252
            .encode("Question0=Vous rencontrez un vieil ami très fâché. Êtes-vous prêt à l'écouter? Élève, déjà, où, çà et là\r\n")
            .unwrap();
        assert_eq!(sniff(&fr), CodePage::Win1252);
        // Polish, with its own letters.
        let pl = CodePage::Win1250
            .encode("Question0=Spotykasz starego przyjaciela, który wygląda na zmartwionego. Czy mówisz mu prawdę, czy też łagodnie kłamiesz? Ślę źdźbło.\r\n")
            .unwrap();
        assert_eq!(sniff(&pl), CodePage::Win1250);
        // Too little to go on, and plain ASCII, both answer the default.
        assert_eq!(sniff(b"GameFile0=Morrowind.esm\r\ncaf\xE9"), CodePage::Win1252);
        assert_eq!(sniff(b"GameFile0=Morrowind.esm\r\n"), CodePage::Win1252);
    }

    #[test]
    fn text_of_keeps_utf8_and_decodes_the_rest() {
        assert_eq!(text_of("Балмора".as_bytes()), "Балмора");
        assert_eq!(text_of(b"\xEF\xBB\xBFabc"), "abc");
        let ru =
            CodePage::Win1251.encode("[Question 0]\r\nQuestion=Вы встречаете старого друга\r\n").unwrap();
        assert_eq!(text_of(&ru), "[Question 0]\r\nQuestion=Вы встречаете старого друга\r\n");
        assert_eq!(text_of(b"caf\xE9"), "café");
    }
}
