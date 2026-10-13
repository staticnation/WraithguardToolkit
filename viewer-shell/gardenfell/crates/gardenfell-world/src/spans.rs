//! How tall each grass mesh is, and what that means for the height offset.
//!
//! # Why the engine needs a mesh's extent at all (round 18ek)
//!
//! Round 18eg made the height offset travel with the blade: a reference is written at
//! `ground + L * scale`, where `L` is Groundcover Generator's built-in 16 plus the rule's
//! own offset. The reason is in [`crate::poisson::lift`] — the offset is a distance
//! measured *up the mesh*, so a mesh half the size has half of it — and the effect is
//! that the point of the mesh at local z = `-L` sits exactly on the ground at every size.
//! Call that the **ground line**. Pinning it is what stops a blade rising out of the dirt
//! when it is scaled down, which is what Robin reported of Lush Synthesis's West Gash set.
//!
//! That rests on an assumption: that the ground line falls *inside* the mesh. It does for
//! every ordinary grass mesh, whose origin sits in the middle of its own geometry with
//! roots below and blades above. It does not for a mesh modelled wholly below its origin,
//! and Lush Synthesis has four of them. `luse18`'s highest vertex is 22.7 units *below*
//! its origin; against a lift of 21 the ground line at -21 is above the entire mesh, so
//! there is nothing left over the ground to draw — at any scale, including 1. (Before
//! 18eg it appeared below about scale 0.93 and vanished above it, which was its own kind
//! of wrong, and is why it took a scaling change to notice.)
//!
//! Robin: "Alright, let's floor the scaled lift by the mesh top then."
//!
//! So: **the ground line is not allowed to rise above the mesh.** Where it would, it is
//! put through the middle of the mesh instead, and the blade shows its own top half at
//! every size. Nothing else moves — the test is `z_top < -L`, which no ordinary grass
//! mesh comes close to satisfying, and the floor only ever raises a reference.
//!
//! The span has to be read from the file, so it arrives the way every other per-mesh fact
//! the engine cannot get from a plugin does: a store the caller fills from the overlay,
//! as [`crate::mesh_scatter::Surfaces`] is filled. An empty store floors nothing, which
//! is exactly the behaviour of every build before this one.

use std::collections::HashMap;

/// One mesh's vertical extent in its own space: the lowest drawn vertex and the highest.
pub type Span = [f32; 2];

/// The same case-folding rule as [`crate::mesh_scatter`]'s store, for the same reason:
/// `to_ascii_lowercase` leaves high bytes alone, so a mesh path outside ASCII filed under
/// one spelling is never found under the other.
fn key_of(mesh: &str) -> String {
    if mesh.is_ascii() { mesh.to_ascii_lowercase() } else { mesh.to_lowercase() }
}

/// The drawn extent of the meshes a run's rules name, by mesh path.
#[derive(Default, Clone, Debug)]
pub struct MeshSpans {
    by_mesh: HashMap<String, Span>,
    /// Wanted and unreadable, so "why is that one still sunk" has an answer.
    pub failed: Vec<String>,
}

impl MeshSpans {
    pub fn is_empty(&self) -> bool {
        self.by_mesh.is_empty()
    }
    pub fn len(&self) -> usize {
        self.by_mesh.len()
    }
    /// One mesh's span, or `None` when it was never read - which floors nothing.
    pub fn span(&self, mesh: &str) -> Option<Span> {
        self.by_mesh.get(&key_of(mesh)).copied()
    }
    /// Put a span in by hand. The tests use it; so does a caller that has the geometry
    /// already and would rather not read the file twice.
    pub fn insert(&mut self, mesh: &str, span: Span) {
        self.by_mesh.insert(key_of(mesh), span);
    }
    /// Keep what another store read. Cheap: a span is two floats.
    pub fn take_in(&mut self, other: &MeshSpans) {
        for (k, v) in &other.by_mesh {
            self.by_mesh.entry(k.clone()).or_insert(*v);
        }
    }
    /// Read every mesh named here that is not already in, from the overlay. Returns how
    /// many were read. A mesh with no drawn geometry is a failure, not a zero span: a
    /// zero would floor every lift to nothing.
    pub fn load(&mut self, vfs: &crate::vfs::Vfs, wanted: &[String]) -> usize {
        let todo: Vec<String> = wanted
            .iter()
            .map(|m| key_of(m))
            .filter(|m| !m.is_empty() && !self.by_mesh.contains_key(m))
            .collect();
        if todo.is_empty() {
            return 0;
        }
        let read = crate::pool::par_map(&todo, |_, mesh| {
            let p = vfs.resolve_mesh(mesh)?;
            let buf = vfs.load(&p)?;
            let parts = crate::nif::parse_draw(&buf)?;
            span_of(&parts)
        });
        for (i, s) in read.into_iter().enumerate() {
            match s {
                Some(s) => {
                    self.by_mesh.insert(todo[i].clone(), s);
                }
                None => self.failed.push(todo[i].clone()),
            }
        }
        todo.len()
    }
    /// The lift to use for this mesh: the rule's own, floored so the mesh's top stays
    /// over the ground. See the module note.
    pub fn lift(&self, mesh: &str, z_mod: f32) -> f32 {
        floor_lift(z_mod, self.span(mesh))
    }
}

/// The drawn extent of some parts, or `None` when they draw nothing.
pub fn span_of(parts: &[crate::nif::DrawPart]) -> Option<Span> {
    let (mut lo, mut hi) = (f32::MAX, f32::MIN);
    for d in parts {
        for c in d.pos.chunks(3) {
            if c.len() < 3 || !c[2].is_finite() {
                continue;
            }
            lo = lo.min(c[2]);
            hi = hi.max(c[2]);
        }
    }
    (lo <= hi).then_some([lo, hi])
}

/// The rule's lift, floored by the mesh.
///
/// The ground line sits at mesh-space `-z_mod`. A mesh whose highest vertex is below that
/// line has nothing above the ground at any scale, so the line is moved down to the
/// middle of the mesh - which shows its top half, at every size, and leaves the scale
/// invariance of round 18eg intact because the new line is a mesh-space constant too.
///
/// Every other mesh is left exactly as it was. The floor never lowers a reference: when
/// it fires, `-(z_lo + z_top)/2 >= -z_top > z_mod`.
pub fn floor_lift(z_mod: f32, span: Option<Span>) -> f32 {
    let Some([z_lo, z_top]) = span else { return z_mod };
    if !z_lo.is_finite() || !z_top.is_finite() || z_top >= -z_mod {
        return z_mod;
    }
    -(z_lo + z_top) * 0.5
}
