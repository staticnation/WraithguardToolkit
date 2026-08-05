# Merged Lands: function-by-function coverage

Every `fn` in `merged_lands-main/src` — **all 191 of them**, named individually
— with where its behaviour lives here.

Written after a file-level audit proved insufficient: it showed every file
"ported" while five faults that would have shipped visibly broken terrain sat
inside functions nobody had read. Then rewritten again, because the first
version of *this* table grouped related functions onto one row and reported the
row count as the function count — so `land/` was labelled 37 functions when it
has 64, and forty functions were covered only by a heading that never named
them. A table that claims completeness has to be able to prove it.

**So the tables below are generated.** `tools/gen_merged_lands_table.py` parses
the Rust source, looks each function up in a hand-written coverage map, and
**fails** if any function has no entry or any entry names no function. Run it
with `--check` against a copy of the source to re-verify:

```
python tools/gen_merged_lands_table.py --src ../merged_lands-main/src --check
```

`tests/test_merged_lands_coverage.py` runs the same check whenever the source
tree is present.

Merged Lands is MIT (David Von Derau, 2022). Licence text vendored at
`License/MergedLands/LICENSE`; see `CREDITS.md`.

Status column:

- **ported** — the behaviour exists here.
- **verified** — ported, *and* checked statement by statement against the Rust
  rather than by matching names. Used where the details decide the output:
  exact formulas, rounding, precedence, ordering.
- **gap** — ported only after this audit found it missing. Nine of these; each
  is written up below.
- **absorbed** — Rust scaffolding Python does not need (traits, iterators,
  newtype wrappers, `RelativeTo`/`RoundTo` arithmetic). Named individually so
  "absorbed" is a claim about a specific function, not a category.
- **replaced** — done differently and deliberately, with the reason given.
- **n/a** — belongs to a concern outside the merge (CLI parsing, logging).

Totals across the five tables: **113 ported, 46 absorbed, 11 gap, 10 verified,
7 n/a, 4 replaced = 191.** Eleven rows are marked *gap* against ten faults
because fault 5 and fault 2 both run through
`create_merged_lands_from_reference`.

## `land/` — 64 functions

| fn | file | status | here |
|---|---|---|---|
| `convert_terrain_map` | `land/conversions.rs` | absorbed | list comprehension |
| `vertex_normals` | `land/conversions.rs` | ported | `tes3fields.landscape.decode_vertex_normals` |
| `vertex_colors` | `land/conversions.rs` | ported | `tes3fields.landscape.decode_vertex_colors` |
| `world_map_data` | `land/conversions.rs` | ported | `tes3fields.landscape.decode_world_map` |
| `texture_indices` | `land/conversions.rs` | ported | `tes3fields.landscape.decode_texture_indices`, which de-swizzles the sixteen 4x4 blocks |
| `landscape_flags` | `land/conversions.rs` | ported | `diff.parse_landscape_flags` |
| `coordinates` | `land/conversions.rs` | ported | `diff.LandscapeLayers.from_record` |
| `Index2D::new` | `land/grid_access.rs` | absorbed | an `(x, y)` tuple |
| `Iterator for GridIterator2D<X, Y>::next` | `land/grid_access.rs` | absorbed | nested `range()` |
| `trait SquareGridIterator::iter_grid` | `land/grid_access.rs` | absorbed | `range()` |
| `trait GridAccessor2D::get` | `land/grid_access.rs` | ported | `RelativeGrid.value_at` |
| `trait GridAccessor2D::get_mut` | `land/grid_access.rs` | ported | `RelativeGrid.set_value` |
| `truncate_gradient` | `land/height_map.rs` | ported | `heights._fit`, clamping a delta to one signed byte |
| `calculate_vertex_heights` | `land/height_map.rs` | ported | `heights.encode_vertex_heights`. Verified: 400/400 real cells round-trip exactly, 0 clamps |
| `calculate_vertex_heights_tes3` | `land/height_map.rs` | ported | same function -- the two Rust variants differ only in return type |
| `calculate_height_map` | `land/height_map.rs` | ported | `heights.decode_heights_from_deltas`, the doubly-cumulative sum |
| `calculate_vertex_normals_map` | `land/height_map.rs` | ported | `heights.vertex_normals_from_heights` |
| `fix_coords` | `land/height_map.rs` | ported | edge reuse inside `vertex_normals_from_heights` |
| `try_calculate_height_map` | `land/height_map.rs` | replaced | the sanity assert became `heights.round_trips`, which returns rather than aborting the run on one cell |
| `LandscapeDiff::is_modified` | `land/landscape_diff.rs` | ported | `LandscapeDiff.is_modified` |
| `LandscapeDiff::modified_data` | `land/landscape_diff.rs` | ported | `LandscapeDiff.modified_data` |
| `LandscapeDiff::from_reference` | `land/landscape_diff.rs` | gap | `pipeline.inherit_reference_layers` -- fault 2 |
| `LandscapeDiff::from_difference` | `land/landscape_diff.rs` | ported | `diff.diff_against_reference` |
| `LandscapeDiff::apply_mask` | `land/landscape_diff.rs` | ported | `seams.mask_normals_to_moved_heights` |
| `LandscapeDiff::calculate_differences_with_mask` | `land/landscape_diff.rs` | ported | `diff_against_reference` |
| `LandscapeDiff::calculate_differences` | `land/landscape_diff.rs` | ported | `diff_against_reference` |
| `LandscapeDiff::calculate_reference` | `land/landscape_diff.rs` | ported | `pipeline.inherit_reference_layers` |
| `Vec2<T>::new` | `land/terrain_map.rs` | absorbed | a coordinate tuple |
| `From<[T; 2]> for Vec2<T>::from` | `land/terrain_map.rs` | absorbed | a coordinate tuple |
| `From<Vec2<T>> for [T; 2]::from` | `land/terrain_map.rs` | absorbed | a coordinate tuple |
| `Vec3<T>::new` | `land/terrain_map.rs` | absorbed | components are interleaved in one flat list |
| `From<[T; 3]> for Vec3<T>::from` | `land/terrain_map.rs` | absorbed | interleaved flat list |
| `From<Vec3<T>> for [T; 3]::from` | `land/terrain_map.rs` | absorbed | interleaved flat list |
| `GridAccessor2D<U> for TerrainMap<U, T>::get` | `land/terrain_map.rs` | ported | `RelativeGrid.value_at` |
| `GridAccessor2D<U> for TerrainMap<U, T>::get_mut` | `land/terrain_map.rs` | ported | `RelativeGrid.set_value` |
| `SquareGridIterator<T> for TerrainMap<U, T>::iter_grid` | `land/terrain_map.rs` | absorbed | `range()` |
| `From<LandscapeFlags> for LandData::from` | `land/terrain_map.rs` | ported | `diff.parse_landscape_flags`, including the derived world-map rule |
| `IndexVTEX::new` | `land/textures.rs` | absorbed | a plain `int` |
| `IndexVTEX::as_u16` | `land/textures.rs` | absorbed | a plain `int` |
| `From<IndexVTEX> for f64::from` | `land/textures.rs` | absorbed | a plain `int` |
| `RelativeTo for IndexVTEX::subtract` | `land/textures.rs` | absorbed | integer subtraction |
| `RelativeTo for IndexVTEX::add` | `land/textures.rs` | absorbed | integer addition |
| `IndexLTEX::new` | `land/textures.rs` | absorbed | a plain `int` |
| `IndexLTEX::as_u16` | `land/textures.rs` | absorbed | a plain `int` |
| `From<IndexLTEX> for IndexVTEX::from` | `land/textures.rs` | ported | `textures.vtex_of` |
| `TryFrom<IndexVTEX> for IndexLTEX::try_from` | `land/textures.rs` | ported | `textures.ltex_of`, which returns `None` for the reserved 0 |
| `RemappedTextures::with_capacity` | `land/textures.rs` | absorbed | a `dict` |
| `RemappedTextures::new` | `land/textures.rs` | ported | `KnownTextures.translation` |
| `RemappedTextures::from` | `land/textures.rs` | ported | `textures.compact_textures`, built from the values actually painted |
| `RemappedTextures::try_remapped_index` | `land/textures.rs` | ported | `dict.get` |
| `RemappedTextures::remapped_index` | `land/textures.rs` | ported | `textures.translate_indices`, which passes an unknown value through and reports it rather than repainting the terrain |
| `KnownTexture::id` | `land/textures.rs` | ported | `KnownTexture.identifier` |
| `KnownTexture::index` | `land/textures.rs` | ported | `KnownTexture.index` |
| `KnownTexture::clone_landscape_texture` | `land/textures.rs` | ported | `emit.build_texture_records` |
| `KnownTexture::texture_index` | `land/textures.rs` | ported | field read in `observe` |
| `KnownTextures::new` | `land/textures.rs` | ported | `KnownTextures.__init__` |
| `KnownTextures::sorted` | `land/textures.rs` | ported | `KnownTextures.sorted` |
| `KnownTextures::update_texture` | `land/textures.rs` | verified | `KnownTextures.observe`: a later plugin's *different* file name wins and takes ownership; an absent one changes nothing |
| `KnownTextures::add_texture` | `land/textures.rs` | verified | `observe`: keyed on the `LTEX` id, so the first plugin to declare an id fixes its shared index |
| `KnownTextures::add_remapped_texture` | `land/textures.rs` | ported | `observe` returns the plugin's translation |
| `KnownTextures::remove_unused` | `land/textures.rs` | ported | `textures.compact_textures` |
| `KnownTextures::len` | `land/textures.rs` | ported | `KnownTextures.__len__` |
| `KnownTextures::next_texture_index` | `land/textures.rs` | ported | `len(self._by_id)` |
| `KnownTextures::add_next_texture` | `land/textures.rs` | gap | `observe`. Its `DELETED` assert became `diff.is_deleted`, which skips and logs -- fault 9 |

## `merge/` — 53 functions

| fn | file | status | here |
|---|---|---|---|
| `merge_cell_into` | `merge/cells.rs` | ported | `cells.merge_cell_into`, plus a bug fix: the original clones `new` from itself, so region, map colour, water height and atmosphere never update |
| `merge_cells_into` | `merge/cells.rs` | ported | `cells.merge_cells` |
| `merge_cells` | `merge/cells.rs` | ported | `cells.merge_cells` |
| `trait ConflictResolver::average` | `merge/conflict.rs` | ported | `merge.average_delta` |
| `Default for ConflictParams::default` | `merge/conflict.rs` | ported | `merge.ConflictParams` defaults: 0.3 / 10.0 / 64.0 |
| `Default for ConflictParams::classify_conflict` | `merge/conflict.rs` | verified | `merge.average_delta`. Weight `\|lhs\|/(\|lhs\|+\|rhs\|)` raised to 1.5 and renormalised; severity measured from the *signed* minimum against `min(max(0.3*min, 10), 64)`; `RoundTo` is `as i32`, which truncates toward zero, and so does `int()` |
| `Default for ConflictParams::average` | `merge/conflict.rs` | ported | `average_delta`; equal values return no conflict |
| `ConflictResolver for Vec3<T>::average` | `merge/conflict.rs` | ported | `merge.merge_layer` resolves each component; the vertex is Major if any component is |
| `MergeStrategy for IgnoreStrategy::apply` | `merge/ignore_strategy.rs` | ported | `ConflictStrategy.IGNORE`: contested vertices keep the earlier edit |
| `trait MergeStrategy::apply` | `merge/merge_strategy.rs` | ported | `merge.merge_layer` |
| `trait MergeStrategy::apply_strategy` | `merge/merge_strategy.rs` | ported | `pipeline._fold`: with one side absent, the other is taken whole |
| `trait MergeStrategy::apply_preferred_strategy` | `merge/merge_strategy.rs` | ported | `merge._resolve_for` |
| `trait MergeStrategy::apply_merge_strategy` | `merge/merge_strategy.rs` | verified | `merge.DEFAULT_STRATEGY`: resolve for heights, normals, colours and world map; overwrite for textures |
| `MergeStrategy for OverwriteStrategy::apply` | `merge/overwrite_strategy.rs` | ported | `ConflictStrategy.OVERWRITE`: contested vertices take the later edit |
| `SquareGridIterator<T> for RelativeTerrainMap<U, T>::iter_grid` | `merge/relative_terrain_map.rs` | absorbed | `range()` |
| `RelativeTerrainMap<U, T>::default` | `merge/relative_terrain_map.rs` | ported | `RelativeGrid.__init__` with a zero reference |
| `RelativeTerrainMap<U, T>::empty` | `merge/relative_terrain_map.rs` | ported | `RelativeGrid.__init__` |
| `RelativeTerrainMap<U, T>::from_difference` | `merge/relative_terrain_map.rs` | ported | `RelativeGrid.from_difference` |
| `RelativeTerrainMap<U, T>::differences` | `merge/relative_terrain_map.rs` | ported | `RelativeGrid.changed_vertices` |
| `RelativeTerrainMap<U, T>::get_value` | `merge/relative_terrain_map.rs` | ported | `RelativeGrid.value_at` |
| `RelativeTerrainMap<U, T>::set_value` | `merge/relative_terrain_map.rs` | ported | `RelativeGrid.set_value` |
| `RelativeTerrainMap<U, T>::get_difference` | `merge/relative_terrain_map.rs` | ported | `RelativeGrid.delta_at` / `deltas_at` |
| `RelativeTerrainMap<U, T>::set_difference` | `merge/relative_terrain_map.rs` | ported | `RelativeGrid.set_deltas` |
| `RelativeTerrainMap<U, T>::has_difference` | `merge/relative_terrain_map.rs` | ported | `RelativeGrid.has_difference`, one flag per *vertex* not per component |
| `RelativeTerrainMap<U, T>::clean_all` | `merge/relative_terrain_map.rs` | ported | `RelativeGrid.clear` over the grid |
| `RelativeTerrainMap<U, T>::clean_some` | `merge/relative_terrain_map.rs` | ported | `RelativeGrid.clear` |
| `RelativeTerrainMap<U, T>::to_terrain` | `merge/relative_terrain_map.rs` | ported | `RelativeGrid.to_flat` / `to_rows` |
| `trait IsModified::is_modified` | `merge/relative_terrain_map.rs` | ported | `RelativeGrid.is_modified` |
| `trait IsModified::num_differences` | `merge/relative_terrain_map.rs` | ported | `RelativeGrid.num_differences` |
| `IsModified for RelativeTerrainMap<U, T>::is_modified` | `merge/relative_terrain_map.rs` | ported | `RelativeGrid.is_modified` |
| `IsModified for RelativeTerrainMap<U, T>::num_differences` | `merge/relative_terrain_map.rs` | ported | `RelativeGrid.num_differences` |
| `IsModified for OptionalTerrainMap<U, T>::is_modified` | `merge/relative_terrain_map.rs` | ported | `LandscapeDiff.is_modified`, which treats an absent layer as unmodified |
| `IsModified for OptionalTerrainMap<U, T>::num_differences` | `merge/relative_terrain_map.rs` | ported | `LandscapeDiff.num_differences` |
| `IsModified for OptionalTerrainMap<U, T>::recompute_vertex_normals` | `merge/relative_terrain_map.rs` | gap | `pipeline.resolve_normals`: recomputes where a height moved, inherits the authored normal where it did not -- fault 4 |
| `trait RelativeTo::subtract` | `merge/relative_to.rs` | absorbed | integer subtraction |
| `trait RelativeTo::add` | `merge/relative_to.rs` | absorbed | integer addition |
| `RelativeTo for i32::subtract` | `merge/relative_to.rs` | absorbed | integer subtraction |
| `RelativeTo for i32::add` | `merge/relative_to.rs` | absorbed | integer addition |
| `RelativeTo for u8::subtract` | `merge/relative_to.rs` | absorbed | integer subtraction |
| `RelativeTo for u8::add` | `merge/relative_to.rs` | absorbed | integer addition |
| `RelativeTo for i8::subtract` | `merge/relative_to.rs` | absorbed | integer subtraction |
| `RelativeTo for i8::add` | `merge/relative_to.rs` | absorbed | integer addition |
| `RelativeTo for u16::subtract` | `merge/relative_to.rs` | absorbed | integer subtraction |
| `RelativeTo for u16::add` | `merge/relative_to.rs` | absorbed | integer addition |
| `RelativeTo for Vec3<T>::subtract` | `merge/relative_to.rs` | absorbed | per-component subtraction over the interleaved list |
| `RelativeTo for Vec3<T>::add` | `merge/relative_to.rs` | absorbed | per-component addition over the interleaved list |
| `MergeStrategy for ResolveConflictStrategy::apply` | `merge/resolve_conflict_strategy.rs` | verified | `ConflictStrategy.RESOLVE`. Averages the *deltas*, not the values; only-one-side-moved takes that side whole |
| `trait RoundTo::round_to` | `merge/round_to.rs` | absorbed | `int()`, which truncates toward zero exactly as Rust's `as` does |
| `RoundTo<i32> for f32::round_to` | `merge/round_to.rs` | absorbed | `int()` |
| `RoundTo<i8> for f32::round_to` | `merge/round_to.rs` | absorbed | `int()` |
| `RoundTo<u8> for f32::round_to` | `merge/round_to.rs` | absorbed | `int()` |
| `RoundTo<u16> for f32::round_to` | `merge/round_to.rs` | absorbed | `int()` |
| `RoundTo<IndexVTEX> for f32::round_to` | `merge/round_to.rs` | absorbed | `int()` |

## `repair/` — 14 functions

| fn | file | status | here |
|---|---|---|---|
| `has_difference` | `repair/cleaning.rs` | verified | `cleaning.differs`. The Rust routes through `ConflictResolver::average`, which returns `None` exactly when the two values are equal -- so it is an equality test, and it is false the moment either side is absent |
| `has_any_difference` | `repair/cleaning.rs` | gap | `cleaning.differs_any`. Tests all five layers; we tested heights -- fault 6 |
| `clean_landmass_diff` | `repair/cleaning.rs` | gap | `cleaning.clean_landmass`. Its opening `assert_eq!(repair_landmass_seams(landmass), 0)` -- the merge's post-condition -- was not ported at all; it is now `seams.find_tears` via `pipeline._check_borders` -- fault 10 |
| `clean_known_textures` | `repair/cleaning.rs` | ported | `textures.compact_textures`, plus a bug fix: the original's second loop is named `plugins` but iterates `masters`, so a mod's `LTEX` file name never reaches the output |
| `update_known_textures` | `repair/cleaning.rs` | ported | `KnownTextures.observe` |
| `add_vertex_colors` | `repair/debugging.rs` | ported | `debug_colors.paint_conflicts` |
| `add_debug_vertex_colors_to_landscape` | `repair/debugging.rs` | ported | `debug_colors.paint_conflicts` |
| `add_debug_vertex_colors_to_landmass` | `repair/debugging.rs` | ported | `debug_colors.paint_conflicts` over the landmass |
| `coords_with_offset` | `repair/seam_detection.rs` | absorbed | tuple arithmetic |
| `push_back_neighbors` | `repair/seam_detection.rs` | ported | `seams._border_pairs` |
| `sort_pair` | `repair/seam_detection.rs` | ported | `seams._border_pairs` orders each pair so a border is visited once |
| `repair_corner_seams` | `repair/seam_detection.rs` | gap | `seams.repair_corners`, extended with anchoring for corners shared with terrain outside the merge -- fault 5 |
| `repair_landmass_seams` | `repair/seam_detection.rs` | ported | `seams.repair_seams`. Called a second time by `clean_landmass_diff` as an assertion; that use is `seams.find_tears` |
| `try_repair_seam` | `repair/seam_detection.rs` | verified | `seams.repair_edges`, averaging through `seams.mean`, which truncates toward zero to match Rust's integer `/` rather than Python's flooring `//` |

## `io/` — 33 functions

| fn | file | status | here |
|---|---|---|---|
| `Default for MergeSettings::default` | `io/meta_schema.rs` | ported | `meta.MergeSettings` field defaults: `included=True`, `Auto` |
| `Default for MergeSettings::default_bool_true` | `io/meta_schema.rs` | absorbed | `included: bool = True` on the dataclass |
| `Default for MergeSettings::skip_default` | `io/meta_schema.rs` | replaced | serde's write-skip. We never write a settings file, only the `MergedLands` marker -- see `meta.write_merged_marker` |
| `parse_records` | `io/parsed_plugins.rs` | replaced | `service._records_via`: tes3conv JSON, not a TES3 reader |
| `read_lines` | `io/parsed_plugins.rs` | ported | `tools.survey_landscape.read_order`, which refuses a file with no plugin entries rather than returning every line |
| `is_esm` | `io/parsed_plugins.rs` | verified | `service._split_order`. Case-insensitive extension test, plus `.omwgame` |
| `is_esp` | `io/parsed_plugins.rs` | verified | `cleaning.is_mod`. Case-insensitive, plus `.omwaddon` |
| `sort_plugins` | `io/parsed_plugins.rs` | replaced | load order comes from the toolkit's own sort, not file mtimes |
| `meta_name` | `io/parsed_plugins.rs` | ported | `meta.meta_path_for` |
| `ParsedPlugin::empty` | `io/parsed_plugins.rs` | absorbed | `PluginRecords(name, [])` |
| `ParsedPlugin::from` | `io/parsed_plugins.rs` | absorbed | dataclass constructor |
| `PartialEq<Self> for ParsedPlugin::eq` | `io/parsed_plugins.rs` | absorbed | dataclass equality |
| `Hash for ParsedPlugin::hash` | `io/parsed_plugins.rs` | absorbed | plugins are keyed by their name string |
| `Hash for ParsedPlugin::read_ini_file` | `io/parsed_plugins.rs` | ported | `read_order` also parses `Morrowind.ini` `GameFile` lines |
| `ParsedPlugins::check_dir_exists` | `io/parsed_plugins.rs` | ported | `service.build_merged_lands` refuses an empty folder list; `service.resolve_plugin` skips a folder it cannot read |
| `ParsedPlugins::new` | `io/parsed_plugins.rs` | ported | `service.build_merged_lands` reading phase, over every `data=` folder |
| `save_resized_image` | `io/save_to_image.rs` | ported | `conflict_image._blit`, scaling by `SCALE = 4` |
| `GridAccessor2D<P> for ImageBuffer<P, Container>::get` | `io/save_to_image.rs` | absorbed | list indexing |
| `GridAccessor2D<P> for ImageBuffer<P, Container>::get_mut` | `io/save_to_image.rs` | absorbed | list assignment |
| `trait SaveToImage::save_to_image` | `io/save_to_image.rs` | ported | `conflict_image.cell_conflict_image` |
| `SaveToImage for RelativeTerrainMap<Vec3<i8>, T>::save_to_image` | `io/save_to_image.rs` | ported | same, vertex normals layer |
| `SaveToImage for RelativeTerrainMap<u16, T>::save_to_image` | `io/save_to_image.rs` | ported | same, texture indices layer |
| `SaveToImage for RelativeTerrainMap<Vec3<u8>, T>::save_to_image` | `io/save_to_image.rs` | ported | same, vertex colours layer |
| `SaveToImage for RelativeTerrainMap<Vec3<u8>, T>::calculate_min_max` | `io/save_to_image.rs` | ported | range scan inside `cell_conflict_image` |
| `SaveToImage for RelativeTerrainMap<u8, T>::save_to_image` | `io/save_to_image.rs` | ported | same, world map layer |
| `SaveToImage for RelativeTerrainMap<i32, T>::save_to_image` | `io/save_to_image.rs` | ported | same, height map layer |
| `SaveToImage for RelativeTerrainMap<i32, T>::save_image` | `io/save_to_image.rs` | ported | `wraithguard.images.png` writer -- no image crate needed |
| `SaveToImage for RelativeTerrainMap<i32, T>::save_landscape_images` | `io/save_to_image.rs` | ported | `conflict_image.cell_conflict_image` |
| `SaveToImage for RelativeTerrainMap<i32, T>::save_landmass_images` | `io/save_to_image.rs` | ported | `conflict_image.landmass_conflict_image` |
| `convert_landscape_diff_to_landscape` | `io/save_to_plugin.rs` | ported | `emit.build_landscape_record`. Diverges on flags: we declare only the layers we actually write |
| `convert_landmass_diff_to_landmass` | `io/save_to_plugin.rs` | ported | `service._build_records` |
| `to_master_record` | `io/save_to_plugin.rs` | gap | `service._contributors` + `service._write`. The master list is the plugins that *contributed*, not the load order's masters -- fault 8 |
| `save_plugin` | `io/save_to_plugin.rs` | gap | `service._write`, which now also writes the `.mergedlands.toml` marker beside the output -- fault 7 |

## `main.rs` — 27 functions

| fn | file | status | here |
|---|---|---|---|
| `Landmass::new` | `main.rs` | ported | `landmass.Landmass` |
| `Landmass::insert_land` | `main.rs` | ported | `Landmass.cells[coords] = ...` |
| `Landmass::sorted` | `main.rs` | ported | `sorted(landmass.cells)` |
| `LandmassDiff::new` | `main.rs` | ported | `pipeline.MergeOutcome` |
| `LandmassDiff::sorted` | `main.rs` | ported | `sorted(outcome.cells)` |
| `From<CliLevelFilter> for LevelFilter::from` | `main.rs` | n/a | log level mapping; the toolkit configures `logging` itself |
| `Cli::read_args` | `main.rs` | n/a | `tools/build_merged_lands.py` argparse |
| `Cli::plugins` | `main.rs` | ported | `tools.survey_landscape.read_order` |
| `Cli::should_write_log_file` | `main.rs` | n/a | the toolkit's own logging |
| `Cli::merged_lands_dir` | `main.rs` | ported | `--conflicts-dir` |
| `Cli::data_files_dir` | `main.rs` | ported | `service.resolve_plugin` over every `data=` folder |
| `Cli::output_file_dir` | `main.rs` | ported | `--out` |
| `Cli::stack_size` | `main.rs` | n/a | Rust thread stack sizing; the merge here is iterative, not recursive |
| `Cli::main` | `main.rs` | n/a | entry point |
| `Cli::wait_for_user_exit` | `main.rs` | n/a | console behaviour |
| `Cli::merge_all` | `main.rs` | verified | `pipeline.finish` + `service.build_merged_lands`, in the same order: reference, diff, merge, repair, clean, textures, convert, save |
| `Cli::init_log` | `main.rs` | n/a | logging setup |
| `Cli::try_copy_landscape_and_remap_textures` | `main.rs` | ported | `landmass._decode_cell` + `textures.translate_indices` |
| `Cli::try_create_landmass` | `main.rs` | ported | `landmass.build_reference` / `landmass.plugin_differences` |
| `Cli::merge_tes3_landscape` | `main.rs` | gap | `landmass.merge_master_layers`: masters combine per *layer*, flags unioned -- fault 3 |
| `Cli::merge_tes3_landmasses` | `main.rs` | ported | `landmass.build_reference` |
| `Cli::find_allowed_data` | `main.rs` | gap | `diff.diff_against_reference` intersects `allowed` with the record's declared flags -- fault 1 |
| `Cli::find_landmass_diff` | `main.rs` | ported | `landmass.plugin_differences` |
| `Cli::merge_landscape_diff` | `main.rs` | ported | `pipeline._fold_change` |
| `Cli::merge_landmass_into` | `main.rs` | ported | `pipeline.merge_landmass` |
| `Cli::create_tes3_landmass` | `main.rs` | ported | `landmass.build_reference` |
| `Cli::create_merged_lands_from_reference` | `main.rs` | gap | `pipeline.inherit_reference_layers` + `pipeline.add_reference_neighbours` -- faults 2 and 5 |

## The fourteen faults this audit found

Twelve would have shipped. Faults 12 and 13 were in the check that caught the
eleventh -- both false alarms that blocked a valid write rather than letting a
bad one through, which is the right way round for a check to fail. Fault 14 was
found by the game. None raised, crashed, or failed an existing test.
Every one was inside a function that a file-level audit had already marked
"ported".

Faults 1 to 5 came from the first pass, which read every function once. Faults 6
to 9 came from the second, which compared the remaining ones statement by
statement — and which only happened because the first table's own counts did not
add up. Fault 10 came from the third, which went through the functions the
second pass had *mapped* but never run -- and fault 11 came from running the
whole thing against a real load order, where fault 10's new post-condition
caught it.

### 1. Undeclared layers were merged as data

`find_allowed_data` starts from the record's own `landscape_flags`. We started
from the caller's choice alone. `DATA` says which grids a `LAND` record uses and
the engine ignores the rest — but tes3conv emits *every* grid, so an undeclared
one arrives full of zeros and diffed as *this mod flattened the terrain and
painted it black*.

Not hypothetical: of 290 real landscape records, **21 carry texture data the
flags do not declare, 20 carry vertex colours, 6 carry heights.**

Fixed in `diff_against_reference`; pinned by `TestUndeclaredLayersAreIgnored`.

### 2. Unchanged layers were dropped from the output

`create_merged_lands_from_reference` seeds every cell with the reference via
`from_reference`, so `to_terrain()` returns real terrain even for layers nobody
edited. We carried only *changed* layers.

A merged `LAND` record replaces the whole record. Carrying heights alone leaves
the cell with no texture data at all — the mod's and the game's both gone, since
the record holding them has been superseded. **Untextured terrain, silently.**

Measured before the fix: of 24 cells written from two Solstheim mods, **13 lost
textures the reference had, 14 lost vertex colours, 12 lost the world map.**
After: zero. The plugin grew from 188 KB to 729 KB, which is the correct size
for terrain that is actually complete.

Fixed by `pipeline.inherit_reference_layers`; pinned by
`TestUnchangedLayersSurvive`.

### 3. Masters were combined wholesale, not per layer

`merge_tes3_landscape` starts from the earlier master and replaces only the
layers the later one both *declares* and *carries*, unioning the flags. We
replaced the whole record.

A master redefining a cell with heights alone therefore erased the earlier
master's vertex colours and textures. Invisible on a stock install — Tribunal
has no landscape and Bloodmoon is Solstheim, so the vanilla three never overlap
— and live the moment `Tamriel_Data.esm`, `OAAB_Data.esm` or any other
master-flagged expansion is present, which is most large load orders.

Fixed in `landmass.merge_master_layers`; pinned by `TestMastersCombinePerLayer`.

### 4. Hand-authored normals were discarded

`recompute_vertex_normals` recomputes from the merged heights but keeps the
*original* normal at any vertex whose height did not change. We recomputed all
of them, discarding normals a mod authored deliberately to fake a lighting
effect its geometry does not produce.

Fixed in `pipeline.resolve_normals`, which also moves normal computation to
after seam repair and the slope limiter — they are a function of the final
heights, and computing them earlier would describe terrain that no longer
exists. Pinned by `TestNormalsFollowTheHeights`.

### 5. Torn borders against terrain outside the merge

Three faults in one place, each uncovered by fixing the last.

**Only modified cells were merged**, so a merged cell's border with untouched
vanilla was never reconciled — 16 borders disagreeing, worst 5,024 world units.
Fixed by borrowing reference neighbours before repair.

**Only orthogonal neighbours were borrowed.** A corner is shared by *four*
cells, so the diagonal was still absent and its corner unreconcilable. Fixed by
borrowing diagonally as well.

**Where a sharing cell genuinely is not written**, averaging the rest moves our
side of the ground and not theirs. Pinning all four was tried and is worse: the
cells we *do* write then keep three different values and tear against each other
(4 borders, worst 3,944). The rule that works is **anchoring** — the absent
ground is not going to move, so it decides, and every present cell adopts the
reference height. Where the absent cell has no terrain at all, there is nothing
to tear against and the present cells simply agree among themselves.

Result on the Solstheim pair: **0 torn borders**, against 16 at the start.
Pinned by `TestNoTornBorders`.

### 6. Cleaning judged on heights alone

`has_any_difference` tests heights, normals, world map, colours **and**
textures. We tested heights.

A cell where one mod repainted the textures and another recoloured the vertices
has heights identical to the reference, so it was dropped as "unmodified". Once
the merged record is gone the load order resolves that cell by last-wins, and
one of the two edits disappears. Nothing in the merge reports it — heights are
what the merge talks about, and heights were fine.

Fixed in `cleaning.differs_any`. Grids are compared by 32-byte BLAKE2b digest
rather than by value: holding five real grids for every single-editor cell would
cost about 120 KB a cell against 17 KB for heights alone — over a gigabyte on a
large load order — for a comparison that only ever asks *are these the same*.
Pinned by `TestCleaningJudgesEveryLayer`.

### 7. The output was not marked as generated

`save_plugin` writes a `.mergedlands.toml` beside the plugin with
`meta_type = MergedLands`, and `merge_all` skips any plugin whose meta says
that. We read the marker and never wrote one.

`Merged Lands.esp` is an `.esp` that edits every cell it wrote and loads last,
so the second run read it as a mod and reconciled its terrain as one more
opinion — a merge of a merge. Nothing fails; the terrain drifts a little further
from what any author wrote on every run, and the drift is invisible because the
tool has no memory of the previous output.

Fixed by `meta.write_merged_marker`, called from `service.build_merged_lands`
immediately after the write. Failure to write it raises rather than warns: a
merged plugin with no marker beside it is a trap for the next run.

### 8. Every master was declared, contributors were not

`save_plugin` builds its master list from the plugins that actually
contributed — every plugin behind a surviving `LTEX`, every plugin that edited a
written cell — and sorts it by load order. We declared the load order's masters.

Both halves were wrong. Declaring twenty-seven masters when five were read is
noise, and *not* declaring the `.esp` mods the terrain came from means the
merged file will happily load without them, describing edits to land that is no
longer there. A TES3 master list is a dependency list; nothing stops an `.esp`
being on it, and putting it there is what makes the engine refuse rather than
render.

We add one thing the original gets for free through its `LandscapeDiff.plugins`
chain: the master supplying each written cell's *reference* terrain is declared
too. A `LAND` record is an override of the record in the file that first defined
that cell, and a merged file that does not name `Morrowind.esm` is not a patch
of Morrowind.

Fixed in `service._contributors`; pinned by `TestDeclaredMasters`.

### 9. Deleted records were read as terrain

`add_next_texture` and `merge_tes3_landscape` both assert on a record carrying
`ObjectFlags::DELETED`. We had no check at all, so a deleted `LTEX` entered the
shared table — reinstating a texture a mod removed and occupying an index — and
a deleted `LAND` was diffed as an edit, merging whatever stale grid happened to
be in the file.

Fixed by `diff.is_deleted`, consulted in `textures.observe` and
`landmass._landscape_records`. We skip and log where the original aborts:
ending a nine-hundred-mod merge because one plugin deleted one texture is not a
useful outcome, and skipping leaves the reference terrain standing, which is
what a terrain-only patch should do with a record it cannot interpret.

### 10. The merge had no post-condition

`clean_landmass_diff` opens with
`assert_eq!(repair_landmass_seams(landmass), 0)`. It repairs the seams, then
repairs them **again** and requires the second pass to find nothing. We ported
the repair and dropped the assertion — the function was marked "ported" because
the cleaning it does is there, and the first line of it is not cleaning.

That left the one defect a player sees in the first minute with no check at
all: a wall or a chasm along a cell boundary, in a game where cells are 8,192
units across and every boundary is somewhere you walk.

And it matters more here than in the original, because more runs after the
repair. The slope limiter moves vertices. Feathering moves vertices. Cleaning
removes whole cells. Each is written to preserve borders — `_shift` moves every
copy of a shared vertex in lockstep and refuses outright when a sharing cell is
absent; cleaning keeps any single-source cell the repair moved — and each is a
place a future change could quietly stop doing so. The 0-tear result the port
was signed off with came from a throwaway script, not from anything that would
run again.

`seams.find_tears` is now the post-condition, called from
`pipeline._check_borders` after everything has finished moving and against the
cells that will actually be **written**, not the working set. Borders with cells
the merge is *not* writing are checked too, against the reference: a written
cell does not get to be merely self-consistent, because the ground next door
still exists and is not going to move.

`service.build_merged_lands` **refuses to write** when a tear survives, naming
the border. Shipping visibly broken terrain is worse than failing, and this is a
defect here rather than anything the user did.

Pinned by `TestNothingTearsAtTheEnd`, which includes a negative control —
the same merge with repair disabled, which must report exactly one tear.
A post-condition that cannot fail is decoration.

### 11. Borrowed vanilla cells were averaged, not adopted

`add_reference_neighbours` is ours, not the original's -- Merged Lands seeds
the whole reference and cleans at the end, we borrow only the ring that borders
an edit. But a borrowed cell was then treated as an equal party: `repair_edges`
averaged the merged cell against it and moved both.

That moves *vanilla*. And the cell one further out -- not borrowed, still
holding its original heights -- does not follow. The tear is not removed; it is
relocated one cell away from the edit, into terrain no mod asked to change.

Measured on a real 27-master, 940-mod order: **25 borders genuinely
disagreeing.** (The run reported 783; 758 of those were fault 12, below. The
initial write-up of this fault quoted the whole 783 and the 17,560-unit figure,
both of which belonged to the false positive. The real defect is smaller than
that and worth stating at its true size.)

It is small because `try_repair_seam` only moves the 65 vertices *on* a shared
border, so averaging a merged cell against a borrowed one does not disturb the
borrowed cell's other three sides. What does tear is corners: a corner vertex is
shared by four cells, and `repair_corner_seams` moves it in all of them.

A borrowed cell is now **authoritative**: it is a copy of what the game already
has, so the merged side adopts its heights whole and it never changes. Two
consequences beyond the fix -- the slope limiter has to respect the same rule
(it runs afterwards, and moving one of those vertices would undo the repair),
and a borrowed cell now always survives to cleaning unmodified, so it is
dropped rather than written.

**The guard has to hold in two places, and the first attempt only held in one.**
Seam repair honouring it is not enough: the slope limiter sweeps the same grids
afterwards, and a large mod-versus-vanilla difference gives it plenty to correct.
`_is_movable` learned the rule, but `limit_slopes` called it without passing
`authoritative`, so the check was inert and the limiter went on editing vanilla.
An 800-unit test difference was too small to expose that; 18,000 -- the scale a
real load order reaches -- was not.

Pinned by `TestBorrowedCellsAreNotMoved`: a negative control that removes the
guard and checks vanilla gets dragged, a test that runs repair and the limiter
in sequence and requires vanilla untouched after *both*, and an end-to-end case
at 18,000 units.

### 12. The post-condition invented tears where cleaning had dropped a cell

Fault 10 added the check the port was missing. It was then run *after*
cleaning, on the cells that survive -- which sounds stricter and is simply
wrong.

Cleaning drops a cell exactly when the load order already delivers that
terrain: either nothing edited it, or one mod did and its own record produces
the same result. The ground is still there in the game; it just comes from a
different file. Treating it as absent and falling back to the reference
measures a merged cell against *vanilla* when the terrain next door is the
mod's, and reports a tear the size of that mod's edit.

On the same 940-mod order: **758 borders reported, the worst 17,560 units, for
borders that were intact** -- and the check refused to write the plugin over it.
The tell was that the worst tear did not move at all when fault 11 was fixed,
because it was never a tear.

The check now runs on the repaired landmass, before cleaning. That is also the
stronger statement, not the weaker one: cleaning cannot open a border the check
closed, because it only ever removes a record whose content the load order
reproduces exactly.

Pinned by `TestTheCheckDoesNotInventTears`, which includes the case that was
misreported *and* an unrepaired landmass that must still be caught -- a check
relaxed into uselessness would pass the first test alone.

### 13. The post-condition blamed the tool for vanilla's own seams

Fixing fault 12 moved the check before cleaning, which put the borrowed cells
in front of it for the first time. Those are the cells the merge does not edit
and will not write, and `repair_edges` refuses to move either side of a border
two of them share -- whatever they disagree about is the game's own, predates
the merge, and survives whether or not the tool runs. Two adjacent masters
disagreeing at a province boundary is an ordinary thing in a 27-master order.

The check did not know that, and reported them: **62 borders, the worst 2,648
units**, on terrain nothing in the output touches. It then refused to write.

`find_tears` now takes the borrowed set and skips borders where *both* sides are
in it. A border with *one* of them is still checked -- that is the entire reason
they were borrowed, and it is what fault 11 was about.

Pinned by `TestVanillaSeamsAreNotOurs`, which checks all four combinations:
neither side ours, one side ours, both sides ours, and a borrowed cell's border
with terrain further out.

### 14. A Skyrim plugin was read as Morrowind terrain

The first merged plugin that ever passed every check crashed OpenMW on load::

    ESM4::Reader::updateModIndices required dependency 'Skyrim.esm' not found

`ESM4` is OpenMW's Oblivion/Skyrim reader. It only runs on a file whose header
magic is `TES4`, so the merged plugin had dragged a Skyrim plugin into the load.

`native.py` had **no format check at all**, and every step of the chain worked
exactly as designed:

1. Skyrim has `LAND` records too, so the byte pre-scan matched.
2. tes3conv correctly refused the file -- `TES4` is not a format it reads.
3. That refusal is precisely when we fall back to the native reader.
4. Which walked it with a 16-byte record header. Skyrim's are 24. It did not
   fail; it *succeeded*, inventing landscape out of whatever the bytes spelled.
5. Those cells entered the merge, so the plugin was declared a contributing
   master -- and OpenMW then had to load it.

The tolerance that makes this reader worth having -- *skip anything you do not
recognise, by its declared length* -- is exactly what makes it dangerous on a
format whose lengths are laid out differently. It survives unknown **records**;
it must refuse unknown **formats**.

`read_landscape_records` now raises unless the file begins `TES3`, naming what
it found instead, and `has_landscape` returns false for the same reason rather
than sending it down the fallback path.

Pinned by `TestForeignFormatsAreRefused`, using a genuine `TES4` fixture with a
`LAND` record and a `Skyrim.esm` master -- the shape that caused this. Adding
the guard also broke five older tests that built plugins with no header at all,
which no real plugin does; those fixtures were wrong and were fixed rather than
the guard relaxed.

**This is the first fault the game found.** Twelve were found by reading, one by
running the tool, and this one needed the engine. It is the argument for the
last gate being a load, not a test.

## The bug that stopped a real load order

Separately from the parity sweep, the first run against a real 27-master, 940-mod
order failed at `could not read master distant_seafloor_2.00.esm`, and the
message was all the user got. Two faults combined:

* **One data folder was searched.** OpenMW composes a load order from many
  `data=` directories — one per mod is normal — and the toolkit passed whichever
  single folder held the first plugin it found. Every master installed elsewhere
  was unreachable. Fixed by `service.resolve_plugin`, which searches every scan
  directory and matches case-insensitively, because `openmw.cfg` and the
  filesystem routinely disagree (`RepopulatedMorrowind.ESM` against
  `…​.esm`).
* **The reason was discarded.** `_records_via` collapsed a converter error, a
  600-second timeout, a JSON parse failure and an out-of-memory into the same
  empty list, and `stderr` was never read. It now returns the reason, and the
  master that stops the merge says which of those happened.

A mod that cannot be found is no longer fatal either — a load order may list
something the user removed. A *master* still is: it is the reference terrain
everything else is measured against.

## The other direction: what is ours

The tables above prove *their* 191 functions all landed somewhere here. They say
nothing about the reverse, and the reverse is now the larger risk. Of **158
functions in `wraithguard/land`, only 83 trace to a Rust counterpart.**

Most of the remaining 75 is Python plumbing -- flattening helpers, digest
properties, report accessors, the tes3conv-JSON byte packing that replaces
`tes3`'s own `Save`. But three modules move vertices Merged Lands never moves,
and this is where "faithful port" stops being the right description:

| module | what it does that the original does not |
|---|---|
| `slope.py` | **No counterpart at all.** The original clamps at encode time and accepts the drift; this conditions the terrain first so it does not have to. It moved 241,342 vertices on a real load order |
| `seams.py` | The seam repair is a port; the **inward feathering, the corner anchoring, the authoritative rule and `find_tears` are not.** Each moves vertices, or refuses to, on rules the original has no notion of |
| `curvature.py` | No counterpart. Opt-in only -- `ConflictStrategy.CURVATURE` is never a default, so it changes nothing unless a `.mergedlands.toml` asks for it |

Everything else is marked `ours-aux`: no counterpart, but it decides no vertex
value. `native.py` reads records. `service.py` sequences the run.

`tools/gen_merged_lands_table.py` carries this classification as `MODULES`, and
`TestTheReverseDirection` fails if a module in `wraithguard/land` has no entry,
or if a *fourth* module ever joins the terrain-changing list. A short, named
list is the only thing that makes the claim about this port honest.

**What could not be checked.** A runtime differential -- build Merged Lands,
run both on the same load order, diff the plugins -- is the verification this
port really wants, and it is not available: the repository ships source only,
it pins `nightly-2022-07-26`, and there is no Rust toolchain here. Everything
above is a static correspondence. It is why the twelve faults were found by
reading rather than by running, and why the thirteenth was found by you running
it.

## Deliberate divergences

| what | why |
|---|---|
| `Resolve` refused on `texture_indices` | averaging index 3 and 7 gives index 5 — a third, unrelated texture |
| new land never averaged | no common ancestor; blending two authored landscapes makes a third nobody wrote |
| unencodable gradient reported, not fatal | the original asserts and aborts the whole run on one cell |
| seam corrections feathered inward | the original's boundary-only repair left 125 unencodable vertices; feathering leaves 6 |
| slope limiter | **the original has none.** Its only gradient handling anywhere in `src/` is `truncate_gradient`, called at encode time, which clamps a delta to `i8` and moves on -- so wherever a merged step exceeds 1,016 units it silently writes terrain different from what it computed, and because heights are cumulative sums the shift carries along the rest of that row. `try_calculate_height_map` asserts the round-trip on *input* terrain, which came from a file and is encodable by construction; nothing checks the merged output. Our limiter conditions the terrain instead, and what it cannot fix it reports |
| reference *neighbours* borrowed, not the whole landmass | same borders reconciled, a fraction of the memory |
| `merge_cell_into` reads the incoming value | the original clones `new` from itself, so region, map colour, water height and atmosphere never update |
| `.omwaddon` counts as a mod | OpenMW's name for the same format; the original checks `.esp` only |
| deleted `LTEX`/`LAND` skipped, not fatal | the original asserts; one deleted record should not end a nine-hundred-mod merge |
| `landscape_flags` declare only live layers | the original always writes `0x1|0x2|0x4|0x8`. The `0x8` bit is unnamed in `tes3` and emitting it is rejected outright; declaring a layer whose grid is absent asks the engine to load data that is not there |
| master sizes from `stat()` | the original uses the on-disk (`file_real_size`) size; the header field is a sanity check, and the apparent size is what every other tool writes |
| `clean_known_textures` refreshes file names from mods too | the original iterates `parsed_plugins.masters` twice — the second loop is named `plugins` but reads `masters`, so a mod's `LTEX` file name never reaches the output |
