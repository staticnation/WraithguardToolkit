//! The per-record half of the load-order lint (`lint_plugins` in
//! `wraithguard_toolkit.py`): one pass over a plugin's raw records that pulls
//! out the facts the checks need -- evil GMSTs, interior cells and their fog,
//! interior path grids, header gaps, masters, and expansion-only script calls.
//!
//! Ported step for step from the Python `_lint_*` helpers it replaced, down to
//! how a truncated record is clamped, which duplicate subrecord wins, Python's
//! `str.strip()` whitespace set and `re`'s Unicode word boundaries over
//! Latin-1 text. The warning wording, the load-order-wide accumulators and the
//! skip lists stay in Python; `tests/test_lint_native_parity.py` checks the
//! two agree on synthetic plugins and on a real load order.

use std::collections::{BTreeSet, HashMap};

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyTuple};

/// Top-level records: `(tag, body)`, the body clamped to what the file holds.
fn records(raw: &[u8]) -> impl Iterator<Item = (&[u8], &[u8])> {
    frames(raw, 16)
}

/// Subrecords of a record body: `(tag, data)`, clamped likewise.
fn subrecords(body: &[u8]) -> impl Iterator<Item = (&[u8], &[u8])> {
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

/// Python's `str.isspace()` over Latin-1.
fn is_py_space(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\x0b' | '\x0c' | '\r' | '\x1c'..='\x1f' | ' ' | '\u{85}' | '\u{a0}')
}

/// Python `re`'s `\w` over Latin-1 text: `str.isalnum()` or underscore.
fn is_py_word(b: u8) -> bool {
    let c = b as char;
    c == '_' || c.is_alphanumeric()
}

fn latin1(b: &[u8]) -> String {
    b.iter().map(|&x| x as char).collect()
}

/// A NUL-terminated string field: up to the first NUL, Latin-1, stripped.
fn zstr(b: &[u8]) -> String {
    let end = b.iter().position(|&x| x == 0).unwrap_or(b.len());
    latin1(&b[..end]).trim_matches(is_py_space).to_string()
}

/// The expansion-function matcher: `^[^;\n]*?\b(NAME|...)\b`, case-insensitive,
/// per line. At most one match per line -- the leftmost, and at one position the
/// first name in list order -- returned as spelled in the script.
struct Calls {
    names: Vec<Vec<u8>>,
}

impl Calls {
    fn scan(&self, text: &[u8], found: &mut BTreeSet<String>) {
        let mut line_start = 0usize;
        while line_start <= text.len() {
            let line_end = text[line_start..].iter().position(|&c| c == b'\n').map_or(text.len(), |p| line_start + p);
            let stop = text[line_start..line_end].iter().position(|&c| c == b';').map_or(line_end, |p| line_start + p);
            'pos: for p in line_start..stop {
                let before_word = p > 0 && is_py_word(text[p - 1]);
                if before_word || !is_py_word(text[p]) {
                    continue;
                }
                for name in &self.names {
                    let end = p + name.len();
                    if end > text.len() || !text[p..end].eq_ignore_ascii_case(name) {
                        continue;
                    }
                    if end < text.len() && is_py_word(text[end]) {
                        continue;
                    }
                    found.insert(latin1(&text[p..end]));
                    break 'pos;
                }
            }
            if line_end == text.len() {
                break;
            }
            line_start = line_end + 1;
        }
    }
}

/// Reads one plugin's records into the facts the lint checks consume.
#[pyclass(frozen)]
struct PluginLinter {
    evil: HashMap<String, (Vec<u8>, Vec<u8>)>,
    tribunal: Calls,
    bloodmoon: Calls,
    skip_cells: BTreeSet<String>,
}

#[pymethods]
impl PluginLinter {
    /// `evil_gmsts` maps a lower-cased setting name to `(value tag, raw value)`;
    /// the function lists are the Tribunal and Bloodmoon script functions;
    /// `skip_cells` are lower-cased cell names left out of the pathgrid check.
    #[new]
    fn new(
        evil_gmsts: HashMap<String, (String, Vec<u8>)>,
        tribunal_funcs: Vec<String>,
        bloodmoon_funcs: Vec<String>,
        skip_cells: BTreeSet<String>,
    ) -> Self {
        let strip = |v: Vec<u8>| {
            let end = v.iter().rposition(|&b| b != 0).map_or(0, |p| p + 1);
            v[..end].to_vec()
        };
        Self {
            evil: evil_gmsts.into_iter().map(|(k, (t, v))| (k, (t.into_bytes(), strip(v)))).collect(),
            tribunal: Calls { names: tribunal_funcs.into_iter().map(String::into_bytes).collect() },
            bloodmoon: Calls { names: bloodmoon_funcs.into_iter().map(String::into_bytes).collect() },
            skip_cells,
        }
    }

    /// One plugin's lint facts. `is_custom` turns on the header and
    /// expansion-call checks, which apply only to the user's own mods.
    ///
    /// Returns a dict: `events` (in record order: `("header", [missing])` or
    /// `("cell", name, cell_id, fog_bug)`), `evil_gmsts`, `pathgrids`,
    /// `masters`, `tribunal`, `bloodmoon`.
    fn scan<'py>(&self, py: Python<'py>, raw: &[u8], is_custom: bool) -> PyResult<Bound<'py, PyDict>> {
        let facts = py.detach(|| self.facts(raw, is_custom));
        let out = PyDict::new(py);
        let events = PyList::empty(py);
        for e in facts.events {
            match e {
                Event::Header(missing) => events.append(("header", missing))?,
                Event::Cell { name, cell_id, fog_bug } => events.append(PyTuple::new(
                    py,
                    [
                        "cell".into_pyobject(py)?.into_any(),
                        name.into_pyobject(py)?.into_any(),
                        cell_id.into_pyobject(py)?.into_any(),
                        pyo3::types::PyBool::new(py, fog_bug).to_owned().into_any(),
                    ],
                )?)?,
            }
        }
        out.set_item("events", events)?;
        out.set_item("evil_gmsts", facts.evil_gmsts)?;
        out.set_item("pathgrids", facts.pathgrids)?;
        out.set_item("masters", facts.masters.into_iter().collect::<Vec<_>>())?;
        out.set_item("tribunal", facts.tribunal.into_iter().collect::<Vec<_>>())?;
        out.set_item("bloodmoon", facts.bloodmoon.into_iter().collect::<Vec<_>>())?;
        Ok(out)
    }
}

enum Event {
    Header(Vec<&'static str>),
    Cell { name: String, cell_id: String, fog_bug: bool },
}

#[derive(Default)]
struct Facts {
    events: Vec<Event>,
    evil_gmsts: Vec<String>,
    pathgrids: Vec<String>,
    masters: BTreeSet<String>,
    tribunal: BTreeSet<String>,
    bloodmoon: BTreeSet<String>,
}

impl PluginLinter {
    fn facts(&self, raw: &[u8], is_custom: bool) -> Facts {
        let mut f = Facts::default();
        for (tag, body) in records(raw) {
            match tag {
                b"SCPT" | b"INFO" if is_custom => {
                    let want: &[u8] = if tag == b"SCPT" { b"SCTX" } else { b"BNAM" };
                    for (st, data) in subrecords(body) {
                        if st == want && !data.is_empty() {
                            self.tribunal.scan(data, &mut f.tribunal);
                            self.bloodmoon.scan(data, &mut f.bloodmoon);
                        }
                    }
                }
                b"TES3" => {
                    for (st, data) in subrecords(body) {
                        if st == b"MAST" {
                            f.masters.insert(zstr(data).to_lowercase());
                        }
                    }
                    if is_custom {
                        let missing = header_gaps(body);
                        if !missing.is_empty() {
                            f.events.push(Event::Header(missing));
                        }
                    }
                }
                b"GMST" => {
                    if let Some(name) = self.evil_gmst(body) {
                        f.evil_gmsts.push(name);
                    }
                }
                b"CELL" => {
                    if let Some(e) = self.cell(body) {
                        f.events.push(e);
                    }
                }
                b"PGRD" => {
                    if let Some(name) = interior_pathgrid(body) {
                        f.pathgrids.push(name);
                    }
                }
                _ => {}
            }
        }
        f
    }

    /// The setting's name when both name and value match the evil table.
    fn evil_gmst(&self, body: &[u8]) -> Option<String> {
        let mut name: Option<String> = None;
        let mut value: Option<(&[u8], &[u8])> = None;
        for (st, data) in subrecords(body) {
            match st {
                b"NAME" => name = Some(zstr(data).to_lowercase()),
                b"STRV" | b"INTV" | b"FLTV" => value = Some((st, data)),
                _ => {}
            }
        }
        let name = name.filter(|n| !n.is_empty())?;
        let (want_tag, want_value) = self.evil.get(&name)?;
        let (tag, data) = value?;
        let end = data.iter().rposition(|&b| b != 0).map_or(0, |p| p + 1);
        (tag == want_tag.as_slice() && &data[..end] == want_value.as_slice()).then_some(name)
    }

    /// An interior cell's name, pathgrid id and fog-bug flag; `None` for an
    /// exterior or a `DATA` too short to read.
    fn cell(&self, body: &[u8]) -> Option<Event> {
        let mut name = String::new();
        let mut data: Option<&[u8]> = None;
        let mut ambience: Option<&[u8]> = None;
        for (st, payload) in subrecords(body) {
            match st {
                b"NAME" => name = zstr(payload),
                b"DATA" if data.is_none() => data = Some(payload),
                b"AMBI" => ambience = Some(payload),
                _ => {}
            }
        }
        let data = data.filter(|d| d.len() >= 12)?;
        let flags = u32::from_le_bytes(data[0..4].try_into().expect("4 bytes"));
        if flags & 1 == 0 {
            return None;
        }
        let cell_id = name.to_lowercase();
        let mut fog_bug = false;
        if flags & 128 == 0 {
            let fog = match ambience {
                Some(a) if a.len() == 16 => f32::from_le_bytes(a[12..16].try_into().expect("4 bytes")),
                _ => f32::from_le_bytes(data[8..12].try_into().expect("4 bytes")),
            };
            fog_bug = fog == 0.0;
        }
        let cell_id = if self.skip_cells.contains(&cell_id) { String::new() } else { cell_id };
        Some(Event::Cell { name, cell_id, fog_bug })
    }
}

/// Which of author and description the first full `HEDR` leaves blank.
fn header_gaps(body: &[u8]) -> Vec<&'static str> {
    for (st, data) in subrecords(body) {
        if st == b"HEDR" && data.len() >= 296 {
            let mut out = Vec::new();
            if zstr(&data[8..40]).is_empty() {
                out.push("author");
            }
            if zstr(&data[40..296]).is_empty() {
                out.push("description");
            }
            return out;
        }
    }
    Vec::new()
}

/// The lower-cased cell name of an interior path grid (grid `(0, 0)`, named).
fn interior_pathgrid(body: &[u8]) -> Option<String> {
    let mut name = String::new();
    let mut grid: Option<(i32, i32)> = None;
    for (st, data) in subrecords(body) {
        match st {
            b"NAME" => name = zstr(data),
            b"DATA" if data.len() >= 8 => {
                grid = Some((
                    i32::from_le_bytes(data[0..4].try_into().expect("4 bytes")),
                    i32::from_le_bytes(data[4..8].try_into().expect("4 bytes")),
                ));
            }
            _ => {}
        }
    }
    (grid == Some((0, 0)) && !name.is_empty()).then(|| name.to_lowercase())
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PluginLinter>()?;
    Ok(())
}
