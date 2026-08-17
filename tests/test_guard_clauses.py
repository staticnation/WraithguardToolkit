"""Error guards and edge branches left uncovered across several near-complete files.

Each is a one-liner -- an unsupported-format raise, a skipped non-matching line,
an empty-value guard, a colour ramp, an empty-band fallback -- that a module's
main tests never happened to reach. Grouped here because none belongs to a larger
feature; together they close the last statement in a handful of files.
"""

from __future__ import annotations

import pytest

from wraithguard.configurator.cfglines import detect_data_quoting, normalize_data_path
from wraithguard.images.bitmap import BitmapError, _expand_row
from wraithguard.viz.palette import coverage_band_index, terrain_tint


def test_expand_row_rejects_an_unsupported_depth() -> None:
    """A bit depth the BMP format does not define is refused, not guessed at."""
    with pytest.raises(BitmapError, match="unsupported bitmap depth"):
        _expand_row(b"\x00", width=1, depth=7, palette=[], masks=None)


def test_detect_data_quoting_skips_non_data_lines() -> None:
    """A line that is not a ``data=`` line is ignored, not miscounted."""
    assert detect_data_quoting(["# a comment", 'data="C:/mods/x"']) is True


def test_normalize_data_path_of_empty_is_empty() -> None:
    """An empty value normalises to an empty comparison key, not an error."""
    assert normalize_data_path("") == ""


def test_terrain_tint_returns_a_hex_colour() -> None:
    """A position along the terrain ramp maps to a ``#rrggbb`` string."""
    tint = terrain_tint(0.5)
    assert tint.startswith("#")
    assert len(tint) == 7


def test_coverage_band_index_handles_an_empty_band_set() -> None:
    """With no bands (worst < 1) the index falls back to 0 rather than indexing."""
    assert coverage_band_index(1, 0) == 0
