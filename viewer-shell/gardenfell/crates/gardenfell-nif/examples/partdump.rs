// Every draw part of a NIF: name, alpha flags/threshold, material alpha, textures, tris.
fn main() {
    for p in std::env::args().skip(1) {
        let b = std::fs::read(&p).unwrap();
        let parts = gardenfell_nif::nif::parse_draw(&b).unwrap();
        for d in &parts {
            println!("{:24} alpha 0x{:04x} thr {:?} matA {:.2} emis {:?} tex {} glow {} env {} tris {} vcol {} dm {}",
                d.name, d.alpha_flags, d.alpha_threshold, d.mat_alpha, d.emissive, d.tex, d.glow_tex, d.env_tex, d.idx.len()/3, d.col.len()>0, d.draw_mode);
        }
    }
}
