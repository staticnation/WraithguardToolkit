"""Assemble a Morrowind cell for previewing.

Turns a cell's object references plus the sorted load order into an ordered list
of *placements* -- each object's id, its resolved mesh, and its world transform --
together with an audit of what the load order does to the cell (references a
later plugin overrode, deleted or moved; references whose object has no mesh).

The audit is the data behind a "what does this mod change in this cell" view; the
placements feed the 3D scene builder (a later milestone). Everything here is
pure -- no file or mesh IO -- so it is unit-tested against synthetic records; the
caller parses plugins with :mod:`wraithguard.esp.plugin` and hands the records in
load order.
"""

from __future__ import annotations

from wraithguard.scene.build import (
    BuiltScene,
    InstancedCell,
    InstancedGroup,
    build_instanced,
    build_scene,
    matrix4_columns,
)
from wraithguard.scene.cellview import (
    CellChoice,
    CellKey,
    LoadedPlugin,
    cell_key,
    cell_label,
    cell_layers,
    find_landscape,
    list_cells,
    object_provenance,
    preview_cell,
    preview_cell_instanced,
)
from wraithguard.scene.plugincache import PREVIEW_RECORD_TAGS, PluginParseCache
from wraithguard.scene.resolve import (
    CellAudit,
    ModelIndex,
    Placement,
    build_model_index,
    reference_transform,
    resolve_cell,
)
from wraithguard.scene.terrain import (
    has_terrain,
    landscape_textures,
    terrain_blend_meshes,
    terrain_mesh,
    terrain_meshes,
)
from wraithguard.scene.water import SEA_LEVEL, water_mesh

__all__ = [
    "PREVIEW_RECORD_TAGS",
    "SEA_LEVEL",
    "BuiltScene",
    "CellAudit",
    "CellChoice",
    "CellKey",
    "InstancedCell",
    "InstancedGroup",
    "LoadedPlugin",
    "ModelIndex",
    "Placement",
    "PluginParseCache",
    "build_instanced",
    "build_model_index",
    "build_scene",
    "cell_key",
    "cell_label",
    "cell_layers",
    "find_landscape",
    "has_terrain",
    "landscape_textures",
    "list_cells",
    "matrix4_columns",
    "object_provenance",
    "preview_cell",
    "preview_cell_instanced",
    "reference_transform",
    "resolve_cell",
    "terrain_blend_meshes",
    "terrain_mesh",
    "terrain_meshes",
    "water_mesh",
]
