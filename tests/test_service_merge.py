"""Driving build_merged_lands and _records_via without a real tes3conv.

A plugin with no ``LAND``/``LTEX`` bytes short-circuits ``has_landscape``, so a
load order of terrain-free plugins runs the whole merge orchestration -- read
masters, read mods, merge, seam-repair, clean, "nothing to merge" -- with no
converter needed. That reaches the reporting, verbose, not-found, bad-meta and
master-read-failure branches. A POSIX fake converter covers the remaining
``_records_via`` edges (no JSON, non-list JSON). The happy path that actually
writes records still needs a real tes3conv (covered by the fixture-gated tests).
"""

from __future__ import annotations

import subprocess
from pathlib import Path
from typing import TYPE_CHECKING, Any

import pytest

import wraithguard.land.service as svc
from wraithguard.land.service import (
    MergeServiceError,
    _records_via,
    build_merged_lands,
)

if TYPE_CHECKING:
    import types
    from collections.abc import Callable

_TES3_HEADER = b"TES3" + b"\x00" * 320


@pytest.fixture
def real_tes3conv(core: types.ModuleType) -> str:
    """A real tes3conv executable, or skip.

    The error-exit branch is the only converter failure a *real* tes3conv can
    produce (a real one always writes valid JSON on valid input, so the
    no-JSON/non-list/unparseable branches are exercised with an in-process fake
    instead). Resolution matches the tool's own -- ``MLOX_TES3CONV``, PATH, or
    beside ``wraithguard_toolkit.py``.
    """
    found = core.find_tes3conv()
    if not found:
        pytest.skip("needs a real tes3conv executable (set MLOX_TES3CONV or put it on PATH)")
    return found


def _fake_convert(write: Callable[[Path], None] | None, returncode: int = 0) -> Callable[..., Any]:
    """A stand-in for ``subprocess.run`` that fakes a converter's output.

    Portable where a real converter cannot be: it writes whatever ``write``
    puts at the output path (argv[2]) -- or nothing -- and returns ``returncode``.
    """

    def run(cmd: list[str], **_kwargs: Any) -> subprocess.CompletedProcess[str]:
        if write is not None:
            write(Path(cmd[2]))
        return subprocess.CompletedProcess(cmd, returncode, stdout="", stderr="")

    return run


def _no_terrain(path: Path, name: str) -> None:
    """A readable TES3 plugin with no LAND/LTEX -- 'no terrain', reads empty."""
    (path / name).write_bytes(_TES3_HEADER)


def _has_terrain(path: Path, name: str) -> None:
    """A TES3 plugin whose bytes carry a LAND tag but are otherwise garbage,
    so has_landscape says yes and every real read then fails."""
    (path / name).write_bytes(_TES3_HEADER + b"LAND" + b"\x00" * 64)


def _bad_meta(path: Path, stem: str) -> None:
    """A ``.mergedlands.toml`` declaring a version the parser refuses."""
    (path / f"{stem}.mergedlands.toml").write_text('version = "999"\n', encoding="utf-8")


def _meta_with_layers(path: Path, stem: str) -> None:
    """A valid sidecar that excludes a layer, so the plugin 'carries settings'."""
    (path / f"{stem}.mergedlands.toml").write_text(
        "[height_map]\nincluded = false\n", encoding="utf-8"
    )


class TestEmptyMergeDrivesTheOrchestration:
    """A terrain-free load order runs everything up to 'nothing to merge'."""

    def test_verbose_run_reports_every_stage(self, tmp_path: Path) -> None:
        """Report callback, verbose plugin detail, and a not-found mod all fire."""
        _no_terrain(tmp_path, "Master.esm")
        _no_terrain(tmp_path, "ModA.esp")
        _meta_with_layers(tmp_path, "ModA")  # so it 'carries settings'
        collected: list[str] = []
        result = build_merged_lands(
            [tmp_path],
            ["Master.esm", "ModA.esp", "Missing.esp"],
            converter="tes3conv",
            report=collected.append,
            verbose=True,
        )
        assert result.output is None  # nothing to merge, nothing written
        joined = "\n".join(collected)
        assert "masters:" in joined
        assert "carry .mergedlands.toml settings" in joined
        assert "could not be read" in joined  # Missing.esp

    def test_non_verbose_run_summarises(self, tmp_path: Path) -> None:
        """Without verbose, the unreadable list is summarised on one line."""
        _no_terrain(tmp_path, "Master.esm")
        result = build_merged_lands([tmp_path], ["Master.esm", "Gone.esp"], converter="tes3conv")
        assert result.output is None

    def test_progress_is_reported_every_fifty_mods(self, tmp_path: Path) -> None:
        """A large load order logs progress in batches of fifty."""
        _no_terrain(tmp_path, "Master.esm")
        for i in range(50):
            _no_terrain(tmp_path, f"mod{i:02d}.esp")
        order = ["Master.esm", *[f"mod{i:02d}.esp" for i in range(50)]]
        collected: list[str] = []
        build_merged_lands([tmp_path], order, converter="tes3conv", report=collected.append)
        assert any("/50" in line for line in collected)


class TestMergeErrorBranches:
    """The read-failure and bad-sidecar paths abort with an explanation."""

    def test_a_master_with_an_invalid_sidecar_is_fatal(self, tmp_path: Path) -> None:
        """A master's unreadable ``.mergedlands.toml`` stops the run."""
        _no_terrain(tmp_path, "Master.esm")
        _bad_meta(tmp_path, "Master")
        with pytest.raises(MergeServiceError):
            build_merged_lands([tmp_path], ["Master.esm"], converter="tes3conv")

    def test_a_mod_with_an_invalid_sidecar_is_fatal(self, tmp_path: Path) -> None:
        """A mod's unreadable ``.mergedlands.toml`` stops the run too."""
        _no_terrain(tmp_path, "Master.esm")
        _no_terrain(tmp_path, "Mod.esp")
        _bad_meta(tmp_path, "Mod")
        with pytest.raises(MergeServiceError):
            build_merged_lands([tmp_path], ["Master.esm", "Mod.esp"], converter="tes3conv")

    def test_a_master_tes3conv_refuses_is_read_natively(self, tmp_path: Path) -> None:
        """A master tes3conv cannot open is read directly instead, and the run
        continues -- the native rescue path, counted and reported."""
        _has_terrain(tmp_path, "Master.esm")
        collected: list[str] = []
        result = build_merged_lands(
            [tmp_path], ["Master.esm"], converter="tes3conv", report=collected.append
        )
        assert result.output is None
        assert any("read directly" in line for line in collected)


class TestRecordsVia:
    """The per-plugin read, short of a real conversion."""

    def test_a_plugin_with_no_terrain_returns_empty(self, tmp_path: Path) -> None:
        """has_landscape says no, so no converter is even run."""
        _no_terrain(tmp_path, "plain.esp")
        records, reason = _records_via("tes3conv", tmp_path / "plain.esp", tmp_path)
        assert records == []
        assert reason == ""

    def test_a_converter_that_writes_no_json_is_reported(
        self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        """A converter that exits 0 but writes nothing is a failure, not empty.

        A real converter cannot produce this (it always writes JSON on success),
        so the run is faked in-process -- portable, and no POSIX shell needed.
        """
        _has_terrain(tmp_path, "mod.esp")
        monkeypatch.setattr(svc.subprocess, "run", _fake_convert(write=None))
        records, reason = _records_via("tes3conv", tmp_path / "mod.esp", tmp_path)
        assert records == []
        assert "wrote no JSON" in reason

    def test_a_converter_that_writes_non_list_json_is_reported(
        self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        """JSON that is not a record list is rejected, not merged."""
        _has_terrain(tmp_path, "mod.esp")
        monkeypatch.setattr(
            svc.subprocess,
            "run",
            _fake_convert(lambda out: out.write_text("{}", encoding="utf-8")),
        )
        records, reason = _records_via("tes3conv", tmp_path / "mod.esp", tmp_path)
        assert records == []
        assert "not a record list" in reason

    def test_a_converter_error_exit_is_reported(
        self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch, real_tes3conv: str
    ) -> None:
        """A non-zero exit carries the converter's own complaint.

        This is the one converter failure a real tes3conv genuinely produces, so
        it runs the real binary on a plugin it cannot read -- the real-converter
        fallback -- rather than faking the process.
        """
        monkeypatch.setattr(svc, "has_landscape", lambda *_a, **_k: True)
        bad = tmp_path / "mod.esp"
        bad.write_bytes(b"not a real plugin")
        records, reason = _records_via(real_tes3conv, bad, tmp_path)
        assert records == []
        assert reason.startswith("tes3conv exited")

    def test_unparseable_json_is_reported(
        self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        """Output that is not valid JSON at all is reported, not merged."""
        _has_terrain(tmp_path, "mod.esp")
        monkeypatch.setattr(
            svc.subprocess,
            "run",
            _fake_convert(lambda out: out.write_text("not json {", encoding="utf-8")),
        )
        records, reason = _records_via("tes3conv", tmp_path / "mod.esp", tmp_path)
        assert records == []
        assert "will not parse" in reason
