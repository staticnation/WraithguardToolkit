//! The cell load's reorder (`reorder_triangles_if_not_worse`): never a worse vertex cache.

use gardenfell_nif::triorder::{analyze_vertex_cache, reorder_triangles_if_not_worse};

/// A `n` x `n` vertex grid of quads, two triangles each, row by row (a good order).
fn grid(n: usize) -> (Vec<u16>, Vec<f32>) {
    let mut pos = Vec::new();
    for y in 0..n {
        for x in 0..n {
            pos.extend([x as f32, y as f32, 0.0]);
        }
    }
    let mut idx = Vec::new();
    for y in 0..n - 1 {
        for x in 0..n - 1 {
            let a = (y * n + x) as u16;
            let (b, c, d) = (a + 1, a + n as u16, a + n as u16 + 1);
            idx.extend([a, b, c, b, d, c]);
        }
    }
    (idx, pos)
}

fn acmr(idx: &[u16], n: usize) -> f32 {
    let w: Vec<u32> = idx.iter().map(|&i| i as u32).collect();
    analyze_vertex_cache(&w, n, 16).1
}

#[test]
fn a_shuffled_part_is_reordered_and_better() {
    let (good, pos) = grid(24);
    let n = pos.len() / 3;
    // the triangles in a scattered order
    let tris: Vec<[u16; 3]> = good.chunks_exact(3).map(|t| [t[0], t[1], t[2]]).collect();
    let mut order: Vec<usize> = (0..tris.len()).collect();
    order.sort_by_key(|&i| (i * 7919) % tris.len());
    let mut idx: Vec<u16> = order.iter().flat_map(|&i| tris[i]).collect();
    let before = acmr(&idx, n);
    assert!(reorder_triangles_if_not_worse(&mut idx, &pos));
    assert!(acmr(&idx, n) < before);
}

#[test]
fn a_part_is_never_left_worse() {
    let (mut idx, pos) = grid(24);
    let n = pos.len() / 3;
    let before = acmr(&idx, n);
    let kept = reorder_triangles_if_not_worse(&mut idx, &pos);
    let after = acmr(&idx, n);
    assert!(after <= before + 1e-6);
    // whichever it did, the same triangles
    assert_eq!(idx.len(), 23 * 23 * 6);
    let _ = kept;
}

#[test]
fn bad_input_is_left_alone() {
    let mut idx: Vec<u16> = vec![0, 1];
    assert!(!reorder_triangles_if_not_worse(&mut idx, &[0.0; 9]));
    let mut idx: Vec<u16> = vec![0, 1, 9];
    assert!(!reorder_triangles_if_not_worse(&mut idx, &[0.0; 9]));
    assert_eq!(idx, vec![0, 1, 9]);
}
