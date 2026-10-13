//! One track's rotation keys of a `.kf`, as the direction the node's local +z takes:
//! `kfkeys file.kf "Root Bone"`.
fn main() {
    let kfp = std::env::args().nth(1).expect("a .kf");
    let node = std::env::args().nth(2).unwrap_or_default();
    let kf = gardenfell_nif::nif::parse_kf(&std::fs::read(&kfp).unwrap()).expect("not a kf");
    for t in &kf.tracks {
        if !node.is_empty() && t.node != node { continue; }
        println!("{}:", t.node);
        for (time, q) in &t.data.rot {
            let (w, x, y, z) = (q[0], q[1], q[2], q[3]);
            let up = [2.0 * (x * z + w * y), 2.0 * (y * z - w * x), 1.0 - 2.0 * (x * x + y * y)];
            let fwd = [2.0 * (x * y - w * z), 1.0 - 2.0 * (x * x + z * z), 2.0 * (y * z + w * x)];
            println!("  {time:6.3}  z→[{:.2},{:.2},{:.2}]  y→[{:.2},{:.2},{:.2}]", up[0], up[1], up[2], fwd[0], fwd[1], fwd[2]);
        }
        for (time, v) in &t.data.trans { println!("  {time:6.3}  at [{:.1},{:.1},{:.1}]", v[0], v[1], v[2]); }
    }
}
