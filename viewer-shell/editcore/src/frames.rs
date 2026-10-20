//! A plugin's framing, read without parsing: its records and their subrecords (the
//! clamping of `native/src/lint.rs`, which reads files the same way).

/// Top-level records: `(tag, body)`, the body clamped to what the file holds.
pub fn records(raw: &[u8]) -> impl Iterator<Item = (&[u8], &[u8])> {
    frames(raw, 16)
}

/// Subrecords of a record body: `(tag, data)`, clamped likewise.
pub fn subrecords(body: &[u8]) -> impl Iterator<Item = (&[u8], &[u8])> {
    frames(body, 8)
}

fn frames(buf: &[u8], header: usize) -> impl Iterator<Item = (&[u8], &[u8])> {
    let mut i = 0usize;
    std::iter::from_fn(move || {
        if i + header > buf.len() {
            return None;
        }
        let tag = &buf[i..i + 4];
        let size = u32::from_le_bytes(buf[i + 4..i + 8].try_into().expect("4 bytes")) as usize;
        let start = i + header;
        let end = start.saturating_add(size).min(buf.len());
        i = start.saturating_add(size);
        Some((tag, &buf[start.min(buf.len())..end]))
    })
}

/// A NUL-terminated string field: up to the first NUL, Latin-1, stripped (Python's
/// `str.strip()` whitespace).
pub fn zstr(b: &[u8]) -> String {
    let end = b.iter().position(|&x| x == 0).unwrap_or(b.len());
    let s: String = b[..end].iter().map(|&x| x as char).collect();
    s.trim_matches(|c: char| matches!(c, '\t' | '\n' | '\x0b' | '\x0c' | '\r' | '\x1c'..='\x1f' | ' ' | '\u{85}' | '\u{a0}')).to_string()
}
