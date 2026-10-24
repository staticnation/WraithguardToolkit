//! What a setup loads, read once and the same way for the viewer's engine (viewcore) and
//! the Python backend (native/): so the Editor and the viewer cannot disagree.
//!
//! - [`cfg`]: an OpenMW `openmw.cfg` - its data folders, content and groundcover plugins,
//!   archives and chained cfgs, by the rules OpenMW documents.
//! - [`records`]: which version of a record wins, and what type a placed object is.
//! - [`refs`]: a placed object's identity, and which of its versions stands.
//! - [`patterns`]: a rule's plugin-name patterns (`*`, `?`, `<VER>`) and mlox's comparable
//!   version strings.
//! - [`predicates`]: the `[Requires]`/`[Conflict]`/`[Patch]`/`[Note]` rules - their expression
//!   language, and the warnings that fire for a sorted load order.
//! - [`pyre`]: the part of Python's `re` the rule code uses, matched by hand.
//! - [`updates`]: the downloads - rule files, `plugin-order.yml`, a list's data paths - with the
//!   network left to the caller.
//! - [`export`]: writing `openmw.cfg` and `customizations.toml`.
//! - [`order`]: where the sort puts each custom plugin among the curated ones - positions from
//!   the graph, the `[NearStart]`/`[NearEnd]` hints, and the topological placement.
//! - [`rules`]: the ordering blocks of an mlox rule file's text.
//! - [`graph`]: the sort's graph - which plugins are masters, the cycle check, and the edges
//!   built from the curated order, the customs' header masters and the rules.
//! - [`order`]: the sort's order - where each custom is put among the curated plugins (the
//!   anchors, the `[NearStart]`/`[NearEnd]` hints, the topological placement).
//! - [`cells`]: a cell named one way (an exterior by grid, an interior by its name
//!   without case), whichever spelling it came in.
//!
//! Written from OpenMW's documentation ("Paths", openmw.readthedocs.io) and its observed
//! behaviour, not its code: OpenMW is GPL-3 and this crate is MIT.

pub mod cells;
pub mod cfg;
pub mod export;
pub mod graph;
pub mod order;
pub mod patterns;
pub mod predicates;
pub mod pyre;
pub mod records;
pub mod refs;
pub mod rules;
pub mod updates;
