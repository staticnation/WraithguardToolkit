"""Editing a mesh's fields: the mesh viewer's Save.

The edits are the block panel's (:mod:`wraithguard.nif.inspect`):
``[{"op": "set_field", "block": <index>, "name": <field>, "value": ...}]``, a block by
its index in the file. greatness7's ``tes3::nif`` applies them through typed setters
and writes the file (``wraithguard_native.nif_edit``, ``native/src/nif.rs``). The
fields it can set: names; flags; translation, velocity and scale; a material's
colours, shine and alpha; the alpha test reference; a texture's file; a string extra
data's value; a controller's frequency, phase, start and stop. Written the crate's
way: the blocks reachable from the roots, in walk order.
"""

from __future__ import annotations

import json
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from collections.abc import Mapping, Sequence


class NifEditError(ValueError):
    """An edit could not be applied (no such block or field, or a bad value)."""


def apply_edits(data: bytes, edits: Sequence[Mapping[str, object]]) -> bytes:
    """Apply ``edits`` to the mesh ``data`` and return the edited file.

    Args:
        data: The original ``.nif`` bytes.
        edits: The block panel's edits.

    Returns:
        The edited file's bytes.

    Raises:
        NifEditError: If the file cannot be read or an edit cannot be applied.
    """
    from wraithguard.nif.bsa import _native  # the Rust backend

    try:
        return bytes(_native.nif_edit(bytes(data), json.dumps([dict(e) for e in edits])))
    except ValueError as exc:
        raise NifEditError(str(exc)) from exc
