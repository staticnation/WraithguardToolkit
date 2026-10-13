//! The port against meshoptimizer 1.3's own output (`oracle_values.tsv`, made by
//! `tools/meshopt_oracle/run.sh` from the C++ built from source). Every column must match
//! exactly - the hashes, the counts, and the floats as bit patterns: the remaps are
//! integer work, and the rasterizer is kept operation for operation in `f32`.
//!
//! Each part is checked in the order its file has it (A) and after the vertex cache pass
//! (B), which is Gardenfell's `triorder` here and meshoptimizer's own there; B's hash
//! column checks those two agree before anything is built on them.
//!
//! The corpus run: `MESHOPT_PORT_CORPUS=<nif folder>` with
//! `MESHOPT_PORT_CORPUS_ORACLE=<its .tsv>` (run.sh prints both) checks every mesh there.

mod common;

use common::{fnv, nif_parts, Part};
use gardenfell_nif::triorder::optimize_vertex_cache;
use meshopt_port::*;

/// One order's columns, as `oracle.cpp`'s `order()` prints them.
fn order_cols(idx: &[u32], pos: &[f32], nv: usize) -> Vec<String> {
    let (remap, kept) = optimize_vertex_fetch_remap(idx, nv);
    let ridx = remap_index_buffer(Some(idx), idx.len(), &remap);

    let mut fidx = idx.to_vec();
    let bytes: Vec<u8> = pos.iter().flat_map(|f| f.to_le_bytes()).collect();
    let fpos = optimize_vertex_fetch(&mut fidx, &bytes, nv, 12);
    let fwords: Vec<u32> = (0..fpos.len() / 4).map(|i| u32::from_le_bytes([fpos[i * 4], fpos[i * 4 + 1], fpos[i * 4 + 2], fpos[i * 4 + 3]])).collect();
    assert_eq!(fidx, ridx, "the interleaved fetch rewrites the indices as the remap does");
    let rpos: Vec<[f32; 3]> = remap_vertex_buffer(&(0..pos.len() / 3).map(|i| [pos[i * 3], pos[i * 3 + 1], pos[i * 3 + 2]]).collect::<Vec<_>>(), kept, &remap);
    let rwords: Vec<u32> = rpos.iter().flat_map(|p| p.iter().map(|f| f.to_bits())).collect();
    assert_eq!(rwords, fwords, "remap_vertex_buffer moves the vertices as the interleaved fetch does");

    let vb = analyze_vertex_fetch(idx, nv, 32);
    let va = analyze_vertex_fetch(&ridx, kept, 32);
    let od = analyze_overdraw(idx, pos, nv, 3);
    vec![
        kept.to_string(),
        format!("{:016x}", fnv(&remap)),
        format!("{:016x}", fnv(&ridx)),
        format!("{:016x}", fnv(&fwords)),
        vb.bytes_fetched.to_string(),
        format!("{:08x}", vb.overfetch.to_bits()),
        va.bytes_fetched.to_string(),
        format!("{:08x}", va.overfetch.to_bits()),
        od.pixels_covered.to_string(),
        od.pixels_shaded.to_string(),
        format!("{:08x}", od.overdraw.to_bits()),
    ]
}

/// `oracle.cpp`'s whole line for a part.
fn line(p: &Part) -> Vec<String> {
    let nv = p.vertex_count();
    let mut v = vec![p.name.clone(), nv.to_string(), p.idx.len().to_string()];
    v.extend(order_cols(&p.idx, &p.pos, nv));
    let cv = analyze_coverage(&p.idx, &p.pos, nv, 3);
    for f in [cv.coverage[0], cv.coverage[1], cv.coverage[2], cv.extent] {
        v.push(format!("{:08x}", f.to_bits()));
    }
    let b = optimize_vertex_cache(&p.idx, nv);
    v.push(format!("{:016x}", fnv(&b)));
    v.extend(order_cols(&b, &p.pos, nv));
    v
}

const COLS: [&str; 30] = [
    "name", "vertices", "indices", "A kept", "A remap", "A indices", "A fetch out", "A fetch bytes before", "A overfetch before",
    "A fetch bytes after", "A overfetch after", "A covered", "A shaded", "A overdraw", "coverage x", "coverage y", "coverage z",
    "extent", "B order (triorder vs C++)", "B kept", "B remap", "B indices", "B fetch out", "B fetch bytes before",
    "B overfetch before", "B fetch bytes after", "B overfetch after", "B covered", "B shaded", "B overdraw",
];

fn check(parts: &[Part], tsv: &str) -> usize {
    let want: Vec<Vec<String>> = tsv.lines().filter(|l| !l.is_empty()).map(|l| l.split('\t').map(str::to_string).collect()).collect();
    assert_eq!(parts.len(), want.len(), "a line per part");
    let mut bad = 0;
    for (p, w) in parts.iter().zip(&want) {
        let got = line(p);
        assert_eq!(w.len(), COLS.len(), "{}: the .tsv's columns", p.name);
        for (i, col) in COLS.iter().enumerate() {
            if got[i] != w[i] {
                eprintln!("{}: {col}: port {} C++ {}", p.name, got[i], w[i]);
                bad += 1;
            }
        }
    }
    bad
}

#[test]
fn the_port_answers_as_meshoptimizer_does() {
    let parts = common::all_parts();
    let bad = check(&parts, include_str!("oracle_values.tsv"));
    assert_eq!(bad, 0, "columns that differ from the C++ (above)");
}

#[test]
fn a_corpus_answers_as_meshoptimizer_does() {
    let (Ok(dir), Ok(tsv)) = (std::env::var("MESHOPT_PORT_CORPUS"), std::env::var("MESHOPT_PORT_CORPUS_ORACLE")) else {
        eprintln!("MESHOPT_PORT_CORPUS / MESHOPT_PORT_CORPUS_ORACLE not set: skipped");
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
