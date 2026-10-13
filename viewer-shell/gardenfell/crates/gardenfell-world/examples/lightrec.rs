//! LIGH records by id substring: `lightrec <Data Files> <substring>` — radius, colour, flags.
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let root = std::path::PathBuf::from(&a[1]);
    let pat = a[2].to_lowercase();
    let vfs = gardenfell_install::vfs::Vfs::with_roots(&[("data".to_string(), root.clone())]);
    let w = gardenfell_world::world::World::load(&[root.join("Morrowind.esm")], &vfs, false);
    let mut ids: Vec<&String> = w.lights.keys().collect();
    ids.sort();
    for id in ids {
        if !id.to_lowercase().contains(&pat) { continue; }
        let l = &w.lights[id];
        println!("{:<32} radius {:>5} colour {:?} flags {:#06x}{}{}", id, l.radius, l.colour, l.flags,
            if l.negative() { " NEGATIVE" } else { "" }, if l.off_default() { " OFF" } else { "" });
    }
}
