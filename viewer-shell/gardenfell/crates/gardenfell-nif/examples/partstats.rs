/// Round 18a: what a part's vertex colours and normals add up to — for a mesh that draws
/// darker than the game draws it. `partstats some.nif`
fn main() {
    let path = std::env::args().nth(1).expect("a .nif");
    let buf = std::fs::read(&path).unwrap();
    let Some((parts, _)) = gardenfell_nif::nif::parse_draw_lit(&buf) else { println!("no geometry"); return };
    for p in &parts {
        let n = p.pos.len() / 3;
        let mut col = [0f64; 3];
        let mut cmin = [255u8; 3];
        let mut cmax = [0u8; 3];
        for c in p.col.chunks_exact(3) {
            for k in 0..3 { col[k] += c[k] as f64; cmin[k] = cmin[k].min(c[k]); cmax[k] = cmax[k].max(c[k]); }
        }
        let nc = (p.col.len() / 3).max(1) as f64;
        let mut nrm = [0f64; 3];
        let mut up = 0usize;
        let mut down = 0usize;
        for v in p.nrm.chunks_exact(3) {
            for k in 0..3 { nrm[k] += v[k] as f64; }
            if v[2] > 0.5 { up += 1 } else if v[2] < -0.5 { down += 1 }
        }
        let nn = (p.nrm.len() / 3).max(1) as f64;
        println!("{:<20} verts {} idx {} tris {}", p.name, n, p.idx.len(), p.idx.len() / 3);
        println!("   vcol mean [{:.0},{:.0},{:.0}] min {:?} max {:?}   mode {} unlit {} emissive {:?} diffuse {:?} matAlpha {}",
            col[0] / nc, col[1] / nc, col[2] / nc, cmin, cmax, p.vcol_mode, p.unlit, p.emissive, p.diffuse, p.mat_alpha);
        println!("   nrm mean [{:.2},{:.2},{:.2}] up {} down {} of {}   alpha {:#06x} thr {:?} draw {}",
            nrm[0] / nn, nrm[1] / nn, nrm[2] / nn, up, down, p.nrm.len() / 3, p.alpha_flags, p.alpha_threshold, p.draw_mode);
        // The first few vertices, to see the quads.
        for i in 0..n.min(8) {
            let c = if p.col.len() >= (i + 1) * 3 { format!("{:?}", &p.col[i * 3..i * 3 + 3]) } else { "-".into() };
            println!("     v{} pos [{:.1},{:.1},{:.1}] nrm [{:.2},{:.2},{:.2}] uv [{:.2},{:.2}] col {}",
                i, p.pos[i * 3], p.pos[i * 3 + 1], p.pos[i * 3 + 2],
                p.nrm[i * 3], p.nrm[i * 3 + 1], p.nrm[i * 3 + 2], p.uv[i * 2], p.uv[i * 2 + 1], c);
        }
    }
}
