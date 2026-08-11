"""Locating the ``tes3conv`` / ``tes3cmd`` executables, and building a tes3cmd argv.

These search an explicit path, an environment variable, ``PATH``, then the
script's own folder and any extra dirs -- the discovery the GUI and CLI both lean
on, and untested because it depends on the machine's filesystem. A tmp dir and a
stubbed ``shutil.which`` make it deterministic.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

import wraithguard_toolkit as core

if TYPE_CHECKING:
    from pathlib import Path

    import pytest


def _fake_binary(tmp_path: Path, name: str) -> Path:
    """Write a stand-in executable and return its path."""
    path = tmp_path / name
    path.write_text("#!/bin/sh\n", encoding="utf-8")
    return path


class TestFindTes3conv:
    """The search order: explicit, env, PATH, script dir + extra dirs."""

    def test_an_explicit_existing_path_wins(
        self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        """A path handed in directly is used when it exists."""
        monkeypatch.setattr("shutil.which", lambda _n: None)
        binary = _fake_binary(tmp_path, "tes3conv")
        assert core.find_tes3conv(explicit=str(binary)) == str(binary)

    def test_the_env_var_is_honoured(self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
        """``MLOX_TES3CONV`` names the binary when set."""
        monkeypatch.setattr("shutil.which", lambda _n: None)
        binary = _fake_binary(tmp_path, "tes3conv")
        monkeypatch.setenv("MLOX_TES3CONV", str(binary))
        assert core.find_tes3conv() == str(binary)

    def test_the_path_is_searched(self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
        """A binary on ``PATH`` is found via ``shutil.which``."""
        binary = _fake_binary(tmp_path, "tes3conv")
        monkeypatch.delenv("MLOX_TES3CONV", raising=False)
        monkeypatch.setattr("shutil.which", lambda n: str(binary) if n == "tes3conv" else None)
        assert core.find_tes3conv() == str(binary)

    def test_extra_dirs_are_searched(self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
        """A data folder passed in is searched by binary name."""
        _fake_binary(tmp_path, "tes3conv")
        monkeypatch.delenv("MLOX_TES3CONV", raising=False)
        monkeypatch.setattr("shutil.which", lambda _n: None)
        assert core.find_tes3conv(extra_dirs=[str(tmp_path)]) == str(tmp_path / "tes3conv")

    def test_nothing_found_is_none(self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
        """No candidate anywhere yields ``None``, not an error."""
        monkeypatch.delenv("MLOX_TES3CONV", raising=False)
        monkeypatch.setattr("shutil.which", lambda _n: None)
        assert core.find_tes3conv(extra_dirs=[str(tmp_path)]) is None


class TestFindTes3cmd:
    """Same order, preferring the compiled ``.exe`` over the perl script."""

    def test_an_explicit_path_wins(self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
        """An explicit compiled build is used as given."""
        monkeypatch.setattr("shutil.which", lambda _n: None)
        binary = _fake_binary(tmp_path, "tes3cmd.exe")
        assert core.find_tes3cmd(explicit=str(binary)) == str(binary)

    def test_the_env_var_is_honoured(self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
        """``MLOX_TES3CMD`` names the binary when set."""
        monkeypatch.setattr("shutil.which", lambda _n: None)
        binary = _fake_binary(tmp_path, "tes3cmd.exe")
        monkeypatch.setenv("MLOX_TES3CMD", str(binary))
        assert core.find_tes3cmd() == str(binary)

    def test_extra_dirs_are_searched(self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
        """A data folder is searched, compiled build first."""
        _fake_binary(tmp_path, "tes3cmd.exe")
        monkeypatch.delenv("MLOX_TES3CMD", raising=False)
        monkeypatch.setattr("shutil.which", lambda _n: None)
        assert core.find_tes3cmd(extra_dirs=[str(tmp_path)]) == str(tmp_path / "tes3cmd.exe")

    def test_nothing_found_is_none(self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
        """No candidate yields ``None``."""
        monkeypatch.delenv("MLOX_TES3CMD", raising=False)
        monkeypatch.setattr("shutil.which", lambda _n: None)
        assert core.find_tes3cmd(extra_dirs=[str(tmp_path)]) is None


class TestTes3cmdInvocation:
    """Turning a tes3cmd path into an argv prefix, or explaining why not."""

    def test_a_compiled_exe_runs_directly(self, tmp_path: Path) -> None:
        """An ``.exe`` needs no interpreter and is not even read."""
        exe = tmp_path / "tes3cmd.exe"
        argv, err = core.tes3cmd_invocation(exe)
        assert argv == [str(exe)]
        assert err is None

    def test_a_plain_binary_runs_directly(self, tmp_path: Path) -> None:
        """A non-script binary with no perl marker runs as itself."""
        binary = tmp_path / "tes3cmd"
        binary.write_bytes(b"\x7fELF not a script")
        argv, err = core.tes3cmd_invocation(binary)
        assert argv == [str(binary)]
        assert err is None

    def test_a_perl_script_is_run_through_perl(
        self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        """The pure-perl script is prefixed with a perl found on PATH."""
        script = tmp_path / "tes3cmd"
        script.write_text("#!/usr/bin/perl\n# tes3cmd\n", encoding="utf-8")
        monkeypatch.setattr("shutil.which", lambda n: "/usr/bin/perl" if n == "perl" else None)
        argv, err = core.tes3cmd_invocation(script)
        assert argv == ["/usr/bin/perl", str(script)]
        assert err is None

    def test_a_perl_script_without_perl_explains_why(
        self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        """No perl means a clear message pointing at the compiled build."""
        script = tmp_path / "tes3cmd"
        script.write_text("#!/usr/bin/perl\n", encoding="utf-8")
        monkeypatch.setattr("shutil.which", lambda _n: None)
        argv, err = core.tes3cmd_invocation(script)
        assert argv is None
        assert err is not None
        assert "perl" in err

    def test_an_unreadable_path_is_reported(self, tmp_path: Path) -> None:
        """A path that cannot be opened is reported, not raised."""
        missing = tmp_path / "gone"  # no .exe suffix, does not exist
        argv, err = core.tes3cmd_invocation(missing)
        assert argv is None
        assert err is not None
        assert "can't read" in err
