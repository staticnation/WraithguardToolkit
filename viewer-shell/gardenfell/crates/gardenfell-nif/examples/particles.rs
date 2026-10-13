//! Dumps a NIF's particle systems: `cargo run --release --example particles -- file.nif`
fn main() {
    let path = std::env::args().nth(1).expect("a .nif");
    let buf = std::fs::read(&path).unwrap();
    println!("{}", gardenfell_nif::nif::particles_json(&buf));
}
