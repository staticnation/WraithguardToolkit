//! The simplifier against meshoptimizer 1.3's own output (`simplify_values.tsv`, made by
//! `tools/meshopt_oracle/run.sh`: `oracle_simplify.cpp`, column for column with `line`
//! below). Every count, hash and error bit pattern must match.
//!
//! The corpus run: `MESHOPT_PORT_CORPUS=<nif folder>` with
//! `MESHOPT_PORT_CORPUS_SIMPLIFY=<its .tsv>` (run.sh prints both).

mod common;

use common::{fnv, nif_parts, Part};
use meshopt_port::simplify::*;

fn out(v: &mut Vec<String>, r: &[u32], err: f32) {
    v.push(r.len().to_string());
    v.push(format!("{:016x}", fnv(r)));
    v.push(format!("{:08x}", err.to_bits()));
}

/// `oracle_simplify.cpp`'s whole line for a part.
fn line(p: &Part) -> Vec<String> {
    let nv = p.vertex_count();
    let ni = p.idx.len();
    let pos = &p.pos;

    // two attributes per vertex, discontinuous where positions are shared; as the C++
    let mut attr = vec![0.0f32; nv * 2];
    let mut lock = vec![0u8; nv];
    for i in 0..nv {
        attr[i * 2] = pos[i * 3] * 0.5f32 + pos[i * 3 + 1];
        attr[i * 2 + 1] = (i % 5) as f32 * 0.25f32;
        lock[i] = (if i % 7 == 0 { 1 } else { 0 }) | (if i % 11 == 0 { 4 } else { 0 }) | (if i % 13 == 0 { 2 } else { 0 });
    }
    let weights = [1.0f32, 0.5];

    let target = (ni / 3 / 4) * 3;
    let mut v = vec![p.name.clone(), nv.to_string(), ni.to_string()];

    for o in [0, LOCK_BORDER | PRUNE, REGULARIZE | PRESERVE_FOLDS | ERROR_ABSOLUTE, REGULARIZE_LIGHT | SPARSE] {
        let mut err = 0.0f32;
        let te = if o & ERROR_ABSOLUTE != 0 { 1e-1f32 } else { 1e-2f32 };
        let r = simplify(&p.idx, pos, nv, 3, target, te, o, Some(&mut err));
        out(&mut v, &r, err);
    }

    for o in [0, PERMISSIVE | ERROR_CLAMPED] {
        let mut err = 0.0f32;
        let r = simplify_with_attributes(&p.idx, pos, nv, 3, &attr, 2, &weights, 2, Some(&lock), target, 1e-2, o, Some(&mut err));
        out(&mut v, &r, err);
    }

    {
        let mut uidx = p.idx.clone();
        let mut upos = pos.clone();
        let mut uattr = attr.clone();
        let mut err = 0.0f32;
        let n = simplify_with_update(&mut uidx, &mut upos, nv, 3, Some(&mut uattr), 2, &weights, 2, Some(&lock), target, 1e-2, 0, Some(&mut err));
        out(&mut v, &uidx[..n], err);
        let pw: Vec<u32> = upos.iter().map(|f| f.to_bits()).collect();
        let aw: Vec<u32> = uattr.iter().map(|f| f.to_bits()).collect();
        v.push(format!("{:016x}", fnv(&pw)));
        v.push(format!("{:016x}", fnv(&aw)));
    }

    {
        let mut err = 0.0f32;
        let r = simplify_sloppy(&p.idx, pos, nv, 3, None, target, 2e-2, Some(&mut err));
        out(&mut v, &r, err);
        let r = simplify_sloppy(&p.idx, pos, nv, 3, Some(&lock), target, 0.0, Some(&mut err));
        out(&mut v, &r, err);
    }

    {
        let r = simplify_prune(&p.idx, pos, nv, 3, 5e-2);
        out(&mut v, &r, 0.0);
    }

    {
        // colours: the positions themselves (the attributes are two floats, colours three)
        let r = simplify_points(pos, nv, 3, Some(pos), 3, 1.0, nv / 4);
        out(&mut v, &r, 0.0);
    }

    v.push(format!("{:08x}", simplify_scale(pos, nv, 3).to_bits()));
    v
}

const COLS: [&str; 35] = [
    "name", "vertices", "indices",
    "simplify count", "simplify hash", "simplify error",
    "lock border+prune count", "lock border+prune hash", "lock border+prune error",
    "regularize+folds+absolute count", "regularize+folds+absolute hash", "regularize+folds+absolute error",
    "regularize light+sparse count", "regularize light+sparse hash", "regularize light+sparse error",
    "attributes count", "attributes hash", "attributes error",
    "attributes permissive+clamped count", "attributes permissive+clamped hash", "attributes permissive+clamped error",
    "update count", "update hash", "update error", "update positions", "update attributes",
    "sloppy count", "sloppy hash", "sloppy error",
    "sloppy locked count", "sloppy locked hash", "sloppy locked error",
    "prune count", "prune hash", "prune error",
];

fn check(parts: &[Part], tsv: &str) -> usize {
    let want: Vec<Vec<String>> = tsv.lines().filter(|l| !l.is_empty()).map(|l| l.split('\t').map(str::to_string).collect()).collect();
    assert_eq!(parts.len(), want.len(), "a line per part");
    let mut bad = 0;
    for (p, w) in parts.iter().zip(&want) {
        let got = line(p);
        assert_eq!(got.len(), w.len(), "{}: the .tsv's columns", p.name);
        for i in 0..got.len() {
            if got[i] != w[i] {
                let col = COLS.get(i).copied().unwrap_or(if i + 1 == got.len() { "scale" } else { "points" });
                eprintln!("{}: {col}: port {} C++ {}", p.name, got[i], w[i]);
                bad += 1;
            }
        }
    }
    bad
}

#[test]
fn the_simplifier_answers_as_meshoptimizer_does() {
    let parts = common::all_parts();
    let bad = check(&parts, include_str!("simplify_values.tsv"));
    assert_eq!(bad, 0, "columns that differ from the C++ (above)");
}

#[test]
fn a_corpus_simplifies_as_meshoptimizer_does() {
    let (Ok(dir), Ok(tsv)) = (std::env::var("MESHOPT_PORT_CORPUS"), std::env::var("MESHOPT_PORT_CORPUS_SIMPLIFY")) else {
        eprintln!("MESHOPT_PORT_CORPUS / MESHOPT_PORT_CORPUS_SIMPLIFY not set: skipped");
        return;
    };
    let root = std::path::Path::new(&dir);
    let mut files = Vec::new();
    fn walk(d: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        let Ok(rd) = std::fs::read_dir(d) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("nif")) {
                out.push(p);
            }
        }
    }
    walk(root, &mut files);
    files.sort();
    let mut parts = Vec::new();
    for f in files {
        let rel = f.strip_prefix(root).unwrap_or(&f).to_string_lossy().replace('\\', "/");
        if let Ok(b) = std::fs::read(&f) {
            parts.extend(nif_parts(&b, &rel));
        }
    }
    let bad = check(&parts, &std::fs::read_to_string(tsv).unwrap());
    eprintln!("{} parts", parts.len());
    assert_eq!(bad, 0, "columns that differ from the C++ (above)");
}
