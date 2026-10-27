//! Merging one layer of one cell from two plugins' edits (`land/merge.py`).
//!
//! Where only one plugin moved a vertex, its edit is taken whole. Where both did, the
//! strategy decides: `Overwrite` the later, `Ignore` the earlier, `Resolve` a blend weighted
//! toward the larger edit, `Curvature` a blend weighted by the structure each edit adds
//! (heights only). `Auto` is the later plugin (load-order winner) on every layer.

use crate::curvature::curvature_at;
use crate::grid::RelativeGrid;
use crate::meta::{layer, Strategy};

/// The thresholds below which a blended vertex counts as a minor compromise (`ConflictParams`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ConflictParams {
    pub minor_threshold_pct: f64,
    pub minor_threshold_min: f64,
    pub minor_threshold_max: f64,
}

impl Default for ConflictParams {
    fn default() -> Self {
        ConflictParams { minor_threshold_pct: 0.3, minor_threshold_min: 10.0, minor_threshold_max: 64.0 }
    }
}

/// What merging one layer did (`MergeReport`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MergeReport {
    pub strategy: Strategy,
    pub taken_from_one: usize,
    pub taken_from_two: usize,
    pub contested: usize,
    pub minor: usize,
    pub major: usize,
    pub major_vertices: Vec<(usize, usize)>,
}

fn is_major(first: i64, second: i64, blended: f64, p: ConflictParams) -> bool {
    let smaller = first.min(second) as f64;
    let threshold = (p.minor_threshold_pct * smaller).max(p.minor_threshold_min).min(p.minor_threshold_max);
    (smaller - blended).abs() >= threshold
}

/// `average_delta`: the two edits blended toward the larger, and whether the compromise
/// is major.
pub fn average_delta(first: i64, second: i64, p: ConflictParams) -> (i64, bool) {
    let (s1, s2) = (first.abs() as f64, second.abs() as f64);
    let total = s1 + s2;
    if total == 0.0 {
        return (0, false);
    }
    let weight = s1 / total;
    let biased = weight.powf(1.5);
    let other = (1.0 - weight).powf(1.5);
    let weight = biased / (biased + other);
    let blended = weight * first as f64 + (1.0 - weight) * second as f64;
    (blended.trunc() as i64, is_major(first, second, blended, p))
}

/// `weighted_delta`: the two edits blended with the caller's weights.
pub fn weighted_delta(first: i64, second: i64, w1: f64, w2: f64, p: ConflictParams) -> (i64, bool) {
    let total = w1 + w2;
    if total <= 0.0 {
        return average_delta(first, second, p);
    }
    let share = w1 / total;
    let blended = share * first as f64 + (1.0 - share) * second as f64;
    (blended.trunc() as i64, is_major(first, second, blended, p))
}

/// The layer's name as `LandData` spells it, for the refusals.
fn layer_name(bits: u32) -> &'static str {
    match bits {
        layer::VERTEX_COLORS => "VERTEX_COLORS",
        layer::TEXTURES => "TEXTURES",
        layer::VERTEX_HEIGHTS => "VERTEX_HEIGHTS",
        layer::VERTEX_NORMALS => "VERTEX_NORMALS",
        layer::WORLD_MAP => "WORLD_MAP",
        _ => "NONE",
    }
}

/// The strategy a layer actually gets (`_resolve_for`): `Auto` is `Overwrite` everywhere;
/// averaging texture indices, or `Curvature` off the heights, is refused.
pub fn resolve_for(layer_bits: u32, strategy: Strategy) -> Result<Strategy, String> {
    if strategy == Strategy::Auto {
        return Ok(Strategy::Overwrite);
    }
    let averaging = matches!(strategy, Strategy::Resolve | Strategy::Curvature);
    if averaging && layer_bits == layer::TEXTURES {
        return Err(format!(
            "{} holds identifiers, not quantities: averaging index 3 and index 7 gives index 5, which is a third unrelated texture. Use OVERWRITE or IGNORE.",
            layer_name(layer_bits)
        ));
    }
    if strategy == Strategy::Curvature && layer_bits != layer::VERTEX_HEIGHTS {
        return Err(format!(
            "CURVATURE weights an edit by the structure it adds to a height surface, and {} has no surface to bend. Use RESOLVE.",
            layer_name(layer_bits)
        ));
    }
    Ok(strategy)
}

/// `merge_layer`: two plugins' edits to one layer, combined.
pub fn merge_layer(
    layer_bits: u32,
    first: &RelativeGrid,
    second: &RelativeGrid,
    strategy: Strategy,
    p: ConflictParams,
) -> Result<(RelativeGrid, MergeReport), String> {
    if first.side != second.side || first.components != second.components {
        return Err(format!(
            "cannot merge a {0}x{0}x{1} grid with a {2}x{2}x{3} one",
            first.side, first.components, second.side, second.components
        ));
    }
    let chosen = resolve_for(layer_bits, strategy)?;
    merge_grids(first, second, chosen, p)
}

/// The per-vertex loop of `merge_layer`, the strategy already resolved (`native`'s
/// `merge_grids`): `Overwrite` and `Ignore` take one side, anything else blends.
pub fn merge_grids(first: &RelativeGrid, second: &RelativeGrid, chosen: Strategy, p: ConflictParams) -> Result<(RelativeGrid, MergeReport), String> {
    if first.side != second.side || first.components != second.components {
        return Err(format!(
            "cannot merge a {0}x{0}x{1} grid with a {2}x{2}x{3} one",
            first.side, first.components, second.side, second.components
        ));
    }
    let (side, comps) = (first.side, first.components);
    let mut merged = RelativeGrid::new(first.reference.clone(), side, comps)?;
    let mut report = MergeReport { strategy: chosen, ..Default::default() };

    // Curvature weighting reads each side's terrain: the reference, and each applied.
    let surfaces = (chosen == Strategy::Curvature).then(|| {
        let f = |v: Vec<i64>| v.into_iter().map(|x| x as f64).collect::<Vec<f64>>();
        (f(first.reference.clone()), f(first.flat()), f(second.flat()))
    });

    let put = |merged: &mut RelativeGrid, v: usize, deltas: &[i64]| {
        let start = v * comps;
        let mut changed = false;
        for (o, d) in deltas.iter().enumerate() {
            merged.delta[start + o] = *d;
            changed = changed || *d != 0;
        }
        merged.changed[v] = changed;
    };

    for y in 0..side {
        for x in 0..side {
            let v = y * side + x;
            let d1 = &first.delta[v * comps..(v + 1) * comps];
            let d2 = &second.delta[v * comps..(v + 1) * comps];
            match (first.changed[v], second.changed[v]) {
                (false, false) => continue,
                (true, false) => {
                    report.taken_from_one += 1;
                    put(&mut merged, v, d1);
                    continue;
                }
                (false, true) => {
                    report.taken_from_two += 1;
                    put(&mut merged, v, d2);
                    continue;
                }
                (true, true) => {}
            }
            report.contested += 1;
            match chosen {
                Strategy::Overwrite => {
                    put(&mut merged, v, d2);
                    continue;
                }
                Strategy::Ignore => {
                    put(&mut merged, v, d1);
                    continue;
                }
                _ => {}
            }
            let mut blended = Vec::with_capacity(comps);
            let mut worst = false;
            if let Some((reference, t1, t2)) = &surfaces {
                let base = curvature_at(reference, side, x, y);
                let added1 = (curvature_at(t1, side, x, y) - base).max(0.0);
                let added2 = (curvature_at(t2, side, x, y) - base).max(0.0);
                let w1 = d1[0].abs() as f64 * (1.0 + added1 * 8.0);
                let w2 = d2[0].abs() as f64 * (1.0 + added2 * 8.0);
                let (value, sev) = weighted_delta(d1[0], d2[0], w1, w2, p);
                blended.push(value);
                worst = sev;
            } else {
                for (a, b) in d1.iter().zip(d2) {
                    let (value, sev) = average_delta(*a, *b, p);
                    blended.push(value);
                    worst = worst || sev;
                }
            }
            put(&mut merged, v, &blended);
            if worst {
                report.major += 1;
                report.major_vertices.push((x, y));
            } else {
                report.minor += 1;
            }
        }
    }
    Ok((merged, report))
}

/// One plugin's change merged onto what the load order has merged so far, on *values*
/// (the OpenMW fork's `apply_merge_strategy` over rolling-reference diffs): `acc` holds the
/// merge relative to its own reference (the masters', or the first plugin to define the
/// layer), `incoming` the plugin's change relative to what loaded before it. Where only the
/// plugin moved a vertex it is taken; where both did, `Overwrite` takes the plugin's value,
/// `Ignore` keeps the merge's, `Resolve` blends the two values (`classify_conflict`: equal
/// values are no conflict), `Curvature` blends them weighted by each edit's size and the
/// structure it adds. A vertex counts as contested only when the two values differ.
pub fn merge_onto(acc: &mut RelativeGrid, incoming: &RelativeGrid, layer_bits: u32, strategy: Strategy, p: ConflictParams) -> Result<MergeReport, String> {
    if acc.side != incoming.side || acc.components != incoming.components {
        return Err(format!(
            "cannot merge a {0}x{0}x{1} grid with a {2}x{2}x{3} one",
            acc.side, acc.components, incoming.side, incoming.components
        ));
    }
    let chosen = resolve_for(layer_bits, strategy)?;
    let (side, comps) = (acc.side, acc.components);
    let mut report = MergeReport { strategy: chosen, ..Default::default() };
    let before = acc.flat();
    let theirs = incoming.flat();
    let surfaces = (chosen == Strategy::Curvature).then(|| {
        let f = |v: &[i64]| v.iter().map(|&x| x as f64).collect::<Vec<f64>>();
        (f(&acc.reference), f(&before), f(&theirs))
    });
    let set = |acc: &mut RelativeGrid, v: usize, values: &[i64]| {
        let mut changed = false;
        for (o, value) in values.iter().enumerate() {
            let i = v * comps + o;
            acc.delta[i] = value - acc.reference[i];
            changed = changed || acc.delta[i] != 0;
        }
        acc.changed[v] = changed;
    };
    for y in 0..side {
        for x in 0..side {
            let v = y * side + x;
            let mine = &before[v * comps..(v + 1) * comps];
            let new = &theirs[v * comps..(v + 1) * comps];
            match (acc.changed[v], incoming.changed[v]) {
                (_, false) => {
                    if acc.changed[v] {
                        report.taken_from_one += 1;
                    }
                    continue;
                }
                (false, true) => {
                    report.taken_from_two += 1;
                    set(acc, v, new);
                    continue;
                }
                (true, true) => {}
            }
            if mine == new {
                continue;
            }
            report.contested += 1;
            let mut worst = false;
            let blended: Vec<i64> = match chosen {
                Strategy::Overwrite => new.to_vec(),
                Strategy::Ignore => continue,
                Strategy::Curvature => {
                    let (reference, t1, t2) = surfaces.as_ref().unwrap();
                    let base = curvature_at(reference, side, x, y);
                    let added1 = (curvature_at(t1, side, x, y) - base).max(0.0);
                    let added2 = (curvature_at(t2, side, x, y) - base).max(0.0);
                    let d1 = acc.delta[v * comps];
                    let d2 = incoming.delta[v * comps];
                    let w1 = d1.abs() as f64 * (1.0 + added1 * 8.0);
                    let w2 = d2.abs() as f64 * (1.0 + added2 * 8.0);
                    let (value, sev) = weighted_delta(mine[0], new[0], w1, w2, p);
                    worst = sev;
                    vec![value]
                }
                _ => mine
                    .iter()
                    .zip(new)
                    .map(|(a, b)| {
                        if a == b {
                            return *a;
                        }
                        let (value, sev) = average_delta(*a, *b, p);
                        worst = worst || sev;
                        value
                    })
                    .collect(),
            };
            set(acc, v, &blended);
            if matches!(chosen, Strategy::Resolve | Strategy::Curvature) {
                if worst {
                    report.major += 1;
                    report.major_vertices.push((x, y));
                } else {
                    report.minor += 1;
                }
            }
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blends_toward_the_larger_edit() {
        let p = ConflictParams::default();
        assert_eq!(average_delta(0, 0, p), (0, false));
        let (v, major) = average_delta(100, 10, p);
        assert!(v > 55 && v < 100, "{v}");
        assert!(major);
        assert_eq!(weighted_delta(10, 20, 0.0, 0.0, p), average_delta(10, 20, p));
    }

    #[test]
    fn strategies() {
        let r = vec![0; 4];
        let a = RelativeGrid::from_difference(r.clone(), &[5, 0, 3, 0], 2, 1).unwrap();
        let b = RelativeGrid::from_difference(r.clone(), &[9, 7, 0, 0], 2, 1).unwrap();
        let (m, rep) = merge_layer(layer::VERTEX_HEIGHTS, &a, &b, Strategy::Auto, ConflictParams::default()).unwrap();
        assert_eq!(m.flat(), vec![9, 7, 3, 0]);
        assert_eq!((rep.taken_from_one, rep.taken_from_two, rep.contested, rep.strategy), (1, 1, 1, Strategy::Overwrite));
        let (m, _) = merge_layer(layer::VERTEX_HEIGHTS, &a, &b, Strategy::Ignore, ConflictParams::default()).unwrap();
        assert_eq!(m.flat()[0], 5);
        assert!(merge_layer(layer::TEXTURES, &a, &b, Strategy::Resolve, ConflictParams::default()).unwrap_err().starts_with("TEXTURES holds identifiers"));
        assert!(merge_layer(layer::VERTEX_COLORS, &a, &b, Strategy::Curvature, ConflictParams::default()).unwrap_err().contains("VERTEX_COLORS has no surface"));
    }

    #[test]
    fn merges_values_onto_the_load_order() {
        // The masters have 0 everywhere; A raised vertices 0 and 1, B (against A's terrain)
        // moved vertex 1 again and vertex 2.
        let p = ConflictParams::default();
        let start = || RelativeGrid::new(vec![0; 4], 2, 1).unwrap();
        let a = RelativeGrid::from_difference(vec![0; 4], &[100, 100, 0, 0], 2, 1).unwrap();
        let b = RelativeGrid::from_difference(vec![100, 100, 0, 0], &[100, 300, 50, 0], 2, 1).unwrap();
        let mut acc = start();
        merge_onto(&mut acc, &a, layer::VERTEX_HEIGHTS, Strategy::Auto, p).unwrap();
        let r = merge_onto(&mut acc, &b, layer::VERTEX_HEIGHTS, Strategy::Auto, p).unwrap();
        assert_eq!(acc.flat(), vec![100, 300, 50, 0]);
        assert_eq!((r.taken_from_one, r.taken_from_two, r.contested), (1, 1, 1));
        let mut acc = start();
        merge_onto(&mut acc, &a, layer::VERTEX_HEIGHTS, Strategy::Auto, p).unwrap();
        merge_onto(&mut acc, &b, layer::VERTEX_HEIGHTS, Strategy::Ignore, p).unwrap();
        assert_eq!(acc.flat(), vec![100, 100, 50, 0]);
        let mut acc = start();
        merge_onto(&mut acc, &a, layer::VERTEX_HEIGHTS, Strategy::Auto, p).unwrap();
        let r = merge_onto(&mut acc, &b, layer::VERTEX_HEIGHTS, Strategy::Resolve, p).unwrap();
        let v = acc.flat()[1];
        assert!(v > 100 && v < 300, "{v}");
        assert_eq!(r.major + r.minor, 1);
        // The fork's own pin: B after A in the same place wins under Auto.
        let mut acc = start();
        merge_onto(&mut acc, &RelativeGrid::from_difference(vec![0; 4], &[800; 4], 2, 1).unwrap(), layer::VERTEX_HEIGHTS, Strategy::Auto, p).unwrap();
        merge_onto(&mut acc, &RelativeGrid::from_difference(vec![800; 4], &[1600; 4], 2, 1).unwrap(), layer::VERTEX_HEIGHTS, Strategy::Auto, p).unwrap();
        assert_eq!(acc.flat(), vec![1600; 4]);
    }
}
