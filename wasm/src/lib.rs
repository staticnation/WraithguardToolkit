//! A bridge from `tes3`'s NIF reader to the 3D viewer page.
//!
//! **Status: written here, never compiled.** The environment this was authored
//! in has no Rust toolchain and no way to install one, so nothing below has
//! been built or run. Everything else in this repository is verified against
//! something; this is not.
//!
//! # Why this is small on purpose
//!
//! A fuller version was written first — world-space transform composition,
//! triangle flattening, texture path extraction — and then deleted. Not
//! because it was hard, but because it guessed at `tes3`'s API in three places
//! and was **wrong in two of them** (`shape.base.base.name` when `Deref` makes
//! it `shape.name`; `property.base_map` when the field is
//! `texture_maps: Vec<Option<TextureMap>>`). Both were caught by reading the
//! source. A third, `NiLink::from_index`, does not appear to exist at all.
//!
//! Guessing at an API that cannot be compiled produces confident-looking code
//! whose errors are invisible until someone else finds them. So this exposes
//! **only what has been verified against `tes3`'s source**:
//!
//! * `NiStream::from_bytes(&[u8]) -> io::Result<Self>`
//! * `stream.objects_of_type::<T>()`
//! * `stream.get_as::<_, U>(link) -> Option<&U>`
//! * field access through the `Deref` that the `Meta` derive emits
//!
//! # What it is for
//!
//! Every number here is one `wraithguard.nif` also produces, which makes the
//! first thing to build with this a **differential check rather than a
//! feature**. If the two readers agree on object and triangle counts across
//! the corpus, the bridge is trustworthy and geometry extraction is worth
//! writing — by someone who can run `cargo build` and let the compiler answer
//! the questions this file had to guess at.
//!
//! See README.md in this folder for that plan and for the toolchain.
//!
//! # Licensing
//!
//! `tes3` is MIT (repository root `LICENSE`). See `NIF_PROVENANCE.md` for the
//! boundary and the date. The compiled `.wasm` and its JS shim are build
//! artifacts of an MIT library rather than a vendored copy of it — a simpler
//! attribution question than three.js.

use nif::{NiStream, NiTriShape, NiTriShapeData};
use wasm_bindgen::prelude::*;

/// One shape, reduced to what a cross-check needs.
#[wasm_bindgen]
pub struct Shape {
    name: String,
    vertices: u32,
    triangles: u32,
}

#[wasm_bindgen]
impl Shape {
    /// The shape's name as the file records it.
    #[wasm_bindgen(getter)]
    pub fn name(&self) -> String {
        self.name.clone()
    }

    /// How many vertices its geometry data holds.
    #[wasm_bindgen(getter)]
    pub fn vertices(&self) -> u32 {
        self.vertices
    }

    /// How many triangles its geometry data holds.
    #[wasm_bindgen(getter)]
    pub fn triangles(&self) -> u32 {
        self.triangles
    }
}

/// A parsed NIF, held so the page can ask it questions.
#[wasm_bindgen]
pub struct Parsed {
    objects: usize,
    shapes: Vec<Shape>,
}

#[wasm_bindgen]
impl Parsed {
    /// How many objects the stream holds.
    ///
    /// Comparable with `len(NifFile.blocks)` on the Python side, which is the
    /// cheapest cross-check available and the reason this is exposed.
    #[wasm_bindgen(getter)]
    pub fn objects(&self) -> usize {
        self.objects
    }

    /// How many shapes were found.
    #[wasm_bindgen(getter)]
    pub fn shape_count(&self) -> usize {
        self.shapes.len()
    }

    /// One shape by position, or `None` past the end.
    ///
    /// Not a panic: a panic in a wasm module aborts the whole instance, and in
    /// a viewer that means a blank pane with nothing useful in the console.
    pub fn shape(&self, index: usize) -> Option<Shape> {
        self.shapes.get(index).map(|s| Shape {
            name: s.name.clone(),
            vertices: s.vertices,
            triangles: s.triangles,
        })
    }
}

/// Parse a NIF from raw bytes.
///
/// # Errors
///
/// Returns the reader's own message as a JS string when the file is not a NIF
/// this library can read. Deliberately not a panic: a malformed mesh is an
/// ordinary thing to find in a mod collection and must be reportable, exactly
/// as it is on the Python side.
#[wasm_bindgen]
pub fn parse_nif(bytes: &[u8]) -> Result<Parsed, JsValue> {
    let stream = NiStream::from_bytes(bytes).map_err(|e| JsValue::from_str(&e.to_string()))?;

    let mut shapes = Vec::new();
    for shape in stream.objects_of_type::<NiTriShape>() {
        // `name` and `geometry_data` are reached through auto-deref, not the
        // `base` chain: tes3's `Meta` derive emits a `Deref` to `base` on every
        // type, so `shape.name` walks NiTriShape -> NiTriBasedGeom ->
        // NiGeometry -> NiAVObject -> NiObjectNET by itself. Spelling the chain
        // out by hand is both wrong-by-two-levels and unnecessary, which is how
        // this was written the first time.
        let (vertices, triangles) = stream
            .get_as::<_, NiTriShapeData>(shape.geometry_data)
            .map_or((0, 0), |data| {
                (
                    u32::try_from(data.vertices.len()).unwrap_or(u32::MAX),
                    u32::try_from(data.triangles.len()).unwrap_or(u32::MAX),
                )
            });
        shapes.push(Shape {
            name: shape.name.to_string(),
            vertices,
            triangles,
        });
    }

    Ok(Parsed {
        objects: stream.objects.len(),
        shapes,
    })
}

/// The version this module was built from, for the page to display.
///
/// A viewer that silently loads a stale `.wasm` is a debugging trap; showing
/// the version makes "did my rebuild take" a question with an answer.
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
