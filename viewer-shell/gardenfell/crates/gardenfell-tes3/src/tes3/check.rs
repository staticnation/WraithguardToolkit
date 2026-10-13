//! The round-trip check: what each layer gives back for a real file.
//!
//! Round 18gg. The typed layer is only worth building on if it holds real plugins, so this
//! runs a file through all three layers and counts, per record type, what came back the
//! same. Three questions are asked of every record:
//!
//! 1. *raw exact*: does `raw::parse_plugin` then `raw::write_plugin` give the file back
//!    byte for byte? This must always be yes, for any file; a no is a bug in `raw`.
//! 2. *typed*: does the record fit its layout? A record that does not is counted with
//!    the first reason, and its tag and offset are kept so the layout can be looked at.
//! 3. *canonical exact*: does `to_raw(from_raw(rec))` give the same bytes as the file
//!    held? This is where a file written by another tool differs from the canonical
//!    form (a string without its NUL, subrecords in another order, padding), so it is a
//!    percentage, not a gate. And *idempotent*: does `from_raw(to_raw(x)) == x`? This
//!    must always hold for a record that typed at all; a no means the layout loses a
//!    field on the way out.
//!
//! The `roundtrip` command in `GardenfellCLI` prints a [`Report`] per file.

use std::collections::BTreeMap;
use std::fmt;

use super::bytes::Writer;
use super::raw::{self, Record};
use super::types::Object;

/// What one record type did across a file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TagReport {
    pub records: usize,
    /// Read into its typed form.
    pub typed: usize,
    /// Typed, and written back to the very bytes the file held.
    pub exact: usize,
    /// Typed, and read again from its own output to the same value.
    pub idempotent: usize,
    /// The first record that did not type, as (record index in file, reason).
    pub first_unfit: Option<(usize, String)>,
    /// The first record that typed but did not write back exactly, as (record index,
    /// the record's id if it has a NAME, the first differing byte in the body).
    pub first_inexact: Option<(usize, String, usize)>,
}

/// What a whole file did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Report {
    pub bytes: usize,
    pub records: usize,
    /// `raw` gave the file back byte for byte, or the reason it did not.
    pub raw_exact: Result<(), String>,
    /// Records whose tag the format does not have, by tag.
    pub unknown: BTreeMap<String, usize>,
    pub tags: BTreeMap<String, TagReport>,
}

impl Default for Report {
    fn default() -> Report {
        Report { bytes: 0, records: 0, raw_exact: Ok(()), unknown: BTreeMap::new(), tags: BTreeMap::new() }
    }
}

impl Report {
    pub fn typed(&self) -> usize {
        self.tags.values().map(|t| t.typed).sum()
    }
    pub fn exact(&self) -> usize {
        self.tags.values().map(|t| t.exact).sum()
    }
    pub fn idempotent(&self) -> usize {
        self.tags.values().map(|t| t.idempotent).sum()
    }
    /// Every record typed and every typed record read its own output back to itself.
    pub fn clean(&self) -> bool {
        self.raw_exact.is_ok()
            && self.unknown.is_empty()
            && self.tags.values().all(|t| t.typed == t.records && t.idempotent == t.typed)
    }
}

/// One record's own bytes, header included.
fn record_bytes(rec: &Record) -> Vec<u8> {
    let mut w = Writer::with_capacity(16 + rec.body_len());
    rec.write(&mut w);
    w.into_bytes()
}

fn id_of(rec: &Record) -> String {
    rec.sub(b"NAME").map(|s| super::bytes::decode(&s.data)).unwrap_or_default()
}

/// Runs the three layers over a file.
pub fn check(buf: &[u8]) -> Report {
    let mut rep = Report { bytes: buf.len(), ..Default::default() };
    let recs = match raw::parse_plugin(buf) {
        Ok(r) => r,
        Err(e) => {
            rep.raw_exact = Err(format!("raw parse failed: {e}"));
            return rep;
        }
    };
    rep.records = recs.len();
    let back = raw::write_plugin(&recs);
    rep.raw_exact = if back == buf {
        Ok(())
    } else {
        let at = back.iter().zip(buf).position(|(a, b)| a != b).unwrap_or(back.len().min(buf.len()));
        Err(format!("raw round trip differs at byte {at} ({} bytes in, {} out)", buf.len(), back.len()))
    };
    for (i, rec) in recs.iter().enumerate() {
        let tag = rec.tag_str();
        match Object::from_raw(rec) {
            Ok(None) => *rep.unknown.entry(tag).or_default() += 1,
            Ok(Some(obj)) => {
                let t = rep.tags.entry(tag).or_default();
                t.records += 1;
                t.typed += 1;
                match obj.to_raw() {
                    Ok(out) => {
                        let a = record_bytes(&out);
                        let b = record_bytes(rec);
                        if a == b {
                            t.exact += 1;
                        } else if t.first_inexact.is_none() {
                            // The first differing body byte; the header's size word differs
                            // whenever the body does, so it is not what is wanted.
                            let (x, y) = (&a[16..], &b[16..]);
                            let at =
                                x.iter().zip(y).position(|(p, q)| p != q).unwrap_or(x.len().min(y.len()));
                            t.first_inexact = Some((i, id_of(rec), at));
                        }
                        // Read back to the same value, or to the same bytes: a NaN in a
                        // GLOB compares unequal to itself and is still the same record.
                        match Object::from_raw(&out) {
                            Ok(Some(again))
                                if again == obj
                                    || again.to_raw().map(|r| record_bytes(&r) == a).unwrap_or(false) =>
                            {
                                t.idempotent += 1
                            }
                            Ok(Some(_)) => {
                                if t.first_unfit.is_none() {
                                    t.first_unfit =
                                        Some((i, format!("{} read back as a different value", id_of(rec))));
                                }
                            }
                            Ok(None) => {
                                if t.first_unfit.is_none() {
                                    t.first_unfit = Some((i, "wrote a record of an unknown tag".to_string()));
                                }
                            }
                            Err(e) => {
                                if t.first_unfit.is_none() {
                                    t.first_unfit = Some((i, format!("own output does not read back: {e}")));
                                }
                            }
                        }
                    }
                    Err(e) => {
                        if t.first_unfit.is_none() {
                            t.first_unfit = Some((i, format!("does not write: {e}")));
                        }
                    }
                }
            }
            Err(e) => {
                let t = rep.tags.entry(tag).or_default();
                t.records += 1;
                if t.first_unfit.is_none() {
                    t.first_unfit = Some((i, e.to_string()));
                }
            }
        }
    }
    rep
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{} bytes, {} records", self.bytes, self.records)?;
        match &self.raw_exact {
            Ok(()) => writeln!(f, "raw        byte for byte")?,
            Err(e) => writeln!(f, "raw        NOT exact: {e}")?,
        }
        let typed = self.typed();
        let exact = self.exact();
        let idem = self.idempotent();
        let known: usize = self.tags.values().map(|t| t.records).sum();
        writeln!(
            f,
            "typed      {typed} of {known} ({}), exact {exact} ({}), idempotent {idem} ({})",
            pct(typed, known),
            pct(exact, typed),
            pct(idem, typed)
        )?;
        for (tag, n) in &self.unknown {
            writeln!(f, "unknown    {tag} x{n}")?;
        }
        writeln!(f, "{:<6} {:>7} {:>7} {:>7} {:>7}", "tag", "records", "typed", "exact", "idem")?;
        for (tag, t) in &self.tags {
            writeln!(f, "{:<6} {:>7} {:>7} {:>7} {:>7}", tag, t.records, t.typed, t.exact, t.idempotent)?;
            if let Some((i, why)) = &t.first_unfit {
                writeln!(f, "       first unfit: record #{i}: {why}")?;
            }
            if let Some((i, id, at)) = &t.first_inexact {
                writeln!(f, "       first inexact: record #{i} {id:?}, body byte {at}")?;
            }
        }
        Ok(())
    }
}

fn pct(a: usize, b: usize) -> String {
    if b == 0 {
        "n/a".to_string()
    } else {
        format!("{:.2}%", a as f64 * 100.0 / b as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plugin_of_one_static_is_clean_and_exact() {
        let mut rec = Record::new(b"STAT");
        rec.push(b"NAME", b"rock\0".to_vec());
        rec.push(b"MODL", b"f\\rock.nif\0".to_vec());
        let buf = raw::write_plugin(&[rec]);
        let rep = check(&buf);
        assert!(rep.clean(), "{rep}");
        assert_eq!(rep.exact(), 1);
        let text = rep.to_string();
        assert!(text.contains("STAT         1       1       1       1"), "{text}");
    }

    #[test]
    fn a_string_without_its_nul_is_typed_and_counted_inexact() {
        let mut rec = Record::new(b"STAT");
        rec.push(b"NAME", b"rock".to_vec());
        let buf = raw::write_plugin(&[rec]);
        let rep = check(&buf);
        assert!(rep.clean(), "{rep}");
        assert_eq!(rep.exact(), 0);
        let t = &rep.tags["STAT"];
        assert_eq!(t.first_inexact.as_ref().map(|x| x.1.as_str()), Some("rock"));
    }

    #[test]
    fn an_unknown_tag_and_an_unfit_record_are_reported_not_fatal() {
        let mut odd = Record::new(b"QQQQ");
        odd.push(b"NAME", b"x\0".to_vec());
        let mut bad = Record::new(b"STAT");
        bad.push(b"ZZZZ", vec![1]);
        let buf = raw::write_plugin(&[odd, bad]);
        let rep = check(&buf);
        assert!(!rep.clean());
        assert_eq!(rep.unknown["QQQQ"], 1);
        assert!(rep.tags["STAT"].first_unfit.as_ref().unwrap().1.contains("STAT::ZZZZ"));
    }
}
