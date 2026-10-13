fn main() {
    let p = std::env::args().nth(1).unwrap();
    let b = std::fs::read(&p).unwrap();
    for part in gardenfell_nif::nif::parse_draw(&b).unwrap() {
        match &part.flip {
            Some(f) => println!(
                "{}: slot {} start {} delta {} freq {} window {}..{} frames {} [{} .. {}]",
                part.name, f.slot, f.tex_start, f.delta, f.frequency, f.start, f.stop,
                f.frames.len(), f.frames.first().cloned().unwrap_or_default(),
                f.frames.last().cloned().unwrap_or_default()
            ),
            None => println!("{}: no flip (tex {})", part.name, part.tex),
        }
    }
}
