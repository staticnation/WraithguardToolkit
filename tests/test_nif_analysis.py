"""Tests for the on-demand mesh analysis service.

The interesting assertions here are all about *refusing* to answer. Parsing a
mesh is the easy half; the half that matters is that a mod folder full of
files written by strangers over twenty years cannot make a scan crash, and
cannot make the tool claim a mesh lost its collision when the truth is that we
could not read it.
"""

from __future__ import annotations

import struct
from typing import TYPE_CHECKING

import pytest

from wraithguard.nif.analysis import MeshAnalyser, MeshFinding, file_digest

if TYPE_CHECKING:
    from pathlib import Path

HEADER = b"NetImmerse File Format, Version 4.0.0.2\n"

#: A version no Morrowind mesh uses. Picked as a *later* NIF version rather
#: than an earlier one, because 4.0.0.0 was once the obvious choice here and
#: then became supported -- which quietly turned this test into an assertion
#: about the accepted-version set instead of about error handling.
UNSUPPORTED_VERSION = 0x0A010000


def minimal_nif(*, version: int = 0x04000002, blocks: int = 0) -> bytes:
    """A NIF with a valid header and no blocks.

    Args:
        version: The version word to write.
        blocks: The block count to declare.

    Returns:
        The file bytes.
    """
    return HEADER + struct.pack("<II", version, blocks)


def write(path: Path, data: bytes) -> Path:
    """Write bytes and return the path.

    Args:
        path: Where to write.
        data: What to write.

    Returns:
        The path written.
    """
    path.write_bytes(data)
    return path


class TestGracefulDegradation:
    """A mod folder must not be able to break a scan."""

    def test_the_two_versions_morrowind_ships_are_both_accepted(self, tmp_path: Path) -> None:
        """4.0.0.0 and 4.0.0.2 differ in the header, not in the layouts.

        Verified on 40 mod meshes: rewriting only the version word made every
        one parse identically to the layout-free scan.
        """
        analyser = MeshAnalyser()
        for version in (0x04000000, 0x04000002):
            mesh = write(tmp_path / f"v{version:x}.nif", minimal_nif(version=version))
            assert not isinstance(analyser.structure(mesh), str), version

    def test_a_newer_nif_version_is_a_finding_not_an_exception(self, tmp_path: Path) -> None:
        """Mod folders hold meshes for other engines and other versions."""
        old = write(tmp_path / "a.nif", minimal_nif(version=UNSUPPORTED_VERSION))
        good = write(tmp_path / "b.nif", minimal_nif())
        finding = MeshAnalyser().compare_providers("meshes/x.nif", old, good)
        assert finding.unreadable
        assert "version" in finding.unreadable.lower()
        assert finding.difference is None

    def test_a_file_that_is_not_a_nif_at_all_is_a_finding(self, tmp_path: Path) -> None:
        """Extensions lie. A .nif holding a JPEG must not raise."""
        junk = write(tmp_path / "a.nif", b"\xff\xd8\xff\xe0 this is not a mesh")
        good = write(tmp_path / "b.nif", minimal_nif())
        finding = MeshAnalyser().compare_providers("meshes/x.nif", junk, good)
        assert finding.unreadable
        assert not finding.reliable

    def test_a_missing_file_is_a_finding(self, tmp_path: Path) -> None:
        """A provider can vanish between the scan and the analysis."""
        good = write(tmp_path / "b.nif", minimal_nif())
        finding = MeshAnalyser().compare_providers("meshes/x.nif", tmp_path / "gone.nif", good)
        assert finding.unreadable
        assert not finding.reliable

    def test_an_empty_file_is_a_finding(self, tmp_path: Path) -> None:
        """Truncated downloads are common and are zero bytes surprisingly often."""
        empty = write(tmp_path / "a.nif", b"")
        good = write(tmp_path / "b.nif", minimal_nif())
        assert MeshAnalyser().compare_providers("m/x.nif", empty, good).unreadable


class TestAbsenceIsNeverProvenByAPartialRead:
    """The guarantee the whole interface exists to keep."""

    def test_an_unreadable_side_is_never_reliable(self, tmp_path: Path) -> None:
        """No difference, so nothing to report as a loss."""
        bad = write(tmp_path / "a.nif", b"nope")
        good = write(tmp_path / "b.nif", minimal_nif())
        finding = MeshAnalyser().compare_providers("m/x.nif", bad, good)
        assert not finding.reliable
        assert not finding.worth_reporting

    @pytest.mark.parametrize(
        ("loser_partial", "winner_partial"),
        [(True, False), (False, True), (True, True)],
    )
    def test_a_partial_read_on_either_side_blocks_the_claim(
        self, loser_partial: bool, winner_partial: bool
    ) -> None:
        """A partial read can prove presence, never absence.

        Reporting "lost collision" from a truncated parse would be a false
        alarm about the one finding a user acts on immediately.
        """
        finding = MeshFinding(
            "m/x.nif",
            difference=None,
            loser_partial=loser_partial,
            winner_partial=winner_partial,
        )
        assert not finding.reliable

    def test_a_clean_pair_is_reliable(self, tmp_path: Path) -> None:
        """A negative control: the guard must not refuse everything.

        Without this, ``reliable`` could be hard-wired to ``False`` and every
        test above would still pass.
        """
        one = write(tmp_path / "a.nif", minimal_nif())
        two = write(tmp_path / "b.nif", minimal_nif())
        assert MeshAnalyser().compare_providers("m/x.nif", one, two).reliable


class TestCachingIsByContent:
    """Keying on bytes rather than path is the point of the cache."""

    def test_identical_bytes_at_different_paths_parse_once(self, tmp_path: Path) -> None:
        """Mods re-ship identical assets constantly.

        Keying on path would re-parse one mesh body once per provider, which is
        exactly the case the cache exists for.
        """
        data = minimal_nif()
        first = write(tmp_path / "one.nif", data)
        second = write(tmp_path / "two.nif", data)
        analyser = MeshAnalyser()
        analyser.structure(first)
        analyser.structure(second)
        assert analyser.parsed == 1
        assert analyser.cache_hits == 1

    def test_different_bytes_are_parsed_separately(self, tmp_path: Path) -> None:
        """A negative control: the cache must not collapse distinct meshes."""
        first = write(tmp_path / "one.nif", minimal_nif(blocks=0))
        second = write(tmp_path / "two.nif", minimal_nif(blocks=1))
        analyser = MeshAnalyser()
        analyser.structure(first)
        analyser.structure(second)
        assert analyser.parsed == 2
        assert analyser.cache_hits == 0

    def test_an_unreadable_file_is_not_remembered_as_a_failure(self, tmp_path: Path) -> None:
        """A file that could not be *hashed* has no identity to cache under.

        ``file_digest`` returns an empty digest, and an empty digest must not
        become a cache key -- otherwise the first unreadable file would answer
        for every later one.
        """
        analyser = MeshAnalyser()
        analyser.structure(tmp_path / "missing_one.nif")
        analyser.structure(tmp_path / "missing_two.nif")
        assert analyser.cache_hits == 0

    def test_a_missing_file_hashes_to_nothing(self, tmp_path: Path) -> None:
        """And an empty digest is falsy, so callers can test it plainly."""
        assert file_digest(tmp_path / "absent.nif") == ""
        assert file_digest(write(tmp_path / "there.nif", minimal_nif()))


class TestDigestReuse:
    """Hashing dominates once parsing is cached, so it must be avoidable."""

    def test_a_caller_supplied_digest_skips_hashing(self, tmp_path: Path) -> None:
        """``detect_resource_conflicts`` already hashes these files.

        Recomputing was five seconds over a corpus where every parse was
        already a cache hit -- the cache had removed the wrong cost.
        """
        mesh = write(tmp_path / "a.nif", minimal_nif())
        analyser = MeshAnalyser()
        analyser.structure(mesh, digest="deadbeef")
        assert analyser.hashed == 0

    def test_an_unchanged_file_is_not_rehashed(self, tmp_path: Path) -> None:
        """Path, size and mtime identify a file well enough to skip the read."""
        mesh = write(tmp_path / "a.nif", minimal_nif())
        analyser = MeshAnalyser()
        analyser.structure(mesh)
        analyser.structure(mesh)
        assert analyser.hashed == 1

    def test_a_changed_file_is_rehashed(self, tmp_path: Path) -> None:
        """A negative control: the memo must not outlive the bytes it describes.

        Without this, the shortcut above would happily answer for a file the
        user had edited between two scans.
        """
        mesh = write(tmp_path / "a.nif", minimal_nif(blocks=0))
        analyser = MeshAnalyser()
        analyser.structure(mesh)
        import os
        import time

        time.sleep(0.01)
        write(mesh, minimal_nif(blocks=1))
        os.utime(mesh, ns=(time.time_ns(), time.time_ns()))
        analyser.structure(mesh)
        assert analyser.hashed == 2


class TestReadErrorsAreNotParseErrors:
    """An unreadable path and an unparseable file are different findings."""

    def test_a_directory_where_a_mesh_should_be_is_reported(self, tmp_path: Path) -> None:
        """Mod archives extract oddly; a folder can end up named ``x.nif``.

        This is an ``OSError`` rather than a parse failure, and it took an
        audit to notice the branch had never been exercised.
        """
        folder = tmp_path / "a.nif"
        folder.mkdir()
        outcome = MeshAnalyser().structure(folder)
        assert isinstance(outcome, str)
        assert outcome

    def test_something_worth_reporting_is_reported(self, tmp_path: Path) -> None:
        """A negative control for ``worth_reporting``, which only ever ran false.

        Every existing test asserted it was ``False``; nothing checked it could
        become ``True``, so a hard-wired ``False`` would have passed them all.
        """
        from wraithguard.nif.report import Difference

        finding = MeshFinding(
            "m/a.nif",
            difference=Difference(0.1, True, False, ["new.dds"], [], False),
        )
        assert finding.reliable
        assert finding.worth_reporting
