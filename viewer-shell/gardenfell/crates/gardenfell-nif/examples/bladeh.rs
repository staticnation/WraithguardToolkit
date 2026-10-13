// Prints each mesh's z extent (world, after the part transforms are baked into pos).
fn main() {
    for p in std::env::args().skip(1) {
        let b = std::fs::read(&p).unwrap();
        let parts = gardenfell_nif::nif::parse_draw(&b).unwrap();
        let (mut lo, mut hi) = (f32::MAX, f32::MIN);
        let mut tris = 0;
        for d in &parts { for c in d.pos.chunks(3) { lo = lo.min(c[2]); hi = hi.max(c[2]); } tris += d.idx.len()/3; }
        println!("{:60} z {:7.1} .. {:7.1}  h {:6.1}  tris {}", p.rsplit('/').next().unwrap(), lo, hi, hi-lo, tris);
    }
}
