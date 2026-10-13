//! How much each clip of a `.kf` moves: `kfmotion file.kf [more.kf ...]`. Per clip, the
//! largest angle any track turns away from where it stands at the clip's start, in
//! degrees, and the clip the reader would pick as the idle.
fn quat_angle(a: [f32; 4], b: [f32; 4]) -> f32 {
    let d = (a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3]).abs().min(1.0);
    2.0 * d.acos().to_degrees()
}
fn main() {
    for kfp in std::env::args().skip(1) {
        let Some(kf) = gardenfell_nif::nif::parse_kf(&std::fs::read(&kfp).unwrap()) else { println!("{kfp}: not a kf"); continue };
        let name = std::path::Path::new(&kfp).file_name().unwrap().to_string_lossy().to_string();
        let mut clips: Vec<(String, f32, f32)> = Vec::new();
        if let Some((a, b)) = kf.clip("Idle") { clips.push(("Idle".into(), a, b)); }
        for n in 2..=9 { if let Some((a, b)) = kf.clip(&format!("Idle{n}")) { clips.push((format!("Idle{n}"), a, b)); } }
        let chosen = kf.idle();
        let mut line = format!("{name:<32}");
        for (c, a, b) in &clips {
            let mut worst = 0f32;
            for t in &kf.tracks {
                let keys: Vec<&(f32, [f32; 4])> = t.data.rot.iter().filter(|(k, _)| *k >= a - 1e-4 && *k <= b + 1e-4).collect();
                if let Some(first) = keys.first() {
                    for k in &keys { worst = worst.max(quat_angle(first.1, k.1)); }
                }
            }
            let mark = if chosen == Some((*a, *b)) { "*" } else { " " };
            line.push_str(&format!("  {c}{mark} {:.1}..{:.1}s {worst:5.1}°", a, b));
        }
        println!("{line}");
    }
}
