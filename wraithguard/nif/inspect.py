"""What the mesh viewer's block panel shows: a mesh's block tree and each block's fields.

The viewer shell's mesh viewer (``viewer-shell/ui/src/41_wg_meshview.js``) draws the
mesh itself; this is the structure beside it. Both come from greatness7's
``tes3::nif`` (``wraithguard_native.nif_blocks``, ``native/src/nif.rs``): the block
hierarchy from the crate's links, and every block's fields, the ones the editor can
set marked editable (see :mod:`wraithguard.nif.edit`). Wraithguard writes it into the
viewer's extra file.
"""

from __future__ import annotations

import json


def inspect_mesh(data: bytes) -> dict[str, object] | None:
    """The block panel's data for one mesh.

    Args:
        data: The ``.nif`` bytes.

    Returns:
        ``{"tree": [...], "blocks": {index: {"type", "fields": [...]}}, "complete": True}``
        (fields as ``{"name", "kind", "value", "editable"}``), or None when the file
        cannot be read.
    """
    from wraithguard.nif.bsa import _native  # the Rust backend

    try:
        result: dict[str, object] = json.loads(_native.nif_blocks(bytes(data)))
    except ValueError:
        return None
    return result
