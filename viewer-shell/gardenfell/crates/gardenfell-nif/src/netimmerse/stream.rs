//! A NIF as its blocks: the header line, the version, the blocks in file order, the roots.
//!
//! Links are the block indices the file holds (`-1` for none), typed by what they point
//! at so `node.children[0]` says it is an `NiAVObject` without saying which struct. A
//! [`Stream`] writes its blocks back in the order it holds them, so a file read and
//! written comes back byte for byte; `tes3` walks from the roots instead and reorders.

use std::marker::PhantomData;

use crate::bytes::{Error, Reader, Result, Writer};

use super::types::{NiObject, NiType};

/// A link to a block of the same stream: its index, or `-1` for none.
pub struct Link<T> {
    pub index: i32,
    marker: PhantomData<fn() -> T>,
}

impl<T> Link<T> {
    pub const NONE: Link<T> = Link { index: -1, marker: PhantomData };

    pub fn new(index: i32) -> Link<T> {
        Link { index, marker: PhantomData }
    }
    pub fn is_none(&self) -> bool {
        self.index < 0
    }
    pub fn is_some(&self) -> bool {
        self.index >= 0
    }
    /// The index as a position in `Stream::blocks`, when there is one.
    pub fn at(&self) -> Option<usize> {
        if self.index < 0 {
            None
        } else {
            Some(self.index as usize)
        }
    }
    /// The same link seen as another type, for a field the format types loosely.
    pub fn cast<U>(&self) -> Link<U> {
        Link::new(self.index)
    }
}

impl<T> Clone for Link<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for Link<T> {}
impl<T> Default for Link<T> {
    fn default() -> Self {
        Link::NONE
    }
}
impl<T> PartialEq for Link<T> {
    fn eq(&self, other: &Self) -> bool {
        self.index == other.index
    }
}
impl<T> Eq for Link<T> {}
impl<T> std::fmt::Debug for Link<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.index < 0 {
            write!(f, "Link(none)")
        } else {
            write!(f, "Link({})", self.index)
        }
    }
}

/// The reads every block layout is made of, on top of the plain numbers in
/// [`Reader`]. Vectors are the file's: three floats, a 3x3 row by row, a quaternion as
/// `[w, x, y, z]`, a color as RGBA floats.
pub trait NifRead<'a> {
    fn link<T>(&mut self) -> Result<Link<T>>;
    /// A `u32` count, then that many links.
    fn links<T>(&mut self) -> Result<Vec<Link<T>>>;
    /// A `u32` count, then that many bytes of text (no terminator).
    fn string(&mut self) -> Result<String>;
    fn vec2(&mut self) -> Result<[f32; 2]>;
    fn vec3(&mut self) -> Result<[f32; 3]>;
    fn vec4(&mut self) -> Result<[f32; 4]>;
    /// Nine floats, the file's row-major 3x3.
    fn mat3(&mut self) -> Result<[[f32; 3]; 3]>;
    /// Four floats, `[w, x, y, z]` as the file stores a quaternion.
    fn quat(&mut self) -> Result<[f32; 4]>;
    /// One byte, non-zero for true.
    fn bool_u8(&mut self) -> Result<bool>;
    /// Four bytes, non-zero for true.
    fn bool_u32(&mut self) -> Result<bool>;
    /// A `u32` count that is about to size a sequence; refused when it cannot fit in
    /// the bytes that remain, so a bad count is an error and not an allocation.
    fn count(&mut self, item_bytes: usize) -> Result<usize>;
    /// `n` items read by `f`.
    fn seq<T>(&mut self, n: usize, f: impl FnMut(&mut Self) -> Result<T>) -> Result<Vec<T>>;
    /// A `u32` count, then that many items read by `f`.
    fn counted<T>(&mut self, item_bytes: usize, f: impl FnMut(&mut Self) -> Result<T>) -> Result<Vec<T>>;
    fn f32s_n(&mut self, n: usize) -> Result<Vec<f32>>;
    fn u16s_n(&mut self, n: usize) -> Result<Vec<u16>>;
    fn vec3s_n(&mut self, n: usize) -> Result<Vec<[f32; 3]>>;
    fn vec2s_n(&mut self, n: usize) -> Result<Vec<[f32; 2]>>;
    fn vec4s_n(&mut self, n: usize) -> Result<Vec<[f32; 4]>>;
}

impl<'a> NifRead<'a> for Reader<'a> {
    fn link<T>(&mut self) -> Result<Link<T>> {
        Ok(Link::new(self.i32()?))
    }
    fn links<T>(&mut self) -> Result<Vec<Link<T>>> {
        let n = self.count(4)?;
        self.seq(n, |r| r.link())
    }
    fn string(&mut self) -> Result<String> {
        self.sized_string()
    }
    fn vec2(&mut self) -> Result<[f32; 2]> {
        self.f32s::<2>()
    }
    fn vec3(&mut self) -> Result<[f32; 3]> {
        self.f32s::<3>()
    }
    fn vec4(&mut self) -> Result<[f32; 4]> {
        self.f32s::<4>()
    }
    fn mat3(&mut self) -> Result<[[f32; 3]; 3]> {
        Ok([self.f32s::<3>()?, self.f32s::<3>()?, self.f32s::<3>()?])
    }
    fn quat(&mut self) -> Result<[f32; 4]> {
        self.f32s::<4>()
    }
    fn bool_u8(&mut self) -> Result<bool> {
        Ok(self.u8()? != 0)
    }
    fn bool_u32(&mut self) -> Result<bool> {
        Ok(self.u32()? != 0)
    }
    fn count(&mut self, item_bytes: usize) -> Result<usize> {
        let n = self.u32()? as usize;
        if item_bytes > 0 && n.saturating_mul(item_bytes) > self.remaining() {
            return self.error(format!(
                "a count of {n} items of {item_bytes} bytes with {} bytes left",
                self.remaining()
            ));
        }
        Ok(n)
    }
    fn seq<T>(&mut self, n: usize, mut f: impl FnMut(&mut Self) -> Result<T>) -> Result<Vec<T>> {
        let mut out = Vec::with_capacity(n.min(1 << 20));
        for _ in 0..n {
            out.push(f(self)?);
        }
        Ok(out)
    }
    fn counted<T>(&mut self, item_bytes: usize, f: impl FnMut(&mut Self) -> Result<T>) -> Result<Vec<T>> {
        let n = self.count(item_bytes)?;
        self.seq(n, f)
    }
    fn f32s_n(&mut self, n: usize) -> Result<Vec<f32>> {
        self.seq(n, |r| r.f32())
    }
    fn u16s_n(&mut self, n: usize) -> Result<Vec<u16>> {
        self.seq(n, |r| r.u16())
    }
    fn vec3s_n(&mut self, n: usize) -> Result<Vec<[f32; 3]>> {
        self.seq(n, |r| r.vec3())
    }
    fn vec2s_n(&mut self, n: usize) -> Result<Vec<[f32; 2]>> {
        self.seq(n, |r| r.vec2())
    }
    fn vec4s_n(&mut self, n: usize) -> Result<Vec<[f32; 4]>> {
        self.seq(n, |r| r.vec4())
    }
}

/// The writes that mirror [`NifRead`].
pub trait NifWrite {
    fn link<T>(&mut self, l: Link<T>);
    fn links<T>(&mut self, l: &[Link<T>]);
    fn string(&mut self, s: &str) -> Result<()>;
    fn vec2(&mut self, v: &[f32; 2]);
    fn vec3(&mut self, v: &[f32; 3]);
    fn vec4(&mut self, v: &[f32; 4]);
    fn mat3(&mut self, m: &[[f32; 3]; 3]);
    fn quat(&mut self, q: &[f32; 4]);
    fn bool_u8(&mut self, b: bool);
    fn bool_u32(&mut self, b: bool);
    /// A sequence length as a `u32`.
    fn count(&mut self, n: usize);
    fn vec3s(&mut self, v: &[[f32; 3]]);
    fn vec2s(&mut self, v: &[[f32; 2]]);
    fn vec4s(&mut self, v: &[[f32; 4]]);
    fn u16s(&mut self, v: &[u16]);
}

impl NifWrite for Writer {
    fn link<T>(&mut self, l: Link<T>) {
        self.i32(l.index);
    }
    fn links<T>(&mut self, l: &[Link<T>]) {
        self.count(l.len());
        for x in l {
            self.i32(x.index);
        }
    }
    fn string(&mut self, s: &str) -> Result<()> {
        self.sized_string(s)
    }
    fn vec2(&mut self, v: &[f32; 2]) {
        self.f32s(v);
    }
    fn vec3(&mut self, v: &[f32; 3]) {
        self.f32s(v);
    }
    fn vec4(&mut self, v: &[f32; 4]) {
        self.f32s(v);
    }
    fn mat3(&mut self, m: &[[f32; 3]; 3]) {
        for row in m {
            self.f32s(row);
        }
    }
    fn quat(&mut self, q: &[f32; 4]) {
        self.f32s(q);
    }
    fn bool_u8(&mut self, b: bool) {
        self.u8(b as u8);
    }
    fn bool_u32(&mut self, b: bool) {
        self.u32(b as u32);
    }
    fn count(&mut self, n: usize) {
        self.u32(n as u32);
    }
    fn vec3s(&mut self, v: &[[f32; 3]]) {
        for x in v {
            self.f32s(x);
        }
    }
    fn vec2s(&mut self, v: &[[f32; 2]]) {
        for x in v {
            self.f32s(x);
        }
    }
    fn vec4s(&mut self, v: &[[f32; 4]]) {
        for x in v {
            self.f32s(x);
        }
    }
    fn u16s(&mut self, v: &[u16]) {
        for x in v {
            self.u16(*x);
        }
    }
}

/// A file whose first root is not block 0, rewritten with the root at block 0 and every
/// link renumbered; `None` when the file does not read or already has its root first.
/// Round 18gg's engine read such a file through this; since 18gh the engine walks from
/// the stream's own first root, and this stays as a tool.
pub fn root_first(buf: &[u8]) -> Option<Vec<u8>> {
    let mut s = Stream::parse(buf).ok()?;
    if !s.root_first() {
        return None;
    }
    s.write().ok()
}

/// The version every Morrowind mesh carries, `4.0.0.2`.
pub const VERSION: u32 = 0x0400_0002;
/// The header line the game's files carry, newline included.
pub const HEADER: &str = "NetImmerse File Format, Version 4.0.0.2\n";

/// A file: the header line as read (newline included), the version word, the blocks in
/// file order, and the roots.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stream {
    pub header: String,
    pub version: u32,
    pub blocks: Vec<NiType>,
    pub roots: Vec<Link<NiObject>>,
}

impl Stream {
    pub fn new() -> Stream {
        Stream { header: HEADER.to_string(), version: VERSION, blocks: Vec::new(), roots: Vec::new() }
    }

    /// Reads a whole file. Strict: every block must be a type the model has, and the
    /// footer must end the file.
    pub fn parse(buf: &[u8]) -> Result<Stream> {
        let mut r = Reader::new(buf);
        // The header line: text up to the first newline, within the first 64 bytes. The
        // game's own files all carry `HEADER`; a tool's variation on the text is kept and
        // written back as read.
        let line_end = buf.iter().take(64).position(|&b| b == b'\n');
        let Some(line_end) = line_end else {
            return r.error("no header line: not a NIF");
        };
        let header = crate::bytes::decode(r.bytes(line_end + 1)?);
        if !header.starts_with("NetImmerse File Format") {
            return Err(Error { at: 0, what: format!("header line {header:?} is not a NIF's") });
        }
        let version = r.u32()?;
        if version != VERSION {
            return Err(Error {
                at: line_end + 1,
                what: format!("version {version:#010x} is not 4.0.0.2 ({VERSION:#010x})"),
            });
        }
        let n = r.count(0)?;
        let mut blocks = Vec::with_capacity(n.min(1 << 16));
        for i in 0..n {
            let at = r.pos();
            let name =
                r.string().map_err(|e| Error { at, what: format!("block {i}: type name: {}", e.what) })?;
            let block = NiType::read(&name, &mut r).map_err(|e| Error {
                at: e.at,
                what: format!("block {i} ({name}, starts at byte {at}): {}", e.what),
            })?;
            blocks.push(block);
        }
        let roots =
            r.links::<NiObject>().map_err(|e| Error { at: e.at, what: format!("roots: {}", e.what) })?;
        if !r.at_end() {
            return r.error(format!("{} bytes after the roots", r.remaining()));
        }
        Ok(Stream { header, version, blocks, roots })
    }

    /// The file back: header, version, every block with its type name, the roots.
    pub fn write(&self) -> Result<Vec<u8>> {
        let mut w = Writer::new();
        let header = crate::bytes::encode(&self.header)
            .map_err(|c| Error { at: 0, what: format!("{c:?} has no Windows-1252 byte") })?;
        w.bytes(&header);
        w.u32(self.version);
        w.count(self.blocks.len());
        for b in &self.blocks {
            w.string(b.name())?;
            b.write(&mut w)?;
        }
        w.links(&self.roots);
        Ok(w.into_bytes())
    }

    /// The block a link points at, as the struct asked for, when it is one.
    pub fn get<'a, T>(&'a self, link: Link<T>) -> Option<&'a T>
    where
        &'a T: TryFrom<&'a NiType>,
    {
        self.blocks.get(link.at()?).and_then(|b| b.try_into().ok())
    }

    /// Adds a block and returns its link.
    pub fn push<T: Into<NiType>>(&mut self, block: T) -> Link<T> {
        self.blocks.push(block.into());
        Link::new(self.blocks.len() as i32 - 1)
    }

    /// Puts the blocks in a new order and renumbers every link to match. `order[i]` is the
    /// old index of the block that becomes block `i`; it must be a permutation of
    /// `0..blocks.len()`, or nothing is changed and `false` comes back.
    pub fn reorder(&mut self, order: &[usize]) -> bool {
        let n = self.blocks.len();
        if order.len() != n {
            return false;
        }
        // new_of[old] = new
        let mut new_of = vec![usize::MAX; n];
        for (new, &old) in order.iter().enumerate() {
            if old >= n || new_of[old] != usize::MAX {
                return false;
            }
            new_of[old] = new;
        }
        let mut renumber = |l: &mut i32| {
            if *l >= 0 && (*l as usize) < n {
                *l = new_of[*l as usize] as i32;
            }
        };
        for b in &mut self.blocks {
            b.links(&mut renumber);
        }
        for r in &mut self.roots {
            renumber(&mut r.index);
        }
        let mut old: Vec<Option<NiType>> = self.blocks.drain(..).map(Some).collect();
        self.blocks = order.iter().map(|&i| old[i].take().unwrap_or_default()).collect();
        true
    }

    /// Moves the first root to block 0 when it is not there, by swapping the two blocks
    /// and renumbering. Blender's exporter writes the root last; the game does not care,
    /// and neither does this model nor, since 18gh, the engine's reader, which starts at
    /// the first root wherever it is. Returns whether anything moved.
    pub fn root_first(&mut self) -> bool {
        let Some(root) = self.roots.first().and_then(|l| l.at()) else { return false };
        if root == 0 || root >= self.blocks.len() {
            return false;
        }
        let mut order: Vec<usize> = (0..self.blocks.len()).collect();
        order.swap(0, root);
        self.reorder(&order)
    }

    /// Every block of one struct type, with its index.
    pub fn all<'a, T: 'a>(&'a self) -> impl Iterator<Item = (Link<T>, &'a T)> + 'a
    where
        &'a T: TryFrom<&'a NiType>,
    {
        self.blocks.iter().enumerate().filter_map(|(i, b)| {
            let t: &'a T = b.try_into().ok()?;
            Some((Link::new(i as i32), t))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_link_is_its_index_and_none_is_minus_one() {
        let l: Link<NiObject> = Link::new(3);
        assert_eq!(l.at(), Some(3));
        assert!(Link::<NiObject>::NONE.is_none());
        assert_eq!(Link::<NiObject>::default().index, -1);
        let mut w = Writer::new();
        w.links(&[l, Link::NONE]);
        let mut r = Reader::new(&w.out);
        let back: Vec<Link<NiObject>> = r.links().unwrap();
        assert_eq!(back, vec![l, Link::NONE]);
    }

    #[test]
    fn a_count_that_cannot_fit_is_refused() {
        let bytes = [0xFF, 0xFF, 0xFF, 0x7F, 1, 2, 3];
        let mut r = Reader::new(&bytes);
        let e = r.count(4).unwrap_err();
        assert!(e.what.contains("items of 4 bytes"), "{e}");
    }

    #[test]
    fn reordering_renumbers_every_link_and_root_first_puts_the_root_at_nought() {
        use crate::netimmerse::types::*;
        // Blender's order: the shape data first, then the shape, then the root last.
        let mut s = Stream::new();
        let mut data = NiTriShapeData::default();
        data.base.base.vertices = vec![[0.0; 3]; 3];
        data.triangles = vec![[0, 1, 2]];
        let mut shape = NiTriShape::default();
        shape.base.base.geometry_data = Link::new(0);
        shape.base.base.base.base.name = "leaf".into();
        let mut root = NiNode::default();
        root.children = vec![Link::new(1), Link::NONE];
        root.base.base.name = "root".into();
        s.push(data);
        s.push(shape);
        s.push(root);
        s.roots = vec![Link::new(2)];
        let before = s.clone();
        assert!(s.root_first());
        assert_eq!(s.roots, vec![Link::new(0)]);
        let node: &NiNode = s.get(Link::new(0)).unwrap();
        assert_eq!(node.base.base.name, "root");
        assert_eq!(node.children, vec![Link::new(1), Link::NONE]);
        let leaf: &NiTriShape = s.get(Link::new(1)).unwrap();
        assert_eq!(leaf.base.base.geometry_data, Link::new(2));
        assert!(s.get::<NiTriShapeData>(Link::new(2)).is_some());
        // Nothing to do the second time, and the bytes read back to the moved form.
        assert!(!s.root_first());
        let bytes = before.write().unwrap();
        let moved = root_first(&bytes).unwrap();
        assert_eq!(Stream::parse(&moved).unwrap(), s);
        assert!(root_first(&moved).is_none());
        // A bad permutation changes nothing.
        let mut t = before.clone();
        assert!(!t.reorder(&[0, 0, 1]));
        assert!(!t.reorder(&[0, 1]));
        assert_eq!(t, before);
    }

    #[test]
    fn the_header_version_and_footer_are_checked() {
        let e = Stream::parse(b"hello\n").unwrap_err();
        assert!(e.what.contains("not a NIF"), "{e}");
        let mut bad = HEADER.as_bytes().to_vec();
        bad.extend_from_slice(&0x0a01_0000u32.to_le_bytes());
        let e = Stream::parse(&bad).unwrap_err();
        assert!(e.what.contains("not 4.0.0.2"), "{e}");
        let s = Stream::new();
        let mut bytes = s.write().unwrap();
        assert_eq!(Stream::parse(&bytes).unwrap(), s);
        bytes.push(0);
        let e = Stream::parse(&bytes).unwrap_err();
        assert!(e.what.contains("1 bytes after the roots"), "{e}");
    }
}
