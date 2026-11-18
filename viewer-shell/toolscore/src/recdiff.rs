//! A plugin's records before and after a tool changed it (tes3cmd on the Tools page), as a
//! list a page can show, filter and undo record by record (Static 2026-11-18: "results the page
//! can list, filter and undo, in place of tes3cmd's text log").
//!
//! Read at the framing level ([`editcore`'s `frames`] does the same), so any tool's output
//! compares, built in or external, whatever it did: each top-level record is keyed by its type
//! and what names it ([`key`]: an INFO under its topic, a cell by name or grid, LAND by grid, a
//! second record of the same key by its count), and [`diff`] says which keys a run removed,
//! added or changed, and in a changed one which subrecords. [`undo`] puts chosen records of the
//! file as it was back into the file as it is: a changed one's old bytes in its place, a
//! removed one after the record before it in the old file (or first), an added one taken out;
//! the header's record count is counted again. Every other byte stays as it is.

use std::collections::HashMap;

/// A record's key: its type tag and what names it, and which of that key it is (0 the first).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Key {
    pub tag: String,
    pub id: String,
    pub nth: u32,
}

/// One top-level record: its key and the whole record's bytes (header included).
pub struct Rec<'a> {
    pub key: Key,
    pub bytes: &'a [u8],
}

/// What a run did to one record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    /// `removed`, `added`, `changed`.
    pub kind: &'static str,
    pub key: Key,
    /// In a changed record, the subrecords that differ: `FRMR -3`, `DATA changed`; empty else.
    pub detail: String,
}

fn str_of(data: &[u8]) -> String {
    let end = data.iter().position(|&b| b == 0).unwrap_or(data.len());
    data[..end].iter().map(|&b| b as char).collect::<String>().trim().to_string()
}

fn subs(body: &[u8]) -> impl Iterator<Item = (&[u8], &[u8])> {
    let mut i = 0usize;
    std::iter::from_fn(move || {
        if i + 8 > body.len() {
            return None;
        }
        let tag = &body[i..i + 4];
        let size = u32::from_le_bytes(body[i + 4..i + 8].try_into().expect("4 bytes")) as usize;
        let end = (i + 8).saturating_add(size).min(body.len());
        let data = &body[i + 8..end];
        i = (i + 8).saturating_add(size);
        Some((tag, data))
    })
}

fn first<'a>(body: &'a [u8], tag: &[u8]) -> Option<&'a [u8]> {
    subs(body).find(|(t, _)| *t == tag).map(|(_, d)| d)
}

fn grid(d: &[u8], at: usize) -> Option<(i32, i32)> {
    let x = i32::from_le_bytes(d.get(at..at + 4)?.try_into().ok()?);
    let y = i32::from_le_bytes(d.get(at + 4..at + 8)?.try_into().ok()?);
    Some((x, y))
}

/// What names a record of type `tag` with body `body` (`topic`: the DIAL an INFO follows).
fn name_of(tag: &[u8], body: &[u8], topic: &str) -> String {
    match tag {
        b"TES3" => "header".into(),
        b"CELL" => {
            let data = first(body, b"DATA").unwrap_or(&[]);
            let flags = data.get(..4).map(|f| u32::from_le_bytes(f.try_into().expect("4"))).unwrap_or(0);
            let name = first(body, b"NAME").map(str_of).unwrap_or_default();
            if flags & 1 == 0
                && let Some((x, y)) = grid(data, 4)
            {
                return format!("({x}, {y})");
            }
            name
        }
        b"LAND" => first(body, b"INTV").and_then(|d| grid(d, 0)).map(|(x, y)| format!("({x}, {y})")).unwrap_or_default(),
        b"PGRD" => {
            let name = first(body, b"NAME").map(str_of).unwrap_or_default();
            match first(body, b"DATA").and_then(|d| grid(d, 0)) {
                Some((x, y)) if name.is_empty() || (x, y) != (0, 0) => format!("{name} ({x}, {y})").trim().to_string(),
                _ => name,
            }
        }
        b"INFO" => format!("{topic} / {}", first(body, b"INAM").map(str_of).unwrap_or_default()),
        b"SCPT" => first(body, b"SCHD").map(|d| str_of(&d[..d.len().min(32)])).unwrap_or_default(),
        b"SKIL" | b"MGEF" => first(body, b"INDX").and_then(|d| d.get(..4)).map(|d| i32::from_le_bytes(d.try_into().expect("4")).to_string()).unwrap_or_default(),
        _ => first(body, b"NAME").map(str_of).unwrap_or_default(),
    }
}

/// A plugin's top-level records, keyed, in file order. Bytes past the last whole record are
/// left out (a cut file is read as far as it goes).
pub fn records(bytes: &[u8]) -> Vec<Rec<'_>> {
    let mut out = Vec::new();
    let mut seen: HashMap<(String, String), u32> = HashMap::new();
    let mut topic = String::new();
    let mut i = 0usize;
    while i + 16 <= bytes.len() {
        let tag = &bytes[i..i + 4];
        let size = u32::from_le_bytes(bytes[i + 4..i + 8].try_into().expect("4 bytes")) as usize;
        let end = (i + 16).saturating_add(size);
        if end > bytes.len() {
            break;
        }
        let body = &bytes[i + 16..end];
        if tag == b"DIAL" {
            topic = first(body, b"NAME").map(str_of).unwrap_or_default();
        }
        let tag_s: String = tag.iter().map(|&b| b as char).collect();
        let id = name_of(tag, body, &topic);
        let n = seen.entry((tag_s.clone(), id.to_lowercase())).or_insert(0);
        out.push(Rec { key: Key { tag: tag_s, id, nth: *n }, bytes: &bytes[i..end] });
        *n += 1;
        i = end;
    }
    out
}

/// The key a record is found by across the two files: the id compared without case, as the
/// engines compare ids.
fn fold(k: &Key) -> (String, String, u32) {
    (k.tag.clone(), k.id.to_lowercase(), k.nth)
}

/// How the subrecords of a changed record differ, by tag: `FRMR -3`, `NAM5 +1`, `DATA changed`.
fn sub_detail(before: &[u8], after: &[u8]) -> String {
    fn count(b: &[u8]) -> Vec<(String, Vec<&[u8]>)> {
        let mut m: Vec<(String, Vec<&[u8]>)> = Vec::new();
        for (t, d) in subs(b.get(16..).unwrap_or(&[])) {
            let t: String = t.iter().map(|&c| c as char).collect();
            match m.iter_mut().find(|e| e.0 == t) {
                Some(e) => e.1.push(d),
                None => m.push((t, vec![d])),
            }
        }
        m
    }
    let (a, b) = (count(before), count(after));
    let mut parts = Vec::new();
    for (t, xs) in &a {
        let ys = b.iter().find(|e| &e.0 == t).map(|e| &e.1);
        match ys {
            None => parts.push(format!("{t} -{}", xs.len())),
            Some(ys) if ys.len() != xs.len() => parts.push(format!("{t} {:+}", ys.len() as i64 - xs.len() as i64)),
            Some(ys) if ys != xs => parts.push(format!("{t} changed")),
            _ => {}
        }
    }
    for (t, ys) in &b {
        if !a.iter().any(|e| &e.0 == t) {
            parts.push(format!("{t} +{}", ys.len()));
        }
    }
    if parts.is_empty() && before.get(..16) != after.get(..16) {
        parts.push("flags".into());
    }
    parts.join(", ")
}

/// What changed between `before` and `after`, in `before`'s order and then `after`'s new records.
/// The header (TES3) is listed when it changed but is never undone by itself ([`undo`] counts
/// the records again).
pub fn diff(before: &[u8], after: &[u8]) -> Vec<Change> {
    let (a, b) = (records(before), records(after));
    let at_b: HashMap<_, usize> = b.iter().enumerate().map(|(i, r)| (fold(&r.key), i)).collect();
    let at_a: HashMap<_, usize> = a.iter().enumerate().map(|(i, r)| (fold(&r.key), i)).collect();
    let mut out = Vec::new();
    for r in &a {
        match at_b.get(&fold(&r.key)) {
            None => out.push(Change { kind: "removed", key: r.key.clone(), detail: String::new() }),
            Some(&j) if b[j].bytes != r.bytes => out.push(Change { kind: "changed", key: r.key.clone(), detail: sub_detail(r.bytes, b[j].bytes) }),
            _ => {}
        }
    }
    for r in &b {
        if !at_a.contains_key(&fold(&r.key)) {
            out.push(Change { kind: "added", key: r.key.clone(), detail: String::new() });
        }
    }
    out
}

/// `current` with the records `keys` named put back as `before` had them (a record `before` did
/// not have taken out). The header's record count moves by as many records as the file gained
/// or lost against `before` (a count that was wrong stays as wrong as it was), and when every
/// record is as `before` had it, the header is `before`'s too: the file is the file it was.
/// Answers the new bytes.
pub fn undo(before: &[u8], current: &[u8], keys: &[Key]) -> Vec<u8> {
    let want: std::collections::HashSet<_> = keys.iter().map(fold).filter(|k| k.0 != "TES3").collect();
    let old = records(before);
    let at_old: HashMap<_, usize> = old.iter().enumerate().map(|(i, r)| (fold(&r.key), i)).collect();
    let mut recs: Vec<(String, String, u32, Vec<u8>)> = records(current)
        .into_iter()
        .filter(|r| {
            let k = fold(&r.key);
            // An added record undone is taken out.
            !(want.contains(&k) && !at_old.contains_key(&k))
        })
        .map(|r| {
            let k = fold(&r.key);
            let bytes = if want.contains(&k) { old[at_old[&k]].bytes.to_vec() } else { r.bytes.to_vec() };
            (k.0, k.1, k.2, bytes)
        })
        .collect();
    // A removed record undone: after the nearest record before it in the old file that the file
    // has now (first, after the header, when none).
    for (i, r) in old.iter().enumerate() {
        let k = fold(&r.key);
        if !want.contains(&k) || recs.iter().any(|x| (&x.0, &x.1, x.2) == (&k.0, &k.1, k.2)) {
            continue;
        }
        let place = old[..i]
            .iter()
            .rev()
            .find_map(|p| {
                let pk = fold(&p.key);
                recs.iter().position(|x| (&x.0, &x.1, x.2) == (&pk.0, &pk.1, pk.2))
            })
            .map(|p| p + 1)
            .unwrap_or_else(|| usize::from(recs.first().is_some_and(|x| x.0 == "TES3")));
        recs.insert(place, (k.0, k.1, k.2, r.bytes.to_vec()));
    }
    let new_body: Vec<&[u8]> = recs.iter().filter(|r| r.0 != "TES3").map(|r| r.3.as_slice()).collect();
    let old_body: Vec<&[u8]> = old.iter().filter(|r| r.key.tag != "TES3").map(|r| r.bytes).collect();
    if new_body == old_body {
        return before.to_vec();
    }
    let now = recs.iter().filter(|r| r.0 != "TES3").count() as i64;
    let was = old_body.len() as i64;
    let mut out: Vec<u8> = recs.into_iter().flat_map(|r| r.3).collect();
    if let Some(had) = record_count(before) {
        set_record_count(&mut out, (i64::from(had) + now - was).max(0) as u32);
    }
    out
}

/// The TES3 header's HEDR record count, when the file has one.
fn record_count(bytes: &[u8]) -> Option<u32> {
    hedr_at(bytes).map(|at| u32::from_le_bytes(bytes[at..at + 4].try_into().expect("4")))
}

/// Where the TES3 header's HEDR record count lies.
fn hedr_at(bytes: &[u8]) -> Option<usize> {
    if bytes.get(..4) != Some(b"TES3") || bytes.len() < 16 {
        return None;
    }
    let size = u32::from_le_bytes(bytes[4..8].try_into().expect("4")) as usize;
    let end = (16 + size).min(bytes.len());
    let mut i = 16;
    while i + 8 <= end {
        let len = u32::from_le_bytes(bytes[i + 4..i + 8].try_into().expect("4")) as usize;
        if &bytes[i..i + 4] == b"HEDR" && len >= 300 && i + 8 + 300 <= end {
            return Some(i + 8 + 296);
        }
        i += 8 + len;
    }
    None
}

/// The TES3 header's HEDR record count set to `n` (nothing when the file has no such header).
fn set_record_count(bytes: &mut [u8], n: u32) {
    if let Some(at) = hedr_at(bytes) {
        bytes[at..at + 4].copy_from_slice(&n.to_le_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sub(t: &[u8], d: &[u8]) -> Vec<u8> {
        [t, &(d.len() as u32).to_le_bytes(), d].concat()
    }
    fn rec(t: &[u8], subs: &[Vec<u8>]) -> Vec<u8> {
        let body = subs.concat();
        [t, &(body.len() as u32).to_le_bytes()[..], &[0; 8], &body].concat()
    }
    fn header(n: u32) -> Vec<u8> {
        let mut h = 1.3f32.to_le_bytes().to_vec();
        h.extend([0; 4]);
        h.extend([0; 32]);
        h.extend([0; 256]);
        h.extend(n.to_le_bytes());
        rec(b"TES3", &[sub(b"HEDR", &h)])
    }
    fn count(b: &[u8]) -> u32 {
        u32::from_le_bytes(b[16 + 8 + 296..16 + 8 + 300].try_into().unwrap())
    }
    fn ext_cell(x: i32, y: i32, refs: usize) -> Vec<u8> {
        let mut data = 2u32.to_le_bytes().to_vec();
        data.extend(x.to_le_bytes());
        data.extend(y.to_le_bytes());
        let mut s = vec![sub(b"NAME", b"\0"), sub(b"DATA", &data)];
        for i in 0..refs {
            s.push(sub(b"FRMR", &(i as u32).to_le_bytes()));
            s.push(sub(b"NAME", b"rock\0"));
        }
        rec(b"CELL", &s)
    }

    #[test]
    fn a_run_is_listed_by_record_and_undone_by_record() {
        let gmst = rec(b"GMST", &[sub(b"NAME", b"fWerewolfRunMult\0"), sub(b"FLTV", &1f32.to_le_bytes())]);
        let stat = rec(b"STAT", &[sub(b"NAME", b"Rock\0"), sub(b"MODL", b"r.nif\0")]);
        let dial = rec(b"DIAL", &[sub(b"NAME", b"Greeting 1\0")]);
        let info = |i: &[u8], t: &[u8]| rec(b"INFO", &[sub(b"INAM", i), sub(b"NAME", t)]);
        let before = [header(6), gmst.clone(), stat.clone(), ext_cell(1, 2, 3), dial.clone(), info(b"1\0", b"hi\0"), info(b"2\0", b"yo\0")].concat();
        // A clean: the GMST gone, the cell's references down to one, an INFO's text changed, one added.
        let after = [header(6), stat.clone(), ext_cell(1, 2, 1), dial.clone(), info(b"1\0", b"hi\0"), info(b"2\0", b"YO\0"), info(b"3\0", b"new\0")].concat();
        let d = diff(&before, &after);
        let show: Vec<String> = d.iter().map(|c| format!("{} {} {}{}", c.kind, c.key.tag, c.key.id, if c.detail.is_empty() { String::new() } else { format!(" [{}]", c.detail) })).collect();
        assert_eq!(
            show,
            [
                "removed GMST fWerewolfRunMult",
                "changed CELL (1, 2) [NAME -2, FRMR -2]",
                "changed INFO Greeting 1 / 2 [NAME changed]",
                "added INFO Greeting 1 / 3"
            ]
        );
        // Everything undone is the file as it was, but the count, made right.
        let keys: Vec<Key> = d.iter().map(|c| c.key.clone()).collect();
        let back = undo(&before, &after, &keys);
        assert_eq!(back, before);
        // One at a time, in any order, comes to the same.
        let mut cur = after.clone();
        for k in keys.iter().rev() {
            cur = undo(&before, &cur, std::slice::from_ref(k));
        }
        assert_eq!(cur, before);
        // Only the GMST put back: first after the header, the count one more.
        let one = undo(&before, &after, &[d[0].key.clone()]);
        assert_eq!(records(&one)[1].key.tag, "GMST");
        assert_eq!(count(&one), 7);
        // The other three still differ, and the header with them (its count is 7, it was 6).
        let left: Vec<String> = diff(&before, &one).into_iter().map(|c| c.key.tag).collect();
        assert_eq!(left, ["TES3", "CELL", "INFO", "INFO"]);
    }

    #[test]
    fn interiors_by_name_exteriors_by_grid_and_a_second_of_a_key_by_count() {
        let int = |n: &[u8]| rec(b"CELL", &[sub(b"NAME", n), sub(b"DATA", &[1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])]);
        let file = [header(3), int(b"Balmora, Guild\0"), ext_cell(-3, 4, 0), int(b"balmora, guild\0")].concat();
        let keys: Vec<(String, String, u32)> = records(&file).into_iter().map(|r| (r.key.tag, r.key.id, r.key.nth)).collect();
        assert_eq!(keys[1], ("CELL".into(), "Balmora, Guild".into(), 0));
        assert_eq!(keys[2], ("CELL".into(), "(-3, 4)".into(), 0));
        assert_eq!(keys[3], ("CELL".into(), "balmora, guild".into(), 1));
    }
}
