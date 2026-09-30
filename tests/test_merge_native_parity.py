"""The native merge (merge_to_master, vendored) against the Python port it replaced.

:func:`wraithguard.merge.merge_plugins` and :func:`~wraithguard.merge.merge_load_order`
run greatness7's own merge through ``wraithguard_native``; the Python port stays as
:func:`~wraithguard.merge.api.merge_plugins_reference` and
:func:`~wraithguard.merge.api.merge_load_order_reference`. Both must merge merge_to_master's
fixtures to the same content, compared structurally as ``test_merge_golden`` compares
against ``Expect.esm``. (The golden test itself now runs the native merge.)

The fixtures live in the upstream checkout beside the repo; without it these skip.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

import pytest

from tests.test_merge_golden import _ASSETS, _FIXTURES, _assert_equal, _fixture_runnable
from wraithguard.esp.plugin import read_header
from wraithguard.merge import MergeOptions, merge_load_order, merge_plugins
from wraithguard.merge.api import merge_load_order_reference, merge_plugins_reference

if TYPE_CHECKING:
    from pathlib import Path


def _base(name: str) -> Path:
    base = _ASSETS / name
    if not base.exists():
        pytest.skip(f"fixture {name} not available")
    if not _fixture_runnable(base):
        pytest.skip(f"fixture {name} needs masters not bundled with it")
    return base


@pytest.mark.parametrize("name", sorted(_FIXTURES))
def test_merge_plugins_matches_python(name: str) -> None:
    """Plugin into master: the native merge and the Python port agree."""
    base = _base(name)
    options = _FIXTURES[name]
    got = merge_plugins(base / "Plugin.esp", base / "Master.esm", options)
    want = merge_plugins_reference(base / "Plugin.esp", base / "Master.esm", options)
    _assert_equal(got, want)


@pytest.mark.parametrize("name", sorted(_FIXTURES))
def test_merge_load_order_matches_python(name: str) -> None:
    """The plugin's masters and the plugin, as a load order: the two agree."""
    base = _base(name)
    masters = read_header((base / "Plugin.esp").read_bytes()).masters
    order = [*(base / m for m, _ in masters), base / "Plugin.esp"]
    options = _FIXTURES[name]
    _assert_equal(merge_load_order(order, options), merge_load_order_reference(order, options))


def test_missing_file_is_oserror(tmp_path: Path) -> None:
    """A plugin that is not there is an OSError, as reading it in Python would be."""
    with pytest.raises(OSError, match=r"nope\.esp"):
        merge_plugins(tmp_path / "nope.esp", tmp_path / "Master.esm", MergeOptions())


def test_target_not_last_master_is_valueerror() -> None:
    """Merging into a master that is not the plugin's last is a ValueError, not a crash."""
    base = _base("info_preserve_gaps")
    first = read_header((base / "Plugin.esp").read_bytes()).masters[0][0]
    if first.lower() == "master.esm":
        pytest.skip("the fixture's plugin has one master")
    with pytest.raises(ValueError, match="last master"):
        merge_plugins(base / "Plugin.esp", base / first, MergeOptions())
