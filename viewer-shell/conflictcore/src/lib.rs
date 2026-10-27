//! Conflicts, read one way for the viewer's Conflicts page and the Python tools (port step 4.3,
//! `viewer-shell/CONFLICTS-PORT.md`):
//!
//! - [`patch`]: the rules behind patching from a conflict (which plugin starts chosen, what a
//!   typed value starts from, the fields that cannot be typed).
//! - [`records`]: which records two or more plugins of a load order define (the last one
//!   wins), the records only one defines, and the report and CSV of them
//!   (`wraithguard_toolkit.detect_conflicts`, `list_subset_singles`, `list_other_singles`,
//!   `format_conflict_report`, `write_conflict_csv`).
//! - [`resources`]: the loose files two or more data folders provide, whether their bytes
//!   differ, and the report and CSV (`detect_resource_conflicts`, `format_resource_report`,
//!   `write_resource_csv`).
//! - [`fields`]: one conflicting record's fields across its plugins, and what each field
//!   and plugin is doing to it (`diff_record_fields`, `wraithguard.patch.status`).
//! - [`summary`]: the plugin view - a scan as plugin -> type -> records, a record judged for
//!   each plugin, and a plugin's fields taken across its records (`wraithguard.patch.summary`,
//!   `wraithguard.patch.bulk`).
//! - [`map`]: conflicting records gathered onto the world grid (`viz/geometry.py`).
//! - [`mesh`]: what a winning mesh changes against those it overrides
//!   (`analyse_mesh_conflicts`, `wraithguard/nif/report.py`, `analysis.py`).
//! - [`textures`]: the two providers a texture comparison shows and what comparing them
//!   concludes (`wraithguard/images/compare.py`).
//! - [`vfs`]: the folders' files and the byte compare, on a few threads.
//!
//! Written from the Python's behaviour (the reference until it retires) and OpenMW's
//! documented VFS order (a later `data=` folder wins), not from OpenMW's code.

pub mod fields;
pub mod keycache;
pub mod map;
pub mod mesh;
pub mod nif_order;
pub mod nifedit;
pub mod patch;
pub mod records;
pub mod resources;
pub mod summary;
pub mod textures;
pub mod vfs;
