//! Round 18ey: the same footprints at four times the map's pixel resolution, so the
//! question "how would it look" is not answered by the tile size alone.
use gardenfell_world::worldmap::{tile, TILE};
const CELL: f32 = 8192.0;
use std::sync::OnceLock;
static MUL: OnceLock<usize> = OnceLock::new();
fn res() -> usize { TILE * *MUL.get().unwrap_or(&4) }
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let root = std::path::PathBuf::from(&a[1]);
    let g = (a[2].parse::<i32>().unwrap(), a[3].parse::<i32>().unwrap());
    let minr: f32 = a.get(4).map(|s| s.parse().unwrap()).unwrap_or(0.0);
    let _ = MUL.set(a.get(5).map(|s| s.parse().unwrap()).unwrap_or(4usize));
    let roots = vec![("data".to_string(), root.clone())];
    let mut v = gardenfell_install::vfs::Vfs::with_roots(&roots);
    v.add_archives(&[root.join("Morrowind.bsa")]);
    let w = gardenfell_world::world::World::load(&[root.join("Morrowind.esm")], &v, true);
    let pal = std::collections::HashMap::new();
    let base = tile(&w, w.lands.get(&g).unwrap(), &pal);
    // The ground, bilinear to res() so only the footprints are sharp.
    let n_hi = res();
    let mut px = vec![0u8; n_hi * n_hi * 4];
    for y in 0..res() {
        for x in 0..res() {
            let (sx, sy) = (x * TILE / res(), y * TILE / res());
            let o = (y * res() + x) * 4;
            let s = (sy * TILE + sx) * 4;
            px[o..o + 4].copy_from_slice(&base[s..s + 4]);
        }
    }
    let c = w.cells.get(&g).unwrap();
    let (ox, oy) = (g.0 as f32 * CELL, g.1 as f32 * CELL);
    let step = CELL / res() as f32;
    let mut n = 0;
    for r in c.refs.values() {
        if r.deleted || r.moved_to.is_some() { continue }
        let Some(m) = w.models.get(&r.id.to_ascii_lowercase()) else { continue };
        let Some(geom) = w.geom.get(&m.to_ascii_lowercase()) else { continue };
        let s = if r.scale > 0.0 { r.scale } else { 1.0 };
        if geom.radius * s < minr { continue }
        let (cz, sz) = (r.rot[2].cos(), r.rot[2].sin());
        let (lo3, hi3) = (geom.aabb_min, geom.aabb_max);
        let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
        for (x, y) in [(lo3[0], lo3[1]), (hi3[0], lo3[1]), (hi3[0], hi3[1]), (lo3[0], hi3[1])] {
            let (x, y) = (x * s, y * s);
            let (wx, wy) = (x * cz - y * sz + r.pos[0], x * sz + y * cz + r.pos[1]);
            lo[0] = lo[0].min(wx); hi[0] = hi[0].max(wx);
            lo[1] = lo[1].min(wy); hi[1] = hi[1].max(wy);
        }
        if hi[0] < ox || lo[0] > ox + CELL || hi[1] < oy || lo[1] > oy + CELL { continue }
        n += 1;
        let x0 = (((lo[0] - ox) / step).floor() as i64).clamp(0, res() as i64 - 1) as usize;
        let x1 = (((hi[0] - ox) / step).ceil() as i64).clamp(0, res() as i64 - 1) as usize;
        let y0 = ((res() as f32 - (hi[1] - oy) / step).floor() as i64).clamp(0, res() as i64 - 1) as usize;
        let y1 = ((res() as f32 - (lo[1] - oy) / step).ceil() as i64).clamp(0, res() as i64 - 1) as usize;
        for y in y0..=y1 {
            for x in x0..=x1 {
                let o = (y * res() + x) * 4;
                for k in 0..3 { px[o + k] = (px[o + k] as f32 * 0.68) as u8; }
            }
        }
    }
    eprintln!("{n} objects at radius >= {minr}");
    let mut ppm = format!("P6\n{} {}\n255\n", res(), res()).into_bytes();
    for p in px.chunks(4) { ppm.extend_from_slice(&p[..3]); }
    std::fs::write(format!("/tmp/claude-0/maphi_{}_{}.ppm", minr as i32, res()), ppm).unwrap();
}
