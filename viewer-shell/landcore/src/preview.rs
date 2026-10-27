//! What each conflict strategy would do to one cell (`land/preview.py`): the plugins'
//! heights merged with the first as the base, under Overwrite, Resolve, Ignore and Curvature,
//! so the choice a `.mergedlands.toml` makes can be seen before it is written.

use crate::grid::{RelativeGrid, LAND_NUM_VERTS, LAND_SIZE};
use crate::merge::{merge_layer, ConflictParams};
use crate::meta::{layer, Strategy};

/// The strategies a preview shows, in order (`PREVIEW_STRATEGIES`).
pub const PREVIEW_STRATEGIES: [Strategy; 4] = [Strategy::Overwrite, Strategy::Resolve, Strategy::Ignore, Strategy::Curvature];

/// The heights merged under one strategy (`merge_preview`): each grid in load order, the
/// first the base.
pub fn merge_preview(grids: &[Vec<i64>], strategy: Strategy) -> Result<Vec<i64>, String> {
    let Some(reference) = grids.first() else { return Err("no terrain to merge".into()) };
    if grids.iter().any(|g| g.len() != LAND_NUM_VERTS) {
        return Err(format!("each terrain grid must be {LAND_SIZE}x{LAND_SIZE}"));
    }
    let mut merged = RelativeGrid::new(reference.clone(), LAND_SIZE, 1)?;
    for g in &grids[1..] {
        let incoming = RelativeGrid::from_difference(reference.clone(), g, LAND_SIZE, 1)?;
        merged = merge_layer(layer::VERTEX_HEIGHTS, &merged, &incoming, strategy, ConflictParams::default())?.0;
    }
    Ok(merged.flat())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strategies_differ_only_where_contested() {
        let base = vec![0; LAND_NUM_VERTS];
        let mut a = base.clone();
        a[10] = 80;
        a[20] = 40;
        let mut b = base.clone();
        b[10] = 16;
        let grids = [base, a, b];
        assert_eq!(merge_preview(&grids, Strategy::Overwrite).unwrap()[10], 16);
        assert_eq!(merge_preview(&grids, Strategy::Ignore).unwrap()[10], 80);
        let r = merge_preview(&grids, Strategy::Resolve).unwrap();
        assert!(r[10] > 16 && r[10] < 80);
        assert!(PREVIEW_STRATEGIES.iter().all(|s| merge_preview(&grids, *s).unwrap()[20] == 40));
        assert!(merge_preview(&[], Strategy::Resolve).is_err());
    }
}
