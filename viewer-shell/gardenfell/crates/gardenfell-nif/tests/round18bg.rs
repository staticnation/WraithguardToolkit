//! Round 18bg — the last two findings of the bug hunt: §G2 and §G6.
//!
//! Both are NIF format readers, and both were held back for a round of their own because
//! being wrong about either costs more than the record it is about. Robin: "We can keep G2
//! and G6 for the next round to tackle them separately as you proposed."
//!
//! **§G2 — `NiTriStrips` read as empty.** The node kind was in the dispatch; the data
//! record, `NiTriStripsData`, was not, so the shape's data reference pointed at a record
//! the reader had resynced past. The shape came back with no vertices and no faces and was
//! dropped in silence: no grass kept out of it, no grass grown on it, no hull, and nothing
//! said. Measured first: across 48 meshes of Robin's own load order — Remiros' Groundcover,
//! Aesthesia, Graht Swamp Trees (including its 3.6 MB `bcTreeHuge.nif`) and Vurt's Animated
//! Solstheim Trees — **not one is stripified**. Morrowind's exporters write `NiTriShape` at
//! 4.0.0.2 and the mods follow them. So this is fixed for the failure mode rather than for
//! the meshes: silence is the worst answer a reader can give.
//!
//! **§G6 — an empty `NiColorData` desynced the reader.** The interpolation kind is written
//! only when a key group has at least one key; this record read it unconditionally, so an
//! empty colour track took four bytes out of the *next* record and every record after it
//! read the wrong bytes. A TBC-keyed one lost twelve bytes a key on top. The cost is never
//! the colour track — it is the rest of the file.
//!
//! The fixtures are built byte by byte here, as `round18be.rs`'s are and for the same
//! reason: what these test is arithmetic inside the reader, not a shape.

use gardenfell_nif::nif;

/// Round 18gh: whether the typed model reads the whole file, which is what the byte
/// reader's `scanned == 0` claimed (every record measured, none hunted for). The engine
/// reads through the model now, so this is the one claim left to make about a layout.
fn reads_to_the_end(buf: &[u8]) -> bool {
    gardenfell_nif::netimmerse::stream::Stream::parse(buf).is_ok()
}

// ---------------------------------------------------------------------------
// A NIF, built byte by byte
// ---------------------------------------------------------------------------

fn f32s(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}
fn strb(s: &str) -> Vec<u8> {
    let mut o = (s.len() as u32).to_le_bytes().to_vec();
    o.extend_from_slice(s.as_bytes());
    o
}
const IDENT3: [f32; 9] = [1., 0., 0., 0., 1., 0., 0., 0., 1.];

fn av(name: &str, flags: u16, trans: [f32; 3], rot: &[f32; 9], scale: f32, props: &[i32]) -> Vec<u8> {
    let mut o = strb(name);
    o.extend_from_slice(&(-1i32).to_le_bytes()); // extra data
    o.extend_from_slice(&(-1i32).to_le_bytes()); // controller
    o.extend_from_slice(&flags.to_le_bytes());
    o.extend_from_slice(&f32s(&trans));
    o.extend_from_slice(&f32s(rot));
    o.extend_from_slice(&scale.to_le_bytes());
    o.extend_from_slice(&f32s(&[0.0, 0.0, 0.0])); // velocity
    o.extend_from_slice(&(props.len() as u32).to_le_bytes());
    for p in props {
        o.extend_from_slice(&p.to_le_bytes());
    }
    o.extend_from_slice(&0u32.to_le_bytes()); // has bounding volume
    o
}

fn node(ty: &str, name: &str, kids: &[i32]) -> Vec<u8> {
    let mut o = strb(ty);
    o.extend_from_slice(&av(name, 0x000e, [0.0; 3], &IDENT3, 1.0, &[]));
    o.extend_from_slice(&(kids.len() as u32).to_le_bytes());
    for k in kids {
        o.extend_from_slice(&k.to_le_bytes());
    }
    o.extend_from_slice(&0u32.to_le_bytes()); // effects
    o
}

/// A geometry node of either kind — the only difference between the two is the name.
fn shape_of(ty: &str, name: &str, data: i32) -> Vec<u8> {
    let mut o = strb(ty);
    o.extend_from_slice(&av(name, 0x000e, [0.0; 3], &IDENT3, 1.0, &[]));
    o.extend_from_slice(&data.to_le_bytes());
    o.extend_from_slice(&(-1i32).to_le_bytes()); // skin
    o
}

/// The part of a `NiTriBasedGeomData` both kinds share, so the two fixtures below can
/// only differ where the format does.
fn geom_head(verts: &[[f32; 3]]) -> Vec<u8> {
    let mut o = Vec::new();
    o.extend_from_slice(&(verts.len() as u16).to_le_bytes());
    o.extend_from_slice(&1u32.to_le_bytes()); // has vertices
    for v in verts {
        o.extend_from_slice(&f32s(v));
    }
    o.extend_from_slice(&0u32.to_le_bytes()); // no normals
    o.extend_from_slice(&f32s(&[0.0, 0.0, 0.0, 4000.0])); // bounding sphere
    o.extend_from_slice(&0u32.to_le_bytes()); // no vertex colours
    o.extend_from_slice(&0u16.to_le_bytes()); // no UV sets
    o.extend_from_slice(&0u32.to_le_bytes());
    o
}

fn shape_data(verts: &[[f32; 3]], tris: &[[u16; 3]]) -> Vec<u8> {
    let mut o = strb("NiTriShapeData");
    o.extend_from_slice(&geom_head(verts));
    o.extend_from_slice(&(tris.len() as u16).to_le_bytes());
    o.extend_from_slice(&((tris.len() * 3) as u32).to_le_bytes());
    for t in tris {
        for i in t {
            o.extend_from_slice(&i.to_le_bytes());
        }
    }
    o.extend_from_slice(&0u16.to_le_bytes()); // match groups
    o
}

/// `NiTriStripsData`: the shared head, a triangle count, then one length and one run of
/// points per strip. No `hasPoints` flag at 4.0.0.2, and no match groups.
fn strips_data(verts: &[[f32; 3]], strips: &[Vec<u16>]) -> Vec<u8> {
    let mut o = strb("NiTriStripsData");
    o.extend_from_slice(&geom_head(verts));
    let ntris: usize = strips.iter().map(|s| s.len().saturating_sub(2)).sum();
    o.extend_from_slice(&(ntris as u16).to_le_bytes());
    o.extend_from_slice(&(strips.len() as u16).to_le_bytes());
    for s in strips {
        o.extend_from_slice(&(s.len() as u16).to_le_bytes());
    }
    for s in strips {
        for p in s {
            o.extend_from_slice(&p.to_le_bytes());
        }
    }
    o
}

/// `NiColorData`: a key count, the interpolation kind **only when there are keys**, then
/// the keys. `interp` 1 is linear, 2 quadratic (a forward and a backward colour), 3 TBC
/// (tension, bias, continuity).
fn colour_data(keys: &[[f32; 5]], interp: u32) -> Vec<u8> {
    let mut o = strb("NiColorData");
    o.extend_from_slice(&(keys.len() as u32).to_le_bytes());
    if keys.is_empty() {
        return o;
    }
    o.extend_from_slice(&interp.to_le_bytes());
    for k in keys {
        o.extend_from_slice(&f32s(k));
        match interp {
            2 => o.extend_from_slice(&f32s(&[0.0; 8])), // forward and backward
            3 => o.extend_from_slice(&f32s(&[0.0; 3])), // tension, bias, continuity
            _ => {}
        }
    }
    o
}

fn nif_file(records: &[Vec<u8>]) -> Vec<u8> {
    let mut o = b"NetImmerse File Format, Version 4.0.0.2\n".to_vec();
    o.extend_from_slice(&0x0400_0002u32.to_le_bytes());
    o.extend_from_slice(&(records.len() as u32).to_le_bytes());
    for r in records {
        o.extend_from_slice(r);
    }
    o.extend_from_slice(&1u32.to_le_bytes());
    o.extend_from_slice(&0i32.to_le_bytes());
    o
}

/// The quad every winding test below is about: two triangles in the z = 0 plane, wound so
/// that both face +z.
///
/// As a strip it is the four corners in order; as a shape it is `(0,1,2)` and `(2,1,3)` —
/// the second one *swapped*, which is exactly what the strip reader has to do for itself
/// on every odd triangle.
fn quad() -> (Vec<[f32; 3]>, Vec<[u16; 3]>, Vec<u16>) {
    (
        vec![[0., 0., 0.], [10., 0., 0.], [0., 10., 0.], [10., 10., 0.]],
        vec![[0, 1, 2], [2, 1, 3]],
        vec![0, 1, 2, 3],
    )
}

/// Every triangle of a parsed hull, as (a, b, c) vertex triples.
fn faces(g: &nif::MeshGeom) -> Vec<[[f32; 3]; 3]> {
    g.tris.chunks(3).map(|c| [c[0], c[1], c[2]]).collect()
}

/// The face normal of one triangle, unnormalised — only its direction is ever asked.
fn cross(t: &[[f32; 3]; 3]) -> [f32; 3] {
    let u = [t[1][0] - t[0][0], t[1][1] - t[0][1], t[1][2] - t[0][2]];
    let v = [t[2][0] - t[0][0], t[2][1] - t[0][1], t[2][2] - t[0][2]];
    [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]]
}

// ---------------------------------------------------------------------------
// G2 — NiTriStrips
// ---------------------------------------------------------------------------

#[test]
fn g2_a_stripified_shape_is_read_at_all() {
    /* The whole of the finding: with `NiTriStripsData` missing from the dispatch this
    returned nothing — `parse` found a mesh with no geometry, and a mesh with no
    geometry is a rock that keeps no grass out and grows none. */
    let (v, _, strip) = quad();
    let f = nif_file(&[
        node("NiNode", "Root", &[1]),
        node("RootCollisionNode", "collision", &[2]),
        shape_of("NiTriStrips", "Tri Hull", 3),
        strips_data(&v, &[strip]),
    ]);
    let g = nif::parse(&f).expect("the mesh parses");
    assert_eq!(g.shape, nif::ColShape::Hull, "the hull is what was found");
    assert_eq!(faces(&g).len(), 2, "a four-point strip is two triangles");
    // Round 18gh: `scanned` is gone; the typed model reading the file to its end is the claim.
    assert!(reads_to_the_end(&f), "and the file reads to its end");
    assert!((g.aabb_max[0] - 10.0).abs() < 1e-3 && (g.aabb_max[1] - 10.0).abs() < 1e-3);
}

#[test]
fn g2_the_strip_and_the_shape_are_the_same_mesh() {
    // The same quad written both ways reads as the same geometry, triangle for triangle.
    let (v, tris, strip) = quad();
    let mk = |data: Vec<u8>, ty: &str| {
        nif_file(&[
            node("NiNode", "Root", &[1]),
            node("RootCollisionNode", "collision", &[2]),
            shape_of(ty, "Tri Hull", 3),
            data,
        ])
    };
    let a = nif::parse(&mk(shape_data(&v, &tris), "NiTriShape")).expect("shape parses");
    let b = nif::parse(&mk(strips_data(&v, &[strip]), "NiTriStrips")).expect("strips parse");
    assert_eq!(faces(&a), faces(&b), "the strip came out a different mesh");
    assert_eq!(a.aabb_min, b.aabb_min);
    assert_eq!(a.aabb_max, b.aabb_max);
}

#[test]
fn g2_every_other_triangle_is_swapped_back() {
    /* The part worth getting right. A strip's triangles alternate handedness, so the odd
    ones have to be swapped or half the mesh comes out inside out — and a face read
    inside out is a normal pointing *into* the mesh, which is §H4's failure by another
    road: blades growing into a rock, and a closed hull whose outside reads as its
    inside. Both of this quad's triangles face +z; read without the swap, the second
    faces -z. */
    let (v, _, strip) = quad();
    let f = nif_file(&[
        node("NiNode", "Root", &[1]),
        node("RootCollisionNode", "collision", &[2]),
        shape_of("NiTriStrips", "Tri Hull", 3),
        strips_data(&v, &[strip]),
    ]);
    let g = nif::parse(&f).expect("the mesh parses");
    let fs = faces(&g);
    assert_eq!(fs.len(), 2);
    for (i, t) in fs.iter().enumerate() {
        let n = cross(t);
        assert!(n[2] > 0.0, "triangle {i} faces {n:?} — it should face +z");
    }
}

#[test]
fn g2_a_strip_of_five_alternates_all_the_way_along() {
    /* Two triangles only ever exercise one swap. Five points is three triangles — even,
    odd, even — over a ladder of quads laid along x.

    The ladder zig-zags y = 10, 0, 10, 0, 10 rather than 0, 10, 0, 10, 0, which is the
    way round that makes its first triangle wind counter-clockwise and so face +z; the
    other way every triangle faces -z, correctly read and uniformly backwards, which is
    a fixture saying something about itself rather than about the reader. What the
    assertion is really about is that all three agree. */
    let v: Vec<[f32; 3]> =
        (0..5).map(|i| [(i / 2) as f32 * 10.0, if i % 2 == 0 { 10.0 } else { 0.0 }, 0.0]).collect();
    let f = nif_file(&[
        node("NiNode", "Root", &[1]),
        node("RootCollisionNode", "collision", &[2]),
        shape_of("NiTriStrips", "Tri Hull", 3),
        strips_data(&v, &[vec![0, 1, 2, 3, 4]]),
    ]);
    let g = nif::parse(&f).expect("the mesh parses");
    let fs = faces(&g);
    assert_eq!(fs.len(), 3, "five points are three triangles");
    for (i, t) in fs.iter().enumerate() {
        assert!(cross(t)[2] > 0.0, "triangle {i} of the strip is inside out");
    }
}

#[test]
fn g2_a_degenerate_join_is_dropped() {
    /* A file joins two runs into one strip by repeating a point, which makes a triangle
    with no area. Kept, it would be a face the collision walk has to test and the
    normal maths divides by zero over. */
    let (v, _, _) = quad();
    let f = nif_file(&[
        node("NiNode", "Root", &[1]),
        node("RootCollisionNode", "collision", &[2]),
        shape_of("NiTriStrips", "Tri Hull", 3),
        // 0,1,2 · 1,2,2(degenerate) · 2,2,3(degenerate) · 2,3,1 — two real triangles.
        strips_data(&v, &[vec![0, 1, 2, 2, 3, 1]]),
    ]);
    let g = nif::parse(&f).expect("the mesh parses");
    assert_eq!(faces(&g).len(), 2, "the two zero-area joins should be gone");
    for t in faces(&g) {
        let n = cross(&t);
        assert!(n[0].abs() + n[1].abs() + n[2].abs() > 1e-6, "a zero-area face survived");
    }
}

#[test]
fn g2_several_strips_in_one_record_are_all_read() {
    // Each strip restarts the alternation: the second one's first triangle is even again.
    let (v, _, _) = quad();
    let f = nif_file(&[
        node("NiNode", "Root", &[1]),
        node("RootCollisionNode", "collision", &[2]),
        shape_of("NiTriStrips", "Tri Hull", 3),
        strips_data(&v, &[vec![0, 1, 2], vec![2, 1, 3]]),
    ]);
    let g = nif::parse(&f).expect("the mesh parses");
    let fs = faces(&g);
    assert_eq!(fs.len(), 2, "one triangle from each strip");
    for (i, t) in fs.iter().enumerate() {
        assert!(cross(t)[2] > 0.0, "strip {i} restarted its alternation wrongly");
    }
}

#[test]
fn g2_a_strips_record_leaves_the_rest_of_the_file_alone() {
    /* The other half of reading a record properly: stopping in the right place. A shape
    *after* the strips data has to be found by its own index. Round 18gh: the typed model
    measures the record or refuses the file, so "the file reads to its end" is the claim
    the byte reader's `scanned == 0` used to make. */
    let (v, tris, strip) = quad();
    let f = nif_file(&[
        node("NiNode", "Root", &[1, 4]),
        node("RootCollisionNode", "collision", &[2]),
        shape_of("NiTriStrips", "Tri Strips", 3),
        strips_data(&v, &[strip]),
        shape_of("NiTriShape", "Tri Later", 5),
        shape_data(&[[100., 0., 0.], [110., 0., 0.], [100., 10., 0.]], &tris[..1]),
    ]);
    let _g = nif::parse(&f).expect("the mesh parses");
    assert!(reads_to_the_end(&f), "the file reads to its end through the strips record");
    // The hull is the collision node's, so the later shape shows up in the bounds of the
    // drawn mesh rather than here — what matters is that the file read to its end.
    let (parts, _) = nif::parse_draw_lit_kf(&f, None).expect("the draw pass reads it");
    assert!(
        parts.iter().any(|p| p.name == "Tri Later"),
        "the shape after the strips record was lost: {:?}",
        parts.iter().map(|p| &p.name).collect::<Vec<_>>()
    );
}

#[test]
fn g2_a_stripified_shape_is_drawn_as_well_as_hulled() {
    // The draw pass reads the same `datas` slot, so a stripified shape is a drawn shape
    // too — "what you see is what blocks grass" holds for it like any other.
    let (v, _, strip) = quad();
    let f = nif_file(&[
        node("NiNode", "Root", &[1]),
        shape_of("NiTriStrips", "Tri Quad", 2),
        strips_data(&v, &[strip]),
    ]);
    let (parts, _) = nif::parse_draw_lit_kf(&f, None).expect("the draw pass reads it");
    let p = parts.iter().find(|p| p.name == "Tri Quad").expect("the strip shape is drawn");
    assert_eq!(p.pos.len(), 12, "four vertices");
    assert_eq!(p.idx.len(), 6, "two triangles");
}

// ---------------------------------------------------------------------------
// G6 — an empty NiColorData
// ---------------------------------------------------------------------------

/// A file with a colour track in the middle of it and a shape after, which is where the
/// damage shows: four bytes lost in the track are four bytes taken out of the shape.
fn file_with_colour_track(keys: &[[f32; 5]], interp: u32) -> Vec<u8> {
    let (v, tris, _) = quad();
    nif_file(&[
        node("NiNode", "Root", &[1]),
        node("RootCollisionNode", "collision", &[3]),
        colour_data(keys, interp),
        shape_of("NiTriShape", "Tri Hull", 4),
        shape_data(&v, &tris),
    ])
}

#[test]
fn g6_an_empty_colour_track_does_not_eat_the_next_record() {
    /* The finding. `numKeys` is always written; the interpolation kind is written only
    when there is at least one key. Reading it anyway takes four bytes out of the record
    that follows — and from there the file is gibberish, because every record is found
    by reading the one before it to its end. */
    let f = file_with_colour_track(&[], 1);
    let g = nif::parse(&f).expect("the mesh parses");
    assert!(reads_to_the_end(&f), "the file reads to its end through the empty track (18gh)");
    assert_eq!(faces(&g).len(), 2, "the shape after the empty colour track was lost");
    assert!((g.aabb_max[0] - 10.0).abs() < 1e-3, "and it is the shape it should be");
}

/* Round 18gh: when the engine first read through the typed model, the model had only the
linear colour key and these two files were refused. The byte reader had measured them
right since 18bg (nif.xml's `KeyGroup<Color4>`, Gamebryo's `NiTCBColorKey` and
`NiBezColorKey`), so the model grew the two kinds (`netimmerse/types/nicolorkey.rs`) and
the tests read as they always did. The byte reader was the one to believe here. */

#[test]
fn g6_a_tbc_keyed_colour_track_reads_its_whole_key() {
    // Tension, bias and continuity: three floats a key, whatever the value's width. Read
    // as nothing, a two-key track leaves 24 bytes in the middle of the next record.
    let keys = [[0.0, 1.0, 0.0, 0.0, 1.0], [1.0, 0.0, 0.0, 1.0, 1.0]];
    let f = file_with_colour_track(&keys, 3);
    assert!(reads_to_the_end(&f), "the model reads a TCB colour key");
    let g = nif::parse(&f).expect("the mesh parses");
    assert_eq!(faces(&g).len(), 2, "the shape after a TBC colour track was lost");
}

#[test]
fn g6_a_quadratic_keyed_colour_track_still_reads() {
    /* The control, and the one case the hand-written reader did get right: a quadratic
    key carries a forward and a backward *colour* — eight floats, not the three a TBC
    key carries. One rule now covers both: the kind decides, and the value's width
    decides how wide a tangent is. */
    let keys = [[0.0, 1.0, 1.0, 1.0, 1.0], [0.5, 0.0, 0.0, 0.0, 1.0]];
    let f = file_with_colour_track(&keys, 2);
    assert!(reads_to_the_end(&f), "the model reads a Bezier colour key");
    let g = nif::parse(&f).expect("the mesh parses");
    assert_eq!(faces(&g).len(), 2);
}

#[test]
fn g6_a_linear_colour_track_still_reads() {
    let keys = [[0.0, 1.0, 1.0, 1.0, 1.0], [1.0, 0.0, 0.0, 0.0, 0.0]];
    let f = file_with_colour_track(&keys, 1);
    let g = nif::parse(&f).expect("the mesh parses");
    assert!(reads_to_the_end(&f));
    assert_eq!(faces(&g).len(), 2);
}
