"""Tests for the pure parts of ``tools/survey_landscape.py``.

The load-order and pre-scan helpers decide which plugins are read and in what
order, and both can be wrong silently: a mis-ordered survey mistranslates
texture indices, and a pre-scan that skips the wrong file reports terrain as
unedited. Neither failure raises, so both are tested directly.
"""

from __future__ import annotations

import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from tools.survey_landscape import (
    apply_order,
    mentions_landscape,
    read_json_records,
    read_order,
)


class TestReadOrder:
    """Load order files come from mlox, mod managers and hand editing."""

    def test_names_are_read_in_order(self, tmp_path: Path) -> None:
        """The file's order is the load order."""
        path = tmp_path / "order.txt"
        path.write_text("B.esp\nA.esp\n", encoding="utf-8")
        assert read_order(path) == ["b.esp", "a.esp"]

    def test_comments_and_blanks_are_ignored(self, tmp_path: Path) -> None:
        """mlox output carries both, and neither is a plugin."""
        path = tmp_path / "order.txt"
        path.write_text("# a comment\n\nA.esp\n   \n", encoding="utf-8")
        assert read_order(path) == ["a.esp"]

    def test_paths_are_reduced_to_names(self, tmp_path: Path) -> None:
        """A list of full paths is still a load order."""
        path = tmp_path / "order.txt"
        path.write_text("C:/Games/Data Files/A.esp\n", encoding="utf-8")
        assert read_order(path) == ["a.esp"]

    def test_a_missing_file_stops_the_run(self, tmp_path: Path) -> None:
        """Silently surveying alphabetically after being given an order would
        produce a wrong answer that looks like the requested one."""
        with pytest.raises(SystemExit):
            read_order(tmp_path / "nope.txt")

    def test_openmw_cfg_content_lines(self, tmp_path: Path) -> None:
        """An openmw.cfg is what users actually have, so it must work directly."""
        path = tmp_path / "openmw.cfg"
        path.write_text(
            "encoding=win1252\n"
            "fallback=lightattenuation_useconstant,1\n"
            "data=C:/Games/Data Files\n"
            "content=Morrowind.esm\n"
            "content=Tribunal.esm\n"
            "content=Some Mod.esp\n",
            encoding="utf-8",
        )
        assert read_order(path) == ["morrowind.esm", "tribunal.esm", "some mod.esp"]

    def test_settings_lines_are_not_mistaken_for_plugins(self, tmp_path: Path) -> None:
        """The bug this guards against, reproduced.

        Taking every non-comment line as a plugin turned a real openmw.cfg into
        2,836 "plugins" -- ``encoding=win1252`` and friends -- none of which
        matched anything. The load order silently had no effect *and* suppressed
        the "no order given" warning, so the run merged alphabetically while
        reporting that it had not.
        """
        path = tmp_path / "openmw.cfg"
        path.write_text("encoding=win1252\nfallback=x,1\ncontent=Real.esp\n", encoding="utf-8")
        assert read_order(path) == ["real.esp"]

    def test_morrowind_ini_game_files(self, tmp_path: Path) -> None:
        """The other config people have."""
        path = tmp_path / "Morrowind.ini"
        path.write_text(
            "[Game Files]\nGameFile0=Morrowind.esm\nGameFile1=Mod.esp\n", encoding="utf-8"
        )
        assert read_order(path) == ["morrowind.esm", "mod.esp"]

    @pytest.mark.parametrize("suffix", [".esm", ".esp", ".omwaddon", ".omwgame"])
    def test_every_plugin_extension_is_recognised(self, tmp_path: Path, suffix: str) -> None:
        """OpenMW's own names for the format count too."""
        path = tmp_path / "order.txt"
        path.write_text(f"Thing{suffix}\n", encoding="utf-8")
        assert read_order(path) == [f"thing{suffix}"]

    def test_a_file_with_no_plugins_is_refused(self, tmp_path: Path) -> None:
        """Refusing beats proceeding: an empty order silently means alphabetical."""
        path = tmp_path / "openmw.cfg"
        path.write_text("encoding=win1252\nfallback=x,1\n", encoding="utf-8")
        with pytest.raises(SystemExit, match="no plugin entries"):
            read_order(path)


class TestApplyOrder:
    """Sorting found plugins by a declared order."""

    def test_the_declared_order_wins(self) -> None:
        """Alphabetical order is discarded in favour of the load order."""
        assert apply_order(["A", "B"], ["b.esp", "a.esp"]) == ["B", "A"]

    def test_unlisted_plugins_go_last_and_are_kept(self) -> None:
        """An incomplete load order narrows the guesswork; it does not drop data."""
        assert apply_order(["A", "B", "Z"], ["z.esp"]) == ["Z", "A", "B"]

    def test_matching_ignores_case(self) -> None:
        """Windows file names and mlox output disagree on case constantly."""
        assert apply_order(["Alpha"], ["ALPHA.ESP"]) == ["Alpha"]

    def test_an_empty_order_leaves_the_input_order(self) -> None:
        """Nothing declared means nothing reordered."""
        assert apply_order(["B", "A"], []) == ["A", "B"]


class TestMentionsLandscape:
    """The pre-scan that skips plugins holding no terrain."""

    def test_a_landscape_record_is_found(self, tmp_path: Path) -> None:
        """The common case."""
        path = tmp_path / "a.json"
        path.write_text('[{"type": "Landscape", "grid": [0, 0]}]', encoding="utf-8")
        assert mentions_landscape(path)

    def test_a_land_texture_is_found(self, tmp_path: Path) -> None:
        """``LandscapeTexture`` contains the marker, so one check covers both."""
        path = tmp_path / "a.json"
        path.write_text('[{"type": "LandscapeTexture", "id": "x"}]', encoding="utf-8")
        assert mentions_landscape(path)

    def test_a_plugin_without_terrain_is_skipped(self, tmp_path: Path) -> None:
        """The whole point: do not parse a hundred megabytes to find nothing."""
        path = tmp_path / "a.json"
        path.write_text('[{"type": "Static", "id": "rock"}]', encoding="utf-8")
        assert not mentions_landscape(path)

    def test_the_marker_is_found_across_a_chunk_boundary(self, tmp_path: Path) -> None:
        """A word split between two reads must still be found.

        This is the failure the overlap exists to prevent, and it would show up
        only on files of particular sizes -- so it is pinned here rather than
        left to chance.
        """
        path = tmp_path / "a.json"
        filler = "x" * ((1 << 20) - 4)
        path.write_text(f"{filler}Landscape rest", encoding="utf-8")
        assert mentions_landscape(path)

    def test_a_missing_file_is_not_an_error(self, tmp_path: Path) -> None:
        """An unreadable file is reported by the caller, not raised here."""
        assert not mentions_landscape(tmp_path / "gone.json")


class TestReadJsonRecords:
    """Reading an already-converted plugin."""

    def test_a_record_list_is_returned(self, tmp_path: Path) -> None:
        """The normal case."""
        path = tmp_path / "a.json"
        path.write_text('[{"type": "Static"}]', encoding="utf-8")
        assert read_json_records(path) == [{"type": "Static"}]

    @pytest.mark.parametrize("content", ["not json", '{"type": "Static"}', ""])
    def test_unusable_content_yields_nothing(self, tmp_path: Path, content: str) -> None:
        """Malformed, or valid JSON that is not a record list."""
        path = tmp_path / "a.json"
        path.write_text(content, encoding="utf-8")
        assert read_json_records(path) == []

    def test_a_missing_file_yields_nothing(self, tmp_path: Path) -> None:
        """One unreadable plugin must not stop a survey."""
        assert read_json_records(tmp_path / "gone.json") == []
