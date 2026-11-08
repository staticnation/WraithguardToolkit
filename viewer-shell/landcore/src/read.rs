//! `LAND` and `LTEX` records read straight out of a plugin (`land/native.py`).
//!
//! A TES3 file does not have to be understood to be walked: every record and subrecord is
//! length-prefixed, so a reader that only wants `LAND` and `LTEX` skips everything else by
//! its declared size - including record types (OpenMW's `LUAL`) that a full reader refuses
//! the whole file over. A file not starting `TES3` is refused: an Oblivion or Skyrim plugin
//! lays its headers out differently, and walking one invents landscape out of its bytes.

use std::path::Path;

const RECORD_HEADER: usize = 16;
const SUBRECORD_HEADER: usize = 8;
const DELETED: u32 = 0x20;
const VHGT_SIZE: usize = 4232;

/// One record the merge reads.
#[derive(Clone, Debug, PartialEq)]
pub enum Record {
    /// A `LAND` record: the `DATA` flags (`None` without one), each grid's raw payload
    /// (`None` when absent or empty), heights as their `f32` offset and 4,225 deltas.
    Land {
        deleted: bool,
        grid: (i64, i64),
        flags: Option<u32>,
        heights: Option<(f32, Vec<u8>)>,
        normals: Option<Vec<u8>>,
        world_map: Option<Vec<u8>>,
        colors: Option<Vec<u8>>,
        textures: Option<Vec<u8>>,
    },
    /// An `LTEX` record (index 0 without an `INTV`).
    Ltex { deleted: bool, id: String, index: crate::grid::Value, file_name: Option<String> },
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

/// `(tag, object flags, body)` for each top-level record; a truncated last record ends it.
fn records(data: &[u8]) -> Vec<(&[u8], u32, &[u8])> {
    let mut out = Vec::new();
    let mut pos = 0;
    while pos + RECORD_HEADER <= data.len() {
        let tag = &data[pos..pos + 4];
        let size = u32_at(data, pos + 4) as usize;
        let flags = u32_at(data, pos + 12);
        let start = pos + RECORD_HEADER;
        let Some(end) = start.checked_add(size).filter(|e| *e <= data.len()) else { break };
        out.push((tag, flags, &data[start..end]));
        pos = end;
    }
    out
}

fn subrecords(body: &[u8]) -> Vec<(&[u8], &[u8])> {
    let mut out = Vec::new();
    let mut pos = 0;
    while pos + SUBRECORD_HEADER <= body.len() {
        let tag = &body[pos..pos + 4];
        let size = u32_at(body, pos + 4) as usize;
        let start = pos + SUBRECORD_HEADER;
        let Some(end) = start.checked_add(size).filter(|e| *e <= body.len()) else { break };
        out.push((tag, &body[start..end]));
        pos = end;
    }
    out
}

/// A NUL-terminated string, as latin-1 (`_text`).
fn text(payload: &[u8]) -> String {
    let end = payload.iter().position(|&b| b == 0).unwrap_or(payload.len());
    payload[..end].iter().map(|&b| b as char).collect()
}

fn nonempty(p: &[u8]) -> Option<Vec<u8>> {
    (!p.is_empty()).then(|| p.to_vec())
}

fn landscape(flags: u32, body: &[u8]) -> Option<Record> {
    let mut deleted = flags & DELETED != 0;
    let (mut grid, mut data_flags) = (None, None);
    let (mut heights, mut normals, mut world_map, mut colors, mut textures) = (None, None, None, None, None);
    for (tag, p) in subrecords(body) {
        match tag {
            b"INTV" if p.len() >= 8 => {
                grid = Some((i32::from_le_bytes([p[0], p[1], p[2], p[3]]) as i64, i32::from_le_bytes([p[4], p[5], p[6], p[7]]) as i64));
            }
            b"DATA" if p.len() >= 4 => data_flags = Some(u32_at(p, 0)),
            b"VHGT" if p.len() >= VHGT_SIZE => heights = Some((f32::from_le_bytes([p[0], p[1], p[2], p[3]]), p[4..4 + 65 * 65].to_vec())),
            b"VNML" => normals = nonempty(p),
            b"WNAM" => world_map = nonempty(p),
            b"VCLR" => colors = nonempty(p),
            b"VTEX" => textures = nonempty(p),
            b"DELE" => deleted = true,
            _ => {}
        }
    }
    let grid = grid?;
    Some(Record::Land { deleted, grid, flags: data_flags, heights, normals, world_map, colors, textures })
}

fn texture(flags: u32, body: &[u8]) -> Option<Record> {
    let mut deleted = flags & DELETED != 0;
    let (mut id, mut index, mut file_name) = (None, 0, None);
    for (tag, p) in subrecords(body) {
        match tag {
            b"NAME" => id = Some(text(p)),
            b"INTV" if p.len() >= 4 => index = u32_at(p, 0) as crate::grid::Value,
            b"DATA" => file_name = Some(text(p)),
            b"DELE" => deleted = true,
            _ => {}
        }
    }
    let id = id.filter(|s| !s.is_empty())?;
    Some(Record::Ltex { deleted, id, index, file_name })
}

/// Every `LAND` and `LTEX` record of a plugin's bytes, in file order
/// (`read_landscape_records`).
pub fn landscape_records_of(data: &[u8], name: &str) -> Result<Vec<Record>, String> {
    if !data.starts_with(b"TES3") {
        let found: String = if data.len() >= 4 { data[..4].iter().map(|&b| b as char).collect() } else { "nothing".into() };
        return Err(format!(
            "{name} is not a Morrowind plugin: it begins '{found}', not 'TES3'. Reading it as one would invent landscape out of whatever its bytes happen to spell."
        ));
    }
    Ok(records(data)
        .into_iter()
        .filter_map(|(tag, flags, body)| match tag {
            b"LAND" => landscape(flags, body),
            b"LTEX" => texture(flags, body),
            _ => None,
        })
        .collect())
}

/// [`landscape_records_of`] a file.
pub fn read_landscape_records(path: &Path) -> Result<Vec<Record>, String> {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let data = std::fs::read(path).map_err(|e| format!("could not read {name}: {e}"))?;
    landscape_records_of(&data, &name)
}

/// A plugin's terrain records as the merge wants them, the file read once: none, without a
/// failure, for a file that is not a TES3 plugin or has no terrain. What `has_landscape` then
/// `read_landscape_records` gave, without the second read and the scan of every byte for the
/// two tags first (the framing walk below skips a record by its size, and finds none in a
/// plugin with no terrain as surely).
pub fn read_terrain_records(path: &Path) -> Result<Vec<Record>, String> {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let data = std::fs::read(path).map_err(|e| format!("could not read {name}: {e}"))?;
    if !data.starts_with(b"TES3") {
        return Ok(Vec::new());
    }
    landscape_records_of(&data, &name)
}

/// Whether a plugin mentions landscape at all (`has_landscape`): `LAND` or `LTEX` anywhere
/// in a file that starts `TES3`; `true` when it cannot be read, so the real reader says why.
pub fn has_landscape(path: &Path) -> bool {
    let Ok(data) = std::fs::read(path) else { return true };
    if !data.starts_with(b"TES3") {
        return false;
    }
    data.windows(4).any(|w| w == b"LAND" || w == b"LTEX")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sub(tag: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut v = tag.to_vec();
        v.extend((data.len() as u32).to_le_bytes());
        v.extend_from_slice(data);
        v
    }

    fn record(tag: &[u8; 4], flags: u32, body: &[u8]) -> Vec<u8> {
        let mut v = tag.to_vec();
        v.extend((body.len() as u32).to_le_bytes());
        v.extend([0u8; 4]);
        v.extend(flags.to_le_bytes());
        v.extend_from_slice(body);
        v
    }

    fn header() -> Vec<u8> {
        record(b"TES3", 0, &sub(b"HEDR", &[0u8; 300]))
    }

    fn land(x: i32, y: i32) -> Vec<u8> {
        let mut grid = x.to_le_bytes().to_vec();
        grid.extend(y.to_le_bytes());
        let mut heights = 12.5f32.to_le_bytes().to_vec();
        heights.extend((0..VHGT_SIZE - 4).map(|i| (i % 251) as u8));
        let mut body = sub(b"INTV", &grid);
        body.extend(sub(b"DATA", &9u32.to_le_bytes()));
        body.extend(sub(b"VHGT", &heights));
        record(b"LAND", 0, &body)
    }

    fn ltex(id: &str, index: u32, file: &str) -> Vec<u8> {
        let mut body = sub(b"NAME", &[id.as_bytes(), &[0]].concat());
        body.extend(sub(b"INTV", &index.to_le_bytes()));
        body.extend(sub(b"DATA", &[file.as_bytes(), &[0]].concat()));
        record(b"LTEX", 0, &body)
    }

    fn scratch(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("wg-landread-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// What the merge did before: `has_landscape`, then a second read.
    fn before(path: &Path) -> Result<Vec<Record>, String> {
        if !has_landscape(path) {
            return Ok(Vec::new());
        }
        read_landscape_records(path)
    }

    #[test]
    fn one_read_gives_what_the_scan_then_the_read_gave() {
        let dir = scratch("same");
        let cases: Vec<(&str, Vec<u8>)> = vec![
            ("terrain.esp", [header(), ltex("tx_a", 3, "tx_a.dds"), land(-2, -9), land(3, -11), ltex("tx_b", 4, "tx_b.dds")].concat()),
            ("none.esp", [header(), record(b"STAT", 0, &sub(b"NAME", b"rock\0"))].concat()),
            // The tag's letters inside a string, not as a record.
            ("inside.esp", [header(), record(b"BOOK", 0, &sub(b"TEXT", b"the LAND and LTEX of Vvardenfell\0"))].concat()),
            ("deleted.esp", [header(), record(b"LAND", DELETED, &sub(b"INTV", &[1, 0, 0, 0, 2, 0, 0, 0]))].concat()),
            ("truncated.esp", { let mut v = [header(), land(1, 1)].concat(); v.extend(b"LAND\xff\xff\xff\x7f"); v }),
            ("skyrim.esp", b"TES4 not a morrowind plugin LAND LTEX".to_vec()),
            ("empty.esp", Vec::new()),
            ("tiny.esp", b"TES".to_vec()),
        ];
        for (name, bytes) in &cases {
            let p = dir.join(name);
            std::fs::write(&p, bytes).unwrap();
            assert_eq!(read_terrain_records(&p), before(&p), "{name}");
        }
        let terrain = read_terrain_records(&dir.join("terrain.esp")).unwrap();
        assert_eq!(terrain.len(), 4, "two lands and two textures");
        assert!(read_terrain_records(&dir.join("none.esp")).unwrap().is_empty());
        assert!(read_terrain_records(&dir.join("skyrim.esp")).unwrap().is_empty(), "not a TES3 plugin: no terrain, no failure");
        // A file that is not there fails as it did, in the same words.
        let missing = dir.join("missing.esp");
        assert_eq!(read_terrain_records(&missing), before(&missing));
        assert!(read_terrain_records(&missing).unwrap_err().starts_with("could not read missing.esp"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
