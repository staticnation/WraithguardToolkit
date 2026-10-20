//! A cell, named the one way: an exterior by its grid square, an interior by its name
//! without regard to case. The page, the engine and the Python pool each spelled cells
//! their own way (`int:Name`, `x,y`, `(x, y)`, a bare name); [`CellKey::parse`] reads all
//! of them and [`CellKey::fold`] is the key two spellings of one cell share.
//!
//! An interior's name folds as `gardenfell_world::world::room_key` does (ASCII lowered, and
//! any other letter lowered by Unicode's rules), and as the Python pool's `str.lower()`
//! does, so the engine, the viewer and the Editor agree on which cell a name is. Cases:
//! `tests/fixtures/cell_cases.json`, run here and by `tests/test_cell_key.py`.

/// One cell. An interior keeps its name as written, for display; compare by [`fold`].
///
/// [`fold`]: CellKey::fold
#[derive(Clone, Debug)]
pub enum CellKey {
    Exterior(i32, i32),
    Interior(String),
}

impl CellKey {
    /// Reads any of the spellings in use: `int:Name` (any case of `int:`), `x,y`, `x, y`,
    /// `(x, y)`, or a bare name (an interior). `None` for an empty name.
    pub fn parse(spec: &str) -> Option<CellKey> {
        let s = spec.trim();
        if let Some(rest) = s.get(..4).filter(|p| p.eq_ignore_ascii_case("int:")).map(|_| &s[4..]) {
            let name = rest.trim();
            return (!name.is_empty()).then(|| CellKey::Interior(name.to_string()));
        }
        if let Some(g) = grid(s) {
            return Some(CellKey::Exterior(g.0, g.1));
        }
        (!s.is_empty()).then(|| CellKey::Interior(s.to_string()))
    }

    /// The key two spellings of one cell share: `x,y`, or `int:` and the folded name.
    pub fn fold(&self) -> String {
        match self {
            CellKey::Exterior(x, y) => format!("{x},{y}"),
            CellKey::Interior(name) => format!("int:{}", fold_name(name)),
        }
    }

    /// The page's and the engine's spelling: `int:Name` or `x,y`.
    pub fn spec(&self) -> String {
        match self {
            CellKey::Exterior(x, y) => format!("{x},{y}"),
            CellKey::Interior(name) => format!("int:{name}"),
        }
    }

    /// The Python pool's spelling, which is also how plugins' records name the cell in the
    /// Editor: the name, or `(x, y)`.
    pub fn pool_key(&self) -> String {
        match self {
            CellKey::Exterior(x, y) => format!("({x}, {y})"),
            CellKey::Interior(name) => name.clone(),
        }
    }

    /// The grid square, for an exterior.
    pub fn grid(&self) -> Option<(i32, i32)> {
        match self {
            CellKey::Exterior(x, y) => Some((*x, *y)),
            CellKey::Interior(_) => None,
        }
    }
}

impl PartialEq for CellKey {
    fn eq(&self, other: &CellKey) -> bool {
        self.fold() == other.fold()
    }
}

impl Eq for CellKey {}

impl std::hash::Hash for CellKey {
    fn hash<H: std::hash::Hasher>(&self, h: &mut H) {
        self.fold().hash(h);
    }
}

/// An interior's name without regard to case (`room_key`'s rule: ASCII lowered fast, any
/// other name by Unicode's lowercase).
pub fn fold_name(name: &str) -> String {
    if name.is_ascii() { name.to_ascii_lowercase() } else { name.to_lowercase() }
}

/// `x,y`, `x, y` or `(x, y)` (spaces anywhere between) to the grid square.
fn grid(s: &str) -> Option<(i32, i32)> {
    let inner = match (s.strip_prefix('('), s.ends_with(')')) {
        (Some(rest), true) => &rest[..rest.len() - 1],
        (None, false) => s,
        _ => return None,
    };
    let (a, b) = inner.split_once(',')?;
    Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spellings_of_one_cell_are_one_key() {
        let a = CellKey::parse("int:Balmora, Guild of Mages").unwrap();
        let b = CellKey::parse("balmora, guild of mages").unwrap();
        assert_eq!(a, b);
        assert_eq!(CellKey::parse("(-2, -9)"), CellKey::parse(" -2,-9 "));
        assert_ne!(CellKey::parse("int:1,2"), CellKey::parse("1,2"));
    }
}
