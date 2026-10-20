//! A compiled script read back: its `bytecode` field decoded (base64, zstd'd or not, as
//! tes3conv and the crate write it) and disassembled into named instructions
//! (`wraithguard/mwscript/tes3conv.py` and `disassembler.py`, port step 4.1, slice 5d).
//!
//! The stream is little-endian `u16` opcodes, most with operands whose shapes the opcode
//! table gives (`data/opcodes.json`, from `wraithguard/mwscript/opcodes.py`). Expressions
//! (the `x == 1` of an `if`) are not opcodes but semi-textual data inside the stream, so
//! what the table cannot account for is shown as raw spans, and decoding picks up again at
//! the next byte that plausibly starts a known instruction.

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use base64::Engine as _;
use serde_json::Value;

use crate::pyval::float_repr;

const FLAG_BYTE: u64 = 0x1;
const FLAG_SHORT: u64 = 0x2;
const FLAG_LONG: u64 = 0x4;
const FLAG_FLOAT: u64 = 0x8;
const FLAG_STRING: u64 = 0x10;
const FLAG_ID: u64 = 0x20;
const FLAG_OPTIONAL: u64 = 0x800;

/// Leading bytes of a zstd frame.
const ZSTD_MAGIC: &[u8] = b"\x28\xb5\x2f\xfd";

/// The opcode table: opcode -> (name, parameter flags), and the compiler's own and the
/// MWSE / MW-Enhanced opcodes.
pub struct Opcodes {
    pub functions: HashMap<u16, (String, Vec<u64>)>,
    pub internal: HashSet<u16>,
    pub extended: HashSet<u16>,
    pub min: u16,
    pub max: u16,
}

/// The table, read once.
pub fn opcodes() -> &'static Opcodes {
    static T: OnceLock<Opcodes> = OnceLock::new();
    T.get_or_init(|| {
        let v: Value = serde_json::from_str(include_str!("../../data/opcodes.json")).expect("data/opcodes.json is JSON");
        let ops = |k: &str| -> HashSet<u16> { v[k].as_array().into_iter().flatten().filter_map(Value::as_u64).map(|n| n as u16).collect() };
        let mut functions = HashMap::new();
        for f in v["functions"].as_array().into_iter().flatten() {
            let op = f[0].as_u64().unwrap_or(0) as u16;
            let params = f[2].as_array().into_iter().flatten().filter_map(Value::as_u64).collect();
            functions.insert(op, (f[1].as_str().unwrap_or("").to_string(), params));
        }
        let min = functions.keys().copied().min().unwrap_or(0);
        let max = functions.keys().copied().max().unwrap_or(0);
        Opcodes { functions, internal: ops("internal"), extended: ops("extended"), min, max }
    })
}

/// A `bytecode` (or `variables`) field back to raw bytes: base64, and the zstd frame
/// under it when there is one.
pub fn decode_field(value: &Value) -> Result<Vec<u8>, String> {
    let raw = match value {
        Value::String(s) => base64::engine::general_purpose::GeneralPurpose::new(&base64::alphabet::STANDARD, base64::engine::general_purpose::PAD.with_decode_allow_trailing_bits(true))
            .decode(s)
            .map_err(|e| {
                // As Python's binascii says it.
                let why = match e {
                    base64::DecodeError::InvalidByte(_, b) if b != b'=' => "Only base64 data is allowed",
                    _ => "Incorrect padding",
                };
                format!("not valid base64: {why}")
            })?,
        Value::Array(a) => a.iter().map(|b| b.as_u64().unwrap_or(0) as u8).collect(),
        _ => return Err("not valid base64: not text".into()),
    };
    if raw.starts_with(ZSTD_MAGIC) {
        let out = zstd::stream::decode_all(&raw[..]).map_err(|e| format!("corrupt zstd frame: {e}"))?;
        if out.is_empty() {
            return Err("zstd frame decompressed to nothing -- it is corrupt or truncated. Reporting this rather than showing an empty script, which would read as 'this plugin's script is blank'.".into());
        }
        return Ok(out);
    }
    Ok(raw)
}

/// A `variables` field's local names, in declaration order (after its 4-byte length).
pub fn decode_variables(value: &Value) -> Result<Vec<String>, String> {
    let raw = decode_field(value)?;
    let body = if raw.len() >= 4 { &raw[4..] } else { &[][..] };
    Ok(body.split(|&b| b == 0).filter(|n| !n.is_empty()).map(|n| n.iter().map(|&b| b as char).collect()).collect())
}

/// An operand.
#[derive(Clone, Debug, PartialEq)]
pub enum Operand {
    Int(i64),
    Float(f32),
    Text(String),
}

/// One decoded instruction, or a span the table could not account for.
#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    Instruction { offset: usize, opcode: u16, name: String, operands: Vec<Operand>, size: usize },
    Raw { offset: usize, data: Vec<u8> },
}

impl Item {
    fn size(&self) -> usize {
        match self {
            Item::Instruction { size, .. } => *size,
            Item::Raw { data, .. } => data.len(),
        }
    }
}

/// A disassembled script.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Listing {
    /// The length the 4-byte prefix records.
    pub declared_length: Option<u32>,
    pub items: Vec<Item>,
}

impl Listing {
    /// The share of bytes decoded as instructions.
    pub fn decoded_ratio(&self) -> f64 {
        let total: usize = self.items.iter().map(Item::size).sum();
        if total == 0 {
            return 0.0;
        }
        let decoded: usize = self.items.iter().filter(|i| matches!(i, Item::Instruction { .. })).map(Item::size).sum();
        decoded as f64 / total as f64
    }
}

/// A float a real script would use (bytes of an expression read as one are wild).
fn plausible_float(v: f64) -> bool {
    v.is_finite() && (v == 0.0 || (1e-6..=1e7).contains(&v.abs()))
}

/// Text that could be an id (a length byte that was not one runs into other data).
fn plausible_identifier(text: &str) -> bool {
    !text.is_empty() && text.chars().all(|c| c.is_alphanumeric() || "_- '.".contains(c))
}

/// One instruction's operands and where they end, or `None` when they run past the data
/// or use an encoding this does not read.
fn read_operands(data: &[u8], mut pos: usize, params: &[u64]) -> Option<(Vec<Operand>, usize)> {
    let mut out = Vec::new();
    for &flags in params {
        if flags & FLAG_OPTIONAL != 0 && pos >= data.len() {
            break;
        }
        let fixed = [(FLAG_BYTE, 1usize), (FLAG_SHORT, 2), (FLAG_LONG, 4), (FLAG_FLOAT, 4)].into_iter().find(|(bit, _)| flags & bit != 0);
        if let Some((bit, width)) = fixed {
            if pos + width > data.len() {
                return None;
            }
            let b = &data[pos..pos + width];
            out.push(match bit {
                FLAG_BYTE => Operand::Int(b[0] as i8 as i64),
                FLAG_SHORT => Operand::Int(i16::from_le_bytes([b[0], b[1]]) as i64),
                FLAG_LONG => Operand::Int(i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as i64),
                _ => {
                    let f = f32::from_le_bytes([b[0], b[1], b[2], b[3]]);
                    if !plausible_float(f as f64) {
                        return None;
                    }
                    Operand::Float(f)
                }
            });
            pos += width;
        } else if flags & (FLAG_STRING | FLAG_ID) != 0 {
            let len = *data.get(pos)? as usize;
            if pos + 1 + len > data.len() {
                return None;
            }
            let text: String = data[pos + 1..pos + 1 + len].iter().map(|&c| c as char).collect();
            if !plausible_identifier(&text) {
                return None;
            }
            out.push(Operand::Text(text));
            pos += 1 + len;
        } else if flags & FLAG_OPTIONAL != 0 {
            break;
        } else {
            return None;
        }
    }
    Some((out, pos))
}

/// Disassembles `SCDT` bytes (with their 4-byte length first). With the script's source,
/// only functions it names are decoded (an opcode's value inside expression data is not
/// taken for one).
pub fn disassemble(data: &[u8], has_length_prefix: bool, source_text: Option<&str>) -> Listing {
    let table = opcodes();
    let allowed: Option<HashSet<String>> = source_text.map(|s| {
        let lowered = s.to_lowercase();
        table.functions.values().map(|(n, _)| n.to_lowercase()).filter(|n| lowered.contains(n.as_str())).collect()
    });
    let mut listing = Listing::default();
    let mut pos = 0;
    if has_length_prefix && data.len() >= 4 {
        listing.declared_length = Some(u32::from_le_bytes([data[0], data[1], data[2], data[3]]));
        pos = 4;
    }
    let mut pending: Vec<u8> = Vec::new();
    let mut pending_at = pos;
    let flush = |listing: &mut Listing, pending: &mut Vec<u8>, at: usize| {
        if !pending.is_empty() {
            listing.items.push(Item::Raw { offset: at, data: std::mem::take(pending) });
        }
    };
    while pos < data.len() {
        if pos + 2 > data.len() {
            if pending.is_empty() {
                pending_at = pos;
            }
            pending.extend_from_slice(&data[pos..]);
            break;
        }
        let opcode = u16::from_le_bytes([data[pos], data[pos + 1]]);
        let mut entry = if (table.min..=table.max).contains(&opcode) { table.functions.get(&opcode) } else { None };
        if let (Some((name, _)), Some(allowed)) = (entry, &allowed)
            && !table.internal.contains(&opcode)
            && !allowed.contains(&name.to_lowercase())
        {
            entry = None;
        }
        let decoded = entry.and_then(|(name, params)| read_operands(data, pos + 2, params).map(|d| (name, d)));
        match decoded {
            None => {
                if pending.is_empty() {
                    pending_at = pos;
                }
                pending.push(data[pos]);
                pos += 1;
            }
            Some((name, (operands, new_pos))) => {
                flush(&mut listing, &mut pending, pending_at);
                listing.items.push(Item::Instruction { offset: pos, opcode, name: name.clone(), operands, size: new_pos - pos });
                pos = new_pos;
            }
        }
    }
    flush(&mut listing, &mut pending, pending_at);
    listing
}

/// Python's `f"{x:.0%}"`.
fn percent(x: f64) -> String {
    format!("{:.0}%", x * 100.0)
}

/// The listing as text: one line per instruction (MWSE/MWE-only functions marked), and an
/// offset/hex/ASCII dump of each raw span.
pub fn format_listing(listing: &Listing, width: usize) -> String {
    let table = opcodes();
    let mut lines = Vec::new();
    if let Some(n) = listing.declared_length {
        lines.push(format!("; declared bytecode length: {n} bytes"));
        lines.push(format!("; decoded: {} of the stream", percent(listing.decoded_ratio())));
    }
    let ascii = |b: &[u8]| b.iter().map(|&c| if (32..127).contains(&c) { c as char } else { '.' }).collect::<String>();
    for item in &listing.items {
        match item {
            Item::Instruction { offset, opcode, name, operands, .. } => {
                let args: Vec<String> = operands
                    .iter()
                    .map(|o| match o {
                        Operand::Int(i) => i.to_string(),
                        Operand::Float(f) => float_repr(*f as f64),
                        Operand::Text(t) => format!("\"{t}\""),
                    })
                    .collect();
                let args = args.join(", ");
                let mark = if table.extended.contains(opcode) { "   ; MWSE/MWE" } else { "" };
                let sep = if args.is_empty() { String::new() } else { format!(" {args}") };
                lines.push(format!("{offset:04X}  {name}{sep}{mark}"));
            }
            Item::Raw { offset, data } => {
                for (i, chunk) in data.chunks(width).enumerate() {
                    let hex: Vec<String> = chunk.iter().map(|b| format!("{b:02x}")).collect();
                    lines.push(format!("{:04X}  {:<w$}  |{}|", offset + i * width, hex.join(" "), ascii(chunk), w = width * 3));
                }
            }
        }
    }
    lines.join("\n")
}

/// A `bytecode` field as a readable disassembly; never an error (a failure is a `;`
/// comment in the listing).
pub fn listing_for_bytecode_field(value: &Value, source_text: Option<&str>) -> String {
    match decode_field(value) {
        Err(e) => format!("; could not decode this bytecode field:\n;   {e}"),
        Ok(raw) => format_listing(&disassemble(&raw, true, source_text), 16),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_compiled_script_reads_back() {
        let data = crate::mwscript::compiler::compile(b"begin t\nplayer->AddItem gold_001 5\nreturn\nend\n", &crate::mwscript::records::RecordIndex::default(), crate::mwscript::compiler::Levels::Weak, true, true).data;
        let mut field = (data.len() as u32).to_le_bytes().to_vec();
        field.extend(&data);
        let b64 = base64::engine::general_purpose::STANDARD.encode(&field);
        let text = listing_for_bytecode_field(&Value::from(b64), Some("player->AddItem gold_001 5\nreturn"));
        assert!(text.contains("0004  _SetReference \"player\""), "{text}");
        assert!(text.starts_with("; declared bytecode length:"), "{text}");
        assert_eq!(percent(0.125), "12%");
        assert_eq!(percent(0.135), format!("{:.0}%", 0.135f64 * 100.0));
    }
}
