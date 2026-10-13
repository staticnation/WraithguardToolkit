/// What the reader makes of one .nif: every drawable part with the properties the
/// viewport draws it by, and the box it occupies. A debugging tool — round 17h, for
/// three meshes that came out wrong (a banner too low, white flora, opaque water).
fn main() {
    let path = std::env::args().nth(1).expect("a .nif");
    let buf = std::fs::read(&path).unwrap();
    match gardenfell_nif::nif::parse_draw_lit(&buf) {
        None => println!("no geometry"),
        Some((parts, attach)) => {
            println!("{} part(s)  AttachLight {:?}", parts.len(), attach);
            for p in &parts {
                let mut lo = [f32::MAX; 3];
                let mut hi = [f32::MIN; 3];
                for v in p.pos.chunks_exact(3) {
                    for k in 0..3 {
                        lo[k] = lo[k].min(v[k]);
                        hi[k] = hi[k].max(v[k]);
                    }
                }
                println!(
                    "  {:<28} tex {:<40} detail {:<28} verts {:<6} alphaFlags {:#06x} thr {:?} matAlpha {} diffuse {:?} vcol {} mode {} {} draw {} dn {} uv2 {} box [{:.1},{:.1},{:.1}]..[{:.1},{:.1},{:.1}]",
                    p.name, p.tex, if p.detail_tex.is_empty() { "-" } else { &p.detail_tex }, p.pos.len() / 3,
                    p.alpha_flags, p.alpha_threshold, p.mat_alpha, p.diffuse,
                    p.col.len() / 3, p.vcol_mode, if p.unlit { "UNLIT" } else { "lit" }, p.draw_mode, p.day_night, p.uv2.len() / 2, lo[0], lo[1], lo[2], hi[0], hi[1], hi[2]
                );
                if p.emissive != [0.0; 3] {
                    println!("      emissive {:?}", p.emissive);
                }
                if let Some(a) = &p.uv_anim {
                    println!(
                        "      uv freq {} phase {} start {} stop {} u {:?} v {:?} us {:?} vs {:?}",
                        a.frequency, a.phase, a.start, a.stop,
                        a.u_trans, a.v_trans, a.u_scale, a.v_scale
                    );
                }
            }
        }
    }
}
