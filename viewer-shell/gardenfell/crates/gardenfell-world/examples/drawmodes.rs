//! Round 17w: across an archive, how many shapes say what about culling and lighting.
//! `drawmodes Morrowind.bsa`
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let bsa = gardenfell_bsa::bsa::Bsa::open(std::path::Path::new(&a[1])).unwrap();
    let mut by_mode = [0usize; 4];
    let mut unlit = 0usize;
    let mut vmode = [0usize; 3];
    let mut shapes = 0usize;
    let mut ex_both = Vec::new();
    let mut ex_unlit = Vec::new();
    let mut ex_cw = Vec::new();
    for n in bsa.names() {
        if !n.to_ascii_lowercase().ends_with(".nif") { continue; }
        let Some(buf) = bsa.get(&gardenfell_install::vfs::norm(n)) else { continue };
        let Some((parts, _)) = gardenfell_nif::nif::parse_draw_lit(buf) else { continue };
        for p in &parts {
            shapes += 1;
            by_mode[(p.draw_mode & 3) as usize] += 1;
            vmode[(p.vcol_mode.min(2)) as usize] += 1;
            if p.unlit { unlit += 1; if ex_unlit.len() < 8 { ex_unlit.push(format!("{} / {}", n, p.name)); } }
            if p.draw_mode == 3 && ex_both.len() < 8 { ex_both.push(format!("{} / {}", n, p.name)); }
            if p.draw_mode == 2 && ex_cw.len() < 8 { ex_cw.push(format!("{} / {}", n, p.name)); }
        }
    }
    println!("{} shapes: draw mode none {} ccw {} cw {} both {}; unlit {}; vcol mode ignore {} emissive {} multiply {}",
        shapes, by_mode[0], by_mode[1], by_mode[2], by_mode[3], unlit, vmode[0], vmode[1], vmode[2]);
    println!("both-sided e.g. {:?}", ex_both);
    println!("clockwise e.g. {:?}", ex_cw);
    println!("unlit e.g. {:?}", ex_unlit);
}
