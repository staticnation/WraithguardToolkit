//! Gardenfell's `json`: Robin Hjelte's, from `gardenfell-bytes` (viewer-shell/gardenfell,
//! vendored unmodified), re-exported so viewcore's callers keep their paths.
//! Below it, what Wraithguard adds.

pub use gardenfell_bytes::json::*;

/// Wraithguard: standard base64 (with or without padding; whitespace and a `data:...,`
/// prefix ignored) to bytes, or `None` on a character outside the alphabet.
pub fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let s = match s.find(',') {
        Some(i) if s.starts_with("data:") => &s[i + 1..],
        _ => s,
    };
    let val = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => (c - b'A') as u32,
            b'a'..=b'z' => (c - b'a' + 26) as u32,
            b'0'..=b'9' => (c - b'0' + 52) as u32,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => return None,
        })
    };
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for &c in s.as_bytes() {
        if c == b'=' || c.is_ascii_whitespace() {
            continue;
        }
        acc = (acc << 6) | val(c)?;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

#[cfg(test)]
mod b64_tests {
    #[test]
    fn decodes() {
        assert_eq!(super::base64_decode("aGVsbG8=").unwrap(), b"hello");
        assert_eq!(super::base64_decode("data:image/png;base64,aGk").unwrap(), b"hi");
        assert!(super::base64_decode("a$").is_none());
    }
}
