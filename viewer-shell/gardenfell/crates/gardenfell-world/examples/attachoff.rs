/// Round 17o diagnostic: where a lamp's light lands relative to the lamp you can see.
///
/// Robin: "lights seems to be shining in one direction from a lamp, or is offset faulty
/// from the lamps themselves." OpenMW hangs the light on a node called AttachLight, and
/// with no such node attaches it to the object root - the mesh origin. This measures how
/// far that origin is from the middle of the geometry, for every light mesh in the BSA.
fn main() {
    let p = std::env::var("GARDENFELL_BSA").expect("GARDENFELL_BSA");
    let bsa = gardenfell_bsa::bsa::Bsa::open(std::path::Path::new(&p)).unwrap();
    let names: Vec<String> = bsa
        .names()
        .iter()
        .filter(|n| {
            let l = n.to_ascii_lowercase();
            l.ends_with(".nif") && (l.contains("light") || l.contains("lamp") || l.contains("candle") || l.contains("lantern") || l.contains("torch"))
        })
        .cloned()
        .collect();
    eprintln!("archive holds {} entries, {} matched the name filter", bsa.len(), names.len());
    if let Some(n) = bsa.names().iter().take(3).next() { eprintln!("first entry: {n}"); }
    let (mut with, mut without) = (0usize, 0usize);
    let mut far: Vec<(f32, String, [f32; 3], [f32; 3])> = Vec::new();
    for n in &names {
        let Some(buf) = bsa.get(&gardenfell_install::vfs::norm(n)) else { continue };
        let Some((parts, attach)) = gardenfell_nif::nif::parse_draw_lit(buf) else { continue };
        if parts.is_empty() { continue; }
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for p in &parts {
            for v in p.pos.chunks_exact(3) {
                for k in 0..3 { lo[k] = lo[k].min(v[k]); hi[k] = hi[k].max(v[k]); }
            }
        }
        if lo[0] > hi[0] { continue; }
        let c = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0];
        let a = match attach { Some(a) => { with += 1; a } None => { without += 1; [0.0; 3] } };
        let d = ((a[0] - c[0]).powi(2) + (a[1] - c[1]).powi(2) + (a[2] - c[2]).powi(2)).sqrt();
        let span = ((hi[0]-lo[0]).powi(2) + (hi[1]-lo[1]).powi(2) + (hi[2]-lo[2]).powi(2)).sqrt();
        far.push((d / span.max(1e-3), n.clone(), a, c));
    }
    if let Ok(only)=std::env::var("ONLY") {
        let Some(buf)=bsa.get(&gardenfell_install::vfs::norm(&only)) else { panic!("no {only}") };
        let (parts,attach)=gardenfell_nif::nif::parse_draw_lit(buf).unwrap();
        println!("\n{only}: AttachLight {attach:?}");
        for p in &parts {
            let (mut lo,mut hi)=([f32::MAX;3],[f32::MIN;3]);
            for v in p.pos.chunks_exact(3){for k in 0..3{lo[k]=lo[k].min(v[k]);hi[k]=hi[k].max(v[k]);}}
            println!("  {:<34} tex {:<28} box [{:.0},{:.0},{:.0}]..[{:.0},{:.0},{:.0}] emissive {:?}",
                p.name,p.tex,lo[0],lo[1],lo[2],hi[0],hi[1],hi[2],p.emissive);
        }
        return;
    }
    far.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
    println!("{} light meshes: {} with AttachLight, {} without", with + without, with, without);
    println!("\nfurthest from the middle of the mesh, as a fraction of the mesh's own span:");
    for (r, n, a, c) in far.iter().take(20) {
        println!("  {:>5.2}  {:<44} light {:?}  middle [{:.0},{:.0},{:.0}]", r, n, a, c[0], c[1], c[2]);
    }
}
