"""The xEdit-style Material conflict palette.

Pure colour logic -- no tkinter -- so the mapping from a verdict to its
``(background, foreground)`` and the contrast picker are checked here without a
display.
"""

from __future__ import annotations

from wraithguard.gui.conflict_colors import (
    ALL_TEXT,
    THIS_TEXT,
    all_bg_by_tag,
    all_colors,
    all_text,
    all_text_by_tag,
    readable_on,
    this_text,
)
from wraithguard.patch.status import ConflictAll, ConflictThis
from wraithguard.patch.summary import ALL_TAGS


class TestReadableOn:
    """Contrast-picking a legible text colour for a background."""

    def test_light_background_gets_dark_text(self) -> None:
        """A bright Material fill (amber) reads with dark text."""
        assert readable_on("#FFD600") == "#101010"

    def test_dark_background_gets_light_text(self) -> None:
        """A deep fill (the conflict red, the green) reads with light text."""
        assert readable_on("#B71C1C") == "#f4f4f4"
        assert readable_on("#00C853") == "#f4f4f4"


class TestBackgroundPalette:
    """Overall status -> the row's (background, foreground)."""

    def test_the_three_conflict_states_carry_a_fill(self) -> None:
        """Agree, benign and conflict each get a dark tint + bright text."""
        assert all_colors(ConflictAll.NO_CONFLICT) == ("#0e3320", "#00c853")
        assert all_colors(ConflictAll.OVERRIDE_BENIGN) == ("#33290a", "#ffd600")
        assert all_colors(ConflictAll.CONFLICT) == ("#331010", "#d50000")

    def test_the_fill_is_dark_and_the_text_is_bright(self) -> None:
        """The xEdit dark-mode look: dark row, bright same-family text."""
        for status in (
            ConflictAll.NO_CONFLICT,
            ConflictAll.OVERRIDE_BENIGN,
            ConflictAll.CONFLICT,
        ):
            bg, fg = all_colors(status)
            assert bg is not None
            assert readable_on(bg) == "#f4f4f4"  # the tint is dark enough for light text
            # the text itself is brighter than the fill it sits on
            assert sum(int(fg[i : i + 2], 16) for i in (1, 3, 5)) > sum(
                int(bg[i : i + 2], 16) for i in (1, 3, 5)
            )

    def test_the_quiet_states_have_no_fill(self) -> None:
        """A single definition or an unknown status leaves the row's background."""
        assert all_colors(ConflictAll.ONLY_ONE)[0] is None
        assert all_colors(ConflictAll.UNKNOWN)[0] is None


class TestForegroundPalettes:
    """Every verdict has a legible foreground for the text-coloured trees."""

    def test_every_this_status_has_text(self) -> None:
        """The nav tree can colour any per-plugin status."""
        for status in ConflictThis:
            assert this_text(status).startswith("#")
            assert status in THIS_TEXT

    def test_every_all_status_has_text(self) -> None:
        """The record list can colour any overall status."""
        for status in ConflictAll:
            assert all_text(status).startswith("#")
            assert status in ALL_TEXT


class TestTagMaps:
    """The tag-name views used to configure a whole tree in one pass."""

    def test_backgrounds_are_keyed_by_the_all_tags_names(self) -> None:
        """Every ALL_TAGS tag name has a (bg, fg) entry."""
        by_tag = all_bg_by_tag()
        for name, _why in ALL_TAGS.values():
            assert name in by_tag
        assert by_tag["status-conflict"] == ("#331010", "#d50000")

    def test_text_map_is_keyed_by_the_all_tags_names(self) -> None:
        """Every ALL_TAGS tag name has a foreground entry."""
        by_tag = all_text_by_tag()
        for name, _why in ALL_TAGS.values():
            assert name in by_tag and by_tag[name].startswith("#")
