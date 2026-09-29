"""Malformed-archive guards in :mod:`wraithguard.nif.bsa`.

The archive reader treats a ``.bsa`` as untrusted third-party data: every way a
header can lie or a file can be truncated becomes a
:class:`~wraithguard.nif.bsa.BsaError`, so one bad archive in a data folder is
skipped rather than crashing the scan. The validation itself is greatness7's
(``tes3::bsa``); these pin that each case still surfaces as ``BsaError``. The happy
path and the "corrupt archive does not hide a good one" case are covered in
``test_mesh_from_archive``.
"""

from __future__ import annotations

import struct
from typing import TYPE_CHECKING

import pytest

from wraithguard.nif.bsa import BsaArchive, BsaError, normalise

if TYPE_CHECKING:
    from pathlib import Path

_TES3_VERSION = 0x100


def _write(path: Path, data: bytes) -> Path:
    """Write raw bytes to ``path`` and return it."""
    path.write_bytes(data)
    return path


@pytest.mark.parametrize(
    ("name", "data"),
    [
        ("short", b"\x00\x01\x02"),
        ("later", b"BSA\x00" + b"\x00" * 8),
        ("version", struct.pack("<III", 0x999, 0, 0)),
        ("many", struct.pack("<III", _TES3_VERSION, 0, (1 << 30))),
        ("inconsistent", struct.pack("<III", _TES3_VERSION, 0, 1) + b"\x00" * 12),
        ("truncated", struct.pack("<III", _TES3_VERSION, 100, 1)),
    ],
)
def test_a_malformed_archive_is_refused(tmp_path: Path, name: str, data: bytes) -> None:
    """Each way a header can lie is a BsaError, never a crash or a half-read index."""
    with pytest.raises(BsaError, match="not a readable Morrowind archive"):
        BsaArchive(_write(tmp_path / f"{name}.bsa", data))


def test_a_directory_is_a_read_error(tmp_path: Path) -> None:
    """Not an archive at all: the OS error, reported as unreadable."""
    with pytest.raises(BsaError, match="cannot read"):
        BsaArchive(tmp_path)


def test_normalise() -> None:
    assert normalise(" \\Textures\\A.DDS ") == "textures/a.dds"
