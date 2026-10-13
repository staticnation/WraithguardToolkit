//! Whether an id is an actor or a model in the base game: `whatis <Data Files> <id>`.
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let root = std::path::PathBuf::from(&a[1]);
    let mut vfs = gardenfell_install::vfs::Vfs::with_roots(&[("data".to_string(), root.clone())]);
    // The archive too: the `x` twins the game swaps in (round 18ag) live in Morrowind.bsa.
    vfs.add_archives(&[root.join("Morrowind.bsa")]);
    let w = gardenfell_world::world::World::load(&[root.join("Morrowind.esm")], &vfs, false);
    let k = a[2].to_lowercase();
    println!("actor: {:?}", w.actors.get(&k).map(|x| (x.model.clone(), x.creature, x.health, x.persistent)));
    println!("model: {:?}", w.models.get(&k));
}
