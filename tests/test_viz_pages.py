"""Tests for the cell map, its client assets, housekeeping and the new ramps.

These cover the pieces added when the cell map was lifted out of
``wraithguard_toolkit.py``. The emphasis is on the properties a screenshot cannot
show:

* an untrusted plugin name cannot break out of an attribute or a script;
* the mod filter token cannot partially match a longer filename;
* a corrupt grid coordinate is dropped and *reported*, not silently plotted;
* the legend is generated from the same ramp the map draws with, so the two
  cannot drift;
* housekeeping only ever deletes files this tool wrote, and deletes nothing at
  all when it is off.
"""

from __future__ import annotations

import re
from datetime import datetime
from itertools import pairwise
from pathlib import Path
from typing import Any

import pytest

from wraithguard.viz.cellmap import (
    CELL_GRID_LIMIT,
    _anchor,
    _focus_options,
    _in_bounds,
    _modattr,
    generate_cell_map_html,
)
from wraithguard.viz.cellmap_js import CELLMAP_CSS, CELLMAP_JS
from wraithguard.viz.housekeeping import (
    DEFAULT_KEEP,
    GENERATED_STEMS,
    describe,
    find_generated,
    prune_generated,
    sidecar_folder,
)
from wraithguard.viz.html import table
from wraithguard.viz.palette import (
    _COVERAGE_STOPS,
    COVERAGE_MAX_BANDS,
    NEUTRAL,
    _ramp,
    coverage_band_index,
    coverage_bands,
    coverage_heat,
    coverage_legend_stops,
    divergence,
    severity_band_table,
    severity_banded,
)

HEX = re.compile(r"^#[0-9a-f]{6}$")


def _luminance(color: str) -> float:
    """Perceived brightness of a ``#rrggbb`` color.

    Args:
        color: A six-digit hex color.

    Returns:
        Rec. 601 luma, 0-1.
    """
    red, green, blue = (int(color[i : i + 2], 16) / 255 for i in (1, 3, 5))
    return 0.299 * red + 0.587 * green + 0.114 * blue


def coverage(
    exterior: dict[tuple[int, int], list[str]] | None = None,
    interior: dict[str, list[str]] | None = None,
    subset: set[str] | None = None,
    scanned: int = 3,
) -> dict[str, Any]:
    """Build a ``build_cell_coverage``-shaped payload.

    Args:
        exterior: Exterior grid coordinate to the mods touching it.
        interior: Interior cell name to the mods touching it.
        subset: Lower-cased filenames of the user's own mods.
        scanned: How many plugins were scanned.

    Returns:
        The mapping ``generate_cell_map_html`` expects.
    """
    return {
        "exterior": exterior if exterior is not None else {(0, 0): ["a.esp"]},
        "interior": interior or {},
        "subset_lower": subset or set(),
        "scanned": scanned,
    }


#: A fixed stamp, so the header assertion is exact. Naive on purpose: the page
#: stamps the local wall clock the user reads, which is what production passes.
FIXED = datetime(2026, 7, 27, 9, 30, 15)  # noqa: DTZ001 - local clock, matching the caller


class TestCellMapPage:
    """The assembled cell map document."""

    def test_page_is_self_contained(self) -> None:
        """No CDN or external asset: the tool runs offline and ships frozen."""
        html = generate_cell_map_html(coverage())
        assert "<!DOCTYPE html>" in html
        assert "http://" not in html.replace("http://www.w3.org/2000/svg", "")
        assert "https://" not in html
        assert "<script src" not in html
        assert "<link" not in html

    def test_generated_stamp_is_rendered(self) -> None:
        """A map found on disk must be tellable from a fresh one."""
        html = generate_cell_map_html(coverage(), generated_at=FIXED)
        assert "Generated 2026-07-27 09:30:15" in html

    def test_stamp_defaults_to_now(self) -> None:
        """Callers that do not care still get a stamp."""
        html = generate_cell_map_html(coverage())
        assert re.search(r"Generated \d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}", html)

    def test_counts_appear_in_the_tab_labels(self) -> None:
        """The tabs report how much is behind them before you click."""
        html = generate_cell_map_html(
            coverage(
                exterior={(0, 0): ["a.esp"], (1, 0): ["a.esp", "b.esp"]},
                interior={"Balmora, Guild": ["a.esp"]},
            )
        )
        assert "Exterior list (2)" in html
        assert "Interior list (1)" in html

    def test_conflicted_cell_counts(self) -> None:
        """ "Touched by 2+ mods" counts cells, not mod appearances."""
        html = generate_cell_map_html(
            coverage(
                exterior={
                    (0, 0): ["a.esp"],
                    (1, 0): ["a.esp", "b.esp"],
                    (2, 0): ["a.esp", "b.esp"],
                },
                interior={"Hall": ["a.esp", "b.esp"]},
            )
        )
        assert "Exterior: 3 cell(s)\n touched (2 by 2+ mods)" in html
        assert "Interior: 1 cell(s) touched\n (1 by 2+ mods)" in html

    def test_lists_have_scrolling_wrappers(self) -> None:
        """The user asked for the same scrolling the conflict map has."""
        html = generate_cell_map_html(coverage())
        assert html.count('<div class="listwrap">') == 2
        assert '<div class="mapwrap">' in html

    def test_no_cross_link_button(self) -> None:
        """The conflict button is gone: generating it here is what broke the map."""
        html = generate_cell_map_html(coverage())
        assert "Conflicts &raquo;" not in html
        assert "conflict_explorer" not in html

    def test_empty_coverage_renders_a_note_not_a_broken_svg(self) -> None:
        """Zero touched cells is a normal state, not an error."""
        html = generate_cell_map_html(coverage(exterior={}))
        assert "No exterior cells touched." in html
        assert "<svg" not in html

    def test_north_is_up(self) -> None:
        """Higher grid Y must draw above lower, matching every Morrowind map."""
        html = generate_cell_map_html(coverage(exterior={(0, 0): ["a.esp"], (0, 5): ["b.esp"]}))
        ys = [int(m) for m in re.findall(r'<rect x="0" y="(\d+)"', html)]
        assert ys == [65, 0]  # (0,0) below (0,5)

    def test_custom_cells_are_outlined(self) -> None:
        """The user's own mods are the reason they opened the map."""
        html = generate_cell_map_html(
            coverage(exterior={(0, 0): ["mine.esp"]}, subset={"mine.esp"})
        )
        assert 'stroke="#ffd24a"' in html
        assert 'class="cust"' in html

    def test_untouched_by_subset_is_not_outlined(self) -> None:
        """The highlight has to mean something, so it must be selective."""
        html = generate_cell_map_html(
            coverage(exterior={(0, 0): ["other.esp"]}, subset={"mine.esp"})
        )
        assert 'stroke="#ffd24a"' not in html


class TestOutOfRangeCells:
    """A corrupt coordinate must not flatten the map."""

    def test_out_of_range_cell_is_dropped(self) -> None:
        """One garbage cell would otherwise stretch the SVG to millions of px."""
        html = generate_cell_map_html(
            coverage(exterior={(0, 0): ["a.esp"], (999999, 4): ["bad.esp"]})
        )
        assert "1 cell(s) had out-of-range coordinates and were dropped." in html
        assert "Exterior: 1 cell(s)" in html

    def test_dropping_is_silent_when_nothing_is_dropped(self) -> None:
        """No scary note on a clean load order."""
        html = generate_cell_map_html(coverage())
        assert "out-of-range" not in html

    def test_dropped_mods_still_appear_in_the_focus_list(self) -> None:
        """The mod exists even if one of its cells could not be plotted."""
        html = generate_cell_map_html(
            coverage(exterior={(0, 0): ["a.esp"], (999999, 4): ["bad.esp"]})
        )
        assert 'value="bad.esp"' in html

    @pytest.mark.parametrize(
        ("key", "ok"),
        [
            ((0, 0), True),
            ((CELL_GRID_LIMIT, CELL_GRID_LIMIT), True),
            ((-CELL_GRID_LIMIT, 0), True),
            ((CELL_GRID_LIMIT + 1, 0), False),
            ((0, -CELL_GRID_LIMIT - 1), False),
        ],
    )
    def test_bounds_are_inclusive(self, key: tuple[int, int], ok: bool) -> None:
        """The limit itself is a real coordinate.

        Args:
            key: The grid coordinate.
            ok: Whether it should be accepted.
        """
        assert _in_bounds(key) is ok


class TestUntrustedNames:
    """Plugin filenames come from disk and are not trusted."""

    def test_script_tag_in_a_plugin_name_is_escaped(self) -> None:
        """The classic break-out: a file literally named ``</script>``."""
        nasty = '</script><script>alert(1)</script>x".esp'
        html = generate_cell_map_html(coverage(exterior={(0, 0): [nasty]}))
        assert "<script>alert(1)</script>" not in html
        assert "&lt;/script&gt;" in html
        assert html.count("<script>") == 1  # only the page's own

    def test_quote_in_a_name_cannot_escape_an_attribute(self) -> None:
        """``data-t`` and ``data-m`` both carry the name."""
        html = generate_cell_map_html(coverage(exterior={(0, 0): ['ev"il.esp']}))
        assert '"ev"il' not in html
        assert "&quot;il.esp" in html

    def test_ampersand_is_escaped_once(self) -> None:
        """Order matters: escaping ``&`` after ``<`` would double-encode."""
        html = generate_cell_map_html(coverage(exterior={(0, 0): ["a&b.esp"]}))
        assert "a&amp;b.esp" in html
        assert "&amp;amp;" not in html


class TestModFilterToken:
    """The focus filter matches on a delimited token, not a substring."""

    def test_token_is_pipe_delimited_and_lowercased(self) -> None:
        """Args and returns are exact by construction here."""
        assert _modattr(["A.esp", "B.ESP"]) == "|a.esp|b.esp|"

    def test_short_name_does_not_match_inside_a_longer_one(self) -> None:
        """``|tr.esp|`` must not hit ``TR_Mainland.esp``."""
        token = _modattr(["TR_Mainland.esp"])
        assert "|tr.esp|" not in token
        assert "|tr_mainland.esp|" in token

    def test_empty_list_still_delimits(self) -> None:
        """An empty token must not match everything."""
        assert _modattr([]) == "||"


class TestFocusOptions:
    """The dropdown over a 989-entry load order."""

    def test_user_mods_sort_first_and_are_starred(self) -> None:
        """Their own mods must be reachable without scrolling."""
        options = _focus_options({(0, 0): ["zzz.esp", "mine.esp"]}, {}, {"mine.esp"})
        assert options.index("mine.esp") < options.index("zzz.esp")
        assert "mine.esp ★" in options
        assert "zzz.esp ★" not in options

    def test_a_mod_is_listed_once_however_many_cells_it_touches(self) -> None:
        """Deduplication is by lower-cased filename."""
        options = _focus_options({(0, 0): ["A.esp"], (1, 0): ["a.esp"]}, {}, set())
        assert options.count("<option") == 1

    def test_interior_only_mods_are_included(self) -> None:
        """An interior-only mod is still something to focus on."""
        options = _focus_options({}, {"Hall": ["int.esp"]}, set())
        assert 'value="int.esp"' in options

    def test_display_name_keeps_original_case(self) -> None:
        """Filtering is case-insensitive; the label should not be."""
        options = _focus_options({(0, 0): ["MyMod.ESP"]}, {}, set())
        assert ">MyMod.ESP<" in options
        assert 'value="mymod.esp"' in options


class TestAnchor:
    """Row ids have to be valid CSS selectors."""

    @pytest.mark.parametrize(
        ("gx", "gy", "expected"),
        [(0, 0, "e_0_0"), (3, 7, "e_3_7"), (-2, 5, "e_m2_5"), (-2, -5, "e_m2_m5")],
    )
    def test_minus_becomes_m(self, gx: int, gy: int, expected: str) -> None:
        """A leading ``-`` in an id breaks ``getElementById`` selectors.

        Args:
            gx: Grid X.
            gy: Grid Y.
            expected: The id.
        """
        assert _anchor(gx, gy) == expected

    def test_anchor_matches_the_onclick_target(self) -> None:
        """A jump target that does not exist is a dead click."""
        html = generate_cell_map_html(coverage(exterior={(-3, -4): ["a.esp"]}))
        assert "jump('e_m3_m4')" in html
        assert 'id="e_m3_m4"' in html


class TestClientAssets:
    """The CSS and JS are plain constants, not templates."""

    def test_no_interpolation_braces_to_escape(self) -> None:
        """These left the f-string precisely so braces need no doubling."""
        assert "{{" not in CELLMAP_CSS
        assert "}}" not in CELLMAP_CSS

    def test_scroll_panes_are_resizable(self) -> None:
        """The user asked to be able to drag the panes taller."""
        assert "resize:vertical" in CELLMAP_CSS
        assert ".listwrap{overflow:auto" in CELLMAP_CSS
        assert ".mapwrap{overflow:auto" in CELLMAP_CSS

    def test_every_handler_the_page_calls_is_defined(self) -> None:
        """A missing handler is a silent console error nobody sees."""
        html = generate_cell_map_html(coverage(exterior={(0, 0): ["a.esp"]}))
        for handler in ("show", "jump", "ff", "setFocus"):
            assert f"{handler}(" in html
            assert f"function {handler}(" in CELLMAP_JS

    def test_tooltip_is_delegated_not_native(self) -> None:
        """Native SVG <title> has a ~1s delay and cannot be styled.

        So the tip travels as a ``data-t`` attribute and one delegated listener
        on the container renders it -- not thousands of per-rect handlers, and
        not a ``<title>`` child.
        """
        assert "mouseover" in CELLMAP_JS
        html = generate_cell_map_html(coverage(exterior={(0, 0): ["a.esp"]}))
        svg = html.split("<svg")[1].split("</svg>")[0]
        assert "<title" not in svg
        assert "data-t=" in svg
        assert "onmouseover" not in svg


class TestCoverageRamp:
    """Coverage is not badness, so it must not read like the conflict map."""

    def test_zero_is_neutral(self) -> None:
        """An untouched cell is not the bottom of the ramp; it is off it."""
        assert coverage_heat(0, 5) == NEUTRAL

    def test_single_mod_map_uses_the_floor(self) -> None:
        """``worst == 1`` would divide by zero on a naive normalisation."""
        assert HEX.match(coverage_heat(1, 1))
        assert coverage_heat(1, 1) == coverage_heat(1, 1)

    def test_all_outputs_are_hex(self) -> None:
        """Anything else is an invalid ``fill`` the browser silently ignores."""
        assert all(HEX.match(coverage_heat(n, 12)) for n in range(1, 13))

    def test_ramp_brightens_monotonically(self) -> None:
        """Luminance is the invariant, not any single channel.

        The ramp rotates through hues (slate, blue, periwinkle, violet, amber),
        so red and "warmth" both dip early on -- the first segment desaturated
        slate to saturated blue goes *cooler*. What has to hold for the map to
        read as a scale is that each step is brighter than the last, which also
        keeps it ordered in greyscale and for a color-blind reader.
        """
        for worst in (6, 12, 30):
            lums = [_luminance(coverage_heat(n, worst)) for n in range(1, worst + 1)]
            assert lums == sorted(lums), worst
            assert lums[0] < lums[-1]

    def test_counts_above_worst_are_clamped(self) -> None:
        """A stale ``worst`` must not produce an out-of-range color."""
        assert coverage_heat(20, 5) == coverage_heat(5, 5)

    def test_endpoints_are_the_declared_stops(self) -> None:
        """The floor and ceiling are documented colors, not emergent ones."""
        assert coverage_heat(1, 8) == "#2e4a63"
        assert coverage_heat(8, 8) == "#d99e3d"

    def test_coverage_is_not_green_to_red(self) -> None:
        """It must not be mistaken for severity at a glance."""
        low = coverage_heat(1, 8)
        blue, green = int(low[5:7], 16), int(low[3:5], 16)
        assert blue > green  # cool floor, unlike severity's teal-green


class TestRampEdges:
    """The defensive ends of the shared interpolator."""

    def test_zero_scale_divergence_is_neutral(self) -> None:
        """A flat cell has no scale to normalise against; it is not "red"."""
        assert divergence(500, 0) == NEUTRAL
        assert divergence(0, 0) == NEUTRAL

    def test_position_past_the_last_stop_returns_the_last_color(self) -> None:
        """Callers clamp, but a ramp that fell through would return ``None``."""
        assert _ramp(1.5, _COVERAGE_STOPS) == _ramp(1.0, _COVERAGE_STOPS)

    def test_a_degenerate_stop_pair_does_not_divide_by_zero(self) -> None:
        """Two stops at the same position is a config error, not a crash."""
        stops = ((0.0, 0.1, 0.1, 0.1), (0.0, 0.9, 0.9, 0.9), (1.0, 0.5, 0.5, 0.5))
        assert HEX.match(_ramp(0.0, stops))


class TestCoverageBands:
    """Counts 1-5 individually, then every five.

    The distinctions that matter are crowded at the bottom: one, two and three
    mods in a cell are different situations, while 23 and 24 are not.
    """

    def test_low_counts_each_get_their_own_band(self) -> None:
        """This is the request: 1 through 5 must be individually readable."""
        assert coverage_bands(5) == [(1, 1), (2, 2), (3, 3), (4, 4), (5, 5)]

    def test_above_five_groups_by_five(self) -> None:
        """6-10, 11-15, and so on."""
        assert coverage_bands(20)[5:] == [(6, 10), (11, 15), (16, 20)]

    def test_top_band_stops_at_worst(self) -> None:
        """A band promising 16-20 on a map whose worst cell is 17 is a lie."""
        assert coverage_bands(17)[-1] == (16, 17)

    def test_bands_are_contiguous_and_cover_every_count(self) -> None:
        """A count falling between two bands would have no color at all."""
        for worst in (1, 4, 5, 6, 9, 23, 47):
            bands = coverage_bands(worst)
            assert bands[0][0] == 1
            for (_low, high), (next_low, _next_high) in pairwise(bands):
                assert high is not None
                assert next_low == high + 1
            assert all(0 <= coverage_band_index(n, worst) < len(bands) for n in range(1, worst + 1))

    def test_huge_worst_stops_at_the_band_ceiling(self) -> None:
        """Forty bands over seven color stops is a gradient again."""
        bands = coverage_bands(500)
        assert len(bands) == COVERAGE_MAX_BANDS
        assert bands[-1][1] is None

    def test_open_ended_band_is_only_used_when_needed(self) -> None:
        """An exact top band must not be reported as open-ended."""
        assert coverage_bands(30)[-1] == (26, 30)

    def test_nothing_touched_has_no_bands(self) -> None:
        """Zero is not a band; it is off the scale."""
        assert coverage_bands(0) == []

    def test_every_count_in_a_band_shares_its_color(self) -> None:
        """Banding is only banding if the band is one color."""
        assert coverage_heat(6, 30) == coverage_heat(10, 30)
        assert coverage_heat(11, 30) != coverage_heat(10, 30)

    def test_each_band_gets_a_distinct_color(self) -> None:
        """Two bands sharing a color is two bands the reader cannot tell apart."""
        for worst in (5, 12, 30):
            colors = {coverage_heat(low, worst) for low, _high in coverage_bands(worst)}
            assert len(colors) == len(coverage_bands(worst))


class TestCoverageLegend:
    """The legend is the map's key, one row per band."""

    def test_swatch_colors_match_the_map(self) -> None:
        """This is the whole reason it is generated rather than written."""
        for (label, color, _dark), (low, _high) in zip(
            coverage_legend_stops(9), coverage_bands(9)
        ):
            assert color == coverage_heat(low, 9)
            assert label.startswith(str(low))

    def test_one_row_per_band(self) -> None:
        """A sampled legend beside a banded map is a legend that lies."""
        assert len(coverage_legend_stops(23)) == len(coverage_bands(23))

    def test_single_counts_are_labelled_bare(self) -> None:
        """ "1-1" would be silly."""
        assert [row[0] for row in coverage_legend_stops(5)] == ["1", "2", "3", "4", "5"]

    def test_grouped_counts_are_labelled_as_ranges(self) -> None:
        """The reader has to know 6-10 share a color."""
        assert [row[0] for row in coverage_legend_stops(15)][-2:] == ["6-10", "11-15"]

    def test_open_ended_band_is_marked(self) -> None:
        """``+`` promises merged counts, so it must appear exactly when they are."""
        assert coverage_legend_stops(500)[-1][0].endswith("+")
        assert not coverage_legend_stops(30)[-1][0].endswith("+")

    def test_small_worst_collapses_without_error(self) -> None:
        """One-mod maps are common and must not raise."""
        assert [row[0] for row in coverage_legend_stops(1)] == ["1"]

    def test_worst_below_one_is_empty(self) -> None:
        """Nothing touched means nothing to explain."""
        assert coverage_legend_stops(0) == []

    def test_light_text_flag_tracks_luminance(self) -> None:
        """The label has to stay readable at both ends of the ramp."""
        rows = coverage_legend_stops(9)
        assert rows[0][2] is True  # dark slate
        assert rows[-1][2] is False  # bright amber

    def test_legend_appears_in_the_page(self) -> None:
        """Generated is no use if it is not wired in."""
        html = generate_cell_map_html(coverage(exterior={(0, 0): ["a.esp", "b.esp"]}))
        assert "Mods per cell:" in html
        assert coverage_heat(2, 2) in html


class TestSeverityBandTable:
    """The table handed to the conflict map's client-side redraw.

    The client used to re-implement the color ramp in JavaScript, which is how
    the focused and unfocused views could drift apart. It now looks a count up
    in this table instead, so the contract is the table's shape.
    """

    def test_one_row_per_band(self) -> None:
        """The client scans it linearly; a missing band is a missing color."""
        assert len(severity_band_table(23)) == len(coverage_bands(23))

    def test_rows_are_low_high_color(self) -> None:
        """The exact shape the page's lookup indexes by position."""
        low, high, color = severity_band_table(12)[0]
        assert (low, high) == (1, 1)
        assert color.startswith("#") and len(color) == 7

    def test_the_bands_cover_every_count_without_gaps(self) -> None:
        """A count falling through every band would render as neutral."""
        table = severity_band_table(40)
        for count in range(1, 41):
            assert any(
                count >= low and (high is None or count <= high) for low, high, _c in table
            ), f"{count} falls in no band"

    def test_the_top_band_is_open_ended_when_it_has_to_be(self) -> None:
        """So a huge outlier still lands somewhere rather than off the end."""
        assert severity_band_table(500)[-1][1] is None

    def test_the_colors_match_what_the_server_drew(self) -> None:
        """The whole point: the client cannot color a count differently.

        Compared against :func:`severity_banded`, which is what painted the
        unfocused map server-side.
        """
        for low, _high, color in severity_band_table(30):
            assert color == severity_banded(low, 30)

    def test_an_empty_map_has_no_bands(self) -> None:
        """Nothing to color, and the client falls back to neutral."""
        assert severity_band_table(0) == []


def touch(folder: Path, name: str) -> Path:
    """Create an empty file.

    Args:
        folder: Containing directory.
        name: Filename.

    Returns:
        The path written.
    """
    path = folder / name
    path.write_text("x", encoding="utf-8")
    return path


class TestFindGenerated:
    """Only this tool's own timestamped output is a candidate."""

    def test_groups_by_stem_newest_first(self, tmp_path: Path) -> None:
        """Sorting is by the timestamp in the name, not mtime.

        Args:
            tmp_path: Pytest temporary directory.
        """
        touch(tmp_path, "conflict_map_20260101_000000.html")
        touch(tmp_path, "conflict_map_20260301_000000.html")
        touch(tmp_path, "pathgrid_20260201_000000.html")
        found = find_generated(tmp_path)
        assert set(found) == {"conflict_map", "pathgrid"}
        assert [p.name for p in found["conflict_map"]] == [
            "conflict_map_20260301_000000.html",
            "conflict_map_20260101_000000.html",
        ]

    def test_untimestamped_file_is_not_a_candidate(self, tmp_path: Path) -> None:
        """``conflict_map.html`` is a file the user named via *Save*.

        Args:
            tmp_path: Pytest temporary directory.
        """
        touch(tmp_path, "conflict_map.html")
        assert find_generated(tmp_path) == {}

    def test_unknown_stem_is_not_a_candidate(self, tmp_path: Path) -> None:
        """Never a blanket ``*.html`` sweep.

        Args:
            tmp_path: Pytest temporary directory.
        """
        touch(tmp_path, "my_notes_20260101_000000.html")
        touch(tmp_path, "index.html")
        assert find_generated(tmp_path) == {}

    def test_non_html_is_ignored(self, tmp_path: Path) -> None:
        """A matching stem on a ``.js`` sidecar is still not a page.

        Args:
            tmp_path: Pytest temporary directory.
        """
        touch(tmp_path, "conflict_map_20260101_000000.js")
        assert find_generated(tmp_path) == {}

    def test_uppercase_extension_matches(self, tmp_path: Path) -> None:
        """Windows users get ``.HTML`` from some tools.

        Args:
            tmp_path: Pytest temporary directory.
        """
        touch(tmp_path, "pathgrid_20260101_000000.HTML")
        assert "pathgrid" in find_generated(tmp_path)

    def test_missing_folder_yields_nothing(self, tmp_path: Path) -> None:
        """A first run has no app directory yet; that is not an error.

        Args:
            tmp_path: Pytest temporary directory.
        """
        assert find_generated(tmp_path / "nope") == {}

    def test_every_declared_stem_is_recognised(self, tmp_path: Path) -> None:
        """A stem in the tuple that the matcher misses is dead config.

        Args:
            tmp_path: Pytest temporary directory.
        """
        for stem in GENERATED_STEMS:
            touch(tmp_path, f"{stem}_20260101_000000.html")
        assert set(find_generated(tmp_path)) == set(GENERATED_STEMS)


class TestPrune:
    """Keeping the newest few, and nothing else."""

    def test_keeps_the_newest_n(self, tmp_path: Path) -> None:
        """Default keeps enough for a before/after comparison.

        Args:
            tmp_path: Pytest temporary directory.
        """
        for day in range(1, 7):
            touch(tmp_path, f"conflict_map_202601{day:02d}_000000.html")
        removed = prune_generated(tmp_path, keep=2)
        assert len(removed) == 4
        survivors = sorted(p.name for p in tmp_path.iterdir())
        assert survivors == [
            "conflict_map_20260105_000000.html",
            "conflict_map_20260106_000000.html",
        ]

    def test_keep_is_per_kind(self, tmp_path: Path) -> None:
        """Two kinds must not compete for the same budget.

        Args:
            tmp_path: Pytest temporary directory.
        """
        for day in (1, 2, 3):
            touch(tmp_path, f"conflict_map_202601{day:02d}_000000.html")
            touch(tmp_path, f"pathgrid_202601{day:02d}_000000.html")
        prune_generated(tmp_path, keep=1)
        assert len(list(tmp_path.iterdir())) == 2

    def test_dry_run_removes_nothing(self, tmp_path: Path) -> None:
        """The report has to be trustworthy before the deletion is.

        Args:
            tmp_path: Pytest temporary directory.
        """
        for day in (1, 2, 3):
            touch(tmp_path, f"pathgrid_202601{day:02d}_000000.html")
        removed = prune_generated(tmp_path, keep=1, dry_run=True)
        assert len(removed) == 2
        assert len(list(tmp_path.iterdir())) == 3

    def test_sidecar_folder_goes_with_its_page(self, tmp_path: Path) -> None:
        """The data folder is useless without the page referencing it.

        Args:
            tmp_path: Pytest temporary directory.
        """
        page = touch(tmp_path, "conflict_map_20260101_000000.html")
        touch(tmp_path, "conflict_map_20260202_000000.html")
        data = sidecar_folder(page)
        (data / "cells").mkdir(parents=True)
        (data / "cells" / "0_0.js").write_text("x", encoding="utf-8")
        removed = prune_generated(tmp_path, keep=1)
        assert data in removed
        assert not data.exists()

    def test_surviving_sidecar_is_untouched(self, tmp_path: Path) -> None:
        """Deleting the kept page's data would break the page.

        Args:
            tmp_path: Pytest temporary directory.
        """
        touch(tmp_path, "conflict_map_20260101_000000.html")
        keeper = touch(tmp_path, "conflict_map_20260202_000000.html")
        data = sidecar_folder(keeper)
        data.mkdir()
        prune_generated(tmp_path, keep=1)
        assert data.is_dir()

    def test_nothing_to_prune_is_a_no_op(self, tmp_path: Path) -> None:
        """Fewer files than ``keep`` must not touch anything.

        Args:
            tmp_path: Pytest temporary directory.
        """
        touch(tmp_path, "pathgrid_20260101_000000.html")
        assert prune_generated(tmp_path, keep=DEFAULT_KEEP) == []

    def test_negative_keep_is_treated_as_zero(self, tmp_path: Path) -> None:
        """A negative slice index would keep the *newest* file by accident.

        Args:
            tmp_path: Pytest temporary directory.
        """
        touch(tmp_path, "pathgrid_20260101_000000.html")
        touch(tmp_path, "pathgrid_20260102_000000.html")
        assert len(prune_generated(tmp_path, keep=-3)) == 2
        assert list(tmp_path.iterdir()) == []

    def test_user_files_survive_an_aggressive_prune(self, tmp_path: Path) -> None:
        """keep=0 is still not licence to clear the folder.

        Args:
            tmp_path: Pytest temporary directory.
        """
        touch(tmp_path, "conflict_map.html")
        touch(tmp_path, "my_load_order.txt")
        touch(tmp_path, "notes_20260101_000000.html")
        touch(tmp_path, "pathgrid_20260101_000000.html")
        prune_generated(tmp_path, keep=0)
        assert sorted(p.name for p in tmp_path.iterdir()) == [
            "conflict_map.html",
            "my_load_order.txt",
            "notes_20260101_000000.html",
        ]

    def test_restricting_stems_restricts_deletion(self, tmp_path: Path) -> None:
        """The stem list is the safety mechanism, so it must be honoured.

        Args:
            tmp_path: Pytest temporary directory.
        """
        touch(tmp_path, "pathgrid_20260101_000000.html")
        touch(tmp_path, "conflict_map_20260101_000000.html")
        prune_generated(tmp_path, keep=0, stems=("pathgrid",))
        assert [p.name for p in tmp_path.iterdir()] == ["conflict_map_20260101_000000.html"]


class TestPruneFailurePaths:
    """A page that cannot be deleted must not break the close handler."""

    def test_locked_page_is_skipped_not_reported(self, tmp_path: Path) -> None:
        """A file open in a viewer is normal; the next cleanup gets it.

        Args:
            tmp_path: Pytest temporary directory.
        """
        locked = touch(tmp_path, "pathgrid_20260101_000000.html")
        touch(tmp_path, "pathgrid_20260202_000000.html")
        original = Path.unlink

        def refuse(self: Path, missing_ok: bool = False) -> None:
            """Fail on the locked page only.

            Args:
                self: The path being unlinked.
                missing_ok: Passed through.

            Raises:
                PermissionError: For the locked page.
            """
            if self == locked:
                raise PermissionError(13, "in use")
            original(self, missing_ok=missing_ok)

        with pytest.MonkeyPatch.context() as patch:
            patch.setattr(Path, "unlink", refuse)
            removed = prune_generated(tmp_path, keep=1)
        assert removed == []
        assert locked.exists()

    def test_undeletable_sidecar_is_not_claimed_as_removed(self, tmp_path: Path) -> None:
        """The summary has to match what actually happened.

        Args:
            tmp_path: Pytest temporary directory.
        """
        page = touch(tmp_path, "pathgrid_20260101_000000.html")
        touch(tmp_path, "pathgrid_20260202_000000.html")
        data = sidecar_folder(page)
        data.mkdir()

        with pytest.MonkeyPatch.context() as patch:
            patch.setattr(Path, "rmdir", _raise_permission)
            removed = prune_generated(tmp_path, keep=1)
        assert removed == [page]
        assert data.is_dir()

    def test_symlink_in_a_sidecar_is_unlinked_not_followed(self, tmp_path: Path) -> None:
        """An explicit walk cannot be redirected out of the folder.

        Args:
            tmp_path: Pytest temporary directory.
        """
        outside = tmp_path / "keep_me"
        outside.mkdir()
        (outside / "important.txt").write_text("x", encoding="utf-8")
        page = touch(tmp_path, "pathgrid_20260101_000000.html")
        touch(tmp_path, "pathgrid_20260202_000000.html")
        data = sidecar_folder(page)
        data.mkdir()
        try:
            (data / "link").symlink_to(outside, target_is_directory=True)
        except (OSError, NotImplementedError):  # pragma: no cover - Windows without privilege
            pytest.skip("symlinks not permitted here")
        prune_generated(tmp_path, keep=1)
        assert not data.exists()
        assert (outside / "important.txt").exists()


def _raise_permission(*_args: object, **_kwargs: object) -> None:
    """Stand in for a directory that cannot be removed.

    Args:
        _args: Ignored.
        _kwargs: Ignored.

    Raises:
        PermissionError: Always.
    """
    raise PermissionError(13, "in use")


class TestDescribe:
    """The one log line a prune produces."""

    def test_empty_is_empty_so_the_caller_can_skip_logging(self) -> None:
        """Silence when nothing happened."""
        assert describe([]) == ""

    def test_counts_pages(self) -> None:
        """Pages are the ``.html`` entries."""
        assert describe([Path("a_20260101_000000.html")]) == "cleaned up 1 old page(s)"

    def test_counts_folders_separately(self) -> None:
        """A page and its data folder are two very different things."""
        summary = describe([Path("a_20260101_000000.html"), Path("a_20260101_000000_data")])
        assert summary == "cleaned up 1 old page(s) and 1 data folder(s)"


class TestSidecarFolder:
    """The naming convention the generators and the pruner share."""

    def test_name_is_the_page_stem_plus_data(self) -> None:
        """Both sides derive it, so it is asserted exactly."""
        assert sidecar_folder(Path("/x/conflict_map_20260101_000000.html")).name == (
            "conflict_map_20260101_000000_data"
        )

    def test_stays_beside_the_page(self) -> None:
        """A relative page must not resolve the folder to the cwd."""
        page = Path("/deep/nested/pathgrid_20260101_000000.html")
        assert sidecar_folder(page).parent == page.parent


class TestTableNeverLosesRows:
    """A short ``row_attrs`` must not truncate the table.

    Found in the 3.1 audit while checking the project's blanket ``B905``
    exemption, which claims every ``zip()`` is either an intentional offset
    pairing or a comparison that reports its own length mismatch. This one was
    neither: ``zip`` stops at the shorter list, and the shorter list is the
    attributes -- so a caller one attribute short would lose a *row*. On the
    conflict map that means losing a conflict.
    """

    def test_a_short_attribute_list_pads_rather_than_truncates(self) -> None:
        """A row with no data-* hooks is unfilterable; a missing row is invisible."""
        rendered = table(["col"], [["one"], ["two"], ["three"]], row_attrs=[{"data-m": "x"}])

        for value in ("one", "two", "three"):
            assert value in rendered, f"row {value!r} was dropped"

    def test_the_supplied_attributes_still_land_on_their_rows(self) -> None:
        """Padding must not shift the attributes it was padding around."""
        rendered = table(["col"], [["one"], ["two"]], row_attrs=[{"data-m": "first"}])

        assert '<tr data-m="first"><td>one</td>' in rendered

    def test_more_attributes_than_rows_is_harmless(self) -> None:
        """The other direction: extra attributes have no row to sit on."""
        rendered = table(["col"], [["only"]], row_attrs=[{"data-m": "a"}, {"data-m": "b"}])

        assert rendered.count("<tr") == 2  # the header row plus one body row
        assert "only" in rendered
