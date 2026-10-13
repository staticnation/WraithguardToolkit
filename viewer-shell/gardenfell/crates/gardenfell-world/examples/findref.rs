//! Where a reference id is placed: `findref <Data Files> <id substring>` — every cell
//! (exterior grid or interior name) and the position, rotation and scale of each match.
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let root = std::path::PathBuf::from(&a[1]);
    let pat = a[2].to_lowercase();
    let vfs = gardenfell_install::vfs::Vfs::with_roots(&[("data".to_string(), root.clone())]);
    let esm = root.join("Morrowind.esm");
    let w = gardenfell_world::world::World::load(&[esm], &vfs, false);
    for (g, c) in &w.cells {
        for r in c.refs.values() {
            if r.id.to_lowercase().contains(&pat) {
                println!("({},{}) {:<24} {:<28} pos {:?} rot {:?} scale {:?}", g.0, g.1, c.name, r.id, r.pos, r.rot, r.scale);
            }
        }
    }
    for (n, i) in &w.interiors {
        for r in i.refs.values() {
            if r.id.to_lowercase().contains(&pat) {
                println!("int {:<30} {:<28} pos {:?} rot {:?} scale {:?}", n, r.id, r.pos, r.rot, r.scale);
            }
        }
    }
}
