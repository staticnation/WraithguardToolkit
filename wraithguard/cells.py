"""A cell, named one way: ``ordercore::cells``, through the native module.

So the Editor's pool, the viewer and the engine read every spelling of a cell -
``int:Name``, ``x,y``, ``(x, y)``, a bare name - as the same cell.

Cases: ``tests/fixtures/cell_cases.json``, run here (``tests/test_cell_key.py``) and by
ordercore's own tests.
"""

from __future__ import annotations

from typing import Any


def cell_key(spec: str) -> dict[str, Any] | None:
    """Read any spelling of a cell.

    Returns:
        ``fold`` (the key its spellings share), ``spec`` (the viewer's spelling), ``pool``
        (the Editor pool's: a name, or ``(x, y)``) and ``grid`` (an exterior's square,
        else None); None for an empty name.
    """
    import wraithguard_native

    return wraithguard_native.cell_key(spec)


def pool_key(spec: str) -> str:
    """Spell a cell as the Editor pool does: an interior's name, or ``(x, y)``.

    Raises:
        ValueError: the name is empty.
    """
    k = cell_key(spec)
    if k is None:
        raise ValueError("no cell named")
    return str(k["pool"])


def grid(spec: str) -> tuple[int, int] | None:
    """Give an exterior's grid square, from any spelling; None for an interior."""
    k = cell_key(spec)
    g = k and k["grid"]
    return (int(g[0]), int(g[1])) if g else None
