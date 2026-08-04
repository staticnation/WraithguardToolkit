"""The mesh view has to read meshes that are not files.

Reported by a user on Linux, opening the 3D view on a vanilla mesh::

    Cannot show this mesh
    cannot read /home/.../Data Files/meshes/b/b_n_argonian_m_head_02.nif:
    [Errno 2] No such file or directory

Nothing was wrong with the path. **Most of Morrowind's meshes are not loose
files** -- they live inside ``Morrowind.bsa``, and plenty of mods ship theirs
the same way. The view read ``<folder>/<path>`` and gave up when that missed.

The tell was that the *texture* comparison in the same window worked perfectly:
``TextureResolver`` has always fallen through to the archives. Only the mesh
side went straight to the filesystem.
"""

from __future__ import annotations

import struct
from typing import TYPE_CHECKING, Final

import pytest

from wraithguard.nif.bsa import BsaArchive
from wraithguard.nif.vfs import archives_in, forget_archives, read_mesh

if TYPE_CHECKING:
    from pathlib import Path

#: The version word a Morrowind archive starts with.
TES3_BSA_VERSION: Final = 0x100

#: A NIF small enough to build by hand but real enough to parse: the header
#: line, version 4.0.0.2, and a block count of zero.
MINIMAL_NIF: Final = b"NetImmerse File Format, Version 4.0.0.2\n" + struct.pack(
    "<II", 0x04000002, 0
)


def build_bsa(path: Path, files: dict[str, bytes]) -> None:
    """Write a Morrowind ``.bsa`` holding ``files``.

    Built to the layout :meth:`~wraithguard.nif.bsa.BsaArchive._read_index`
    parses rather than mocked, so this exercises the real reader.

    Args:
        path: Where to write.
        files: Stored path to contents.
    """
    names = list(files)
    count = len(names)

    name_blob = b""
    name_offsets = []
    for name in names:
        name_offsets.append(len(name_blob))
        name_blob += name.encode("cp1252") + b"\x00"

    # sizes and offsets, name offsets, then the name table -- the hash offset
    # is measured from the end of the header and covers all three.
    hash_offset = count * 12 + len(name_blob)

    records = b""
    running = 0
    for name in names:
        records += struct.pack("<II", len(files[name]), running)
        running += len(files[name])

    blob = struct.pack("<III", TES3_BSA_VERSION, hash_offset, count)
    blob += records
    blob += b"".join(struct.pack("<I", off) for off in name_offsets)
    blob += name_blob
    blob += b"\x00" * (count * 8)  # the hash table; the reader only skips it
    blob += b"".join(files[name] for name in names)
    path.write_bytes(blob)


@pytest.fixture(autouse=True)
def _clean_archive_cache() -> None:
    """Keep one case's archives out of the next.

    The cache is keyed by folder and ``tmp_path`` differs per test, so this is
    belt and braces -- but a cache that leaked across cases would make these
    tests pass for the wrong reason.
    """
    forget_archives()


class TestTheArchiveIsTried:
    """A mesh inside a ``.bsa`` must open exactly like a loose one."""

    def test_the_fixture_archive_is_real(self, tmp_path: Path) -> None:
        """If the fixture were wrong the rest would prove nothing."""
        archive = tmp_path / "Morrowind.bsa"
        build_bsa(archive, {"meshes\\b\\b_n_argonian_m_head_02.nif": MINIMAL_NIF})
        opened = BsaArchive(archive)
        assert len(opened) == 1
        assert opened.read("meshes/b/b_n_argonian_m_head_02.nif") == MINIMAL_NIF

    def test_an_archived_mesh_is_found(self, tmp_path: Path) -> None:
        """The exact case from the report: vanilla mesh, no loose file."""
        build_bsa(
            tmp_path / "Morrowind.bsa",
            {"meshes\\b\\b_n_argonian_m_head_02.nif": MINIMAL_NIF},
        )
        parsed = read_mesh(tmp_path, "meshes/b/b_n_argonian_m_head_02.nif")
        assert parsed is not None

    def test_a_loose_file_still_wins(self, tmp_path: Path) -> None:
        """Loose files override archives, as they do in the game."""
        loose = tmp_path / "meshes" / "x.nif"
        loose.parent.mkdir(parents=True)
        loose.write_bytes(MINIMAL_NIF)
        build_bsa(tmp_path / "Morrowind.bsa", {"meshes\\x.nif": b"not a nif at all"})
        assert read_mesh(tmp_path, "meshes/x.nif") is not None

    def test_separators_and_case_do_not_matter(self, tmp_path: Path) -> None:
        """Archives store backslashes; conflict paths may use either."""
        build_bsa(tmp_path / "Morrowind.bsa", {"Meshes\\B\\Head.NIF": MINIMAL_NIF})
        assert read_mesh(tmp_path, "meshes/b/head.nif") is not None

    def test_a_mesh_in_neither_place_says_both_were_tried(self, tmp_path: Path) -> None:
        """The old message blamed the path; this one says what was searched."""
        build_bsa(tmp_path / "Morrowind.bsa", {"meshes\\other.nif": MINIMAL_NIF})
        with pytest.raises(OSError, match=r"nor in any \.bsa"):
            read_mesh(tmp_path, "meshes/missing.nif")

    def test_a_corrupt_archive_does_not_hide_a_good_one(self, tmp_path: Path) -> None:
        """One bad .bsa in a folder must not lose the rest."""
        (tmp_path / "Broken.bsa").write_bytes(b"nonsense")
        build_bsa(tmp_path / "Morrowind.bsa", {"meshes\\x.nif": MINIMAL_NIF})
        assert read_mesh(tmp_path, "meshes/x.nif") is not None

    def test_a_folder_with_no_archives_is_not_an_error(self, tmp_path: Path) -> None:
        """It just means the mesh really is missing."""
        with pytest.raises(OSError):
            read_mesh(tmp_path, "meshes/x.nif")

    def test_archives_are_opened_once_per_folder(self, tmp_path: Path) -> None:
        """Indexing a real Morrowind.bsa is not free; several providers would
        otherwise re-read the same archive for each of them."""
        build_bsa(tmp_path / "Morrowind.bsa", {"meshes\\x.nif": MINIMAL_NIF})
        first = archives_in(tmp_path)
        second = archives_in(tmp_path)
        assert first is second
