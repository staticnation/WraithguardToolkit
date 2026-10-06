"""Heavy file-system work: walking data folders, comparing files byte for byte.

The Rust backend does both off the interpreter (``wraithguard_native``: ``walk_files``,
``files_identical``, ``files_identical_many``; ``native/src/fsio.rs``), on a few
threads. Without it built, the same answers come from ``os.walk`` and chunked reads
here, which the tests hold the two to.

Copyright (c) 2026 StaticNation.
"""

from __future__ import annotations

import os
from pathlib import Path
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from collections.abc import Iterable, Sequence

_CHUNK = 1 << 20


def _native(name: str) -> Any:  # noqa: ANN401 - a function of the extension module, or None
    """One of the backend's functions, or None when it is not built (or older)."""
    try:
        import wraithguard_native  # optional for this package
    except ImportError:
        return None
    return getattr(wraithguard_native, name, None)


def _skip_set(skip_exts: Iterable[str] | None) -> set[str]:
    """Extensions to leave out, lower-cased and dotted (``esp`` -> ``.esp``)."""
    out = set()
    for raw in skip_exts or ():
        e = raw.strip().lower()
        if e:
            out.add(e if e.startswith(".") else "." + e)
    return out


def walk_py(root: str | os.PathLike[str], skip: set[str], lower: bool = True) -> list[str] | None:
    """:func:`walk_files` for one root, in Python."""
    p = Path(root)
    try:
        if not p.is_dir():
            return None
    except OSError:
        return None
    out: list[str] = []
    try:
        for dirpath, _dirs, files in os.walk(p):
            for fn in files:
                if skip and Path(fn).suffix.lower() in skip:
                    continue
                # os.path: relpath tolerates what Path.relative_to raises on.
                joined = os.path.join(dirpath, fn)  # noqa: PTH118
                rel = os.path.relpath(joined, p).replace("\\", "/")
                out.append(rel.lower() if lower else rel)
    except OSError:
        pass  # what was listed before the error stands
    return out


def walk_files(
    roots: Sequence[str | os.PathLike[str]],
    skip_exts: Iterable[str] | None = None,
    lower: bool = True,
) -> list[list[str] | None]:
    """Every file under each root, relative to it with ``/`` (lower-cased unless asked).

    Args:
        roots: Folders to walk (a MOMW setup's thousand data folders, say).
        skip_exts: Extensions to leave out (``.esp`` or ``esp``, any case).
        lower: Lower-case the names (the VFS's keys); False keeps them as on disk.

    Returns:
        One list per root, in order; None for a root that is not a folder.
    """
    fn = _native("walk_files")
    skip = _skip_set(skip_exts)
    if fn is not None:
        return list(fn([os.fspath(r) for r in roots], sorted(skip), lower))
    from wraithguard.parallel import read_all

    return read_all(list(roots), lambda r: walk_py(r, skip, lower))


def identical_py(paths: Sequence[str | os.PathLike[str]]) -> bool:
    """:func:`files_identical`, in Python: sizes, then the bytes side by side."""
    files = [Path(p) for p in paths]
    if len(files) < 2:
        return len(files) == 1 and files[0].is_file()
    try:
        if len({f.stat().st_size for f in files}) != 1 or not all(f.is_file() for f in files):
            return False
        handles = [f.open("rb") for f in files]
    except OSError:
        return False
    try:
        while True:
            first = handles[0].read(_CHUNK)
            for h in handles[1:]:
                if h.read(len(first) or 1) != first:
                    return False
            if not first:
                return True
    except OSError:
        return False
    finally:
        for h in handles:
            h.close()


def files_identical(paths: Sequence[str | os.PathLike[str]]) -> bool:
    """Whether every file holds the same bytes; False when one cannot be read.

    Args:
        paths: Two or more files.

    Returns:
        True when they all match.
    """
    fn = _native("files_identical")
    if fn is not None:
        return bool(fn([os.fspath(p) for p in paths]))
    return identical_py(paths)


def files_identical_many(groups: Sequence[Sequence[str | os.PathLike[str]]]) -> list[bool]:
    """:func:`files_identical` for each group, compared side by side.

    Args:
        groups: Groups of files.

    Returns:
        One answer per group, in order.
    """
    fn = _native("files_identical_many")
    if fn is not None:
        return list(fn([[os.fspath(p) for p in g] for g in groups]))
    from wraithguard.parallel import read_all

    return read_all(list(groups), identical_py)


def file_digest(path: str | os.PathLike[str]) -> str:
    """A file's BLAKE2b-128 as hex (``hashlib.blake2b(digest_size=16)``), "" if unreadable.

    The Rust backend's is the same digest, so a cache keyed on it stays valid either way.

    Args:
        path: The file.

    Returns:
        The hex digest, or ``""``.
    """
    fn = _native("file_digest")
    if fn is not None:
        return str(fn(os.fspath(path)))
    import hashlib

    digest = hashlib.blake2b(digest_size=16)
    try:
        with Path(path).open("rb") as handle:
            while chunk := handle.read(_CHUNK):
                digest.update(chunk)
    except OSError:
        return ""
    return digest.hexdigest()


def mod_folders_py(
    start: str | os.PathLike[str], asset_dirs: Iterable[str], plugin_exts: Iterable[str]
) -> list[tuple[str, list[str]]]:
    """:func:`scan_mod_folders`, in Python (``os.walk``, pruned at each match)."""
    assets = {d.lower() for d in asset_dirs}
    exts = _skip_set(plugin_exts)
    out: list[tuple[str, list[str]]] = []
    for root, dirs, files in os.walk(os.fspath(start)):
        has_assets = any(d.lower() in assets for d in dirs)
        plugins = [f for f in files if Path(f).suffix.lower() in exts]
        if has_assets or plugins:
            out.append((root, plugins))
            dirs[:] = []  # matched: not looked into further
    return out


def scan_mod_folders(
    start: str | os.PathLike[str], asset_dirs: Iterable[str], plugin_exts: Iterable[str]
) -> list[tuple[str, list[str]]]:
    """Every folder under ``start`` (itself included) holding an asset folder or a plugin.

    A match is not looked into further. In the order a top-down walk meets them.

    Args:
        start: Where to look (a mods folder).
        asset_dirs: Folder names that make a mod folder (``meshes``, ``textures`` ...).
        plugin_exts: Plugin extensions (``.esp`` ...).

    Returns:
        ``(folder, plugin names directly in it)`` per match.
    """
    fn = _native("scan_mod_folders")
    if fn is not None:
        return [
            (str(p), list(pl))
            for p, pl in fn(os.fspath(start), sorted(asset_dirs), sorted(plugin_exts))
        ]
    return mod_folders_py(start, asset_dirs, plugin_exts)


__all__ = [
    "file_digest",
    "files_identical",
    "files_identical_many",
    "scan_mod_folders",
    "walk_files",
]
