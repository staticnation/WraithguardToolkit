//! Round 18ey: what stamping object footprints onto the picker's map would cost, and what
//! it would look like. Robin asked before letting it be built, so this measures rather
//! than guesses. `mapcost <Data Files> [x y]`
//!
//! The footprint stamped here is the object's **collision bounds**, turned by its own yaw
//! and scaled, projected onto the ground - which is what the engine already knows about
//! every placed thing (`World::geom`) and the same box the scatter avoids. Darkened rather
//! than coloured, so a town reads as a town without arguing with the ground textures.
use gardenfell_world::worldmap::{tile, TILE};

const CELL: f32 = 8192.0;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let root = std::path::PathBuf::from(&a[1]);
    let t0 = std::time::Instant::now();
    let roots = vec![("data".to_string(), root.clone())];
    let mut v = gardenfell_install::vfs::Vfs::with_roots(&roots);
    v.add_archives(&[root.join("Morrowind.bsa")]);
    let w = gardenfell_world::world::World::load(&[root.join("Morrowind.esm")], &v, true);
    eprintln!("world loaded in {} ms", t0.elapsed().as_millis());

    let geom_of = |id: &str| -> Option<&gardenfell_nif::nif::MeshGeom> {
        let m = w.models.get(&id.to_ascii_lowercase())?;
        w.geom.get(&m.to_ascii_lowercase())
    };

    let mut counts: Vec<usize> = Vec::new();
    let mut shaped: Vec<usize> = Vec::new();
    let mut busiest = ((0i32, 0i32), 0usize);
    for (g, c) in &w.cells {
        counts.push(c.refs.len());
        let s = c.refs.values().filter(|r| geom_of(&r.id).is_some()).count();
        if s > busiest.1 { busiest = (*g, s); }
        shaped.push(s);
    }
    counts.sort_unstable();
    shaped.sort_unstable();
    let pc = |v: &[usize], p: f64| v[((v.len() as f64 - 1.0) * p) as usize];
    println!("cells {}   refs/cell: median {} p90 {} p99 {} max {}",
             counts.len(), pc(&counts, 0.5), pc(&counts, 0.9), pc(&counts, 0.99), counts.last().unwrap());
    println!("      with a footprint: median {} p90 {} p99 {} max {} (busiest {:?})",
             pc(&shaped, 0.5), pc(&shaped, 0.9), pc(&shaped, 0.99), shaped.last().unwrap(), busiest.0);
    println!("total refs {}   with a footprint {}", counts.iter().sum::<usize>(), shaped.iter().sum::<usize>());

    // The palette the tile renderer wants; empty is fine, it falls back.
    let pal = std::collections::HashMap::new();

    // Time the plain tile over every land, then the same with footprints stamped.
    let grids: Vec<(i32, i32)> = w.real_lands().map(|(g, _)| *g).collect();
    let t = std::time::Instant::now();
    let mut plain: Vec<Vec<u8>> = Vec::with_capacity(grids.len());
    for g in &grids { plain.push(tile(&w, w.lands.get(g).unwrap(), &pal)); }
    let ms_plain = t.elapsed().as_secs_f64() * 1000.0;

    let t = std::time::Instant::now();
    let mut stamped = 0usize;
    let mut with: Vec<Vec<u8>> = Vec::with_capacity(grids.len());
    for (i, g) in grids.iter().enumerate() {
        let mut px = plain[i].clone();
        stamped += stamp(&w, &geom_of, *g, &mut px, 0.0, 0.62);
        with.push(px);
    }
    let ms_stamp = t.elapsed().as_secs_f64() * 1000.0;
    println!("\n{} tiles: ground {:.0} ms ({:.2} ms a tile), footprints {:.0} ms ({:.2} ms a tile), {} stamped",
             grids.len(), ms_plain, ms_plain / grids.len() as f64,
             ms_stamp, ms_stamp / grids.len() as f64, stamped);

    // And a picture of one cell, before and after.
    if a.len() >= 4 {
        let g = (a[2].parse().unwrap(), a[3].parse().unwrap());
        let k = grids.iter().position(|x| *x == g).expect("no landscape there");
        write_png("/tmp/claude-0/map_plain.png", &plain[k]);
        write_png("/tmp/claude-0/map_stamped.png", &with[k]);
        // And the same cell with only the big things, and with a lighter hand.
        for (name, minr, dark) in [("big300", 300.0f32, 0.72f32), ("big600", 600.0, 0.72), ("big600soft", 600.0, 0.84)] {
            let mut px = plain[k].clone();
            let n = stamp(&w, &geom_of, g, &mut px, minr, dark);
            write_png(&format!("/tmp/claude-0/map_{name}.png"), &px);
            println!("  {name}: {n} objects stamped");
        }
        // How many objects survive each threshold, over the whole world.
        for minr in [0.0f32, 150.0, 300.0, 600.0, 1000.0] {
            let n: usize = w.cells.values().map(|c| c.refs.values().filter(|r| {
                let s = if r.scale > 0.0 { r.scale } else { 1.0 };
                geom_of(&r.id).map(|gm| gm.radius * s >= minr).unwrap_or(false)
            }).count()).sum();
            println!("  radius >= {minr:>6}: {n} objects of {}", w.cells.values().map(|c| c.refs.len()).sum::<usize>());
        }
        println!("wrote the pictures for {:?}", g);
    }
}

/// Darkens the tile where an object's collision bounds cover the ground. Returns how many
/// objects it stamped.
fn stamp<'a>(
    w: &'a gardenfell_world::world::World,
    geom_of: &dyn Fn(&str) -> Option<&'a gardenfell_nif::nif::MeshGeom>,
    g: (i32, i32),
    px: &mut [u8],
    min_radius: f32,
    dark: f32,
) -> usize {
    let Some(c) = w.cells.get(&g) else { return 0 };
    let (ox, oy) = (g.0 as f32 * CELL, g.1 as f32 * CELL);
    let step = CELL / TILE as f32;
    let mut n = 0usize;
    for r in c.refs.values() {
        if r.deleted || r.moved_to.is_some() { continue }
        let Some(geom) = geom_of(&r.id) else { continue };
        let s = if r.scale > 0.0 { r.scale } else { 1.0 };
        if geom.radius * s < min_radius { continue }
        // The bounds' four ground corners, turned by the reference's yaw.
        let (cz, sz) = (r.rot[2].cos(), r.rot[2].sin());
        let (a, b) = (geom.aabb_min, geom.aabb_max);
        let mut lo = [f32::MAX; 2];
        let mut hi = [f32::MIN; 2];
        for (x, y) in [(a[0], a[1]), (b[0], a[1]), (b[0], b[1]), (a[0], b[1])] {
            let (x, y) = (x * s, y * s);
            let (wx, wy) = (x * cz - y * sz + r.pos[0], x * sz + y * cz + r.pos[1]);
            lo[0] = lo[0].min(wx); hi[0] = hi[0].max(wx);
            lo[1] = lo[1].min(wy); hi[1] = hi[1].max(wy);
        }
        // Into this tile's pixels. Row 0 is north, so y is flipped.
        let px0 = (((lo[0] - ox) / step).floor() as i64).clamp(0, TILE as i64 - 1) as usize;
        let px1 = (((hi[0] - ox) / step).ceil() as i64).clamp(0, TILE as i64 - 1) as usize;
        let py0 = ((TILE as f32 - (hi[1] - oy) / step).floor() as i64).clamp(0, TILE as i64 - 1) as usize;
        let py1 = ((TILE as f32 - (lo[1] - oy) / step).ceil() as i64).clamp(0, TILE as i64 - 1) as usize;
        if hi[0] < ox || lo[0] > ox + CELL || hi[1] < oy || lo[1] > oy + CELL { continue }
        n += 1;
        for y in py0..=py1 {
            for x in px0..=px1 {
                let o = (y * TILE + x) * 4;
                for k in 0..3 { px[o + k] = (px[o + k] as f32 * dark) as u8; }
            }
        }
    }
    n
}

fn write_png(path: &str, rgba: &[u8]) {
    // Uncompressed-ish PNG via the crate's own writer if there is one; otherwise a PPM.
    let mut ppm = format!("P6\n{} {}\n255\n", TILE, TILE).into_bytes();
    for p in rgba.chunks(4) { ppm.extend_from_slice(&p[..3]); }
    std::fs::write(path.replace(".png", ".ppm"), ppm).unwrap();
}
