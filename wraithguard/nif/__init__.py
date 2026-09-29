"""Morrowind meshes: finding them, summarising them, and editing them.

Reading ``.nif`` files is greatness7's ``tes3::nif`` (through ``wraithguard_native``,
``native/src/nif.rs``): :mod:`~wraithguard.nif.report` summarises what a mesh holds
for Resource Conflicts, :mod:`~wraithguard.nif.analysis` compares the providers of a
conflicting mesh, :mod:`~wraithguard.nif.inspect` and :mod:`~wraithguard.nif.edit`
are the mesh viewer's block panel and field editor. :mod:`~wraithguard.nif.vfs` and
:mod:`~wraithguard.nif.bsa` find a mesh the way the game does, loose or archived, and
:mod:`~wraithguard.nif.textures` resolves the textures it names.
"""

from __future__ import annotations

from wraithguard.nif.analysis import MeshAnalyser, MeshFinding, file_digest
from wraithguard.nif.edit import NifEditError, apply_edits
from wraithguard.nif.inspect import inspect_mesh
from wraithguard.nif.report import (
    COLLISION_NODES,
    Difference,
    NifParseError,
    Shape,
    Structure,
    compare,
    normalise_texture,
    summarise,
)

__all__ = [
    "COLLISION_NODES",
    "Difference",
    "MeshAnalyser",
    "MeshFinding",
    "NifEditError",
    "NifParseError",
    "Shape",
    "Structure",
    "apply_edits",
    "compare",
    "file_digest",
    "inspect_mesh",
    "normalise_texture",
    "summarise",
]
