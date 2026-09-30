"""The cell viewer's "show this record in the conflict viewer" (wraithguard.gui.record_link)."""

from __future__ import annotations

from typing import TYPE_CHECKING

import pytest

from wraithguard.gui.record_link import find_conflict_row, parse_request, record_type_name

if TYPE_CHECKING:
    from pathlib import Path


def test_tags_name_the_conflict_viewer_types() -> None:
    """Four-byte tags become the type names the conflict rows carry."""
    assert record_type_name("NPC_") == "Npc"
    assert record_type_name("MISC") == "MiscItem"
    assert record_type_name("CELL") == "Cell"
    assert record_type_name("ZZZZ") == ""


def test_a_request_is_a_tag_and_an_id() -> None:
    """The viewer's body is checked before anything acts on it."""
    assert parse_request(b'{"tag": "NPC_", "id": "caius cosades"}') == ("NPC_", "caius cosades")
    assert parse_request(b'{"id": "Balmora"}') == ("", "Balmora")
    for bad in (b"x", b"[]", b'{"tag": "npc", "id": "a"}', b'{"tag": "NPC_"}', b'{"id": "a\\nb"}'):
        with pytest.raises(ValueError):
            parse_request(bad)


def test_the_row_is_found_by_type_and_id_else_by_id() -> None:
    """Type and id without case first; the id alone when the viewer's type was a guess."""
    rows = [
        {"type": "Static", "id": "key_caius"},
        {"type": "Npc", "id": "Caius Cosades"},
        {"type": "MiscItem", "id": "key_caius"},
    ]
    assert find_conflict_row(rows, "NPC_", "caius cosades") == 1
    assert find_conflict_row(rows, "MISC", "KEY_CAIUS") == 2
    assert find_conflict_row(rows, "WEAP", "key_caius") == 0
    assert find_conflict_row(rows, "", "key_caius") == 0
    assert find_conflict_row(rows, "NPC_", "nobody") is None


def test_a_conflict_row_is_shown_where_it_stands() -> None:
    """Placeable records and cells go to Cell Preview; spells and topics do not."""
    from wraithguard.gui.record_link import cell_preview_spec, record_tag

    assert record_tag("Npc") == "NPC_"
    assert record_tag("nonsense") == ""
    assert cell_preview_spec("Npc", "caius cosades") == "find:NPC_:caius cosades"
    assert cell_preview_spec("Cell", "(-3, -2)") == "find:CELL:(-3, -2)"
    assert cell_preview_spec("Spell", "fireball") == ""
    assert cell_preview_spec("Static", "") == ""


def test_a_plugin_is_shown_by_its_cells() -> None:
    """A plugin goes to Cell Preview as ``plugin:<name>``; a broken name goes nowhere."""
    from wraithguard.gui.record_link import plugin_spec

    assert plugin_spec("My Mod.esp") == "plugin:My Mod.esp"
    assert plugin_spec("  ") == ""
    assert plugin_spec("a\nb.esp") == ""


def test_a_patch_is_previewed_loaded_last(tmp_path: Path) -> None:
    """The patch's folder is the last data path and the patch the last content line."""
    from wraithguard.gui.record_link import with_extra_plugins

    patch = tmp_path / "out" / "Patch.esp"
    lines = [
        'data="/a"',
        'data="/b"',
        "content=Morrowind.esm",
        "content=patch.esp",
        "content=Mod.esp",
    ]
    got = with_extra_plugins(lines, [patch])
    assert got[2] == f'data="{patch.resolve().parent}"'
    assert [ln for ln in got if ln.startswith("content=")] == [
        "content=Morrowind.esm",
        "content=Mod.esp",
        "content=Patch.esp",
    ]
    assert with_extra_plugins(lines, []) == lines
