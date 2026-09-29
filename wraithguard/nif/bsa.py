r"""Reading Morrowind's BSA archives, which is where most of the game lives.

The base game ships nearly all of its meshes and textures inside
``Morrowind.bsa``, so a vanilla asset resolves to nothing on disk while the
engine finds it perfectly well. Without this, every base-game mesh looks
untextured and every base-game texture looks missing -- both false, and both on
the lines a user acts on.

The reading is Rust: ``wraithguard_native.Archive`` (``native/src/bsa.rs``), on
greatness7's ``tes3::bsa``, which validates the header, tables and hashes. This
module keeps the Python API the rest of the toolkit uses. The index is read on
open and the file closed again; contents are read on demand, so an open archive
never holds the ``.bsa`` -- a mod manager can still replace it while the
toolkit runs. Only Morrowind's format is read; Oblivion's and later (``BSA\0``,
a different format under the same extension) is refused.
"""

from __future__ import annotations

from pathlib import Path  # noqa: TC003 -- used at runtime, not only in annotations

try:
    import wraithguard_native as _native
except ModuleNotFoundError as exc:  # pragma: no cover - a source checkout not built yet
    import sys
    import sysconfig

    # The module is built per interpreter: a copy built for the free-threaded
    # 3.14t does not load in the regular 3.14 (they share a site-packages on
    # Windows, so the package is found but its binary is not). Name the
    # interpreter that is running, and the command that builds for it.
    _ft = bool(sysconfig.get_config_var("Py_GIL_DISABLED"))
    _ver = f"{sys.version_info.major}.{sys.version_info.minor}{'t' if _ft else ''}"
    raise ModuleNotFoundError(
        f"wraithguard_native (the Rust backend, native/) is not built for this Python "
        f"({_ver}, {sys.executable}). From the repo folder run:  "
        f'"{sys.executable}" -m pip install ./native  '
        "(needs Rust 1.88 or newer: https://rustup.rs). If you built it with "
        "py -3.14t, run the tests the same way: py -3.14t -m pytest",
        name="wraithguard_native",
    ) from exc

from wraithguard.logging_setup import get_logger

LOG = get_logger(__name__)


class BsaError(Exception):
    """Raised when an archive cannot be read."""


class BsaArchive:
    """A Morrowind archive: its index, and its files read on demand."""

    def __init__(self, path: Path) -> None:
        """Open an archive and read its index.

        Args:
            path: The ``.bsa`` file.

        Raises:
            BsaError: If it is not a readable Morrowind archive.
        """
        self.path = path
        try:
            self._archive = _native.Archive(path)
        except OSError as exc:
            raise BsaError(f"cannot read {path}: {exc}") from exc
        except ValueError as exc:
            raise BsaError(f"{path.name} is not a readable Morrowind archive: {exc}") from exc
        LOG.debug("indexed %d file(s) in %s", len(self._archive), path.name)

    def __len__(self) -> int:
        """How many files the archive holds."""
        return len(self._archive)

    @property
    def names(self) -> list[str]:
        """Every stored path, normalised, in archive order."""
        return self._archive.names

    def __contains__(self, name: object) -> bool:
        """Whether the archive holds this path (any case, either separator), without reading it."""
        return isinstance(name, str) and name in self._archive

    def read(self, name: str) -> bytes | None:
        """Read one file out of the archive.

        Args:
            name: The stored path, in any case and with either separator.

        Returns:
            The bytes, or ``None`` when the archive does not hold it.

        Raises:
            BsaError: If the archive cannot be read where the file should be.
        """
        try:
            return self._archive.read(name)
        except (OSError, ValueError) as exc:
            raise BsaError(f"cannot read {name} from {self.path.name}: {exc}") from exc


def normalise(name: str) -> str:
    """Reduce a stored or referenced path to a comparable form.

    Args:
        name: A path from an archive or from a mesh.

    Returns:
        Lower-cased, forward slashes, no leading separator.
    """
    return name.strip().replace("\\", "/").lstrip("/").lower()
