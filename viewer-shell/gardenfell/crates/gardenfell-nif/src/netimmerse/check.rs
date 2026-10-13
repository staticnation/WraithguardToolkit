//! The round-trip check for a NIF: does it read to the end, and do the bytes come back.
//!
//! Round 18gg. A NIF has no block sizes, so a layout that is one byte wrong desyncs
//! everything after it; "reads to the end with the footer where it should be" is
//! therefore most of the proof. The rest is "reads its own output back to the same
//! value", which every file must. "Writes back byte for byte" is measured, not required:
//! the game's own meshes carry has-words whose upper bytes are uninitialized memory
//! (`ex_stronghold_wall00.nif` has a texture slot's has-map word of `0x00C40F78`, which
//! the game reads as true), and the model writes those back as 1. The `roundtrip
//! --meshes` command counts all three over an install.

use std::collections::BTreeMap;
use std::fmt;

use super::stream::Stream;

/// What one file did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Report {
    pub bytes: usize,
    /// The blocks by type name, when the file read.
    pub blocks: BTreeMap<String, usize>,
    /// Read to the end, or the reason it did not (which names the block and the byte).
    pub read: Result<(), String>,
    /// Written back byte for byte, or the first differing byte and the sizes.
    pub exact: Result<(), String>,
    /// The output read back to the same value, or why not.
    pub idempotent: Result<(), String>,
}

impl Report {
    /// Read to the end and read its own output back to itself.
    pub fn clean(&self) -> bool {
        self.read.is_ok() && self.idempotent.is_ok()
    }
    pub fn block_count(&self) -> usize {
        self.blocks.values().sum()
    }
}

/// Reads and writes one file.
pub fn check(buf: &[u8]) -> Report {
    let mut rep =
        Report { bytes: buf.len(), blocks: BTreeMap::new(), read: Ok(()), exact: Ok(()), idempotent: Ok(()) };
    let stream = match Stream::parse(buf) {
        Ok(s) => s,
        Err(e) => {
            rep.read = Err(e.to_string());
            rep.exact = Err("not read".to_string());
            rep.idempotent = Err("not read".to_string());
            return rep;
        }
    };
    for b in &stream.blocks {
        *rep.blocks.entry(b.name().to_string()).or_default() += 1;
    }
    let out = match stream.write() {
        Ok(out) => out,
        Err(e) => {
            rep.exact = Err(format!("does not write: {e}"));
            rep.idempotent = Err("does not write".to_string());
            return rep;
        }
    };
    if out != buf {
        let at = out.iter().zip(buf).position(|(a, b)| a != b).unwrap_or(out.len().min(buf.len()));
        // Which block the byte falls in, for the report.
        let block = block_at(&stream, buf, at);
        rep.exact = Err(format!("differs at byte {at}{block} ({} bytes in, {} out)", buf.len(), out.len()));
        // Not the same bytes: does the output at least read back to the same value, or
        // failing that (a NaN compares unequal to itself) to the same bytes again?
        match Stream::parse(&out) {
            Ok(again) if again == stream => {}
            Ok(again) => match again.write() {
                Ok(twice) if twice == out => {}
                Ok(_) => rep.idempotent = Err("the output reads back as a different value".to_string()),
                Err(e) => rep.idempotent = Err(format!("the output's output does not write: {e}")),
            },
            Err(e) => rep.idempotent = Err(format!("the output does not read back: {e}")),
        }
    }
    rep
}

/// The block whose bytes hold offset `at`, found by writing the blocks one at a time.
fn block_at(stream: &Stream, buf: &[u8], at: usize) -> String {
    let mut pos = stream.header.len() + 4 + 4;
    for (i, b) in stream.blocks.iter().enumerate() {
        let mut w = crate::bytes::Writer::new();
        let _ = crate::netimmerse::stream::NifWrite::string(&mut w, b.name());
        let _ = b.write(&mut w);
        let end = pos + w.len();
        if at < end || i + 1 == stream.blocks.len() && at < buf.len() {
            return format!(" in block {i} ({})", b.name());
        }
        pos = end;
    }
    String::new()
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} bytes, {} blocks: ", self.bytes, self.block_count())?;
        match (&self.read, &self.exact, &self.idempotent) {
            (Ok(()), Ok(()), _) => write!(f, "byte for byte"),
            (Ok(()), Err(e), Ok(())) => write!(f, "read and read back the same, but {e}"),
            (Ok(()), Err(e), Err(i)) => write!(f, "read, but {e}, and {i}"),
            (Err(e), _, _) => write!(f, "NOT read: {e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::netimmerse::types::*;

    #[test]
    fn a_small_scene_is_clean_and_a_truncated_one_names_the_block() {
        let mut s = Stream::new();
        let mut root = NiNode::default();
        root.base.base.name = "root".into();
        root.children = vec![Link::new(1)];
        let mut shape = NiTriShape::default();
        shape.base.base.base.base.name = "leaf".into();
        shape.base.base.geometry_data = Link::new(2);
        let mut data = NiTriShapeData::default();
        data.base.base.vertices = vec![[0.0; 3]; 3];
        data.triangles = vec![[0, 1, 2]];
        s.push(root);
        s.push(shape);
        s.push(data);
        s.roots = vec![Link::new(0)];
        let bytes = s.write().unwrap();
        let rep = check(&bytes);
        assert!(rep.clean(), "{rep}");
        assert_eq!(rep.blocks["NiTriShapeData"], 1);
        let cut = check(&bytes[..bytes.len() - 10]);
        assert!(!cut.clean());
        assert!(cut.read.as_ref().unwrap_err().contains("block 2 (NiTriShapeData"), "{cut}");
    }
}
