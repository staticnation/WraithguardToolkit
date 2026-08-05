#!/usr/bin/env python3
r"""Regenerate the function-by-function coverage table for the Merged Lands port.

**Why this is generated rather than written.** The first version of
``MERGED_LANDS_FUNCTIONS.md`` was written by hand, grouped related Rust
functions onto one table row, and then reported the *row* count as the function
count -- so ``land/`` was labelled 37 functions when it has 64, and forty
functions were covered only by a group heading that never named them. A table
that claims completeness has to be able to prove it.

So the list of functions comes from parsing ``merged_lands-main/src`` and the
status of each comes from :data:`COVERAGE`, keyed by ``file::name`` (plus the
enclosing ``impl``/``trait`` where a name repeats). Any function in the source
with no entry is an error, and any entry with no function is an error: the
script fails rather than emitting a table with a hole in it.

Usage::

    python tools/gen_merged_lands_table.py --src ../merged_lands-main/src \\
        --out MERGED_LANDS_FUNCTIONS.md

Merged Lands is MIT (David Von Derau, 2022). See ``CREDITS.md``.
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path
from typing import Final, NamedTuple

#: Matches an ``impl`` header, capturing the type or trait implementation.
_IMPL: Final = re.compile(
    r"(?:pub(?:\([^)]*\))?\s+)?(?:unsafe\s+)?impl(?:<[^>]*>)?\s+(.+?)\s*\{?\s*$"
)

#: Matches a ``trait`` header.
_TRAIT: Final = re.compile(r"(?:pub(?:\([^)]*\))?\s+)?trait\s+(\w+)")

#: Matches a function definition, including trait method declarations.
_FN: Final = re.compile(r"(?:pub(?:\([^)]*\))?\s+)?(?:const\s+)?(?:async\s+)?fn\s+(\w+)")


class Function(NamedTuple):
    """One Rust function found in the source.

    Attributes:
        file: Path relative to ``src``.
        line: Line number, for anyone who wants to go and read it.
        context: The enclosing ``impl`` or ``trait``, or ``""`` for a free
            function. Needed because names such as ``new`` and ``average``
            repeat across types.
        name: The function's name.
    """

    file: str
    line: int
    context: str
    name: str

    @property
    def key(self) -> str:
        """The key used to look this function up in :data:`COVERAGE`."""
        return f"{self.file}::{self.context}::{self.name}"

    @property
    def label(self) -> str:
        """How the function is written in the table."""
        if self.context:
            return f"`{self.context}::{self.name}`"
        return f"`{self.name}`"


#: Status values, and what each one claims.
STATUS: Final[dict[str, str]] = {
    "ported": "the behaviour exists here",
    "verified": "ported, and checked statement by statement against the Rust",
    "gap": "ported only after this audit found it missing",
    "absorbed": "Rust scaffolding Python does not need",
    "replaced": "done differently and deliberately, with the reason given",
    "n/a": "belongs to a concern outside the merge",
}

#: Every function in ``merged_lands-main/src``, with its status and where its
#: behaviour lives here. Keyed by ``file::context::name``.
#:
#: This is the whole point of the file: 191 entries, one per function, no
#: grouping. If a key is missing the generator fails.
COVERAGE: Final[dict[str, tuple[str, str]]] = {
    # ---------------------------------------------------------------- io/
    "io/meta_schema.rs::Default for MergeSettings::default": (
        "ported",
        "`meta.MergeSettings` field defaults: `included=True`, `Auto`",
    ),
    "io/meta_schema.rs::Default for MergeSettings::default_bool_true": (
        "absorbed",
        "`included: bool = True` on the dataclass",
    ),
    "io/meta_schema.rs::Default for MergeSettings::skip_default": (
        "replaced",
        "serde's write-skip. We never write a settings file, only the "
        "`MergedLands` marker -- see `meta.write_merged_marker`",
    ),
    "io/parsed_plugins.rs::::parse_records": (
        "replaced",
        "`service._records_via`: tes3conv JSON, not a TES3 reader",
    ),
    "io/parsed_plugins.rs::::read_lines": (
        "ported",
        "`tools.survey_landscape.read_order`, which refuses a file with no "
        "plugin entries rather than returning every line",
    ),
    "io/parsed_plugins.rs::::is_esm": (
        "verified",
        "`service._split_order`. Case-insensitive extension test, plus " "`.omwgame`",
    ),
    "io/parsed_plugins.rs::::is_esp": (
        "verified",
        "`cleaning.is_mod`. Case-insensitive, plus `.omwaddon`",
    ),
    "io/parsed_plugins.rs::::sort_plugins": (
        "replaced",
        "load order comes from the toolkit's own sort, not file mtimes",
    ),
    "io/parsed_plugins.rs::::meta_name": ("ported", "`meta.meta_path_for`"),
    "io/parsed_plugins.rs::ParsedPlugin::empty": (
        "absorbed",
        "`PluginRecords(name, [])`",
    ),
    "io/parsed_plugins.rs::ParsedPlugin::from": ("absorbed", "dataclass constructor"),
    "io/parsed_plugins.rs::PartialEq<Self> for ParsedPlugin::eq": (
        "absorbed",
        "dataclass equality",
    ),
    "io/parsed_plugins.rs::Hash for ParsedPlugin::hash": (
        "absorbed",
        "plugins are keyed by their name string",
    ),
    "io/parsed_plugins.rs::Hash for ParsedPlugin::read_ini_file": (
        "ported",
        "`read_order` also parses `Morrowind.ini` `GameFile` lines",
    ),
    "io/parsed_plugins.rs::ParsedPlugins::check_dir_exists": (
        "ported",
        "`service.build_merged_lands` refuses an empty folder list; "
        "`service.resolve_plugin` skips a folder it cannot read",
    ),
    "io/parsed_plugins.rs::ParsedPlugins::new": (
        "ported",
        "`service.build_merged_lands` reading phase, over every `data=` folder",
    ),
    "io/save_to_image.rs::::save_resized_image": (
        "ported",
        "`conflict_image._blit`, scaling by `SCALE = 4`",
    ),
    "io/save_to_image.rs::GridAccessor2D<P> for ImageBuffer<P, Container>::get": (
        "absorbed",
        "list indexing",
    ),
    "io/save_to_image.rs::GridAccessor2D<P> for ImageBuffer<P, Container>::get_mut": (
        "absorbed",
        "list assignment",
    ),
    "io/save_to_image.rs::trait SaveToImage::save_to_image": (
        "ported",
        "`conflict_image.cell_conflict_image`",
    ),
    "io/save_to_image.rs::SaveToImage for RelativeTerrainMap<Vec3<i8>, T>::save_to_image": (
        "ported",
        "same, vertex normals layer",
    ),
    "io/save_to_image.rs::SaveToImage for RelativeTerrainMap<u16, T>::save_to_image": (
        "ported",
        "same, texture indices layer",
    ),
    "io/save_to_image.rs::SaveToImage for RelativeTerrainMap<Vec3<u8>, T>::save_to_image": (
        "ported",
        "same, vertex colours layer",
    ),
    "io/save_to_image.rs::SaveToImage for RelativeTerrainMap<Vec3<u8>, T>::calculate_min_max": (
        "ported",
        "range scan inside `cell_conflict_image`",
    ),
    "io/save_to_image.rs::SaveToImage for RelativeTerrainMap<u8, T>::save_to_image": (
        "ported",
        "same, world map layer",
    ),
    "io/save_to_image.rs::SaveToImage for RelativeTerrainMap<i32, T>::save_to_image": (
        "ported",
        "same, height map layer",
    ),
    "io/save_to_image.rs::SaveToImage for RelativeTerrainMap<i32, T>::save_image": (
        "ported",
        "`wraithguard.images.png` writer -- no image crate needed",
    ),
    "io/save_to_image.rs::SaveToImage for RelativeTerrainMap<i32, T>::save_landscape_images": (
        "ported",
        "`conflict_image.cell_conflict_image`",
    ),
    "io/save_to_image.rs::SaveToImage for RelativeTerrainMap<i32, T>::save_landmass_images": (
        "ported",
        "`conflict_image.landmass_conflict_image`",
    ),
    "io/save_to_plugin.rs::::convert_landscape_diff_to_landscape": (
        "ported",
        "`emit.build_landscape_record`. Diverges on flags: we declare only the "
        "layers we actually write",
    ),
    "io/save_to_plugin.rs::::convert_landmass_diff_to_landmass": (
        "ported",
        "`service._build_records`",
    ),
    "io/save_to_plugin.rs::::to_master_record": (
        "gap",
        "`service._contributors` + `service._write`. The master list is the "
        "plugins that *contributed*, not the load order's masters -- fault 8",
    ),
    "io/save_to_plugin.rs::::save_plugin": (
        "gap",
        "`service._write`, which now also writes the `.mergedlands.toml` "
        "marker beside the output -- fault 7",
    ),
    # -------------------------------------------------------------- land/
    "land/conversions.rs::::convert_terrain_map": ("absorbed", "list comprehension"),
    "land/conversions.rs::::vertex_normals": (
        "ported",
        "`tes3fields.landscape.decode_vertex_normals`",
    ),
    "land/conversions.rs::::vertex_colors": (
        "ported",
        "`tes3fields.landscape.decode_vertex_colors`",
    ),
    "land/conversions.rs::::world_map_data": (
        "ported",
        "`tes3fields.landscape.decode_world_map`",
    ),
    "land/conversions.rs::::texture_indices": (
        "ported",
        "`tes3fields.landscape.decode_texture_indices`, which de-swizzles the "
        "sixteen 4x4 blocks",
    ),
    "land/conversions.rs::::landscape_flags": ("ported", "`diff.parse_landscape_flags`"),
    "land/conversions.rs::::coordinates": ("ported", "`diff.LandscapeLayers.from_record`"),
    "land/grid_access.rs::Index2D::new": ("absorbed", "an `(x, y)` tuple"),
    "land/grid_access.rs::Iterator for GridIterator2D<X, Y>::next": (
        "absorbed",
        "nested `range()`",
    ),
    "land/grid_access.rs::trait SquareGridIterator::iter_grid": ("absorbed", "`range()`"),
    "land/grid_access.rs::trait GridAccessor2D::get": ("ported", "`RelativeGrid.value_at`"),
    "land/grid_access.rs::trait GridAccessor2D::get_mut": ("ported", "`RelativeGrid.set_value`"),
    "land/height_map.rs::::truncate_gradient": (
        "ported",
        "`heights._fit`, clamping a delta to one signed byte",
    ),
    "land/height_map.rs::::calculate_vertex_heights": (
        "ported",
        "`heights.encode_vertex_heights`. Verified: 400/400 real cells "
        "round-trip exactly, 0 clamps",
    ),
    "land/height_map.rs::::calculate_vertex_heights_tes3": (
        "ported",
        "same function -- the two Rust variants differ only in return type",
    ),
    "land/height_map.rs::::calculate_height_map": (
        "ported",
        "`heights.decode_heights_from_deltas`, the doubly-cumulative sum",
    ),
    "land/height_map.rs::::calculate_vertex_normals_map": (
        "ported",
        "`heights.vertex_normals_from_heights`",
    ),
    "land/height_map.rs::::fix_coords": (
        "ported",
        "edge reuse inside `vertex_normals_from_heights`",
    ),
    "land/height_map.rs::::try_calculate_height_map": (
        "replaced",
        "the sanity assert became `heights.round_trips`, which returns rather "
        "than aborting the run on one cell",
    ),
    "land/landscape_diff.rs::LandscapeDiff::is_modified": ("ported", "`LandscapeDiff.is_modified`"),
    "land/landscape_diff.rs::LandscapeDiff::modified_data": (
        "ported",
        "`LandscapeDiff.modified_data`",
    ),
    "land/landscape_diff.rs::LandscapeDiff::from_reference": (
        "gap",
        "`pipeline.inherit_reference_layers` -- fault 2",
    ),
    "land/landscape_diff.rs::LandscapeDiff::from_difference": (
        "ported",
        "`diff.diff_against_reference`",
    ),
    "land/landscape_diff.rs::LandscapeDiff::apply_mask": (
        "ported",
        "`seams.mask_normals_to_moved_heights`",
    ),
    "land/landscape_diff.rs::LandscapeDiff::calculate_differences_with_mask": (
        "ported",
        "`diff_against_reference`",
    ),
    "land/landscape_diff.rs::LandscapeDiff::calculate_differences": (
        "ported",
        "`diff_against_reference`",
    ),
    "land/landscape_diff.rs::LandscapeDiff::calculate_reference": (
        "ported",
        "`pipeline.inherit_reference_layers`",
    ),
    "land/terrain_map.rs::Vec2<T>::new": ("absorbed", "a coordinate tuple"),
    "land/terrain_map.rs::From<[T; 2]> for Vec2<T>::from": ("absorbed", "a coordinate tuple"),
    "land/terrain_map.rs::From<Vec2<T>> for [T; 2]::from": ("absorbed", "a coordinate tuple"),
    "land/terrain_map.rs::Vec3<T>::new": (
        "absorbed",
        "components are interleaved in one flat list",
    ),
    "land/terrain_map.rs::From<[T; 3]> for Vec3<T>::from": ("absorbed", "interleaved flat list"),
    "land/terrain_map.rs::From<Vec3<T>> for [T; 3]::from": ("absorbed", "interleaved flat list"),
    "land/terrain_map.rs::GridAccessor2D<U> for TerrainMap<U, T>::get": (
        "ported",
        "`RelativeGrid.value_at`",
    ),
    "land/terrain_map.rs::GridAccessor2D<U> for TerrainMap<U, T>::get_mut": (
        "ported",
        "`RelativeGrid.set_value`",
    ),
    "land/terrain_map.rs::SquareGridIterator<T> for TerrainMap<U, T>::iter_grid": (
        "absorbed",
        "`range()`",
    ),
    "land/terrain_map.rs::From<LandscapeFlags> for LandData::from": (
        "ported",
        "`diff.parse_landscape_flags`, including the derived world-map rule",
    ),
    "land/textures.rs::IndexVTEX::new": ("absorbed", "a plain `int`"),
    "land/textures.rs::IndexVTEX::as_u16": ("absorbed", "a plain `int`"),
    "land/textures.rs::From<IndexVTEX> for f64::from": ("absorbed", "a plain `int`"),
    "land/textures.rs::RelativeTo for IndexVTEX::subtract": ("absorbed", "integer subtraction"),
    "land/textures.rs::RelativeTo for IndexVTEX::add": ("absorbed", "integer addition"),
    "land/textures.rs::IndexLTEX::new": ("absorbed", "a plain `int`"),
    "land/textures.rs::IndexLTEX::as_u16": ("absorbed", "a plain `int`"),
    "land/textures.rs::From<IndexLTEX> for IndexVTEX::from": ("ported", "`textures.vtex_of`"),
    "land/textures.rs::TryFrom<IndexVTEX> for IndexLTEX::try_from": (
        "ported",
        "`textures.ltex_of`, which returns `None` for the reserved 0",
    ),
    "land/textures.rs::RemappedTextures::with_capacity": ("absorbed", "a `dict`"),
    "land/textures.rs::RemappedTextures::new": ("ported", "`KnownTextures.translation`"),
    "land/textures.rs::RemappedTextures::from": (
        "ported",
        "`textures.compact_textures`, built from the values actually painted",
    ),
    "land/textures.rs::RemappedTextures::try_remapped_index": ("ported", "`dict.get`"),
    "land/textures.rs::RemappedTextures::remapped_index": (
        "ported",
        "`textures.translate_indices`, which passes an unknown value through "
        "and reports it rather than repainting the terrain",
    ),
    "land/textures.rs::KnownTexture::id": ("ported", "`KnownTexture.identifier`"),
    "land/textures.rs::KnownTexture::index": ("ported", "`KnownTexture.index`"),
    "land/textures.rs::KnownTexture::clone_landscape_texture": (
        "ported",
        "`emit.build_texture_records`",
    ),
    "land/textures.rs::KnownTexture::texture_index": ("ported", "field read in `observe`"),
    "land/textures.rs::KnownTextures::new": ("ported", "`KnownTextures.__init__`"),
    "land/textures.rs::KnownTextures::sorted": ("ported", "`KnownTextures.sorted`"),
    "land/textures.rs::KnownTextures::update_texture": (
        "verified",
        "`KnownTextures.observe`: a later plugin's *different* file name wins "
        "and takes ownership; an absent one changes nothing",
    ),
    "land/textures.rs::KnownTextures::add_texture": (
        "verified",
        "`observe`: keyed on the `LTEX` id, so the first plugin to declare an "
        "id fixes its shared index",
    ),
    "land/textures.rs::KnownTextures::add_remapped_texture": (
        "ported",
        "`observe` returns the plugin's translation",
    ),
    "land/textures.rs::KnownTextures::remove_unused": ("ported", "`textures.compact_textures`"),
    "land/textures.rs::KnownTextures::len": ("ported", "`KnownTextures.__len__`"),
    "land/textures.rs::KnownTextures::next_texture_index": ("ported", "`len(self._by_id)`"),
    "land/textures.rs::KnownTextures::add_next_texture": (
        "gap",
        "`observe`. Its `DELETED` assert became `diff.is_deleted`, which skips "
        "and logs -- fault 9",
    ),
    # ------------------------------------------------------------- merge/
    "merge/cells.rs::::merge_cell_into": (
        "ported",
        "`cells.merge_cell_into`, plus a bug fix: the original clones `new` "
        "from itself, so region, map colour, water height and atmosphere "
        "never update",
    ),
    "merge/cells.rs::::merge_cells_into": ("ported", "`cells.merge_cells`"),
    "merge/cells.rs::::merge_cells": ("ported", "`cells.merge_cells`"),
    "merge/conflict.rs::trait ConflictResolver::average": ("ported", "`merge.average_delta`"),
    "merge/conflict.rs::Default for ConflictParams::default": (
        "ported",
        "`merge.ConflictParams` defaults: 0.3 / 10.0 / 64.0",
    ),
    "merge/conflict.rs::Default for ConflictParams::classify_conflict": (
        "verified",
        "`merge.average_delta`. Weight `|lhs|/(|lhs|+|rhs|)` raised to 1.5 and "
        "renormalised; severity measured from the *signed* minimum against "
        "`min(max(0.3*min, 10), 64)`; `RoundTo` is `as i32`, which truncates "
        "toward zero, and so does `int()`",
    ),
    "merge/conflict.rs::Default for ConflictParams::average": (
        "ported",
        "`average_delta`; equal values return no conflict",
    ),
    "merge/conflict.rs::ConflictResolver for Vec3<T>::average": (
        "ported",
        "`merge.merge_layer` resolves each component; the vertex is Major if " "any component is",
    ),
    "merge/ignore_strategy.rs::MergeStrategy for IgnoreStrategy::apply": (
        "ported",
        "`ConflictStrategy.IGNORE`: contested vertices keep the earlier edit",
    ),
    "merge/merge_strategy.rs::trait MergeStrategy::apply": ("ported", "`merge.merge_layer`"),
    "merge/merge_strategy.rs::trait MergeStrategy::apply_strategy": (
        "ported",
        "`pipeline._fold`: with one side absent, the other is taken whole",
    ),
    "merge/merge_strategy.rs::trait MergeStrategy::apply_preferred_strategy": (
        "ported",
        "`merge._resolve_for`",
    ),
    "merge/merge_strategy.rs::trait MergeStrategy::apply_merge_strategy": (
        "verified",
        "`merge.DEFAULT_STRATEGY`: resolve for heights, normals, colours and "
        "world map; overwrite for textures",
    ),
    "merge/overwrite_strategy.rs::MergeStrategy for OverwriteStrategy::apply": (
        "ported",
        "`ConflictStrategy.OVERWRITE`: contested vertices take the later edit",
    ),
    "merge/relative_terrain_map.rs::SquareGridIterator<T> for RelativeTerrainMap<U, T>::iter_grid": (
        "absorbed",
        "`range()`",
    ),
    "merge/relative_terrain_map.rs::RelativeTerrainMap<U, T>::default": (
        "ported",
        "`RelativeGrid.__init__` with a zero reference",
    ),
    "merge/relative_terrain_map.rs::RelativeTerrainMap<U, T>::empty": (
        "ported",
        "`RelativeGrid.__init__`",
    ),
    "merge/relative_terrain_map.rs::RelativeTerrainMap<U, T>::from_difference": (
        "ported",
        "`RelativeGrid.from_difference`",
    ),
    "merge/relative_terrain_map.rs::RelativeTerrainMap<U, T>::differences": (
        "ported",
        "`RelativeGrid.changed_vertices`",
    ),
    "merge/relative_terrain_map.rs::RelativeTerrainMap<U, T>::get_value": (
        "ported",
        "`RelativeGrid.value_at`",
    ),
    "merge/relative_terrain_map.rs::RelativeTerrainMap<U, T>::set_value": (
        "ported",
        "`RelativeGrid.set_value`",
    ),
    "merge/relative_terrain_map.rs::RelativeTerrainMap<U, T>::get_difference": (
        "ported",
        "`RelativeGrid.delta_at` / `deltas_at`",
    ),
    "merge/relative_terrain_map.rs::RelativeTerrainMap<U, T>::set_difference": (
        "ported",
        "`RelativeGrid.set_deltas`",
    ),
    "merge/relative_terrain_map.rs::RelativeTerrainMap<U, T>::has_difference": (
        "ported",
        "`RelativeGrid.has_difference`, one flag per *vertex* not per component",
    ),
    "merge/relative_terrain_map.rs::RelativeTerrainMap<U, T>::clean_all": (
        "ported",
        "`RelativeGrid.clear` over the grid",
    ),
    "merge/relative_terrain_map.rs::RelativeTerrainMap<U, T>::clean_some": (
        "ported",
        "`RelativeGrid.clear`",
    ),
    "merge/relative_terrain_map.rs::RelativeTerrainMap<U, T>::to_terrain": (
        "ported",
        "`RelativeGrid.to_flat` / `to_rows`",
    ),
    "merge/relative_terrain_map.rs::trait IsModified::is_modified": (
        "ported",
        "`RelativeGrid.is_modified`",
    ),
    "merge/relative_terrain_map.rs::trait IsModified::num_differences": (
        "ported",
        "`RelativeGrid.num_differences`",
    ),
    "merge/relative_terrain_map.rs::IsModified for RelativeTerrainMap<U, T>::is_modified": (
        "ported",
        "`RelativeGrid.is_modified`",
    ),
    "merge/relative_terrain_map.rs::IsModified for RelativeTerrainMap<U, T>::num_differences": (
        "ported",
        "`RelativeGrid.num_differences`",
    ),
    "merge/relative_terrain_map.rs::IsModified for OptionalTerrainMap<U, T>::is_modified": (
        "ported",
        "`LandscapeDiff.is_modified`, which treats an absent layer as unmodified",
    ),
    "merge/relative_terrain_map.rs::IsModified for OptionalTerrainMap<U, T>::num_differences": (
        "ported",
        "`LandscapeDiff.num_differences`",
    ),
    "merge/relative_terrain_map.rs::IsModified for OptionalTerrainMap<U, T>::recompute_vertex_normals": (
        "gap",
        "`pipeline.resolve_normals`: recomputes where a height moved, inherits "
        "the authored normal where it did not -- fault 4",
    ),
    "merge/relative_to.rs::trait RelativeTo::subtract": ("absorbed", "integer subtraction"),
    "merge/relative_to.rs::trait RelativeTo::add": ("absorbed", "integer addition"),
    "merge/relative_to.rs::RelativeTo for i32::subtract": ("absorbed", "integer subtraction"),
    "merge/relative_to.rs::RelativeTo for i32::add": ("absorbed", "integer addition"),
    "merge/relative_to.rs::RelativeTo for u8::subtract": ("absorbed", "integer subtraction"),
    "merge/relative_to.rs::RelativeTo for u8::add": ("absorbed", "integer addition"),
    "merge/relative_to.rs::RelativeTo for i8::subtract": ("absorbed", "integer subtraction"),
    "merge/relative_to.rs::RelativeTo for i8::add": ("absorbed", "integer addition"),
    "merge/relative_to.rs::RelativeTo for u16::subtract": ("absorbed", "integer subtraction"),
    "merge/relative_to.rs::RelativeTo for u16::add": ("absorbed", "integer addition"),
    "merge/relative_to.rs::RelativeTo for Vec3<T>::subtract": (
        "absorbed",
        "per-component subtraction over the interleaved list",
    ),
    "merge/relative_to.rs::RelativeTo for Vec3<T>::add": (
        "absorbed",
        "per-component addition over the interleaved list",
    ),
    "merge/resolve_conflict_strategy.rs::MergeStrategy for ResolveConflictStrategy::apply": (
        "verified",
        "`ConflictStrategy.RESOLVE`. Averages the *deltas*, not the values; "
        "only-one-side-moved takes that side whole",
    ),
    "merge/round_to.rs::trait RoundTo::round_to": (
        "absorbed",
        "`int()`, which truncates toward zero exactly as Rust's `as` does",
    ),
    "merge/round_to.rs::RoundTo<i32> for f32::round_to": ("absorbed", "`int()`"),
    "merge/round_to.rs::RoundTo<i8> for f32::round_to": ("absorbed", "`int()`"),
    "merge/round_to.rs::RoundTo<u8> for f32::round_to": ("absorbed", "`int()`"),
    "merge/round_to.rs::RoundTo<u16> for f32::round_to": ("absorbed", "`int()`"),
    "merge/round_to.rs::RoundTo<IndexVTEX> for f32::round_to": ("absorbed", "`int()`"),
    # ------------------------------------------------------------ repair/
    "repair/cleaning.rs::::has_difference": (
        "verified",
        "`cleaning.differs`. The Rust routes through `ConflictResolver::average`, "
        "which returns `None` exactly when the two values are equal -- so it is "
        "an equality test, and it is false the moment either side is absent",
    ),
    "repair/cleaning.rs::::has_any_difference": (
        "gap",
        "`cleaning.differs_any`. Tests all five layers; we tested heights -- fault 6",
    ),
    "repair/cleaning.rs::::clean_landmass_diff": (
        "gap",
        "`cleaning.clean_landmass`. Its opening "
        "`assert_eq!(repair_landmass_seams(landmass), 0)` -- the merge's "
        "post-condition -- was not ported at all; it is now "
        "`seams.find_tears` via `pipeline._check_borders` -- fault 10",
    ),
    "repair/cleaning.rs::::clean_known_textures": (
        "ported",
        "`textures.compact_textures`, plus a bug fix: the original's second "
        "loop is named `plugins` but iterates `masters`, so a mod's `LTEX` "
        "file name never reaches the output",
    ),
    "repair/cleaning.rs::::update_known_textures": ("ported", "`KnownTextures.observe`"),
    "repair/debugging.rs::::add_vertex_colors": ("ported", "`debug_colors.paint_conflicts`"),
    "repair/debugging.rs::::add_debug_vertex_colors_to_landscape": (
        "ported",
        "`debug_colors.paint_conflicts`",
    ),
    "repair/debugging.rs::::add_debug_vertex_colors_to_landmass": (
        "ported",
        "`debug_colors.paint_conflicts` over the landmass",
    ),
    "repair/seam_detection.rs::::coords_with_offset": ("absorbed", "tuple arithmetic"),
    "repair/seam_detection.rs::::push_back_neighbors": ("ported", "`seams._border_pairs`"),
    "repair/seam_detection.rs::::sort_pair": (
        "ported",
        "`seams._border_pairs` orders each pair so a border is visited once",
    ),
    "repair/seam_detection.rs::::repair_corner_seams": (
        "gap",
        "`seams.repair_corners`, extended with anchoring for corners shared "
        "with terrain outside the merge -- fault 5",
    ),
    "repair/seam_detection.rs::::repair_landmass_seams": (
        "ported",
        "`seams.repair_seams`. Called a second time by `clean_landmass_diff` as "
        "an assertion; that use is `seams.find_tears`",
    ),
    "repair/seam_detection.rs::::try_repair_seam": (
        "verified",
        "`seams.repair_edges`, averaging through `seams.mean`, which truncates "
        "toward zero to match Rust's integer `/` rather than Python's flooring "
        "`//`",
    ),
    # ----------------------------------------------------------- main.rs
    "main.rs::Landmass::new": ("ported", "`landmass.Landmass`"),
    "main.rs::Landmass::insert_land": ("ported", "`Landmass.cells[coords] = ...`"),
    "main.rs::Landmass::sorted": ("ported", "`sorted(landmass.cells)`"),
    "main.rs::LandmassDiff::new": ("ported", "`pipeline.MergeOutcome`"),
    "main.rs::LandmassDiff::sorted": ("ported", "`sorted(outcome.cells)`"),
    "main.rs::From<CliLevelFilter> for LevelFilter::from": (
        "n/a",
        "log level mapping; the toolkit configures `logging` itself",
    ),
    "main.rs::Cli::read_args": ("n/a", "`tools/build_merged_lands.py` argparse"),
    "main.rs::Cli::plugins": ("ported", "`tools.survey_landscape.read_order`"),
    "main.rs::Cli::should_write_log_file": ("n/a", "the toolkit's own logging"),
    "main.rs::Cli::merged_lands_dir": ("ported", "`--conflicts-dir`"),
    "main.rs::Cli::data_files_dir": (
        "ported",
        "`service.resolve_plugin` over every `data=` folder",
    ),
    "main.rs::Cli::output_file_dir": ("ported", "`--out`"),
    "main.rs::Cli::stack_size": (
        "n/a",
        "Rust thread stack sizing; the merge here is iterative, not recursive",
    ),
    "main.rs::Cli::main": ("n/a", "entry point"),
    "main.rs::Cli::wait_for_user_exit": ("n/a", "console behaviour"),
    "main.rs::Cli::merge_all": (
        "verified",
        "`pipeline.finish` + `service.build_merged_lands`, in the same order: "
        "reference, diff, merge, repair, clean, textures, convert, save",
    ),
    "main.rs::Cli::init_log": ("n/a", "logging setup"),
    "main.rs::Cli::try_copy_landscape_and_remap_textures": (
        "ported",
        "`landmass._decode_cell` + `textures.translate_indices`",
    ),
    "main.rs::Cli::try_create_landmass": (
        "ported",
        "`landmass.build_reference` / `landmass.plugin_differences`",
    ),
    "main.rs::Cli::merge_tes3_landscape": (
        "gap",
        "`landmass.merge_master_layers`: masters combine per *layer*, flags " "unioned -- fault 3",
    ),
    "main.rs::Cli::merge_tes3_landmasses": ("ported", "`landmass.build_reference`"),
    "main.rs::Cli::find_allowed_data": (
        "gap",
        "`diff.diff_against_reference` intersects `allowed` with the record's "
        "declared flags -- fault 1",
    ),
    "main.rs::Cli::find_landmass_diff": ("ported", "`landmass.plugin_differences`"),
    "main.rs::Cli::merge_landscape_diff": ("ported", "`pipeline._fold_change`"),
    "main.rs::Cli::merge_landmass_into": ("ported", "`pipeline.merge_landmass`"),
    "main.rs::Cli::create_tes3_landmass": ("ported", "`landmass.build_reference`"),
    "main.rs::Cli::create_merged_lands_from_reference": (
        "gap",
        "`pipeline.inherit_reference_layers` + `pipeline.add_reference_neighbours` "
        "-- faults 2 and 5",
    ),
}


#: Every module in ``wraithguard/land``, and where it stands relative to
#: Merged Lands. The coverage table above proves *their* functions all landed
#: somewhere here; this is the other direction, and it is the one that matters
#: now: code we wrote that no reference implementation stands behind.
#:
#: ``port``      -- a port of theirs, checked function by function above.
#: ``ours``      -- no counterpart. **Changes the terrain**, so it is the risk.
#: ``ours-aux``  -- no counterpart, but affects no merged vertex (I/O, reports).
MODULES: Final[dict[str, tuple[str, str]]] = {
    "__init__.py": ("port", "package docstring: the six steps, and why no dependencies"),
    "diff.py": ("port", "`landscape_diff.rs` + `relative_terrain_map.rs`"),
    "landmass.py": ("port", "`main.rs` reference assembly and per-plugin diff"),
    "merge.py": ("port", "`merge/` strategies, `conflict.rs` thresholds"),
    "textures.py": ("port", "`land/textures.rs` shared LTEX index space"),
    "heights.py": ("port", "`land/height_map.rs` VHGT encode/decode"),
    "cleaning.py": ("port", "`repair/cleaning.rs`, extended to all five layers"),
    "meta.py": ("port", "`io/meta_schema.rs`; hand-parsed where serde did it"),
    "cells.py": ("port", "`merge/cells.rs`, with references always stripped"),
    "debug_colors.py": ("port", "`repair/debugging.rs`"),
    "conflict_image.py": ("port", "`io/save_to_image.rs`"),
    "emit.py": ("port", "`io/save_to_plugin.rs`, over tes3conv JSON not tes3"),
    "pipeline.py": ("port", "`main.rs::merge_all` ordering"),
    "seams.py": (
        "ours",
        "`repair/seam_detection.rs` **plus** inward feathering, corner "
        "anchoring, the authoritative rule and `find_tears` -- all ours, and "
        "all of them move vertices the original does not",
    ),
    "slope.py": (
        "ours",
        "no counterpart at all. The original clamps at encode time and accepts "
        "the drift; this conditions the terrain so it does not have to",
    ),
    "curvature.py": (
        "ours",
        "no counterpart. Opt-in only: `ConflictStrategy.CURVATURE` is not a "
        "default, so it changes nothing unless a sidecar asks for it",
    ),
    "native.py": (
        "ours-aux",
        "no counterpart -- the original reads plugins through `tes3`. Reads "
        "records; decides nothing about terrain",
    ),
    "service.py": (
        "ours-aux",
        "orchestration the original does inline in `main.rs`. Chooses no " "vertex values",
    ),
}


def scan(src: Path) -> list[Function]:
    """Find every function in a Rust source tree.

    Args:
        src: The ``src`` directory.

    Returns:
        Every function, in file then line order.

    Raises:
        SystemExit: If the directory holds no Rust files.
    """
    found: list[Function] = []
    files = sorted(src.rglob("*.rs"))
    if not files:
        raise SystemExit(f"no .rs files under {src}")
    for path in files:
        context = ""
        for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
            text = line.strip()
            if text.startswith("impl"):
                match = _IMPL.match(text)
                if match:
                    context = match.group(1).rstrip("{").strip()
                    continue
            match = _TRAIT.match(text)
            if match:
                context = "trait " + match.group(1)
                continue
            match = _FN.match(text)
            if match:
                found.append(
                    Function(
                        file=path.relative_to(src).as_posix(),
                        line=number,
                        context=context,
                        name=match.group(1),
                    )
                )
    return found


def check(functions: list[Function]) -> list[str]:
    """Compare the scanned functions against :data:`COVERAGE`.

    Args:
        functions: What the scan found.

    Returns:
        Problems, one per line. Empty when every function is accounted for and
        no entry is stale.
    """
    problems: list[str] = []
    keys = {function.key for function in functions}
    for function in functions:
        if function.key not in COVERAGE:
            problems.append(f"UNCOVERED {function.file}:{function.line} {function.key}")
        else:
            status = COVERAGE[function.key][0]
            if status not in STATUS:
                problems.append(f"BAD STATUS {status!r} for {function.key}")
    problems.extend(f"STALE ENTRY (no such function) {key}" for key in COVERAGE if key not in keys)
    return problems


def render(functions: list[Function]) -> str:
    """Build the markdown table sections.

    Args:
        functions: Every function, in scan order.

    Returns:
        The generated portion of the document.
    """
    groups: dict[str, list[Function]] = {}
    for function in functions:
        groups.setdefault(function.file.split("/")[0] if "/" in function.file else "main.rs", [])
        key = function.file.split("/")[0] if "/" in function.file else "main.rs"
        groups[key].append(function)

    order = ["land", "merge", "repair", "io", "main.rs"]
    titles = {
        "land": "`land/`",
        "merge": "`merge/`",
        "repair": "`repair/`",
        "io": "`io/`",
        "main.rs": "`main.rs`",
    }

    out: list[str] = []
    for group in order:
        members = sorted(groups.get(group, []), key=lambda f: (f.file, f.line))
        out.append(f"## {titles[group]} — {len(members)} functions\n")
        out.append("| fn | file | status | here |")
        out.append("|---|---|---|---|")
        for function in members:
            status, where = COVERAGE[function.key]
            # A raw pipe inside a cell ends the cell, so a formula such as
            # |lhs|/(|lhs|+|rhs|) would silently split one row into six.
            cell = where.replace("|", r"\|")
            out.append(f"| {function.label} | `{function.file}` | {status} | {cell} |")
        out.append("")
    return "\n".join(out)


def main() -> int:
    """Regenerate or verify the coverage table.

    Returns:
        0 on success, 1 when a function is uncovered or an entry is stale.
    """
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--src", type=Path, required=True, help="merged_lands src directory")
    parser.add_argument("--out", type=Path, help="write the tables here")
    parser.add_argument("--check", action="store_true", help="verify only, write nothing")
    args = parser.parse_args()

    functions = scan(args.src)
    problems = check(functions)
    if problems:
        for problem in problems:
            print(problem, file=sys.stderr)
        print(
            f"\n{len(problems)} problem(s); {len(functions)} function(s) scanned", file=sys.stderr
        )
        return 1

    print(f"{len(functions)} function(s), all accounted for")
    if args.check or args.out is None:
        return 0
    args.out.write_text(render(functions), encoding="utf-8")
    print(f"wrote {args.out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
