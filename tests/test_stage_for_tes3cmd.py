"""stage_for_tes3cmd: building a minimal vanilla-layout staging dir for tes3cmd.

tes3cmd walks up from its cwd looking for a 'Morrowind.ini' + 'Data Files'
pair and resolves masters only inside that one folder -- it cannot see an
OpenMW multi-folder VFS. stage_for_tes3cmd works around that by staging one
plugin plus its masters into a throwaway vanilla-shaped layout: masters
hardlinked (falling back to copy) and reused across runs when unchanged, the
plugin always a fresh private copy since tes3cmd rewrites it in place.

Every plugin/master here is a real, minimal, structurally valid TES3 file
built with conftest's write_plugin/tes3_header -- no fixture binaries checked
in, and every field under test (master names, sizes) explicit at the call
site, matching the existing tests/test_plugins.py convention for this file
format.
"""

from __future__ import annotations

import os
import shutil
from pathlib import Path
from typing import Any

import pytest
from conftest import write_plugin

from wraithguard.plugins import PluginFileIndex


def _ini_lines(staging_root: Path) -> list[str]:
    return (staging_root / "Morrowind.ini").read_text(encoding="latin-1").splitlines()


class TestHappyPath:
    def test_masters_and_plugin_are_staged_with_a_correct_ini(
        self, core: Any, tmp_path: Path
    ) -> None:
        mods = tmp_path / "mods"
        mods.mkdir()
        write_plugin(mods / "Morrowind.esm")
        write_plugin(mods / "Tribunal.esm")
        plugin = write_plugin(mods / "MyMod.esp", masters=("Morrowind.esm", "Tribunal.esm"))
        index = PluginFileIndex([mods])
        staging = tmp_path / "staging"

        staged, missing = core.stage_for_tes3cmd(staging, plugin, index)

        assert missing == []
        assert staged == staging / "Data Files" / "MyMod.esp"
        assert staged.read_bytes() == plugin.read_bytes()
        assert (staging / "Data Files" / "Morrowind.esm").is_file()
        assert (staging / "Data Files" / "Tribunal.esm").is_file()
        assert _ini_lines(staging) == [
            "[Game Files]",
            "GameFile0=Morrowind.esm",
            "GameFile1=Tribunal.esm",
            "GameFile2=MyMod.esp",
        ]

    def test_a_nested_staging_root_is_created(self, core: Any, tmp_path: Path) -> None:
        plugin = write_plugin(tmp_path / "MyMod.esp")
        staging = tmp_path / "a" / "b" / "c" / "staging"
        staged, missing = core.stage_for_tes3cmd(staging, plugin, None)
        assert missing == []
        assert staged is not None
        assert staged.parent == staging / "Data Files"


class TestMissingMasters:
    def test_a_master_absent_from_the_index_is_reported_and_excluded(
        self, core: Any, tmp_path: Path
    ) -> None:
        mods = tmp_path / "mods"
        mods.mkdir()
        write_plugin(mods / "Morrowind.esm")  # Tribunal.esm deliberately absent
        plugin = write_plugin(mods / "MyMod.esp", masters=("Morrowind.esm", "Tribunal.esm"))
        index = PluginFileIndex([mods])
        staging = tmp_path / "staging"

        staged, missing = core.stage_for_tes3cmd(staging, plugin, index)

        assert missing == ["Tribunal.esm"]
        assert staged is not None  # the plugin itself still stages
        assert (staging / "Data Files" / "Morrowind.esm").is_file()
        assert not (staging / "Data Files" / "Tribunal.esm").exists()
        assert _ini_lines(staging) == [
            "[Game Files]",
            "GameFile0=Morrowind.esm",
            "GameFile1=MyMod.esp",
        ]

    def test_no_index_marks_every_master_missing(self, core: Any, tmp_path: Path) -> None:
        plugin = write_plugin(
            tmp_path / "MyMod.esp", masters=("Morrowind.esm", "Tribunal.esm")
        )
        staging = tmp_path / "staging"

        staged, missing = core.stage_for_tes3cmd(staging, plugin, None)

        assert missing == ["Morrowind.esm", "Tribunal.esm"]
        assert staged is not None
        assert _ini_lines(staging) == ["[Game Files]", "GameFile0=MyMod.esp"]


class TestMasterCaching:
    """Masters are reused across runs when unchanged -- the 100MB+ files this
    exists for cannot be recopied per plugin cleaned.
    """

    @staticmethod
    def _block(name: str) -> Any:
        """A shutil.copy2 stand-in that fails only for one destination name.

        Lets a test force a specific master's staging to fail without also
        breaking the plugin's own (always-unconditional, uncached) copy,
        which shares the same function.
        """
        real_copy2 = shutil.copy2

        def fake(src: Any, dst: Any, *a: Any, **k: Any) -> Any:
            if Path(dst).name == name:
                raise OSError("simulated failure")
            return real_copy2(src, dst, *a, **k)

        return fake

    def test_an_unchanged_master_is_not_relinked_or_recopied(
        self, core: Any, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        mods = tmp_path / "mods"
        mods.mkdir()
        write_plugin(mods / "Morrowind.esm")
        plugin = write_plugin(mods / "MyMod.esp", masters=("Morrowind.esm",))
        index = PluginFileIndex([mods])
        staging = tmp_path / "staging"

        first_staged, first_missing = core.stage_for_tes3cmd(staging, plugin, index)
        assert first_missing == []

        # Block the master's staging entirely -- if the second run still
        # succeeds, it can only be because the size+mtime match let it reuse
        # the file already sitting there instead of touching link/copy again.
        monkeypatch.setattr(os, "link", self._block("Morrowind.esm"))
        monkeypatch.setattr(shutil, "copy2", self._block("Morrowind.esm"))

        second_staged, second_missing = core.stage_for_tes3cmd(staging, plugin, index)
        assert second_missing == []
        assert second_staged == first_staged

    def test_a_stale_cached_master_is_replaced(self, core: Any, tmp_path: Path) -> None:
        mods = tmp_path / "mods"
        mods.mkdir()
        write_plugin(mods / "Morrowind.esm")
        plugin = write_plugin(mods / "MyMod.esp", masters=("Morrowind.esm",))
        index = PluginFileIndex([mods])
        staging = tmp_path / "staging"
        (staging / "Data Files").mkdir(parents=True)
        # A stale artifact from a differently-sized previous master build --
        # must not be mistaken for a valid cache hit.
        (staging / "Data Files" / "Morrowind.esm").write_bytes(b"stale, wrong content")

        staged, missing = core.stage_for_tes3cmd(staging, plugin, index)

        assert missing == []
        assert staged is not None
        assert (staging / "Data Files" / "Morrowind.esm").read_bytes() == (
            mods / "Morrowind.esm"
        ).read_bytes()


class TestHardlinkFallback:
    def test_a_master_is_hardlinked_when_possible(self, core: Any, tmp_path: Path) -> None:
        mods = tmp_path / "mods"
        mods.mkdir()
        write_plugin(mods / "Morrowind.esm")
        plugin = write_plugin(mods / "MyMod.esp", masters=("Morrowind.esm",))
        index = PluginFileIndex([mods])
        staging = tmp_path / "staging"

        core.stage_for_tes3cmd(staging, plugin, index)

        src_ino = (mods / "Morrowind.esm").stat().st_ino
        dest_ino = (staging / "Data Files" / "Morrowind.esm").stat().st_ino
        assert src_ino == dest_ino, "expected a hardlink (same inode) on a same-volume tmp dir"

    def test_a_hardlink_failure_falls_back_to_a_copy(
        self, core: Any, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        mods = tmp_path / "mods"
        mods.mkdir()
        write_plugin(mods / "Morrowind.esm")
        plugin = write_plugin(mods / "MyMod.esp", masters=("Morrowind.esm",))
        index = PluginFileIndex([mods])
        staging = tmp_path / "staging"

        def always_fails(*_a: Any, **_k: Any) -> None:
            raise OSError("simulated cross-volume link failure")

        monkeypatch.setattr(os, "link", always_fails)

        staged, missing = core.stage_for_tes3cmd(staging, plugin, index)

        assert missing == []
        dest = staging / "Data Files" / "Morrowind.esm"
        assert dest.read_bytes() == (mods / "Morrowind.esm").read_bytes()
        assert dest.stat().st_ino != (mods / "Morrowind.esm").stat().st_ino


class TestThePluginItselfIsNeverCachedOrLinked:
    """Unlike masters, the plugin is the thing tes3cmd rewrites -- it must
    always be a fresh, private copy, never shared via a hardlink and never
    skipped as an unchanged cache hit.
    """

    def test_the_plugin_is_a_copy_not_a_hardlink(self, core: Any, tmp_path: Path) -> None:
        plugin = write_plugin(tmp_path / "MyMod.esp")
        staging = tmp_path / "staging"

        staged, _missing = core.stage_for_tes3cmd(staging, plugin, None)

        assert staged is not None
        assert staged.stat().st_ino != plugin.stat().st_ino

    def test_a_previously_staged_plugin_is_overwritten_with_the_latest_content(
        self, core: Any, tmp_path: Path
    ) -> None:
        plugin = write_plugin(tmp_path / "MyMod.esp", author="first")
        staging = tmp_path / "staging"
        core.stage_for_tes3cmd(staging, plugin, None)

        write_plugin(plugin, author="second")  # same path, new content
        staged, _missing = core.stage_for_tes3cmd(staging, plugin, None)

        assert staged.read_bytes() == plugin.read_bytes()


class TestStagingFailureIsCaughtAndReported:
    def test_a_master_that_cannot_be_linked_or_copied_is_reported_missing(
        self, core: Any, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        mods = tmp_path / "mods"
        mods.mkdir()
        write_plugin(mods / "Morrowind.esm")
        plugin = write_plugin(mods / "MyMod.esp", masters=("Morrowind.esm",))
        index = PluginFileIndex([mods])
        staging = tmp_path / "staging"

        def always_fails(*_a: Any, **_k: Any) -> None:
            raise OSError("simulated disk failure")

        monkeypatch.setattr(os, "link", always_fails)
        monkeypatch.setattr(shutil, "copy2", TestMasterCaching._block("Morrowind.esm"))

        staged, missing = core.stage_for_tes3cmd(staging, plugin, index)

        # Reported, not raised: the caller (t3.py) treats a stage failure as
        # "skip this plugin", not a batch-ending crash.
        assert missing == ["Morrowind.esm"]
        assert staged is not None  # the plugin itself is unaffected

    def test_quiet_suppresses_the_warning_the_default_does_not(
        self, core: Any, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        mods = tmp_path / "mods"
        mods.mkdir()
        write_plugin(mods / "Morrowind.esm")
        plugin = write_plugin(mods / "MyMod.esp", masters=("Morrowind.esm",))
        index = PluginFileIndex([mods])

        def always_fails(*_a: Any, **_k: Any) -> None:
            raise OSError("simulated disk failure")

        monkeypatch.setattr(os, "link", always_fails)
        monkeypatch.setattr(shutil, "copy2", TestMasterCaching._block("Morrowind.esm"))

        calls: list[str] = []
        monkeypatch.setattr(core._LOG, "warning", lambda *a, **k: calls.append(str(a)))

        core.stage_for_tes3cmd(tmp_path / "loud", plugin, index, quiet=False)
        assert len(calls) == 1

        core.stage_for_tes3cmd(tmp_path / "quiet", plugin, index, quiet=True)
        assert len(calls) == 1  # unchanged -- the second call logged nothing


class TestPluginCopyFailureIsNotCaught:
    """Pins the current, narrower contract: only a MASTER's staging failure
    is caught and turned into a `missing` entry. The plugin's own copy has
    no such guard (see stage_for_tes3cmd's source -- it is one bare
    ``_sh.copy2`` call, unlike ``_ensure``'s try/except for masters), so a
    failure there propagates as a real exception rather than the `Path |
    None` return the signature suggests is possible. The GUI caller
    (wraithguard/gui/t3.py) still handles this safely -- it wraps the whole
    per-file call in a broad ``except Exception`` -- so this is not a crash
    in practice, just a narrower contract than the type hint implies.
    """

    def test_a_plugin_copy_failure_raises_rather_than_returning_none(
        self, core: Any, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        plugin = write_plugin(tmp_path / "MyMod.esp")
        real_copy2 = shutil.copy2

        def fake(src: Any, dst: Any, *a: Any, **k: Any) -> Any:
            if Path(dst).name == "MyMod.esp":
                raise OSError("simulated disk failure")
            return real_copy2(src, dst, *a, **k)

        monkeypatch.setattr(shutil, "copy2", fake)

        with pytest.raises(OSError, match="simulated disk failure"):
            core.stage_for_tes3cmd(tmp_path / "staging", plugin, None)


if __name__ == "__main__":
    raise SystemExit(pytest.main([__file__, "-v"]))
