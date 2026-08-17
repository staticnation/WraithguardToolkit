"""Reading a subset file in the minimal-TOML form (``subset=[...] data=[...]``).

The plain-text subset form is exercised elsewhere; the ``.toml`` branch of
``extract_subset_from_subset_file`` -- parse the arrays, classify each entry --
was not.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from wraithguard_toolkit import extract_subset_from_subset_file

if TYPE_CHECKING:
    from pathlib import Path


def test_a_toml_subset_file_yields_plugins_and_data_paths(tmp_path: Path) -> None:
    """The TOML form's ``subset`` becomes plugins and ``data`` becomes inserts."""
    path = tmp_path / "subset.toml"
    path.write_text(
        'subset = ["GoHome.esp", "go-home.omwscripts"]\ndata = ["mods/SomeMod"]\n',
        encoding="utf-8",
    )
    plugins, data_inserts = extract_subset_from_subset_file(path)
    assert "GoHome.esp" in plugins
    assert any(d["value"] == "mods/SomeMod" for d in data_inserts)
