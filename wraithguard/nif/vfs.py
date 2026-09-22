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
import os
from pathlib import Path
from typing import TYPE_CHECKING, Final

from wraithguard.nif.bsa import BsaArchive, BsaError, normalise
from wraithguard.nif.reader import NifFile, NifParseError, read_nif, read_nif_bytes

if TYPE_CHECKING:
    from collections.abc import Iterator, Sequence

LOG: Final = logging.getLogger(__name__)

#: Opened archives per folder. Indexing a real ``Morrowind.bsa`` reads a table
#: of several thousand entries, and a conflict with several providers would
#: otherwise pay that for every one of them.
_ARCHIVES: Final[dict[Path, list[BsaArchive]]] = {}

#: Loose-file index per folder, built the first time a case-sensitive lookup
#: in that folder misses. Windows and macOS filesystems are case-insensitive,
#: which is why a great many mods ship a mesh whose on-disk name does not
#: exactly match how a plugin spells it -- harmless there, and invisible to
#: whoever packaged the mod. A case-sensitive filesystem (ext4, btrfs -- most
#: Linux installs, including the Steam Deck) takes that literally and the
#: file silently fails to resolve, which looks exactly like the file being
#: absent rather than merely misspelled.
_LOOSE_INDEX: Final[dict[Path, dict[str, Path]]] = {}


def loose_index(folder: Path) -> dict[str, Path]:
    """Build (once) a normalised-name to real-path map of every loose file.

    Public because the case-insensitivity gap this closes is not specific to
    meshes: anything that resolves a mod-authored VFS path against a loose
    file on disk (icons, textures, ...) needs the same fallback, and should
    share this cache rather than walk the same folder a second time.

    Only paid for a folder that has already had a case-sensitive miss, and
    only once per folder thereafter -- a full ``Data Files`` tree can hold
    tens of thousands of files.

    Args:
        folder: The data folder.

    Returns:
        Every file beneath ``folder``, keyed by :func:`normalise` of its path
        relative to ``folder``.
    """
    cached = _LOOSE_INDEX.get(folder)
    if cached is not None:
        return cached

    index: dict[str, Path] = {}
    try:
        for item in folder.rglob("*"):
            if item.is_file():
                index[normalise(str(item.relative_to(folder)))] = item
    except OSError as exc:
        LOG.debug("cannot index loose files in %s: %s", folder, exc)
    _LOOSE_INDEX[folder] = index
    return index


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
    """Drop the archive and loose-file caches.

    For a caller that has rescanned, or a test that does not want one case's
    archives visible to the next.
    """
    _ARCHIVES.clear()
    _LOOSE_INDEX.clear()


def read_mesh_bytes(folder: Path, path: str) -> bytes:
    """The raw bytes of one mesh from a data folder, loose or archived.

    The same resolution as :func:`read_mesh` -- loose file, loose-index match,
    then any ``.bsa`` in the folder -- but returns the bytes rather than a parsed
    :class:`~wraithguard.nif.reader.NifFile`. An editor needs the bytes: it reads
    them with ``retain=True``, changes a field, and writes them back, which a
    parse that keeps counts rather than elements cannot round-trip.

    Args:
        folder: The data folder providing it.
        path: The mesh's path within that folder, either separator, any case.

    Returns:
        The file's bytes.

    Raises:
        OSError: If neither the folder nor its archives hold it -- the same
            message :func:`read_mesh` raises, naming both places looked.
    """
    loose = folder / path
    if loose.is_file():
        return loose.read_bytes()
    wanted = normalise(path)
    matched = loose_index(folder).get(wanted)
    if matched is not None:
        return matched.read_bytes()
    for archive in archives_in(folder):
        try:
            data = archive.read(wanted)
        except BsaError as exc:
            LOG.warning("cannot read %s from %s: %s", path, archive.path.name, exc)
            continue
        if data is not None:
            return data
    raise OSError(
        f"{path} is not in {folder} nor in any .bsa there. If the mod was "
        "installed with only some of its files, or an archive is missing, "
        "this is where that shows up."
    )


def _iter_files(root: Path) -> Iterator[Path]:
    """Every file beneath ``root`` (via ``os.walk``, faster than ``rglob``)."""
    if not root.is_dir():
        return
    for dirpath, _dirs, files in os.walk(root):
        base = Path(dirpath)
        for name in files:
            yield base / name


class MeshVfs:
    """A merged ``meshes/`` index across data folders: one lookup, not a probe.

    :func:`read_mesh` resolves one folder; the cell preview needs every mesh in a
    load order and used to probe all data folders per mesh -- hundreds of meshes
    times ~1500 folders, tens of seconds of ``stat`` calls even with the loose
    caches warm. This walks each folder's ``meshes/`` subtree (and its archives)
    exactly once and records the winner per path, so resolving a mesh afterwards
    is a single dict lookup.

    The precedence matches the game and :func:`read_mesh`: a later data folder
    wins over an earlier one, and within a folder a loose file wins over an
    archived one. Built earliest-folder-first with plain overwrite -- archives
    before loose within each folder -- which yields exactly that ordering.
    """

    def __init__(self, dirs: Sequence[Path]) -> None:
        """Index every ``meshes/`` path across ``dirs`` (in load order).

        Args:
            dirs: The data folders, earliest first (the resolved data-path order).
        """
        #: normalised ``meshes/...`` path -> its winning source (a loose ``Path``
        #: or the :class:`~wraithguard.nif.bsa.BsaArchive` that holds it).
        self._index: dict[str, Path | BsaArchive] = {}
        for folder in dirs:
            for archive in archives_in(folder):
                for name in archive.names:
                    if name.startswith("meshes/"):
                        self._index[name] = archive
            root = folder / "meshes"
            for path in _iter_files(root):
                key = normalise("meshes/" + path.relative_to(root).as_posix())
                self._index[key] = path  # loose beats an archive in the same folder

    def __len__(self) -> int:
        """How many distinct mesh paths are indexed."""
        return len(self._index)

    def read(
        self, vfs_path: str, *, geometry: bool = True, animation: bool = False
    ) -> NifFile | None:
        """Read the winning mesh for ``vfs_path``, or ``None`` if unresolved.

        Args:
            vfs_path: A ``meshes/...`` path, any case or separator.
            geometry: Keep vertices and triangles (on for a viewer).
            animation: Keep animation key values (for a viewer that plays them).

        Returns:
            The parsed NIF, or ``None`` when no folder provides it or its bytes
            do not parse -- one bad mesh must not sink a whole cell.
        """
        source = self._index.get(normalise(vfs_path))
        if source is None:
            return None
        try:
            if isinstance(source, Path):
                return read_nif(source, geometry=geometry, animation=animation)
            data = source.read(normalise(vfs_path))
            return (
                read_nif_bytes(data, geometry=geometry, animation=animation)
                if data is not None
                else None
            )
        except (OSError, NifParseError, BsaError) as exc:
            LOG.debug("cannot read %s: %s", vfs_path, exc)
            return None


def read_mesh(
    folder: Path, path: str, *, geometry: bool = True, animation: bool = False
) -> NifFile:
    """Read one mesh from a data folder, loose or archived.

    Args:
        folder: The data folder providing it.
        path: The mesh's path within that folder, with either separator and in
            any case.
        geometry: Keep vertices and triangles. On for a viewer, off for a scan.
        animation: Keep animation key values (for a viewer that plays them). Off
            by default; see :func:`wraithguard.nif.reader.read_nif_bytes`.

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
        return read_nif(loose, geometry=geometry, animation=animation)

    wanted = normalise(path)

    matched = loose_index(folder).get(wanted)
    if matched is not None:
        return read_nif(matched, geometry=geometry, animation=animation)

    for archive in archives_in(folder):
        try:
            data = archive.read(wanted)
        except BsaError as exc:
            LOG.warning("cannot read %s from %s: %s", path, archive.path.name, exc)
            continue
        if data is not None:
            return read_nif_bytes(data, geometry=geometry, animation=animation)

    raise OSError(
        f"{path} is not in {folder} nor in any .bsa there. If the mod was "
        "installed with only some of its files, or an archive is missing, "
        "this is where that shows up."
    )
