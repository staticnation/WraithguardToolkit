//! A reference grid plus the deltas one plugin applied to it (`RelativeGrid`, `land/diff.py`;
//! the Rust of `native/src/land.rs`, without Python).
//!
//! Values are flat and interleaved: a 65x65 grid of three-component normals is one list of
//! 12,675 integers. One changed flag per *vertex*.

/// Vertices per edge of a `LAND` grid.
pub const LAND_SIZE: usize = 65;
/// Vertices in a `LAND` grid.
pub const LAND_NUM_VERTS: usize = LAND_SIZE * LAND_SIZE;
/// Land-texture slots per edge.
pub const TEXTURE_SIZE: usize = 16;
/// Land-texture slots in a cell.
pub const NUM_TEXTURES: usize = TEXTURE_SIZE * TEXTURE_SIZE;
/// Edge of the world-map heightmap.
pub const WNAM_SIZE: usize = 9;
/// The world map samples every eighth height vertex.
pub const WNAM_STEP: usize = (LAND_SIZE - 1) / (WNAM_SIZE - 1);
/// World units per world-map step.
pub const WNAM_HEIGHT_SCALE: f64 = 128.0;
/// Stored heights are divided by this.
pub const HEIGHT_SCALE: i64 = 8;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelativeGrid {
    pub reference: Vec<i64>,
    pub delta: Vec<i64>,
    pub changed: Vec<bool>,
    pub side: usize,
    pub components: usize,
}

impl RelativeGrid {
    /// A reference grid with no deltas.
    pub fn new(reference: Vec<i64>, side: usize, components: usize) -> Result<Self, String> {
        let expected = side * side * components;
        if reference.len() != expected {
            return Err(format!(
                "expected {expected} values for a {side}x{side} grid of {components}-component values, got {}",
                reference.len()
            ));
        }
        Ok(Self { delta: vec![0; expected], changed: vec![false; side * side], reference, side, components })
    }

    /// A plugin's grid against a reference (`from_difference`).
    pub fn from_difference(reference: Vec<i64>, plugin: &[i64], side: usize, components: usize) -> Result<Self, String> {
        let mut grid = Self::new(reference, side, components)?;
        if plugin.len() != grid.reference.len() {
            return Err(format!("plugin grid has {} values, reference has {}", plugin.len(), grid.reference.len()));
        }
        for (i, (was, now)) in grid.reference.iter().zip(plugin).enumerate() {
            if was != now {
                grid.delta[i] = now - was;
                grid.changed[i / components] = true;
            }
        }
        Ok(grid)
    }

    /// The plugin's grid: reference plus delta (`to_flat`).
    pub fn flat(&self) -> Vec<i64> {
        self.reference.iter().zip(&self.delta).map(|(a, b)| a + b).collect()
    }

    pub fn is_modified(&self) -> bool {
        self.changed.iter().any(|&c| c)
    }

    pub fn num_differences(&self) -> usize {
        self.changed.iter().filter(|&&c| c).count()
    }

    /// The changed vertices as `(x, y)`, row by row.
    pub fn changed_vertices(&self) -> Vec<(usize, usize)> {
        let s = self.side;
        self.changed.iter().enumerate().filter(|(_, c)| **c).map(|(i, _)| (i % s, i / s)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn differences() {
        let g = RelativeGrid::from_difference(vec![1, 2, 3, 4, 5, 6, 7, 8], &[1, 2, 3, 9, 5, 6, 7, 8], 2, 2).unwrap();
        assert_eq!(g.changed, vec![false, true, false, false]);
        assert_eq!(g.changed_vertices(), vec![(1, 0)]);
        assert_eq!(g.flat(), vec![1, 2, 3, 9, 5, 6, 7, 8]);
        assert!(RelativeGrid::new(vec![0; 3], 2, 1).is_err());
    }
}
