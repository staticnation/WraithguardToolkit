"""Tidying up generated HTML pages.

Every conflict map, terrain view, path grid and height difference is written to
a timestamped file so successive views can be compared side by side. That is
deliberate -- but it means the app directory accumulates, and on a big load
order a single conflict map is megabytes. Left alone, a few weeks of use leaves
a folder nobody can find anything in.

So generated pages are pruned: the newest few of each kind are kept and older
ones removed. Two properties matter more than the tidying itself:

**It only ever deletes files this tool wrote.** Matching is by the exact
filename stems the generators use, plus a timestamp suffix -- never a blanket
``*.html`` sweep of a directory the user may also keep things in. A file that
does not match the pattern is not a candidate, whatever its extension.

**It is opt-out, and off means off.** The checkbox is a real switch, not a
"remind me later": with cleanup disabled nothing is ever removed, because a tool
that quietly deletes output the user meant to keep is worse than a cluttered
folder.
"""

from __future__ import annotations

import re
from pathlib import Path
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from collections.abc import Iterable, Sequence

#: Filename stems the visualisations generate, each written as
#: ``<stem>_<YYYYmmdd>_<HHMMSS>.html``. Only files matching one of these (with a
#: timestamp) are ever considered for deletion.
GENERATED_STEMS: tuple[str, ...] = (
    "conflict_map",
    "terrain_surface",
    "terrain3d",
    "height_delta",
    "pathgrid",
    "cell_detail",
)

#: How many of each kind to keep by default. More than one so a before/after
#: comparison survives a cleanup, few enough that the folder stays readable.
DEFAULT_KEEP = 3

#: ``<stem>_20260721_170008.html``. The timestamp is required: an un-timestamped
#: ``conflict_map.html`` is a file the user asked for by name (via *Save*), so it
#: is deliberately not a candidate.
_STAMPED = re.compile(r"^(?P<stem>.+)_(?P<stamp>\d{8}_\d{6})$")


def find_generated(
    folder: str | Path, stems: Sequence[str] = GENERATED_STEMS
) -> dict[str, list[Path]]:
    """Group the tool's timestamped HTML output by kind, newest first.

    Args:
        folder: Directory to scan. A missing directory yields no candidates
            rather than raising.
        stems: Filename stems to recognise.

    Returns:
        Stem to its files, each list sorted newest first by the timestamp *in
        the filename* -- not by mtime, which a copy or a sync tool can rewrite.
    """
    found: dict[str, list[tuple[str, Path]]] = {}
    try:
        entries = list(Path(folder).iterdir())
    except OSError:
        return {}
    wanted = set(stems)
    for path in entries:
        if path.suffix.lower() != ".html" or not path.is_file():
            continue
        match = _STAMPED.match(path.stem)
        if match is None or match.group("stem") not in wanted:
            continue
        found.setdefault(match.group("stem"), []).append((match.group("stamp"), path))
    return {
        stem: [path for _stamp, path in sorted(pairs, reverse=True)]
        for stem, pairs in found.items()
    }


def sidecar_folder(page: Path) -> Path:
    """The data folder a generated page may own.

    Args:
        page: The HTML file.

    Returns:
        The sibling ``<stem>_data`` directory, whether or not it exists.
    """
    return page.with_name(page.stem + "_data")


def prune_generated(
    folder: str | Path,
    *,
    keep: int = DEFAULT_KEEP,
    stems: Sequence[str] = GENERATED_STEMS,
    dry_run: bool = False,
) -> list[Path]:
    """Delete all but the newest ``keep`` of each kind of generated page.

    A page's sidecar data folder goes with it, since the folder is useless
    without the page that references it.

    Args:
        folder: Directory to tidy.
        keep: How many of each kind to retain. ``0`` removes all of them; a
            negative value is treated as ``0``.
        stems: Filename stems to recognise.
        dry_run: Report what would be removed without removing it.

    Returns:
        The paths removed (or that would be), pages and sidecar folders both.
    """
    removed: list[Path] = []
    for pages in find_generated(folder, stems).values():
        for page in pages[max(0, keep) :]:
            data = sidecar_folder(page)
            if not dry_run and not _remove(page):
                # A locked file (open in a viewer) is not an error worth
                # surfacing: the next cleanup will get it.
                continue
            removed.append(page)
            if data.is_dir() and (dry_run or _remove_tree(data)):
                removed.append(data)
    return removed


def _remove(path: Path) -> bool:
    """Delete one file, tolerating a lock or a race.

    Args:
        path: The file.

    Returns:
        ``True`` if it is gone.
    """
    try:
        path.unlink()
    except OSError:
        return False
    return True


def _remove_tree(folder: Path) -> bool:
    """Delete a sidecar folder and its contents.

    Deliberately hand-rolled rather than ``shutil.rmtree``: this only ever
    walks a folder this tool created, and an explicit recursion cannot be
    pointed at something unexpected by a symlink in the middle of it.

    Args:
        folder: The directory.

    Returns:
        ``True`` if the directory is gone.
    """
    try:
        for child in sorted(folder.rglob("*"), key=lambda p: len(p.parts), reverse=True):
            if child.is_symlink() or child.is_file():
                child.unlink()
            elif child.is_dir():
                child.rmdir()
        folder.rmdir()
    except OSError:
        return False
    return True


def describe(removed: Iterable[Path]) -> str:
    """Summarise a prune for the log.

    Args:
        removed: What :func:`prune_generated` returned.

    Returns:
        A one-line summary, or an empty string when nothing was removed, so the
        caller can skip logging entirely.
    """
    items = list(removed)
    if not items:
        return ""
    pages = sum(1 for p in items if p.suffix.lower() == ".html")
    folders = len(items) - pages
    if folders:
        return f"cleaned up {pages} old page(s) and {folders} data folder(s)"
    return f"cleaned up {pages} old page(s)"
