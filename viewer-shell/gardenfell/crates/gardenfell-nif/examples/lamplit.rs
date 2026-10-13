/// Round 17o diagnostic: how much of a lamp mesh its own AttachLight can reach.
///
/// 17n took the wrap term off everything but groundcover, which is what OpenMW does.
/// A lamp's own glow shape sits *around* its flame, so its normals point away from it:
/// with `abs(N.L)` it lit itself, with `max(N.L,0)` it cannot. This measures that.
fn main() {
    let path = std::env::args().nth(1).expect("a .nif");
    let buf = std::fs::read(&path).unwrap();
    let (parts, attach) = gardenfell_nif::nif::parse_draw_lit(&buf).expect("geometry");
    let a = attach.unwrap_or([0.0; 3]);
    println!("AttachLight {:?}", a);
    for p in &parts {
        let n = p.pos.len() / 3;
        if p.nrm.len() / 3 != n { println!("  {:<28} no normals", p.name); continue; }
        let (mut lit, mut wrapped) = (0usize, 0.0f64);
        for i in 0..n {
            let v = [p.pos[i * 3], p.pos[i * 3 + 1], p.pos[i * 3 + 2]];
            let nv = [p.nrm[i * 3], p.nrm[i * 3 + 1], p.nrm[i * 3 + 2]];
            let d = [a[0] - v[0], a[1] - v[1], a[2] - v[2]];
            let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt().max(1e-6);
            let nl = (nv[0] * d[0] + nv[1] * d[1] + nv[2] * d[2]) / len;
            if nl > 0.0 { lit += 1; }
            wrapped += nl.abs() as f64;
        }
        println!(
            "  {:<30} verts {:<6} emissive {:?}  glow {:?} uv{}  N.L>0 on {:>5.1}%  mean |N.L| {:.2}",
            p.name, n, p.emissive, p.glow_tex, p.glow_uv_set, 100.0 * lit as f32 / n as f32, wrapped / n as f64
        );
    }
}
