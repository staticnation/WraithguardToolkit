"""A mesh whose file name differs only in case has to be found too.

Reported on Steam Deck, viewing a mesh with three known providers: only the
base game's copy rendered. The two plugin-provided ones were both loose
files, and both simply weren't found -- no error, no missing-mesh message,
they just never made it into the viewer's list of things to show.

Nothing was wrong with either file. **Almost every mod on this game was
packaged on a case-insensitive filesystem** (Windows, or macOS by default),
so a NIF referencing ``Mesh\\Foo_Bar.nif`` and a file on disk actually named
``foo_bar.nif`` have always matched there without anyone noticing the
mismatch. On a case-sensitive filesystem -- ext4, btrfs, which covers most
of Linux including the Steam Deck -- that exact same reference misses.

:func:`~wraithguard.nif.vfs.read_mesh`'s loose-file check was a raw
``(folder / path).is_file()``, so it inherited that. Archives were already
safe: :func:`~wraithguard.nif.bsa.normalise` lower-cases before indexing and
before lookup. Loose files had no equivalent step -- this is that step,
sibling to the archive fallback :mod:`test_mesh_from_archive` covers.
"""

from __future__ import annotations

import struct
from pathlib import Path
from typing import Final

import pytest

from wraithguard.nif.vfs import archives_in, forget_archives, loose_index, read_mesh

#: The version word a Morrowind archive starts with.
TES3_BSA_VERSION: Final = 0x100

#: A NIF small enough to build by hand but real enough to parse.
MINIMAL_NIF: Final = b"NetImmerse File Format, Version 4.0.0.2\n" + struct.pack(
    "<II", 0x04000002, 0
)


def build_bsa(path: Path, files: dict[str, bytes], *, lie_about_size: str | None = None) -> None:
    """Write a Morrowind ``.bsa`` holding ``files``.

    Args:
        path: Where to write.
        files: Stored path to contents.
        lie_about_size: If given, this one entry's declared size is inflated
            past what the archive actually holds on disk -- the only way to
            make a real read fail *after* the entry is found, rather than
            fail to be found at all.
    """
    names = list(files)
    count = len(names)

    name_blob = b""
    name_offsets = []
    for name in names:
        name_offsets.append(len(name_blob))
        name_blob += name.encode("cp1252") + b"\x00"

    hash_offset = count * 12 + len(name_blob)

    records = b""
    running = 0
    for name in names:
        size = len(files[name])
        if name == lie_about_size:
            size += 4096  # more than the archive will actually hold
        records += struct.pack("<II", size, running)
        running += len(files[name])

    blob = struct.pack("<III", TES3_BSA_VERSION, hash_offset, count)
    blob += records
    blob += b"".join(struct.pack("<I", off) for off in name_offsets)
    blob += name_blob
    blob += b"\x00" * (count * 8)  # the hash table; the reader only skips it
    blob += b"".join(files[name] for name in names)
    path.write_bytes(blob)


@pytest.fixture(autouse=True)
def _clean_caches() -> None:
    """Keep one case's loose-file index and archives out of the next.

    Both caches are keyed by folder and ``tmp_path`` differs per test, so
    this is belt and braces -- but a cache that leaked across cases would
    make these tests pass for the wrong reason.
    """
    forget_archives()


class TestALooseFileIsFoundRegardlessOfCase:
    """The exact case from the report: a plugin mesh, wrong case, loose."""

    def test_a_differently_cased_loose_file_is_found(self, tmp_path: Path) -> None:
        real = tmp_path / "meshes" / "x" / "Foo_Bar.NIF"
        real.parent.mkdir(parents=True)
        real.write_bytes(MINIMAL_NIF)
        assert read_mesh(tmp_path, "meshes/x/foo_bar.nif") is not None

    def test_backslashes_and_case_both_differ(self, tmp_path: Path) -> None:
        """The exact spelling a NIF field tends to carry: Windows separators,
        whatever case the exporter happened to write."""
        real = tmp_path / "meshes" / "x" / "Foo_Bar.NIF"
        real.parent.mkdir(parents=True)
        real.write_bytes(MINIMAL_NIF)
        assert read_mesh(tmp_path, "Meshes\\X\\FOO_BAR.nif") is not None

    def test_an_exact_case_match_never_touches_the_index(
        self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        """The common case -- case already matches -- must not pay for a
        fallback it never needs."""
        real = tmp_path / "meshes" / "x.nif"
        real.parent.mkdir(parents=True)
        real.write_bytes(MINIMAL_NIF)

        def _must_not_be_called(_folder: Path) -> dict[str, Path]:
            raise AssertionError("loose_index() was called for an exact-case match")

        monkeypatch.setattr("wraithguard.nif.vfs.loose_index", _must_not_be_called)
        assert read_mesh(tmp_path, "meshes/x.nif") is not None

    def test_a_loose_case_insensitive_match_still_wins_over_an_archive(
        self, tmp_path: Path
    ) -> None:
        """Loose beats archived even when the loose match is case-insensitive --
        the priority the game itself uses must not depend on how exactly the
        fallback found the file."""
        real = tmp_path / "meshes" / "x.nif"
        real.parent.mkdir(parents=True)
        real.write_bytes(MINIMAL_NIF)
        build_bsa(tmp_path / "Morrowind.bsa", {"meshes\\x.nif": b"not a nif at all"})
        assert read_mesh(tmp_path, "meshes/X.NIF") is not None

    def test_no_loose_match_still_falls_through_to_archives(self, tmp_path: Path) -> None:
        """The fallback must not swallow a file that only exists in a BSA."""
        build_bsa(tmp_path / "Morrowind.bsa", {"meshes\\only_archived.nif": MINIMAL_NIF})
        assert read_mesh(tmp_path, "meshes/only_archived.nif") is not None

    def test_a_genuinely_missing_mesh_still_raises(self, tmp_path: Path) -> None:
        """Indexing the folder must not turn a real miss into a false hit."""
        (tmp_path / "meshes").mkdir()
        (tmp_path / "meshes" / "unrelated.nif").write_bytes(MINIMAL_NIF)
        with pytest.raises(OSError, match=r"nor in any \.bsa"):
            read_mesh(tmp_path, "meshes/missing.nif")


class TestTheLooseIndexItself:
    """:func:`loose_index` in its own right, not just through read_mesh."""

    def test_indexed_once_per_folder(self, tmp_path: Path) -> None:
        """Walking a full Data Files tree is not free -- several providers
        would otherwise re-walk the same folder for each of them."""
        (tmp_path / "meshes").mkdir()
        (tmp_path / "meshes" / "x.nif").write_bytes(MINIMAL_NIF)
        first = loose_index(tmp_path)
        second = loose_index(tmp_path)
        assert first is second

    def test_a_subdirectory_is_walked_not_just_listed(self, tmp_path: Path) -> None:
        """Meshes live several folders deep; a shallow listing would miss them."""
        nested = tmp_path / "meshes" / "a" / "b" / "c"
        nested.mkdir(parents=True)
        (nested / "deep.nif").write_bytes(MINIMAL_NIF)
        index = loose_index(tmp_path)
        assert "meshes/a/b/c/deep.nif" in index

    def test_forget_archives_also_drops_the_loose_index(self, tmp_path: Path) -> None:
        """One cache being named after archives must not hide that it also
        holds loose-file state -- forgetting one and not the other would be
        a stale-index bug waiting to happen after any rescan."""
        (tmp_path / "meshes").mkdir()
        (tmp_path / "meshes" / "x.nif").write_bytes(MINIMAL_NIF)
        first = loose_index(tmp_path)
        forget_archives()
        second = loose_index(tmp_path)
        assert first is not second

    def test_a_scan_error_is_absorbed_not_raised(
        self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        """A folder that can't be scanned must behave like one with nothing
        loose in it, the same way a folder with no ``.bsa`` files behaves
        like one with no archives -- a permissions problem here should not
        crash a viewer that only wanted to check whether one file exists."""

        def _raises(_self: Path, _pattern: str) -> None:
            raise OSError("simulated: cannot scan")

        monkeypatch.setattr(Path, "rglob", _raises)
        assert loose_index(tmp_path) == {}


class TestAnArchiveReadCanStillFailAfterBeingFound:
    """A corrupt archive is a different failure than a missing one."""

    def test_a_truncated_entry_does_not_stop_the_search(self, tmp_path: Path) -> None:
        """One corrupt entry must not be indistinguishable from 'not present
        anywhere' -- read_mesh keeps trying, and only gives up once nothing
        anywhere actually holds the file."""
        build_bsa(
            tmp_path / "Broken.bsa",
            {"meshes\\x.nif": MINIMAL_NIF},
            lie_about_size="meshes\\x.nif",
        )
        with pytest.raises(OSError, match=r"nor in any \.bsa"):
            read_mesh(tmp_path, "meshes/x.nif")

    def test_a_good_archive_after_a_truncated_one_is_still_found(self, tmp_path: Path) -> None:
        """Sorted archive order means the broken one is tried first; this
        proves that failure doesn't stop the rest from being tried too."""
        build_bsa(
            tmp_path / "Broken.bsa",
            {"meshes\\x.nif": MINIMAL_NIF},
            lie_about_size="meshes\\x.nif",
        )
        build_bsa(tmp_path / "Zzz_Good.bsa", {"meshes\\x.nif": MINIMAL_NIF})
        assert read_mesh(tmp_path, "meshes/x.nif") is not None


class TestArchivesInAbsorbsAListingError:
    """Pre-existing gap alongside the one above: a folder that can't even be
    listed must not be treated as an error -- just as one with no archives."""

    def test_a_listing_error_is_absorbed_not_raised(
        self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        def _raises(_self: Path) -> None:
            raise OSError("simulated: cannot list")

        monkeypatch.setattr(Path, "iterdir", _raises)
        assert archives_in(tmp_path) == []
