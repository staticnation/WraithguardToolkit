//! meshoptimizer's algorithms in Rust, std only, ported line for line from meshoptimizer
//! 1.3 (Arseny Kapoulkine, MIT; https://github.com/zeux/meshoptimizer).
//!
//! The vertex cache and overdraw passes are not here: Gardenfell's `triorder` (Robin
//! Hjelte, gardenfell-nif) is the port of those, and the viewer runs it on every mesh it
//! loads. This crate is what sits beside it:
//!
//! - [`vfetch`]: the vertex fetch order (`meshopt_optimizeVertexFetchRemap`,
//!   `meshopt_optimizeVertexFetch`) and the remaps that apply it to a mesh's index list
//!   and to each of its per-vertex arrays (`meshopt_remapIndexBuffer`,
//!   `meshopt_remapVertexBuffer`).
//! - [`analyze`]: the vertex fetch, overdraw and coverage analysers
//!   (`meshopt_analyzeVertexFetch`, `meshopt_analyzeOverdraw`, `meshopt_analyzeCoverage`).
//! - [`simplify`]: the simplifier (`meshopt_simplify`, `...WithAttributes`,
//!   `...WithUpdate`, `...Sloppy`, `...Prune`, `...Points`, `...Scale`).
//!
//! Each function keeps the C++'s structure and names in snake case, so a line here can be
//! read against its line there; where the C++ asserts, these return early or document the
//! precondition. Checked against the C++'s own output: `tests/oracle.rs`, values made by
//! `tools/meshopt_oracle` from meshoptimizer 1.3 built from source.

pub mod analyze;
pub mod simplify;
pub mod vfetch;

pub use analyze::{analyze_coverage, analyze_overdraw, analyze_vertex_fetch, CoverageStatistics, OverdrawStatistics, VertexFetchStatistics};
pub use vfetch::{optimize_vertex_fetch, optimize_vertex_fetch_remap, remap_index_buffer, remap_vertex_buffer};
