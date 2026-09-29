"""The cell viewer's setup cfg: what Export would write, standing on its own."""

from __future__ import annotations

from typing import TYPE_CHECKING

import wraithguard_toolkit as core

if TYPE_CHECKING:
    from pathlib import Path


def _plan(lines: list[str]) -> dict:
    """A minimal plan over ``lines`` with its content=/data= positions."""
    content = [i for i, line in enumerate(lines) if line.startswith("content=")]
    data = [i for i, line in enumerate(lines) if line.startswith("data=")]
    return {
        "lines": lines,
        "final_order": [lines[i][len("content=") :] for i in content],
        "content_positions": content,
        "data_positions": data,
        "data_result": [(lines[i], False, None) for i in data],
        "new_groundcover": ["Grass.esp"],
    }


def test_render_uses_the_panel_order_and_declares_groundcover() -> None:
    """Rows as the panels hold them, and the plan's new groundcover line."""
    lines = [
        'data="C:/Games/Data Files"',
        'data="mods/A"',
        "content=Morrowind.esm",
        "content=A.esp",
    ]
    out = core.render_cfg_lines(
        _plan(lines),
        final_order=["A.esp", "Morrowind.esm"],
        data_order=['data="mods/A"', 'data="C:/Games/Data Files"'],
    )
    assert out == [
        'data="mods/A"',
        'data="C:/Games/Data Files"',
        "content=A.esp",
        "content=Morrowind.esm",
        "groundcover=Grass.esp",
    ]


def test_viewer_cfg_makes_relative_data_paths_absolute(tmp_path: Path) -> None:
    """The copy lives elsewhere, so relative data= paths are anchored to the real cfg."""
    absolute = (tmp_path / "abs" / "B").resolve()  # absolute on every platform
    out = core.viewer_setup_cfg(['data="mods/A"', "content=A.esp", f'data="{absolute}"'], tmp_path)
    assert out[0] == f'data="{(tmp_path / "mods/A").resolve()}"'
    assert out[1:] == ["content=A.esp", f'data="{absolute}"']


def test_export_still_splices_the_same_way() -> None:
    """The splice Export writes with is unchanged by the split."""
    lines = ["a", "content=X.esp", "b"]
    assert core.splice_cfg_lines(lines, [([1], ["content=Y.esp", "content=X.esp"])]) == [
        "a",
        "content=Y.esp",
        "content=X.esp",
        "b",
    ]
