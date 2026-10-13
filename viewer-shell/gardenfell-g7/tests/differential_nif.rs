//! G7's crate plus the conversion against Robin's own NIF reader: every mesh read both
//! ways must give the same model, block for block.
//!
//! The test fixture's meshes always run; `GARDENFELL_G7_NIFS` adds folders (separated by
//! the platform's path separator), read recursively for `.nif` and `.kf` files, e.g. a
//! game's unpacked meshes or a corpus of mod meshes.

use std::path::{Path, PathBuf};

use gardenfell_g7::meshes::g7_stream;
use gardenfell_nif::netimmerse::stream::Stream;

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect(&p, out);
        } else if p
            .extension()
            .and_then(|x| x.to_str())
            .is_some_and(|x| x.eq_ignore_ascii_case("nif") || x.eq_ignore_ascii_case("kf"))
        {
            out.push(p);
        }
    }
}

/// Equal, or equal when printed (NaN floats are not equal to themselves).
fn same<T: PartialEq + std::fmt::Debug>(a: &T, b: &T) -> bool {
    a == b || format!("{a:?}") == format!("{b:?}")
}

#[test]
fn g7_and_robin_read_every_mesh_the_same() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    collect(&root.join("../ui/tests/fixture"), &mut files);
    if let Some(extra) = std::env::var_os("GARDENFELL_G7_NIFS") {
        for d in std::env::split_paths(&extra) {
            collect(&d, &mut files);
        }
    }
    files.sort();
    let (mut equal, mut fallback, mut g7_only, mut neither) = (0, 0, 0, 0);
    let mut diffs = Vec::new();
    let mut reasons = std::collections::BTreeMap::<String, usize>::new();
    for f in &files {
        let buf = std::fs::read(f).unwrap();
        let robin = Stream::parse(&buf).ok();
        let g7 = g7_stream(&buf);
        match (robin, g7) {
            (Some(r), Some(g)) => {
                if same(&r, &g) {
                    equal += 1;
                    continue;
                }
                let mut why = String::new();
                if r.blocks.len() != g.blocks.len() {
                    why = format!("{} blocks vs {}", r.blocks.len(), g.blocks.len());
                } else if let Some(i) =
                    (0..r.blocks.len()).find(|&i| !same(&r.blocks[i], &g.blocks[i]))
                {
                    let (a, b) = (format!("{:?}", r.blocks[i]), format!("{:?}", g.blocks[i]));
                    let at = a
                        .bytes()
                        .zip(b.bytes())
                        .position(|(x, y)| x != y)
                        .unwrap_or(0);
                    let lo = at.saturating_sub(80);
                    why = format!(
                        "block {i}: robin …{}… | g7 …{}…",
                        &a[lo..(at + 60).min(a.len())],
                        &b[lo..(at + 60).min(b.len())]
                    );
                } else if !same(&r.roots, &g.roots) {
                    why = format!("roots {:?} vs {:?}", r.roots, g.roots);
                } else if r.header != g.header {
                    why = format!("header {:?} vs {:?}", r.header, g.header);
                }
                diffs.push(format!("{}: {why}", f.display()));
            }
            (Some(r), None) => {
                fallback += 1;
                // Why: G7's crate refuses the file, or a block type is not converted.
                let g7_reads =
                    std::panic::catch_unwind(|| tes3::nif::NiStream::from_bytes(&buf).is_ok())
                        .unwrap_or(false);
                let why = if g7_reads {
                    let mut kinds: Vec<String> = r
                        .blocks
                        .iter()
                        .map(|b| format!("{b:?}").split('(').next().unwrap_or("").to_string())
                        .collect();
                    kinds.sort();
                    kinds.dedup();
                    format!("G7 reads it, not converted (types: {})", kinds.join(" "))
                } else {
                    "G7's crate refuses it".to_string()
                };
                *reasons.entry(why).or_insert(0) += 1;
            }
            (None, Some(_)) => g7_only += 1,
            (None, None) => neither += 1,
        }
    }
    eprintln!(
        "{} meshes: {equal} the same, {fallback} read by Robin's reader only (the fallback), \
         {g7_only} by G7's only, {neither} by neither, {} different",
        files.len(),
        diffs.len()
    );
    for (why, n) in &reasons {
        eprintln!("  fallback x{n}: {why}");
    }
    for d in diffs.iter().take(20) {
        eprintln!("  {d}");
    }
    assert!(equal > 0, "no mesh compared");
    assert!(diffs.is_empty(), "{} meshes read differently", diffs.len());
}
