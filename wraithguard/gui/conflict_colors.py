"""xEdit-style conflict colours: a record's status as a colour, not just text.

The convention is xEdit's, and modders read it fluently: a record's *overall*
status (:class:`~wraithguard.patch.status.ConflictAll`) sets the row's
**background**, and what one plugin does to it
(:class:`~wraithguard.patch.status.ConflictThis`) sets the **text** colour --
so a red row with green text is an identical-to-master edit reverting a
conflict, a yellow row is a benign override, and so on. See the legend in the
STEP guide: https://stepmodifications.org/wiki/Guide:XEdit

The hues are the Material-Design values from a Material dark-mode edit of
xEdit's ``xEdit.ini`` (its ``[ColorConflictThis]`` / ``[ColorConflictAll]``
sections, stored as Delphi ``TColor`` BGR integers and decoded to ``#RRGGBB``).
Only the palette -- a set of colour values -- is borrowed; no xEdit code is
used.

Two adaptations to our dark UI:

* xEdit's dark theme does not fill a row with the bright Material hue; it lays a
  *dark tint* of the family behind **bright coloured text** of the same family
  (a subtly green row with green text for agree, a subtly red row with red text
  for a conflict). So the **background** map (:func:`all_colors`) pairs a dark
  tint with a bright legible foreground, and the foreground-only trees take the
  same bright tints (:data:`THIS_TEXT`, :data:`ALL_TEXT`). A contrast picker
  (:func:`readable_on`) is provided for callers that fill with an arbitrary
  colour and need legible text on it.
* Tk's ``Treeview`` colours a whole row, not a cell, so a field row that spans
  several plugins gets one background (its overall status) rather than xEdit's
  per-column text colours.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, Final

from wraithguard.patch.status import ConflictAll, ConflictThis
from wraithguard.patch.summary import ALL_TAGS

if TYPE_CHECKING:
    from collections.abc import Mapping

_DARK_TEXT: Final = "#101010"
_LIGHT_TEXT: Final = "#f4f4f4"


def readable_on(background: str) -> str:
    """Pick a dark or light text colour that stays legible on ``background``.

    Uses the perceived-luminance rule (ITU-R BT.601 weights): dark text on a
    light background, light text on a dark one.

    Args:
        background: A ``#RRGGBB`` colour.

    Returns:
        ``_DARK_TEXT`` or ``_LIGHT_TEXT``.
    """
    r = int(background[1:3], 16)
    g = int(background[3:5], 16)
    b = int(background[5:7], 16)
    luminance = 0.299 * r + 0.587 * g + 0.114 * b
    return _DARK_TEXT if luminance > 140 else _LIGHT_TEXT


# --- Row backgrounds: the [ColorConflictAll] families, as xEdit's dark mode ----
# xEdit's dark theme does NOT fill a row with the bright Material hue; it lays a
# *dark tint* of the family behind **bright coloured text** of the same family --
# a subtly green row with green text for agree, a subtly red row with red text
# for a conflict. That reads far better on near-black chrome than a bright slab
# with white text, and it is the look people know. So each entry pairs a dark
# tint (from the ini hue) with a bright legible foreground (from THIS_TEXT's
# family). ``None`` background leaves the row's own; those are the quiet states.
# The text is the ini's own hue at full strength (caNoConflict green, caOverride
# amber, ctConflictLoses red -- the colour you see in the screenshot); the
# background is a darkened tint of that same hue so the row is unmistakably its
# colour without drowning the bright text on top.
_ALL_BG: Final[dict[ConflictAll, tuple[str | None, str]]] = {
    ConflictAll.UNKNOWN: (None, "#e0e0e0"),
    ConflictAll.ONLY_ONE: (None, "#8a8a8a"),
    ConflictAll.NO_CONFLICT: ("#0e3320", "#00c853"),  # caNoConflict
    ConflictAll.OVERRIDE_BENIGN: ("#33290a", "#ffd600"),  # caOverride
    ConflictAll.CONFLICT: ("#331010", "#d50000"),  # caConflict fill, ctConflictLoses text
}

# --- Foreground tints: legible on the app's dark chrome -----------------------
# Material light tints (200-400) of the ct/ca families, so a foreground-only row
# still carries its verdict where there is no coloured background behind it.
#: Per-plugin status -> text colour (the nav tree).
THIS_TEXT: Final[dict[ConflictThis, str]] = {
    ConflictThis.UNKNOWN: "#e0e0e0",
    ConflictThis.IGNORED: "#9e9e9e",
    ConflictThis.DELETED: "#e57373",
    ConflictThis.IDENTICAL_TO_MASTER: "#bdbdbd",
    ConflictThis.MASTER: "#ce93d8",  # purple 200
    ConflictThis.OVERRIDE_WINS: "#81c784",  # green 300
    ConflictThis.CONFLICT_WINS: "#ffb74d",  # orange 300
    ConflictThis.CONFLICT_LOSES: "#ef5350",  # red 400
}

#: Overall status -> text colour (the record list, which colours by row text).
ALL_TEXT: Final[dict[ConflictAll, str]] = {
    ConflictAll.UNKNOWN: "#e0e0e0",
    ConflictAll.ONLY_ONE: "#b8b8b8",
    ConflictAll.NO_CONFLICT: "#b8b8b8",  # "agree" recedes, as before
    ConflictAll.OVERRIDE_BENIGN: "#ffca28",  # amber 400
    ConflictAll.CONFLICT: "#ef5350",  # red 400
}

#: The brighter variant for the user's own (★) records in the record list.
ALL_TEXT_MINE: Final[dict[ConflictAll, str]] = {
    ConflictAll.OVERRIDE_BENIGN: "#ffb454",
    ConflictAll.CONFLICT: "#ff4d4d",
    ConflictAll.ONLY_ONE: "#d0d0d0",
    ConflictAll.NO_CONFLICT: "#d0d0d0",
}


def all_colors(status: ConflictAll) -> tuple[str | None, str]:
    """Return ``(background, foreground)`` for a background-coloured row."""
    return _ALL_BG.get(status, (None, "#e0e0e0"))


def this_text(status: ConflictThis) -> str:
    """Return the foreground text colour for a per-plugin status."""
    return THIS_TEXT.get(status, "#e0e0e0")


def all_text(status: ConflictAll) -> str:
    """Return the foreground text colour for an overall status."""
    return ALL_TEXT.get(status, "#e0e0e0")


def all_bg_by_tag() -> Mapping[str, tuple[str | None, str]]:
    """Map each ``ALL_TAGS`` tag name to its ``(background, foreground)``.

    For configuring a field-diff tree's ``status-*`` tags in one pass.
    """
    return {name: all_colors(status) for status, (name, _why) in ALL_TAGS.items()}


def all_text_by_tag() -> Mapping[str, str]:
    """Map each ``ALL_TAGS`` tag name to its foreground text colour.

    For configuring a record-list tree's ``status-*`` tags in one pass.
    """
    return {name: all_text(status) for status, (name, _why) in ALL_TAGS.items()}


__all__ = [
    "ALL_TEXT",
    "ALL_TEXT_MINE",
    "THIS_TEXT",
    "all_bg_by_tag",
    "all_colors",
    "all_text",
    "all_text_by_tag",
    "readable_on",
    "this_text",
]
