"""read_savegame_content_files, check_savegame_against_order, and scan_backups.

All three were at 0% before this file. The savegame pair matters because a
wrong answer here is exactly backwards from what it should protect against:
OpenMW refuses (or badly degrades) a save whose content files aren't all in
the active list, so this is the one check standing between a plugin removal
and a broken save nobody notices until they try to load it.

Saves reuse the plugin record format exactly -- a TES3 header record, then a
SAVE record whose DEPE subrecords each carry one content filename -- so this
reuses conftest's `rec`/`sub`/`zstr` rather than inventing a second builder.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from conftest import rec, sub, zstr

import wraithguard_toolkit as core

if TYPE_CHECKING:
    from pathlib import Path


def _save(*content_files: str) -> bytes:
    """A minimal but structurally valid .omwsave: TES3 header + SAVE record."""
    header = rec("TES3", b"")
    body = b"".join(sub("DEPE", zstr(name)) for name in content_files)
    return header + rec("SAVE", body)


def _write_save(tmp_path: Path, *content_files: str) -> Path:
    path = tmp_path / "quicksave.omwsave"
    path.write_bytes(_save(*content_files))
    return path


class TestReadSavegameContentFiles:
    def test_the_saves_content_files_are_returned_in_order(self, tmp_path: Path) -> None:
        path = _write_save(tmp_path, "Morrowind.esm", "Tribunal.esm", "MyMod.esp")

        files, error = core.read_savegame_content_files(path)

        assert files == ["Morrowind.esm", "Tribunal.esm", "MyMod.esp"]
        assert error is None

    def test_a_save_with_no_content_files_returns_an_empty_list_not_none(
        self, tmp_path: Path
    ) -> None:
        path = _write_save(tmp_path)

        files, error = core.read_savegame_content_files(path)

        assert files == []
        assert error is None

    def test_a_missing_file_reports_the_read_error(self, tmp_path: Path) -> None:
        files, error = core.read_savegame_content_files(tmp_path / "does-not-exist.omwsave")

        assert files is None
        assert error is not None and "can't read save" in error

    def test_bytes_that_are_not_even_tes3_are_rejected_clearly(self, tmp_path: Path) -> None:
        path = tmp_path / "not-a-save.omwsave"
        path.write_bytes(b"this is not a TES3 file at all")

        files, error = core.read_savegame_content_files(path)

        assert files is None
        assert error == "not an OpenMW save (no TES3 header)"

    def test_a_tes3_file_with_no_save_record_is_reported_as_such(self, tmp_path: Path) -> None:
        # A real .esp: TES3 header, but never a SAVE record -- someone pointed
        # this check at a plugin instead of a save.
        path = tmp_path / "actually-a-plugin.esp"
        path.write_bytes(rec("TES3", b"") + rec("STAT", sub("NAME", zstr("torch_01"))))

        files, error = core.read_savegame_content_files(path)

        assert files is None
        assert error == "no SAVE record found -- not a savegame?"


class TestCheckSavegameAgainstOrder:
    def test_every_content_file_present_means_nothing_missing(self, tmp_path: Path) -> None:
        path = _write_save(tmp_path, "Morrowind.esm", "MyMod.esp")

        files, missing, error = core.check_savegame_against_order(
            path, ["Morrowind.esm", "MyMod.esp", "Unrelated.esp"]
        )

        assert files == ["Morrowind.esm", "MyMod.esp"]
        assert missing == []
        assert error is None

    def test_a_content_file_absent_from_the_order_is_reported_missing(self, tmp_path: Path) -> None:
        path = _write_save(tmp_path, "Morrowind.esm", "Removed.esp")

        _files, missing, _error = core.check_savegame_against_order(path, ["Morrowind.esm"])

        assert missing == ["Removed.esp"]

    def test_the_comparison_is_case_insensitive(self, tmp_path: Path) -> None:
        path = _write_save(tmp_path, "MYMOD.ESP")

        _files, missing, _error = core.check_savegame_against_order(path, ["MyMod.esp"])

        assert missing == []

    def test_missing_order_preserves_the_saves_own_order_not_the_active_lists(
        self, tmp_path: Path
    ) -> None:
        path = _write_save(tmp_path, "Zebra.esp", "Alpha.esp")

        _files, missing, _error = core.check_savegame_against_order(path, [])

        assert missing == ["Zebra.esp", "Alpha.esp"]

    def test_an_unreadable_save_propagates_its_error_with_none_lists(self, tmp_path: Path) -> None:
        files, missing, error = core.check_savegame_against_order(
            tmp_path / "does-not-exist.omwsave", ["Morrowind.esm"]
        )

        assert files is None
        assert missing is None
        assert error is not None and "can't read save" in error


class TestScanBackups:
    def test_a_preclean_backup_is_matched_to_its_original_name(self, tmp_path: Path) -> None:
        (tmp_path / "MyMod.esp.preclean.bak").write_bytes(b"")

        results = core.scan_backups([str(tmp_path)])

        assert len(results) == 1
        backup, original, kind = results[0]
        assert backup == tmp_path / "MyMod.esp.preclean.bak"
        assert original == tmp_path / "MyMod.esp"
        assert kind == "preclean.bak"

    def test_a_tes3cmd_tilde_backup_is_matched_to_its_original_name(self, tmp_path: Path) -> None:
        (tmp_path / "MyMod~1.esp").write_bytes(b"")

        results = core.scan_backups([str(tmp_path)])

        assert len(results) == 1
        backup, original, kind = results[0]
        assert backup == tmp_path / "MyMod~1.esp"
        assert original == tmp_path / "MyMod.esp"
        assert kind == "tes3cmd ~N"

    def test_a_file_matching_no_backup_pattern_is_not_reported(self, tmp_path: Path) -> None:
        (tmp_path / "MyMod.esp").write_bytes(b"")

        assert core.scan_backups([str(tmp_path)]) == []

    def test_results_are_sorted_case_insensitively_by_path(self, tmp_path: Path) -> None:
        (tmp_path / "zebra.esp.preclean.bak").write_bytes(b"")
        (tmp_path / "Alpha.esp.preclean.bak").write_bytes(b"")

        results = core.scan_backups([str(tmp_path)])

        assert [str(r[0]) for r in results] == sorted((str(r[0]) for r in results), key=str.lower)

    def test_an_original_that_no_longer_exists_is_still_reported(self, tmp_path: Path) -> None:
        # Restoring a backup whose original was since deleted is a valid
        # recovery -- the caller decides how to flag that, not this scan.
        (tmp_path / "Gone.esp.masterfix.bak").write_bytes(b"")

        results = core.scan_backups([str(tmp_path)])

        assert len(results) == 1
        _backup, original, kind = results[0]
        assert original == tmp_path / "Gone.esp"
        assert not original.exists()
        assert kind == "masterfix.bak"

    def test_a_timestamped_backup_is_matched_to_its_original_name(self, tmp_path: Path) -> None:
        (tmp_path / "MyMod.esp.bak-20240115-093000").write_bytes(b"")

        results = core.scan_backups([str(tmp_path)])

        assert len(results) == 1
        backup, original, kind = results[0]
        assert backup == tmp_path / "MyMod.esp.bak-20240115-093000"
        assert original == tmp_path / "MyMod.esp"
        assert kind == "timestamped .bak"

    def test_a_configurator_backup_is_matched_to_its_original_name(self, tmp_path: Path) -> None:
        (tmp_path / "openmw.cfg.backup.20240115").write_bytes(b"")

        results = core.scan_backups([str(tmp_path)])

        assert len(results) == 1
        backup, original, kind = results[0]
        assert backup == tmp_path / "openmw.cfg.backup.20240115"
        assert original == tmp_path / "openmw.cfg"
        assert kind == "configurator .backup"

    def test_the_cfg_files_own_folder_is_scanned_too(self, tmp_path: Path) -> None:
        # A common real shape: dirs=[] (nothing else to scan yet), but the cfg
        # itself sits next to old Configurator backups worth surfacing.
        cfg_dir = tmp_path / "config"
        cfg_dir.mkdir()
        (cfg_dir / "openmw.cfg").write_bytes(b"")
        (cfg_dir / "openmw.cfg.backup.20240101").write_bytes(b"")

        results = core.scan_backups([], cfg_path=cfg_dir / "openmw.cfg")

        assert len(results) == 1
        assert results[0][0] == cfg_dir / "openmw.cfg.backup.20240101"

    def test_a_root_that_does_not_exist_is_skipped_without_error(self, tmp_path: Path) -> None:
        real = tmp_path / "real"
        real.mkdir()
        (real / "MyMod.esp.preclean.bak").write_bytes(b"")

        results = core.scan_backups([str(tmp_path / "does-not-exist"), str(real)])

        assert len(results) == 1
        assert results[0][0] == real / "MyMod.esp.preclean.bak"

    def test_a_folder_reachable_two_ways_is_not_double_counted(self, tmp_path: Path) -> None:
        # The dedup key: dirs=[tmp_path] and cfg_path also living under
        # tmp_path both reach the same backup file -- it must be reported once.
        (tmp_path / "MyMod.esp.preclean.bak").write_bytes(b"")
        (tmp_path / "openmw.cfg").write_bytes(b"")

        results = core.scan_backups([str(tmp_path)], cfg_path=tmp_path / "openmw.cfg")

        assert len(results) == 1

    def test_max_depth_prunes_folders_nested_past_the_limit(self, tmp_path: Path) -> None:
        shallow = tmp_path / "MyMod.esp.preclean.bak"
        shallow.write_bytes(b"")
        deep_dir = tmp_path / "a" / "b" / "c" / "d" / "e"
        deep_dir.mkdir(parents=True)
        (deep_dir / "TooDeep.esp.preclean.bak").write_bytes(b"")

        results = core.scan_backups([str(tmp_path)], max_depth=2)

        found = {str(r[0]) for r in results}
        assert str(shallow) in found
        assert not any("TooDeep" in name for name in found)
