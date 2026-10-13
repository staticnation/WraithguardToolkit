//! Pull files out of a BSA: `bsaget Morrowind.bsa <substring> [outdir]` — lists the names
//! that contain the substring and, given an outdir, writes each of them there.
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let bsa = gardenfell_bsa::bsa::Bsa::open(std::path::Path::new(&a[1])).unwrap();
    let pat = a.get(2).map(|s| s.to_lowercase()).unwrap_or_default();
    for n in bsa.names() {
        if !n.to_lowercase().contains(&pat) { continue; }
        println!("{}", n);
        if let Some(out) = a.get(3) {
            let data = bsa.get(&gardenfell_install::vfs::norm(n)).unwrap();
            let p = std::path::Path::new(out).join(n.replace('\\', "/"));
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, data).unwrap();
        }
    }
}
