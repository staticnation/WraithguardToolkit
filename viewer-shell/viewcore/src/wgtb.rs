//! Wraithguard: `WGTB`, the engine's lists as bytes - and, read back here, the page's
//! requests that carry bulk numbers (Static 2026-11-17: JSON only where it is needed).
//!
//! "WGTB" | u32 version 1 | u32 table count | per table: name (string), column types
//! (string, one letter a column), u32 row count, then the rows' values in order.
//! Little-endian. The letters:
//!
//! - `s` a string: u32 byte length + UTF-8
//! - `u` a u32, `i` an i32, `d` an f64
//! - `F` a list of f32: u32 count + the values (triangles, a height grid)
//!
//! The page's `unpackTables` and `packTables` (ui/src/50_wg_editor.js) read and write it;
//! `ui/tests/editor/wgtb_tables.json` is the fixture both sides are tested against. The
//! writer does not check a row against its column letters: each list is built in one
//! place and the fixture holds the format. The reader checks everything: a request that
//! does not read is an error, never a short list.

/// The writer: `table` names a table and its columns, then each row's values in order.
pub struct Tables {
    o: Vec<u8>,
    tables: u32,
}

impl Default for Tables {
    fn default() -> Self {
        Self::new()
    }
}

impl Tables {
    pub fn new() -> Self {
        let mut o = b"WGTB".to_vec();
        o.extend_from_slice(&1u32.to_le_bytes());
        o.extend_from_slice(&0u32.to_le_bytes()); // the table count, set by `done`
        Tables { o, tables: 0 }
    }
    pub fn table(&mut self, name: &str, cols: &str, rows: usize) -> &mut Self {
        self.tables += 1;
        self.s(name).s(cols);
        self.o.extend_from_slice(&(rows.min(u32::MAX as usize) as u32).to_le_bytes());
        self
    }
    pub fn s(&mut self, v: &str) -> &mut Self {
        self.o.extend_from_slice(&(v.len() as u32).to_le_bytes());
        self.o.extend_from_slice(v.as_bytes());
        self
    }
    pub fn u(&mut self, v: u64) -> &mut Self {
        self.o.extend_from_slice(&(v.min(u32::MAX as u64) as u32).to_le_bytes());
        self
    }
    pub fn i(&mut self, v: i64) -> &mut Self {
        self.o.extend_from_slice(&(v.clamp(i32::MIN as i64, i32::MAX as i64) as i32).to_le_bytes());
        self
    }
    pub fn d(&mut self, v: f64) -> &mut Self {
        self.o.extend_from_slice(&v.to_le_bytes());
        self
    }
    pub fn f(&mut self, v: &[f32]) -> &mut Self {
        self.o.extend_from_slice(&(v.len() as u32).to_le_bytes());
        for x in v {
            self.o.extend_from_slice(&x.to_le_bytes());
        }
        self
    }
    pub fn done(mut self) -> Vec<u8> {
        let n = self.tables.to_le_bytes();
        self.o[8..12].copy_from_slice(&n);
        self.o
    }
}

/// One value read back.
#[derive(Clone, Debug, PartialEq)]
pub enum Val {
    S(String),
    U(u32),
    I(i32),
    D(f64),
    F(Vec<f32>),
}

impl Val {
    pub fn str(&self) -> &str {
        match self {
            Val::S(s) => s,
            _ => "",
        }
    }
    /// A number of any kind, as f64 (0 for a string or a list).
    pub fn num(&self) -> f64 {
        match self {
            Val::U(v) => *v as f64,
            Val::I(v) => *v as f64,
            Val::D(v) => *v,
            _ => 0.0,
        }
    }
    pub fn floats(&self) -> &[f32] {
        match self {
            Val::F(v) => v,
            _ => &[],
        }
    }
}

/// One table read back: its name, its column letters and its rows.
#[derive(Clone, Debug, PartialEq)]
pub struct Table {
    pub name: String,
    pub cols: String,
    pub rows: Vec<Vec<Val>>,
}

/// Every table of `b`, in order. Anything malformed - the wrong magic or version, a column
/// letter not above, bytes missing or left over - is an error saying where.
pub fn read(b: &[u8]) -> Result<Vec<Table>, String> {
    let need = |at: usize, n: usize| -> Result<(), String> {
        if at + n > b.len() {
            Err(format!("the tables ended early, at byte {at} of {}", b.len()))
        } else {
            Ok(())
        }
    };
    let u32_at = |at: &mut usize| -> Result<u32, String> {
        need(*at, 4)?;
        let v = u32::from_le_bytes([b[*at], b[*at + 1], b[*at + 2], b[*at + 3]]);
        *at += 4;
        Ok(v)
    };
    let str_at = |at: &mut usize| -> Result<String, String> {
        let n = u32_at(at)? as usize;
        need(*at, n)?;
        let s = std::str::from_utf8(&b[*at..*at + n]).map_err(|_| format!("a string at byte {} is not UTF-8", *at))?.to_string();
        *at += n;
        Ok(s)
    };
    need(0, 12)?;
    if &b[0..4] != b"WGTB" {
        return Err("not WGTB bytes".into());
    }
    let mut at = 4usize;
    let version = u32_at(&mut at)?;
    if version != 1 {
        return Err(format!("WGTB version {version}; this reads 1"));
    }
    let count = u32_at(&mut at)?;
    let mut out = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let name = str_at(&mut at)?;
        let cols = str_at(&mut at)?;
        if let Some(c) = cols.chars().find(|c| !"suidF".contains(*c)) {
            return Err(format!("table {name} has a column of a kind this does not read: {c}"));
        }
        let n = u32_at(&mut at)?;
        let mut rows = Vec::with_capacity((n as usize).min(1 << 20));
        for _ in 0..n {
            let mut row = Vec::with_capacity(cols.len());
            for c in cols.chars() {
                row.push(match c {
                    's' => Val::S(str_at(&mut at)?),
                    'u' => Val::U(u32_at(&mut at)?),
                    'i' => Val::I(u32_at(&mut at)? as i32),
                    'd' => {
                        need(at, 8)?;
                        let mut x = [0u8; 8];
                        x.copy_from_slice(&b[at..at + 8]);
                        at += 8;
                        Val::D(f64::from_le_bytes(x))
                    }
                    _ => {
                        let k = u32_at(&mut at)? as usize;
                        need(at, k.saturating_mul(4))?;
                        let v = b[at..at + k * 4].chunks_exact(4).map(|q| f32::from_le_bytes([q[0], q[1], q[2], q[3]])).collect();
                        at += k * 4;
                        Val::F(v)
                    }
                });
            }
            rows.push(row);
        }
        out.push(Table { name, cols, rows });
    }
    if at != b.len() {
        return Err(format!("the tables have {} bytes after the last one", b.len() - at));
    }
    Ok(out)
}

/// The rows of the table called `name` (none when there is no such table).
pub fn rows<'a>(tables: &'a [Table], name: &str) -> &'a [Vec<Val>] {
    tables.iter().find(|t| t.name == name).map(|t| t.rows.as_slice()).unwrap_or(&[])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_and_reads_every_kind() {
        let mut tb = Tables::new();
        tb.table("a", "suidF", 2);
        tb.s("x").u(7).i(-3).d(1.5).f(&[1.0, -2.5]);
        tb.s("").u(0).i(0).d(0.0).f(&[]);
        tb.table("empty", "s", 0);
        let b = tb.done();
        let t = read(&b).unwrap();
        assert_eq!(t.len(), 2);
        assert_eq!(rows(&t, "a")[0], vec![Val::S("x".into()), Val::U(7), Val::I(-3), Val::D(1.5), Val::F(vec![1.0, -2.5])]);
        assert_eq!(rows(&t, "a")[1][4], Val::F(vec![]));
        assert!(rows(&t, "empty").is_empty() && rows(&t, "none").is_empty());
        // Malformed: cut short, a byte too many, the wrong magic, an unknown column.
        assert!(read(&b[..b.len() - 1]).unwrap_err().contains("ended early"));
        let mut more = b.clone();
        more.push(0);
        assert!(read(&more).unwrap_err().contains("after the last"));
        assert!(read(b"WGTX\x01\0\0\0\0\0\0\0").is_err());
        let mut odd = Tables::new();
        odd.table("q", "x", 0);
        assert!(read(&odd.done()).unwrap_err().contains("kind"));
    }
}
