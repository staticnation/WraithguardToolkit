/// Round 17q: the node chain down to AttachLight, translation by translation.
/// If the walk is right, these add up to a flame inside the lamp.
fn main() {
    let p = std::env::var("GARDENFELL_BSA").expect("GARDENFELL_BSA");
    let which = std::env::args().nth(1).expect("a mesh path inside the archive");
    let bsa = gardenfell_bsa::bsa::Bsa::open(std::path::Path::new(&p)).unwrap();
    let buf = bsa.get(&gardenfell_install::vfs::norm(&which)).expect("no such mesh");
    for line in gardenfell_nif::nif::attach_chain(buf) {
        println!("{line}");
    }
}
