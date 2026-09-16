"""Tests for ``wraithguard.nif.vfs.MeshVfs`` -- the merged mesh index.

The cell preview resolves every mesh in a load order through one index instead
of probing each data folder per mesh. These pin the precedence that has to match
the game and :func:`~wraithguard.nif.vfs.read_mesh`: a later data folder wins,
and within a folder a loose file wins over an archived one.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

import pytest

from tests.test_mesh_loose_case import MINIMAL_NIF, build_bsa
from wraithguard.nif.vfs import MeshVfs, forget_archives

if TYPE_CHECKING:
    from pathlib import Path


@pytest.fixture(autouse=True)
def _clean_archive_cache() -> None:
    """The archive cache is process-global; do not let one test leak into another."""
    forget_archives()


def _loose(folder: Path, path: str, body: bytes = MINIMAL_NIF) -> None:
    """Write a loose file under ``folder``."""
    target = folder / path
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_bytes(body)


def test_a_loose_mesh_resolves(tmp_path: Path) -> None:
    _loose(tmp_path, "meshes/x/rock.nif")
    vfs = MeshVfs([tmp_path])
    assert vfs.read("meshes/x/rock.nif") is not None
    assert len(vfs) == 1


def test_an_unindexed_path_is_none(tmp_path: Path) -> None:
    _loose(tmp_path, "meshes/x/rock.nif")
    assert MeshVfs([tmp_path]).read("meshes/x/missing.nif") is None


def test_only_meshes_are_indexed(tmp_path: Path) -> None:
    _loose(tmp_path, "meshes/x/rock.nif")
    _loose(tmp_path, "textures/x/rock.dds", body=b"not a mesh")
    vfs = MeshVfs([tmp_path])
    assert len(vfs) == 1  # the texture is not in the mesh index
    assert vfs.read("textures/x/rock.dds") is None


def test_a_reference_matches_case_insensitively(tmp_path: Path) -> None:
    _loose(tmp_path, "meshes/x/rock.nif")
    # A plugin may spell it with backslashes and different case.
    assert MeshVfs([tmp_path]).read("Meshes\\X\\Rock.NIF") is not None


def test_a_later_folder_wins(tmp_path: Path) -> None:
    early, late = tmp_path / "00", tmp_path / "01"
    _loose(early, "meshes/x/rock.nif")
    _loose(late, "meshes/x/rock.nif")
    vfs = MeshVfs([early, late])  # earliest first
    source = vfs._index["meshes/x/rock.nif"]
    assert str(source).startswith(str(late)), "the later folder's copy must win"


def test_a_loose_file_wins_over_an_archive_in_the_same_folder(tmp_path: Path) -> None:
    from pathlib import Path as _Path

    build_bsa(tmp_path / "meshes.bsa", {"meshes\\x\\rock.nif": b"ARCHIVED-not-a-nif"})
    _loose(tmp_path, "meshes/x/rock.nif")  # loose is a real NIF
    vfs = MeshVfs([tmp_path])
    # Loose wins, so it resolves to the parseable loose file, not the junk archive.
    assert isinstance(vfs._index["meshes/x/rock.nif"], _Path)
    assert vfs.read("meshes/x/rock.nif") is not None


def test_a_later_archive_wins_over_an_earlier_loose_file(tmp_path: Path) -> None:
    early, late = tmp_path / "00", tmp_path / "01"
    _loose(early, "meshes/x/rock.nif")
    late.mkdir(parents=True, exist_ok=True)
    build_bsa(late / "meshes.bsa", {"meshes\\x\\rock.nif": MINIMAL_NIF})
    vfs = MeshVfs([early, late])
    # The later folder wins even though its copy is archived and the earlier is loose.
    assert not isinstance(vfs._index["meshes/x/rock.nif"], type(early))  # a BsaArchive
    assert vfs.read("meshes/x/rock.nif") is not None
