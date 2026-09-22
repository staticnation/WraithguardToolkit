"""Reading a Morrowind ``.kf`` keyframe file and binding it to a model by node name.

A ``.kf`` is itself a NIF, rooted at a :class:`NiSequenceStreamHelper`. Under it
run two parallel chains: an ``extra_data`` chain of ``NiStringExtraData`` giving
target node *names*, and a ``controller`` chain of ``NiKeyframeController`` blocks
(each with a ``NiKeyframeData``). The i-th name is driven by the i-th controller,
so the file resolves to a map of ``node name -> keyframe track``.

The model NIF the ``.kf`` animates does not carry those controllers itself -- the
engine binds them at load time by matching the names against the model's nodes.
:func:`load_kf` produces that map; :func:`wraithguard.nif.geometry.model_shapes`
takes it and injects each track as though the named node had carried a
``NiKeyframeController`` of its own, so KF animation plays through exactly the
same path as an embedded one (see ``UNBAKE_MIGRATION.md`` for that path).
"""

from __future__ import annotations

from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from wraithguard.nif.reader import Block, NifFile

#: How far a chain (extra-data or controller) may run before it is judged a loop.
_MAX_CHAIN = 256


def _chain(by_index: dict[int, Block], start: int, link: str) -> list[Block]:
    """Follow a ``link`` chain from ``start`` into a flat list, cycle-guarded.

    Args:
        by_index: Every block, by index.
        start: The first block's index (``-1`` for an empty chain).
        link: The field naming the next block in the chain.

    Returns:
        The blocks in chain order.
    """
    out: list[Block] = []
    index = start
    seen: set[int] = set()
    while index >= 0 and index not in seen and len(out) < _MAX_CHAIN:
        seen.add(index)
        block = by_index.get(index)
        if block is None:
            break
        out.append(block)
        index = block.link(link)
    return out


def load_kf(nif: NifFile) -> dict[str, dict[str, Any]]:
    """Read a ``.kf`` file into ``{node name: keyframe track}``.

    The file must have been parsed with ``animation=True`` so the keyframe values
    are retained. Names are taken from the ``NiStringExtraData`` blocks on the
    sequence's extra-data chain, tracks from the ``NiKeyframeController`` blocks on
    its controller chain, paired in order (the two chains run in lock-step in a
    Morrowind ``.kf``).

    Args:
        nif: The parsed ``.kf`` file.

    Returns:
        A map of node name to the rotation/translation/scale keyframe dict, or an
        empty map when the file is not a keyframe sequence (no
        ``NiSequenceStreamHelper``) or carries no retained tracks.
    """
    by_index = {block.index: block for block in nif.blocks}
    sequence = next((b for b in nif.blocks if b.type_name == "NiSequenceStreamHelper"), None)
    if sequence is None:
        return {}

    names = [
        str(block.fields.get("string_data", ""))
        for block in _chain(by_index, sequence.link("extra_data"), "next_extra_data")
        if block.type_name == "NiStringExtraData"
    ]
    tracks: list[dict[str, Any]] = []
    for controller in _chain(by_index, sequence.link("controller"), "next_controller"):
        if controller.type_name != "NiKeyframeController":
            continue
        data = by_index.get(controller.link("data"))
        values = data.fields.get("keyframe_data_values") if data is not None else None
        tracks.append(values if isinstance(values, dict) else {})

    return {name: track for name, track in zip(names, tracks, strict=False) if name and track}
