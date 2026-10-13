//! A `.kf` and the mesh it animates, as the reader binds them (round 18ag):
//! `kfdump file.nif file.kf`.
fn main() {
    let nif = std::env::args().nth(1).expect("a .nif");
    let kfp = std::env::args().nth(2).expect("a .kf");
    let kf = gardenfell_nif::nif::parse_kf(&std::fs::read(&kfp).unwrap()).expect("not a kf");
    println!("text keys:");
    for (t, k) in &kf.text_keys {
        println!("  {t:8.3}  {k}");
    }
    println!("idle clip: {:?}   whole span: {:?}", kf.idle(), kf.span());
    println!("tracks: {}", kf.tracks.len());
    for t in &kf.tracks {
        let d = &t.data;
        let tr = |k: &[(f32, [f32; 3])]| -> String {
            if k.is_empty() { return "-".into(); }
            let mut lo = [f32::MAX; 3]; let mut hi = [f32::MIN; 3];
            for (_, v) in k { for i in 0..3 { lo[i] = lo[i].min(v[i]); hi[i] = hi[i].max(v[i]); } }
            format!("moves [{:.0},{:.0},{:.0}]", hi[0]-lo[0], hi[1]-lo[1], hi[2]-lo[2])
        };
        println!("  {:<16} flags {:#06x} {:.2}..{:.2} rot {} xyz {} trans {} ({}) scale {}",
            t.node, t.ctrl.flags, t.ctrl.start, t.ctrl.stop, d.rot.len(),
            d.rot_xyz.as_ref().map(|c| c[0].len()+c[1].len()+c[2].len()).unwrap_or(0),
            d.trans.len(), tr(&d.trans), d.scales.len());
    }
    let buf = std::fs::read(&nif).unwrap();
    let (parts, _) = gardenfell_nif::nif::parse_draw_lit_kf(&buf, Some(&kf)).expect("geometry");
    let mut animated = 0; let mut skinned = 0;
    for (i, p) in parts.iter().enumerate() {
        let mut lo = [f32::MAX; 3]; let mut hi = [f32::MIN; 3];
        for v in p.pos.chunks_exact(3) { for k in 0..3 { lo[k] = lo[k].min(v[k]); hi[k] = hi[k].max(v[k]); } }
        let links = p.anim.as_ref().map(|a| a.nodes.len()).unwrap_or(0);
        if links > 0 { animated += 1; }
        if let Some(sk) = &p.skin {
            skinned += 1;
            let maxb = sk.idx.chunks(4).map(|c| c.iter().filter(|&&b| b != 255).count()).max().unwrap_or(0);
            let chains: Vec<usize> = sk.bones.iter().map(|b| b.chain.len()).collect();
            println!("  part {i} {:<24} SKINNED verts {} bones {} max/vertex {} chains {:?} still box [{:.0},{:.0},{:.0}]..[{:.0},{:.0},{:.0}]", p.name, sk.pos.len()/3, sk.bones.len(), maxb, chains, lo[0],lo[1],lo[2],hi[0],hi[1],hi[2]);
        } else if links > 0 {
            println!("  part {i} {:<24} links {links}", p.name);
        }
    }
    println!("{} parts, {animated} under moving nodes, {skinned} skinned", parts.len());
}
