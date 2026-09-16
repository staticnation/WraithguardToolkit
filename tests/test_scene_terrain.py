"""Tests for ``wraithguard.scene.terrain`` and exterior terrain wiring.

Landscape records are built directly (no ESP round-trip); heights are decoded
through the same code Merge Lands uses, so these check the mesh geometry, world
placement, colour gating, and that an exterior preview picks the winning ``LAND``.
"""

from __future__ import annotations

import struct

from wraithguard.esp.flags import CellFlags, LandscapeFlags
from wraithguard.esp.records.cell import Cell, CellData
from wraithguard.esp.records.landscape import Landscape
from wraithguard.esp.records.landscapetexture import LandscapeTexture
from wraithguard.scene.cellview import (
    CellKey,
    LoadedPlugin,
    _landscape_textures,
    find_landscape,
    preview_cell_instanced,
)
from wraithguard.scene.terrain import (
    _cell_textures,
    _corner_weights,
    _deswizzled_vtex,
    _texture_at,
    _vertex_texture_weights,
    has_terrain,
    landscape_textures,
    terrain_blend_meshes,
    terrain_mesh,
    terrain_meshes,
)
from wraithguard.scene.water import SEA_LEVEL, water_mesh
from wraithguard.tes3fields.landscape import LAND_CELL_UNITS, LAND_NUM_VERTS, LAND_SIZE

_HEIGHTS = LandscapeFlags.USES_VERTEX_HEIGHTS_AND_NORMALS
_COLORS = LandscapeFlags.USES_VERTEX_COLORS
_TEXTURES = LandscapeFlags.USES_TEXTURES


def _vtex(values: list[int]) -> bytes:
    """A 16x16 VTEX blob (256 unsigned shorts) from a flat list of values."""
    padded = (values + [0] * 256)[:256]
    return struct.pack("<256H", *padded)


def _flat_land(
    grid: tuple[int, int],
    *,
    flags: LandscapeFlags = _HEIGHTS,
    vclr: bytes = b"",
    offset: float = 0.0,
) -> Landscape:
    """A landscape with a flat height grid (all deltas zero) at ``grid``."""
    return Landscape(
        grid=grid,
        landscape_flags=flags,
        vertex_heights_offset=offset,
        vertex_heights=bytes(LAND_NUM_VERTS),  # all-zero deltas -> flat at offset
        vertex_colors=vclr or bytes(LAND_NUM_VERTS * 3),
    )


class TestHasTerrain:
    def test_a_record_with_the_heights_flag_has_terrain(self) -> None:
        assert has_terrain(_flat_land((0, 0)))

    def test_a_record_without_it_has_none(self) -> None:
        assert not has_terrain(_flat_land((0, 0), flags=LandscapeFlags(0)))


class TestTerrainMesh:
    def test_the_grid_becomes_a_full_vertex_and_triangle_count(self) -> None:
        mesh = terrain_mesh(_flat_land((0, 0)))
        assert len(mesh.vertices) == LAND_NUM_VERTS  # 65 x 65
        assert len(mesh.triangles) == (LAND_SIZE - 1) * (LAND_SIZE - 1) * 2  # 64 x 64 x 2

    def test_vertices_sit_at_the_cell_world_origin(self) -> None:
        mesh = terrain_mesh(_flat_land((2, -3)))
        assert mesh.vertices[0] == (2 * LAND_CELL_UNITS, -3 * LAND_CELL_UNITS, 0.0)
        # The far corner is one whole cell away on each axis.
        assert mesh.vertices[-1] == (
            2 * LAND_CELL_UNITS + LAND_CELL_UNITS,
            -3 * LAND_CELL_UNITS + LAND_CELL_UNITS,
            0.0,
        )

    def test_the_offset_raises_the_whole_grid(self) -> None:
        land = _flat_land((0, 0))
        land.vertex_heights_offset = 10.0  # stored units; * HEIGHT_SCALE (8) = 80
        mesh = terrain_mesh(land)
        assert mesh.vertices[0][2] == 80.0

    def test_vertex_colors_are_read_when_the_flag_is_set(self) -> None:
        land = _flat_land((0, 0), flags=_HEIGHTS | _COLORS, vclr=b"\xff\x00\x80" * LAND_NUM_VERTS)
        mesh = terrain_mesh(land)
        assert mesh.vertex_colors[0] == (1.0, 0.0, 128 / 255.0, 1.0)

    def test_vertex_colors_are_dropped_without_the_flag(self) -> None:
        # Default all-zero colour bytes must not paint the terrain black.
        assert terrain_mesh(_flat_land((0, 0))).vertex_colors == []


class TestLandscapeTextures:
    def test_it_maps_ltex_index_to_file_last_wins(self) -> None:
        records = [
            LandscapeTexture(id="a", index=0, file_name="tx_grass.dds"),
            LandscapeTexture(id="b", index=1, file_name="tx_rock.dds"),
            LandscapeTexture(id="a2", index=0, file_name="tx_grass_hd.dds"),  # later wins
        ]
        assert landscape_textures(records) == {0: "tx_grass_hd.dds", 1: "tx_rock.dds"}


class TestTerrainTexturing:
    def _textured_land(self, vtex_values: list[int]) -> Landscape:
        land = _flat_land((0, 0), flags=_HEIGHTS | _TEXTURES)
        land.texture_indices = _vtex(vtex_values)
        return land

    def test_the_dominant_texture_is_chosen_and_uvs_added(self) -> None:
        # LTEX index 5 -> stored as VTEX value 6 (index + 1). Make it the winner.
        paths = {5: "tx_ground.dds", 2: "tx_road.dds"}
        land = self._textured_land([6, 6, 6, 3])  # three 6s (idx 5), one 3 (idx 2)
        mesh = terrain_mesh(land, paths)
        assert mesh.texture == "tx_ground.dds"
        assert len(mesh.uvs) == LAND_NUM_VERTS

    def test_no_texture_map_leaves_the_terrain_untextured(self) -> None:
        land = self._textured_land([6, 6])
        mesh = terrain_mesh(land)  # no paths passed
        assert mesh.texture == "" and mesh.uvs == []

    def test_the_textures_flag_is_required(self) -> None:
        land = _flat_land((0, 0), flags=_HEIGHTS)  # no USES_TEXTURES
        land.texture_indices = _vtex([6, 6])
        assert terrain_mesh(land, {5: "tx_ground.dds"}).texture == ""

    def test_unpainted_grid_stays_untextured(self) -> None:
        # All-zero VTEX means "nothing painted"; no dominant texture exists.
        land = self._textured_land([0, 0, 0])
        assert terrain_mesh(land, {5: "tx_ground.dds"}).texture == ""


class TestTerrainMeshEdgeCases:
    def test_a_textures_flag_with_no_vtex_bytes_stays_untextured(self) -> None:
        # USES_TEXTURES set, paths given, but the VTEX field is too short to hold
        # even one texture -- the dominant lookup must give up, not index a hole.
        land = _flat_land((0, 0), flags=_HEIGHTS | _TEXTURES)
        land.texture_indices = b""
        assert terrain_mesh(land, {5: "tx_ground.dds"}).texture == ""

    def test_a_short_vertex_colour_grid_paints_nothing(self) -> None:
        # The colours flag is set but the VCLR bytes are too short for the grid:
        # better no colour than a partial one applied to the wrong vertices.
        land = _flat_land((0, 0), flags=_HEIGHTS | _COLORS, vclr=b"\xff\x00\x80")  # one vertex
        assert terrain_mesh(land).vertex_colors == []


def _two_texture_land() -> Landscape:
    """A cell whose VTEX paints two textures over its halves (LTEX 5 and 2)."""
    land = _flat_land((0, 0), flags=_HEIGHTS | _TEXTURES)
    land.texture_indices = _vtex([6] * 128 + [3] * 128)  # value 6 -> idx 5, value 3 -> idx 2
    return land


_TWO_PATHS = {5: "tx_ground.dds", 2: "tx_road.dds"}


def _three_texture_land() -> Landscape:
    """A fully painted cell whose VTEX names three textures (LTEX 5, 2 and 8)."""
    land = _flat_land((0, 0), flags=_HEIGHTS | _TEXTURES)
    land.texture_indices = _vtex([6] * 86 + [3] * 85 + [9] * 85)  # values -> idx 5, 2, 8
    return land


_THREE_PATHS = {5: "tx_a.dds", 2: "tx_b.dds", 8: "tx_c.dds"}


class TestTerrainSplat:
    def test_two_textures_become_two_meshes_covering_the_whole_grid(self) -> None:
        meshes = terrain_meshes(_two_texture_land(), _TWO_PATHS)
        assert {m.texture for m in meshes} == {"tx_ground.dds", "tx_road.dds"}
        # Every mesh shares the full vertex grid; between them they carry all quads.
        assert all(len(m.vertices) == LAND_NUM_VERTS for m in meshes)
        total_tris = sum(len(m.triangles) for m in meshes)
        assert total_tris == (LAND_SIZE - 1) * (LAND_SIZE - 1) * 2
        assert all(len(m.uvs) == LAND_NUM_VERTS for m in meshes)  # textured -> UVs

    def test_no_paths_falls_back_to_a_single_plain_mesh(self) -> None:
        meshes = terrain_meshes(_two_texture_land())  # no paths
        assert len(meshes) == 1 and meshes[0].texture == ""

    def test_a_short_vtex_grid_falls_back_to_one_mesh(self) -> None:
        land = _flat_land((0, 0), flags=_HEIGHTS | _TEXTURES)
        land.texture_indices = b""  # too short to de-swizzle
        assert len(terrain_meshes(land, _TWO_PATHS)) == 1

    def test_painted_but_unresolved_indices_give_one_plain_mesh(self) -> None:
        # The grid paints textures, but none of the indices are in the path map,
        # so every quad lands in the "" bucket -- a plain mesh, not a splat.
        meshes = terrain_meshes(_two_texture_land(), {99: "tx_other.dds"})
        assert len(meshes) == 1 and meshes[0].texture == "" and meshes[0].uvs == []


class TestTerrainBlend:
    def test_two_textures_become_ordered_layers_base_opaque(self) -> None:
        meshes = terrain_blend_meshes(_two_texture_land(), _TWO_PATHS)
        assert len(meshes) == 2
        # Layers are numbered base-first and each carries the full surface.
        assert [m.blend_layer for m in meshes] == [0, 1]
        assert all(len(m.vertex_colors) == LAND_NUM_VERTS for m in meshes)
        # The base is drawn opaque so the ground is never a hole.
        assert all(rgba[3] == 1.0 for rgba in meshes[0].vertex_colors)
        # The upper layer fades: some vertices are transparent, some opaque.
        upper_alphas = {rgba[3] for rgba in meshes[1].vertex_colors}
        assert upper_alphas != {1.0}

    def test_a_layer_keeps_the_cells_own_vertex_colour_in_rgb(self) -> None:
        land = _two_texture_land()
        land.landscape_flags |= _COLORS
        land.vertex_colors = b"\x40\x80\xc0" * LAND_NUM_VERTS
        base = terrain_blend_meshes(land, _TWO_PATHS)[0]
        r, g, b, _a = base.vertex_colors[0]
        assert (round(r, 3), round(g, 3), round(b, 3)) == (
            round(0x40 / 255, 3),
            round(0x80 / 255, 3),
            round(0xC0 / 255, 3),
        )

    def test_a_single_texture_cell_is_one_opaque_layer(self) -> None:
        land = _flat_land((0, 0), flags=_HEIGHTS | _TEXTURES)
        land.texture_indices = _vtex([6] * 256)  # one texture everywhere
        meshes = terrain_blend_meshes(land, {5: "tx_ground.dds"})
        assert len(meshes) == 1 and meshes[0].texture == "tx_ground.dds"

    def test_no_texture_flag_falls_back_to_a_plain_mesh(self) -> None:
        land = _two_texture_land()
        land.landscape_flags = _HEIGHTS  # drop USES_TEXTURES
        assert len(terrain_blend_meshes(land, _TWO_PATHS)) == 1

    def test_a_short_vtex_grid_falls_back_to_one_mesh(self) -> None:
        land = _flat_land((0, 0), flags=_HEIGHTS | _TEXTURES)
        land.texture_indices = b""
        assert len(terrain_blend_meshes(land, _TWO_PATHS)) == 1

    def test_unpainted_cells_are_skipped_when_counting_layers(self) -> None:
        # Two real textures plus a block of unpainted (0) cells: the blank cells
        # must not become a layer, but the two textures still blend.
        land = _flat_land((0, 0), flags=_HEIGHTS | _TEXTURES)
        land.texture_indices = _vtex([6] * 100 + [3] * 100 + [0] * 56)
        meshes = terrain_blend_meshes(land, _TWO_PATHS)
        assert {m.texture for m in meshes} == {"tx_ground.dds", "tx_road.dds"}

    def test_the_layers_composite_to_each_textures_exact_coverage(self) -> None:
        # The base-bleed guard: composited "over" one another, the layer alphas
        # must reproduce each texture's coverage weight at every vertex -- so the
        # opaque base contributes only its own share, never bleeding through the
        # seams where three textures meet.
        land = _three_texture_land()
        meshes = terrain_blend_meshes(land, _THREE_PATHS)
        assert len(meshes) == 3
        cell_tex = _cell_textures(_deswizzled_vtex(land.texture_indices), _THREE_PATHS)
        for vertex in range(LAND_NUM_VERTS):
            row, col = divmod(vertex, LAND_SIZE)
            weights = _vertex_texture_weights(cell_tex, row, col)
            # Front-to-back "over": each layer contributes alpha * (light still
            # getting through from above), and dims what reaches the layers below.
            contrib: dict[str, float] = {}
            transmit = 1.0
            for mesh in reversed(meshes):
                alpha = mesh.vertex_colors[vertex][3]
                contrib[mesh.texture] = alpha * transmit
                transmit *= 1.0 - alpha
            for mesh in meshes:
                assert abs(contrib[mesh.texture] - weights.get(mesh.texture, 0.0)) < 1e-6


class TestTerrainHelpers:
    def test_deswizzle_returns_a_16x16_grid_and_rejects_short_input(self) -> None:
        grid = _deswizzled_vtex(_vtex([6] * 256))
        assert len(grid) == 16 and all(len(row) == 16 for row in grid)
        assert _deswizzled_vtex(b"\x00\x00") == []

    def test_texture_at_reads_the_containing_vtex_cell(self) -> None:
        grid = _deswizzled_vtex(_vtex([6] * 256))  # value 6 -> LTEX index 5
        assert _texture_at(grid, 0, 0, {5: "tx_ground.dds"}) == "tx_ground.dds"
        # A painted cell whose index is unknown resolves to "".
        assert _texture_at(grid, 0, 0, {}) == ""
        # An unpainted cell (value 0) is "" whatever the paths say.
        empty = _deswizzled_vtex(_vtex([0] * 256))
        assert _texture_at(empty, 0, 0, {5: "tx_ground.dds"}) == ""

    def test_cell_textures_resolves_every_cell(self) -> None:
        grid = _deswizzled_vtex(_vtex([6] * 256))
        resolved = _cell_textures(grid, {5: "tx_ground.dds"})
        assert len(resolved) == 16 and resolved[0][0] == "tx_ground.dds"
        # Unpainted grid resolves to all "".
        assert _cell_textures(_deswizzled_vtex(_vtex([0] * 256)), {5: "x"})[0][0] == ""

    def test_corner_weights_clamp_at_both_edges(self) -> None:
        # The south/west edge samples cannot fall below cell 0...
        low0, high0, _f0 = _corner_weights(0)
        assert low0 == 0 and high0 == 0
        # ...nor the north/east edge above the last cell.
        low64, high64, _f64 = _corner_weights(64)
        assert low64 == 15 and high64 == 15

    def test_vertex_texture_weights_sum_to_one(self) -> None:
        cell_tex = _cell_textures(_deswizzled_vtex(_two_texture_land().texture_indices), _TWO_PATHS)
        weights = _vertex_texture_weights(cell_tex, 32, 32)  # near the seam
        assert abs(sum(weights.values()) - 1.0) < 1e-6


class TestFindLandscape:
    def test_the_last_plugin_with_the_grid_wins(self) -> None:
        early = _flat_land((1, 1))
        late = _flat_land((1, 1))
        plugins = [
            LoadedPlugin("A.esp", [], [early]),
            LoadedPlugin("B.esp", [], [_flat_land((9, 9))]),  # a different grid
            LoadedPlugin("C.esp", [], [late]),
        ]
        assert find_landscape(plugins, (1, 1)) is late

    def test_a_grid_no_plugin_touches_is_none(self) -> None:
        plugins = [LoadedPlugin("A.esp", [], [_flat_land((0, 0))])]
        assert find_landscape(plugins, (5, 5)) is None


class TestPerPluginLandTextures:
    """LTEX indices are per-plugin; a LAND resolves against its own plugin chain.

    The bug this guards: Tamriel Rebuilt (and any big landmass mod) numbers its
    land textures from 0, right over the vanilla range. A global last-wins table
    then makes the later mod's textures bleed over an earlier cell's terrain.
    """

    def test_a_land_resolves_against_its_own_plugin_not_a_later_reuser(self) -> None:
        base = LoadedPlugin(
            "Base.esm", [], [LandscapeTexture(id="b", index=0, file_name="base.tga")]
        )
        # A later mod (master Base) reuses index 0 for a different texture.
        mod = LoadedPlugin(
            "Mod.esm", ["Base.esm"], [LandscapeTexture(id="m", index=0, file_name="mod.tga")]
        )
        plugins = [base, mod]
        assert _landscape_textures(plugins, base) == {0: "base.tga"}  # not mod.tga
        assert _landscape_textures(plugins, mod) == {0: "mod.tga"}  # its own wins over its master

    def test_a_non_master_plugins_indices_do_not_leak_in(self) -> None:
        base = LoadedPlugin(
            "Base.esm", [], [LandscapeTexture(id="b", index=0, file_name="base.tga")]
        )
        other = LoadedPlugin(
            "Other.esm", [], [LandscapeTexture(id="o", index=0, file_name="other.tga")]
        )
        plugins = [base, other]  # Other loads after Base but is not its master
        assert _landscape_textures(plugins, base) == {0: "base.tga"}

    def test_a_plugin_before_the_owner_that_is_not_a_master_is_ignored(self) -> None:
        # Load order: an unrelated mod, then the owner's master, then the owner.
        unrelated = LoadedPlugin(
            "Unrelated.esm", [], [LandscapeTexture(id="u", index=0, file_name="unrelated.tga")]
        )
        base = LoadedPlugin(
            "Base.esm", [], [LandscapeTexture(id="b", index=1, file_name="base.tga")]
        )
        mod = LoadedPlugin(
            "Mod.esm", ["Base.esm"], [LandscapeTexture(id="m", index=0, file_name="mod.tga")]
        )
        # Only Base (a master) and Mod (the owner) contribute; Unrelated is skipped.
        assert _landscape_textures([unrelated, base, mod], mod) == {0: "mod.tga", 1: "base.tga"}

    def test_an_owner_absent_from_the_load_order_yields_only_masters(self) -> None:
        # A defensive path: the owner is not in the list, so the loop runs to the
        # end without breaking and only its masters (none here) contribute.
        base = LoadedPlugin(
            "Base.esm", [], [LandscapeTexture(id="b", index=0, file_name="base.tga")]
        )
        orphan = LoadedPlugin("Orphan.esm", [], [])
        assert _landscape_textures([base], orphan) == {}


class TestExteriorPreviewAddsTerrain:
    def test_an_exterior_cell_gets_a_terrain_group(self) -> None:
        plugin = LoadedPlugin("A.esp", [], [_flat_land((4, 2))])
        _placements, _audit, cell = preview_cell_instanced(
            [plugin], CellKey(grid=(4, 2)), lambda _p: None
        )
        assert any(group.model == "terrain_4_2" for group in cell.groups)
        terrain = next(g for g in cell.groups if g.model == "terrain_4_2")
        assert terrain.count == 1  # drawn once, at identity
        assert len(terrain.matrices) == 16
        assert terrain.focus is True  # the camera frames on the cell, not strays

    def test_an_interior_cell_gets_no_terrain(self) -> None:
        plugin = LoadedPlugin("A.esp", [], [_flat_land((4, 2))])
        _placements, _audit, cell = preview_cell_instanced(
            [plugin], CellKey(interior="somewhere"), lambda _p: None
        )
        assert cell.groups == []

    def test_a_malformed_height_grid_is_skipped_not_fatal(self) -> None:
        bad = _flat_land((0, 0))
        bad.vertex_heights = b"\x00\x00"  # too short -> struct.error inside terrain_mesh
        plugin = LoadedPlugin("A.esp", [], [bad])
        # Must not raise; the exterior just comes back with no terrain group.
        _p, _a, cell = preview_cell_instanced([plugin], CellKey(grid=(0, 0)), lambda _p: None)
        assert cell.groups == []

    def test_a_landscape_without_the_heights_flag_draws_no_terrain(self) -> None:
        # A LAND record with no height grid (the flag is clear) carries nothing to
        # draw, so the exterior comes back with no terrain group.
        flat = _flat_land((0, 0), flags=LandscapeFlags(0))
        plugin = LoadedPlugin("A.esp", [], [flat])
        _p, _a, cell = preview_cell_instanced([plugin], CellKey(grid=(0, 0)), lambda _p: None)
        assert cell.groups == []

    def test_a_blend_that_yields_no_meshes_adds_no_group(self, monkeypatch) -> None:  # type: ignore[no-untyped-def]
        # Defensive guard: if the blend builder ever returns nothing, the preview
        # simply omits the terrain rather than appending an empty group.
        import wraithguard.scene.cellview as cv

        monkeypatch.setattr(cv, "terrain_blend_meshes", lambda *_a, **_k: [])
        plugin = LoadedPlugin("A.esp", [], [_flat_land((0, 0))])
        _p, _a, cell = preview_cell_instanced([plugin], CellKey(grid=(0, 0)), lambda _p: None)
        assert not any(g.model == "terrain_0_0" for g in cell.groups)


def _interior_cell(name: str, *, water: float | None = None) -> Cell:
    """An interior cell, optionally with a water level."""
    flags = CellFlags.IS_INTERIOR | (CellFlags.HAS_WATER if water is not None else CellFlags(0))
    return Cell(name=name, data=CellData(cell_flags=flags), water_height=water)


class TestWaterMesh:
    def test_it_is_a_subdivided_flat_surface_at_the_height(self) -> None:
        mesh = water_mesh(100.0, 0.0, 0.0, 10.0, 20.0)
        # A subdivided grid (so the shader's swell has vertices to ride), flat at
        # the height before animation, alpha-blended, and flagged as water.
        assert len(mesh.vertices) > 4 and len(mesh.triangles) > 2
        assert all(v[2] == 100.0 for v in mesh.vertices)
        assert mesh.alpha_blend is True
        assert mesh.water is True
        assert len(mesh.vertex_colors) == len(mesh.vertices)


class TestExteriorWater:
    def test_terrain_below_sea_level_gets_a_sea_plane(self) -> None:
        below = _flat_land((0, 0), offset=-10.0)  # heights = -80, below sea level
        plugin = LoadedPlugin("A.esp", [], [below])
        _p, _a, cell = preview_cell_instanced([plugin], CellKey(grid=(0, 0)), lambda _p: None)
        water = [g for g in cell.groups if g.model == "water"]
        assert len(water) == 1
        assert water[0].meshes[0].vertices[0][2] == SEA_LEVEL

    def test_terrain_at_or_above_sea_level_gets_no_sea(self) -> None:
        plugin = LoadedPlugin("A.esp", [], [_flat_land((0, 0))])  # flat at z=0
        _p, _a, cell = preview_cell_instanced([plugin], CellKey(grid=(0, 0)), lambda _p: None)
        assert not any(g.model == "water" for g in cell.groups)


class TestInteriorWater:
    def test_a_cell_with_the_water_flag_gets_a_plane_at_its_height(self) -> None:
        cell_rec = _interior_cell("pool", water=250.0)
        plugin = LoadedPlugin("A.esp", [], [cell_rec])
        _p, _a, cell = preview_cell_instanced([plugin], CellKey(interior="pool"), lambda _p: None)
        water = [g for g in cell.groups if g.model == "water"]
        assert len(water) == 1
        assert water[0].meshes[0].vertices[0][2] == 250.0

    def test_a_cell_without_the_water_flag_gets_none(self) -> None:
        plugin = LoadedPlugin("A.esp", [], [_interior_cell("dry")])
        _p, _a, cell = preview_cell_instanced([plugin], CellKey(interior="dry"), lambda _p: None)
        assert not any(g.model == "water" for g in cell.groups)
