"""Find a mesh the way the game does: loose files first, then archives.

**Most of Morrowind's meshes are not files.** They live inside
``Morrowind.bsa``, and plenty of mods ship theirs the same way. Anything that
reads ``<data folder>/<path>`` and stops when that misses fails on the base
game, which is what the 3D view did::

    Cannot show this mesh
    cannot read /home/.../Data Files/meshes/b/b_n_argonian_m_head_02.nif:
    [Errno 2] No such file or directory

Nothing was wrong with that path. The file simply is not one.

This lives here rather than in the window that needed it because resolving a
game asset is not a user-interface concern, and because a GUI module cannot be
tested without a display -- which is exactly how the gap survived: the texture
side of the same window has always fallen through to the archives, and the
mesh side never did.

Loose files win over archived ones, as they do in OpenMW and in Morrowind.
"""

from __future__ import annotations

import logging
from typing import TYPE_CHECKING, Final

from wraithguard.nif.bsa import BsaArchive, BsaError, normalise
from wraithguard.nif.reader import NifFile, read_nif, read_nif_bytes

if TYPE_CHECKING:
    from pathlib import Path

LOG: Final = logging.getLogger(__name__)

#: Opened archives per folder. Indexing a real ``Morrowind.bsa`` reads a table
#: of several thousand entries, and a conflict with several providers would
#: otherwise pay that for every one of them.
_ARCHIVES: Final[dict[Path, list[BsaArchive]]] = {}


def archives_in(folder: Path) -> list[BsaArchive]:
    """Open every ``.bsa`` in a data folder.

    Sorted by name, which is the order OpenMW lists them in and therefore the
    order it consults them. A corrupt or unreadable archive is skipped: one bad
    file in a folder is not a reason to lose the others.

    Args:
        folder: The data folder.

    Returns:
        The archives that opened, possibly empty.
    """
    cached = _ARCHIVES.get(folder)
    if cached is not None:
        return cached

    try:
        names = sorted(item for item in folder.iterdir() if item.suffix.lower() == ".bsa")
    except OSError as exc:
        LOG.debug("cannot list %s: %s", folder, exc)
        names = []
    opened = [archive for archive in map(_opened, names) if archive is not None]
    _ARCHIVES[folder] = opened
    return opened


def _opened(path: Path) -> BsaArchive | None:
    """Open one archive, or report why not.

    Args:
        path: The ``.bsa`` file.

    Returns:
        The archive, or ``None`` when it cannot be indexed. One bad file in a
        folder is not a reason to lose the others.
    """
    try:
        return BsaArchive(path)
    except BsaError as exc:
        LOG.warning("ignoring %s: %s", path.name, exc)
        return None


def forget_archives() -> None:
    """Drop the archive cache.

    For a caller that has rescanned, or a test that does not want one case's
    archives visible to the next.
    """
    _ARCHIVES.clear()


def read_mesh(folder: Path, path: str, *, geometry: bool = True) -> NifFile:
    """Read one mesh from a data folder, loose or archived.

    Args:
        folder: The data folder providing it.
        path: The mesh's path within that folder, with either separator and in
            any case.
        geometry: Keep vertices and triangles. On for a viewer, off for a scan.

    Returns:
        The parsed NIF.

    Raises:
        NifParseError: If the bytes are not a readable NIF.
        OSError: If neither the folder nor its archives hold it. The message
            names both, because "no such file" on its own sends the reader
            looking for a path problem that is not there.
    """
    loose = folder / path
    if loose.is_file():
        return read_nif(loose, geometry=geometry)

    wanted = normalise(path)
    for archive in archives_in(folder):
        try:
            data = archive.read(wanted)
        except BsaError as exc:
            LOG.warning("cannot read %s from %s: %s", path, archive.path.name, exc)
            continue
        if data is not None:
            return read_nif_bytes(data, geometry=geometry)

    raise OSError(
        f"{path} is not in {folder} nor in any .bsa there. If the mod was "
        "installed with only some of its files, or an archive is missing, "
        "this is where that shows up."
    )
