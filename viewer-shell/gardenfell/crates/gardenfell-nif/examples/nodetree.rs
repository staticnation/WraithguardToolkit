//! Every node of a .nif, indented, with its controllers: `nodetree file.nif` (round 17y).
fn main() {
    let path = std::env::args().nth(1).expect("a .nif");
    let buf = std::fs::read(&path).unwrap();
    for line in gardenfell_nif::nif::node_tree(&buf) {
        println!("{line}");
    }
}
