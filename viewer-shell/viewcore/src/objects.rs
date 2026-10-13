//! Gardenfell's object index (`ObjKind`, `ObjectDef`, `RawObject`, `resolve_leveled`):
//! Robin Hjelte's, from `gardenfell_tes3::records` (viewer-shell/gardenfell, vendored),
//! re-exported so viewcore's callers keep their paths. Below it, a record's id read from
//! its raw body, as viewcore's own walker (`crate::rawrec`) hands records over.

pub use gardenfell_tes3::records::*;

use crate::esp::{cstring, Subs, Tag};

/// A record's id — NAME on nearly everything; a script keeps its name in SCHD's first
/// thirty-two bytes and has no NAME at all.
pub fn record_id(tag: Tag, body: &[u8]) -> Option<String> {
    if &tag == b"SCPT" {
        for s in Subs::new(body) {
            if &s.tag == b"SCHD" && s.data.len() >= 32 {
                return Some(cstring(&s.data[..32]));
            }
        }
        return None;
    }
    if matches!(&tag, b"TES3" | b"CELL" | b"LAND" | b"PGRD" | b"INFO" | b"DIAL") {
        return None;
    }
    for s in Subs::new(body) {
        if &s.tag == b"NAME" {
            return Some(cstring(s.data));
        }
    }
    None
}
