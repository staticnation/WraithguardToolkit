"""Locating and reading the vendored three.js build shared by every viewer.

This lives in :mod:`wraithguard.viz` alongside the conflict-map, height-delta,
pathgrid and terrain renderers because it solves the same problem one level
down. None of those four needs a 3D engine, but two other pages do -- the mesh
viewer in :mod:`wraithguard.nif.viewer` and the texture comparison's WebGL
wipe in :mod:`wraithguard.images.viewer` -- and a second consumer is what
turned "load one file" from something worth inlining into
:mod:`~wraithguard.nif.viewer` into a concern worth naming on its own.
:mod:`~wraithguard.viz.serve` exists for the adjacent reason of publishing
this same build once per session instead of re-embedding it in every
document, and the two belong together for that reason, not because either
one is a "page" in the sense the rest of this package is -- see the package
docstring for where the pure-renderer guarantee does and does not reach.

**Why three.js is embedded as a classic script.** Modern three.js ships ESM
only, split across ``three.module.min.js`` and ``three.core.min.js``, and **ES
module scripts do not load from ``file://``** -- the origin is ``null`` and the
CORS check fails. Every page built here is written to disk or served and then
opened in a browser, so a module build cannot work regardless of how it is
packaged. The CommonJS build is a single self-contained file with no
``require()`` of its own, so it runs as an ordinary script behind a
three-line ``exports`` shim. That was verified rather than assumed: the shim
was exercised and used to build a real ``BufferGeometry`` with computed
normals before any of this was written.
"""

from __future__ import annotations

import sys
from pathlib import Path
from typing import Final

from wraithguard.logging_setup import get_logger

LOG = get_logger(__name__)

#: The vendored three.js build, relative to this package.
_THREE_ASSET: Final[str] = "assets/three.cjs"


class ViewerError(Exception):
    """Raised when a viewer page cannot be built.

    Not specific to any one viewer -- the mesh viewer and the texture
    comparison both raise this through :func:`three_source`, and either could
    grow its own reasons to raise it later.
    """


def three_source() -> str:
    """Locate and read the vendored three.js build.

    Looks in the same places, and for the same reason, as the help documents:
    a frozen build unpacks its data to ``sys._MEIPASS``, while a source
    checkout has it beside this module.

    Returns:
        The library source.

    Raises:
        ViewerError: If it was not shipped with this build. Reported rather
            than crashed on: a missing viewer is a disappointment, and the
            caller can say so.
    """
    bundled = getattr(sys, "_MEIPASS", None)
    candidates = [
        *([Path(bundled) / "wraithguard" / "viz" / _THREE_ASSET] if bundled else []),
        *([Path(bundled) / _THREE_ASSET] if bundled else []),
        Path(__file__).resolve().parent / _THREE_ASSET,
    ]
    found = _first_readable(candidates)
    if found is not None:
        return found
    raise ViewerError(
        "the 3D viewer library was not shipped with this build; "
        f"looked in {[str(c) for c in candidates]}"
    )


def _first_readable(candidates: list[Path]) -> str | None:
    """Return the contents of the first candidate that can be read.

    Args:
        candidates: Paths to try, in order.

    Returns:
        The file's text, or ``None`` when none of them could be read.
    """
    for candidate in candidates:
        try:
            if candidate.is_file():
                return candidate.read_text(encoding="utf-8")
        except OSError as exc:  # noqa: PERF203 -- candidates must fail independently
            # An unreadable mount or a partial extraction. Hoisting the try out
            # of the loop would make one bad path skip the remaining ones, and
            # the whole purpose here is to try them in turn.
            LOG.warning("cannot read %s: %s", candidate, exc)
    return None
