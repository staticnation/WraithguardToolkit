"""Regression tests for ``CellPreviewMixin._load_order_plugins``.

The loader once consumed :func:`wraithguard_toolkit.plugin_paths` -- which
returns a ``dict`` of name -> path -- as if it were a list, by zipping the load
order against it. ``zip(order, some_dict)`` walks the dict's *keys*, so every
"path" was really a bare plugin name: ``Path(name).read_bytes()`` then failed
with "No such file", and because dict order is not load order the skip messages
were even mis-paired. These tests pin the dict-lookup contract.

The loader now reads through a :class:`~wraithguard.scene.PluginParseCache`, so a
stub cache stands in for the parse here -- the loop logic (dict lookup, suffix
skip, error skip) is what these check, not the parsing itself.

The method touches no widgets, so it runs unbound -- but it lives in a module
that imports Tk at import time, so (like ``test_gui_smoke``) the whole file skips
when Tk is missing and runs under ``xvfb`` in CI.
"""

from __future__ import annotations

from pathlib import Path

import pytest

pytest.importorskip("tkinter", reason="Tk is not installed")

from wraithguard.esp.io import EspError
from wraithguard.gui import cellpreview
from wraithguard.gui.cellpreview import CellPreviewMixin
from wraithguard.scene import LoadedPlugin


class _FakeCache:
    """A stand-in parse cache: reads the file, uses its bytes as the one record.

    Raises the way the real cache does -- ``OSError`` for a missing file, an
    ``EspError`` for bytes it "cannot parse" -- so the loader's skip paths are
    exercised without a real plugin.
    """

    def __init__(self) -> None:
        self.loaded: list[bytes] = []

    def load(self, name: str, path: str) -> LoadedPlugin:
        data = Path(path).read_bytes()  # OSError if the file is missing
        if data == b"garbage":
            raise EspError("read past end")
        self.loaded.append(data)
        return LoadedPlugin(name=name, masters=[], records=[data.decode()])


def _host(cache: _FakeCache) -> CellPreviewMixin:
    """A mixin instance carrying the stub cache, with no widgets built."""
    host = CellPreviewMixin.__new__(CellPreviewMixin)
    host._plugin_cache = cache  # type: ignore[assignment]
    return host


def _install_fakes(monkeypatch: pytest.MonkeyPatch, resolved: dict[str, str]) -> None:
    """Point the loader's module-level dependencies at fakes."""
    monkeypatch.setattr(cellpreview.core, "plugin_paths", lambda order, index: resolved)
    monkeypatch.setattr(cellpreview, "PluginFileIndex", lambda dirs: object())


def test_it_resolves_each_name_through_the_dict(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    """A plugin present in the map is read from its resolved path, not its name."""
    a = tmp_path / "A.esp"
    a.write_bytes(b"alpha")
    b = tmp_path / "B.esp"
    b.write_bytes(b"beta")
    _install_fakes(monkeypatch, {"A.esp": str(a), "B.esp": str(b)})

    loaded = _host(_FakeCache())._load_order_plugins(["A.esp", "B.esp"], [str(tmp_path)])

    assert [p.name for p in loaded] == ["A.esp", "B.esp"]
    assert [p.records for p in loaded] == [["alpha"], ["beta"]]


def test_a_name_the_index_cannot_find_is_skipped(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    """A plugin missing from the map is dropped, not read from its bare name."""
    a = tmp_path / "A.esp"
    a.write_bytes(b"alpha")
    _install_fakes(monkeypatch, {"A.esp": str(a)})  # "Missing.esp" absent

    loaded = _host(_FakeCache())._load_order_plugins(["Missing.esp", "A.esp"], [str(tmp_path)])

    assert [p.name for p in loaded] == ["A.esp"]


def test_an_unreadable_plugin_is_skipped_not_fatal(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    """One corrupt plugin (a missing file) must not sink the rest of the order."""
    good = tmp_path / "Good.esp"
    good.write_bytes(b"ok")
    _install_fakes(monkeypatch, {"Bad.esp": str(tmp_path / "gone.esp"), "Good.esp": str(good)})

    loaded = _host(_FakeCache())._load_order_plugins(["Bad.esp", "Good.esp"], [str(tmp_path)])

    assert [p.name for p in loaded] == ["Good.esp"]


def test_non_content_files_are_skipped_unparsed(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    """``.omwscripts`` (a text script list) is not fed to the parse cache."""
    script = tmp_path / "Mod.omwscripts"
    script.write_text("Mod.lua\n")
    esp = tmp_path / "Mod.esp"
    esp.write_bytes(b"esp")
    _install_fakes(monkeypatch, {"Mod.omwscripts": str(script), "Mod.esp": str(esp)})

    cache = _FakeCache()
    loaded = _host(cache)._load_order_plugins(["Mod.omwscripts", "Mod.esp"], [str(tmp_path)])

    assert [p.name for p in loaded] == ["Mod.esp"]
    assert cache.loaded == [b"esp"]  # the script list was never parsed


def test_an_esp_error_is_caught_not_fatal(monkeypatch: pytest.MonkeyPatch, tmp_path: Path) -> None:
    """A content file the parser rejects is skipped, not fatal.

    ``EspError`` is not a ``ValueError``; the loop once let it escape and abort
    the whole preview.
    """
    bad = tmp_path / "Bad.esp"
    bad.write_bytes(b"garbage")
    good = tmp_path / "Good.esp"
    good.write_bytes(b"ok")
    _install_fakes(monkeypatch, {"Bad.esp": str(bad), "Good.esp": str(good)})

    loaded = _host(_FakeCache())._load_order_plugins(["Bad.esp", "Good.esp"], [str(tmp_path)])

    assert [p.name for p in loaded] == ["Good.esp"]
