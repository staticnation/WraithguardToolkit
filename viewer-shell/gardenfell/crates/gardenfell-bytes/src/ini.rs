//! Reading the game's and the engines' `.ini` files: one section as key/value pairs, a
//! lookup, a number with a fallback. Round 18ge: moved out of `weather.rs`, which the
//! renderer profile reached into for them.

/// One ini section as key → value, keys lowercased.
pub fn section<'a>(text: &'a str, name: &str) -> Vec<(String, &'a str)> {
    let mut out = Vec::new();
    let mut inside = false;
    let want = name.to_ascii_lowercase();
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            inside = t.trim_matches(|c| c == '[' || c == ']').trim().eq_ignore_ascii_case(&want);
            continue;
        }
        if !inside || t.is_empty() || t.starts_with(';') {
            continue;
        }
        if let Some(eq) = t.find('=') {
            out.push((t[..eq].trim().to_ascii_lowercase(), t[eq + 1..].trim()));
        }
    }
    let _ = want;
    out
}

pub fn lookup<'a>(sec: &[(String, &'a str)], key: &str) -> Option<&'a str> {
    let k = key.to_ascii_lowercase();
    sec.iter().find(|(kk, _)| *kk == k).map(|(_, v)| *v)
}

pub fn num(sec: &[(String, &str)], key: &str, fallback: f32) -> f32 {
    lookup(sec, key).and_then(|v| v.trim().parse::<f32>().ok()).unwrap_or(fallback)
}
