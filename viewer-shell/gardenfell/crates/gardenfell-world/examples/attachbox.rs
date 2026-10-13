/// Round 17q: is a lamp's AttachLight node inside the lamp?
///
/// Robin has now twice reported that lights look offset from the lamps they belong to.
/// Round 17p checked the *code path* (the same node walk as the geometry, the same
/// matrix) and the fallback (the mesh origin, as OpenMW does) and found both right. This
/// checks the answer instead: the flame of a lantern is inside its cage, so if the walk
/// is right, AttachLight lands inside the mesh's own bounding box nearly every time.
fn main() {
    let p = std::env::var("GARDENFELL_BSA").expect("GARDENFELL_BSA");
    let bsa = gardenfell_bsa::bsa::Bsa::open(std::path::Path::new(&p)).unwrap();
    let (mut with, mut inside) = (0usize, 0usize);
    let mut out: Vec<(f32, String, [f32; 3], [f32; 3], [f32; 3])> = Vec::new();
    for name in bsa.names() {
        let l = name.to_ascii_lowercase();
        if !l.ends_with(".nif") { continue; }
        let Some(buf) = bsa.get(&gardenfell_install::vfs::norm(name)) else { continue };
        let Some((parts, Some(a))) = gardenfell_nif::nif::parse_draw_lit(buf) else { continue };
        if parts.is_empty() { continue; }
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for p in &parts {
            for v in p.pos.chunks_exact(3) {
                for k in 0..3 { lo[k] = lo[k].min(v[k]); hi[k] = hi[k].max(v[k]); }
            }
        }
        if lo[0] > hi[0] { continue; }
        with += 1;
        let is_in = (0..3).all(|k| a[k] >= lo[k] - 1.0 && a[k] <= hi[k] + 1.0);
        if is_in { inside += 1; continue; }
        // How far outside, as a fraction of the mesh's own height.
        let span = (hi[2] - lo[2]).max(1.0);
        let over = (0..3)
            .map(|k| (lo[k] - a[k]).max(a[k] - hi[k]).max(0.0))
            .fold(0.0f32, f32::max);
        out.push((over / span, name.clone(), a, lo, hi));
    }
    out.sort_by(|x, y| y.0.partial_cmp(&x.0).unwrap());
    println!(
        "{with} meshes with an AttachLight node: {inside} inside their own box ({:.0}%), {} outside",
        100.0 * inside as f32 / with.max(1) as f32,
        out.len()
    );
    println!("\nthe worst, as a fraction of the mesh's own height:");
    for (r, n, a, lo, hi) in out.iter().take(15) {
        println!("  {r:>5.2}  {n:<44} light [{:.0},{:.0},{:.0}]  box z {:.0}..{:.0}",
                 a[0], a[1], a[2], lo[2], hi[2]);
    }
}
