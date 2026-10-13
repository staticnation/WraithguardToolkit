//! Round 17y: every LIGH record in an install, with its flags — to see what separates a
//! firepit from a lantern. `lights "Data Files"`
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let root = std::path::PathBuf::from(&a[1]);
    let vfs = gardenfell_install::vfs::Vfs::with_roots(&[("Data Files".to_string(), root.clone())]);
    let w = gardenfell_world::world::World::load(&[root.join("Morrowind.esm")], &vfs, false);
    let mut v: Vec<_> = w.lights.values().collect();
    v.sort_by(|a, b| a.id.cmp(&b.id));
    for l in v {
        let model = w.models.get(&l.id.to_lowercase()).cloned().unwrap_or_default();
        println!("{:<34} flags {:#05x} r {:>5} {:?} {}", l.id, l.flags, l.radius, l.colour, model);
    }
}
