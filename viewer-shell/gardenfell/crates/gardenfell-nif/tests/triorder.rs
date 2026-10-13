//! Round 18ho: the triangle order (`triorder`), checked against meshoptimizer 1.3's own
//! output. The expected values were produced by the C++ (`meshopt_optimizeVertexCache`,
//! `meshopt_optimizeOverdraw` at 1.05, `meshopt_analyzeVertexCache` with a 16-entry
//! cache) over the fixture meshes' draw parts as `parse_draw` hands them, before this
//! round's reorder, and pinned here: the cache pass is integer arithmetic and must match
//! byte for byte (an FNV-1a hash of the output); the overdraw pass sums float areas, so it
//! is held to the C++'s cache statistics and to being a permutation of the input.
use gardenfell_nif::nif::parse_draw;
use gardenfell_nif::triorder::{analyze_vertex_cache, optimize_overdraw, optimize_vertex_cache};

fn fnv(v: &[u32]) -> u64 {
    let mut h: u64 = 1469598103934665603;
    for &x in v {
        for k in 0..4 {
            h ^= ((x >> (k * 8)) & 255) as u64;
            h = h.wrapping_mul(1099511628211);
        }
    }
    h
}

/// The part as the file has it: `parse_draw` reorders now, so the test reads the raw
/// stream the way `parse_draw` did before 18ho - by undoing nothing and instead asking the
/// records for the original order through the typed stream.
fn at(path: &str) -> String {
    format!("{}/../../{}", env!("CARGO_MANIFEST_DIR"), path)
}

fn raw_part(path: &str) -> (Vec<u32>, Vec<f32>) {
    use gardenfell_nif::netimmerse::stream::Stream;
    use gardenfell_nif::netimmerse::types::NiType;
    let b = std::fs::read(at(path)).unwrap();
    let s = Stream::parse(&b).unwrap();
    for blk in &s.blocks {
        if let NiType::NiTriShapeData(d) = blk {
            let pos: Vec<f32> = d.base.base.vertices.iter().flat_map(|v| v.iter().copied()).collect();
            let idx: Vec<u32> = d.triangles.iter().flat_map(|t| t.iter().map(|&i| i as u32)).collect();
            return (idx, pos);
        }
    }
    panic!("{} has no NiTriShapeData", path);
}

fn sorted_tris(idx: &[u32]) -> Vec<[u32; 3]> {
    let mut v: Vec<[u32; 3]> = idx.chunks(3).map(|t| [t[0], t[1], t[2]]).collect();
    v.sort();
    v
}

#[test]
fn the_cache_pass_is_the_c_plus_plus_byte_for_byte() {
    // (file, triangles, vertices transformed as-is, after the cache pass, hash after)
    let cases = [
        ("fixture/rr_basket.nif", 232, 271, 192, 0xd24607580857e9c1u64),
        ("fixture/plank.nif", 12, 8, 8, 0x668a43e15b1c2023u64),
        ("fixture/Data Files/Meshes/f/house.nif", 200, 600, 600, 0xc1a62352768e962bu64),
        ("fixture/Data Files/Meshes/f/rock.nif", 40, 120, 120, 0x07ca9c89af0225c3u64),
        ("fixture/Data Files/Meshes/f/tree.nif", 120, 360, 360, 0x696eacc7177f785bu64),
        ("fixture/Data Files/Meshes/Grass/g1.nif", 2, 6, 6, 0x93fa4021ba288d12u64),
    ];
    for (path, tris, before, after, hash) in cases {
        let (idx, pos) = raw_part(path);
        let nv = pos.len() / 3;
        assert_eq!(idx.len() / 3, tris, "{path}");
        assert_eq!(analyze_vertex_cache(&idx, nv, 16).0, before, "{path} as the file has it");
        let out = optimize_vertex_cache(&idx, nv);
        assert_eq!(analyze_vertex_cache(&out, nv, 16).0, after, "{path} after the cache pass");
        assert_eq!(fnv(&out), hash, "{path}: the C++'s bytes");
        assert_eq!(sorted_tris(&out), sorted_tris(&idx), "{path}: the same triangles");
    }
}

#[test]
fn the_overdraw_pass_keeps_the_c_plus_plus_cache_rate_and_every_triangle() {
    // The C++ reports the same transformed count after its overdraw pass on every fixture
    // part (the threshold holds them), and 192 on the basket.
    for (path, after) in [("fixture/rr_basket.nif", 192u32), ("fixture/plank.nif", 8), ("fixture/Data Files/Meshes/f/house.nif", 600)] {
        let (idx, pos) = raw_part(path);
        let nv = pos.len() / 3;
        let cached = optimize_vertex_cache(&idx, nv);
        let out = optimize_overdraw(&cached, &pos, nv, 1.05);
        assert_eq!(out.len(), idx.len());
        assert_eq!(sorted_tris(&out), sorted_tris(&idx), "{path}: the same triangles");
        let t = analyze_vertex_cache(&out, nv, 16).0;
        assert!(t <= (after as f32 * 1.06) as u32, "{path}: {t} transformed after overdraw, the C++ has {after}");
    }
}

#[test]
fn parse_draw_hands_the_basket_in_the_new_order_and_the_same_everything_else() {
    let b = std::fs::read(at("fixture/rr_basket.nif")).unwrap();
    let parts = parse_draw(&b).unwrap();
    let p = &parts[0];
    let nv = p.pos.len() / 3;
    let idx: Vec<u32> = p.idx.iter().map(|&i| i as u32).collect();
    let (raw, pos) = raw_part("fixture/rr_basket.nif");
    assert_eq!(p.pos, pos, "the vertices are untouched");
    assert_eq!(sorted_tris(&idx), sorted_tris(&raw), "the same triangles");
    assert_ne!(idx, raw, "in a new order");
    assert_eq!(analyze_vertex_cache(&idx, nv, 16).0, 192, "29 percent fewer vertex shader runs than the file's 271");
}

#[test]
fn the_passes_leave_an_empty_or_a_malformed_list_alone() {
    use gardenfell_nif::triorder::reorder_triangles;
    let mut none: Vec<u16> = Vec::new();
    reorder_triangles(&mut none, &[0.0; 9]);
    assert!(none.is_empty());
    let mut bad = vec![0u16, 1, 9];
    reorder_triangles(&mut bad, &[0.0; 9]);
    assert_eq!(bad, [0, 1, 9], "an index past the vertices leaves the list as it was");
    assert!(optimize_vertex_cache(&[], 0).is_empty());
    assert!(optimize_overdraw(&[], &[], 0, 1.05).is_empty());
}
