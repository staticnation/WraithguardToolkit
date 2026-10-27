#!/usr/bin/env python3
"""wraithguard_toolkit_gui.py.

Wraithguard's Tk window: the tools not yet ported to the viewer (port steps 4.3 and 4.4) --
Check Conflicts, Resource Conflicts, Cell Map, Cell Preview, Merge Lands, Merge Settings, Lint,
Save Check, tes3cmd, Remove Master, Backups -- over the order panels on the left.

The load order itself is the viewer's Load order page now (port step 4.2, slice 2k): Sort,
Export, the rule files, the rule maker, Update rules and Sources. "1. Load order" opens it;
its Export writes the order to a plan file, which this window takes into its panels by itself
("2. Take the order" reads it again).

Cross-platform: pure Python + tkinter, runs on Windows, Linux and macOS.

Requirements:
    pip install tkinterdnd2 --break-system-packages   (optional -- drag & drop)
    - tkinter ships with the python.org installers on Windows/macOS.
    - On Linux install it via your package manager, e.g.
      Debian/Ubuntu: sudo apt install python3-tk

If tkinterdnd2 isn't installed, the GUI still runs -- you just lose
drag-and-drop of files from your OS (dragging rows within the app's own
lists to reorder them doesn't need tkinterdnd2, so that always works) and
have to use the "Browse..." buttons instead for file inputs.

Run it with:
    python3 wraithguard_toolkit_gui.py

This file must sit next to wraithguard_toolkit.py (it imports it directly
rather than shelling out, so results, exceptions, etc. all stay in-process).
"""

# PEP 563: annotations are strings, so a hint may name a type that is
# only imported for type checking, and no annotation costs import time.
from __future__ import annotations

import json
import os
import queue
import subprocess
import sys
import threading
import traceback
from contextlib import redirect_stderr, redirect_stdout
from functools import partial
from pathlib import Path
from typing import TYPE_CHECKING, Any, ClassVar

from wraithguard.proc import restore_host_library_path

# Before anything starts a child process: a frozen Linux build's own library folder
# must not reach host programs (/bin/sh, xdg-open, the viewer). See proc.py.
restore_host_library_path()

from wraithguard.images.ico import largest_png_frame  # noqa: E402
from wraithguard.land import meta as land_meta, service as land_service  # noqa: E402
from wraithguard.logging_setup import get_logger  # noqa: E402
from wraithguard.patch.summary import ALL_TAGS, field_statuses, search_rows  # noqa: E402

LOG = get_logger(__name__)

if TYPE_CHECKING:
    from collections.abc import Callable, Collection, Mapping, Sequence

# Compiled-script disassembly for the field-diff window. Optional: if the
# package is missing the diff view still works, it just shows the raw base64
# blob it always did.
listing_for_bytecode_field: Callable[..., str] | None
variables_text_for_field: Callable[..., str] | None
try:
    from wraithguard.mwscript import (
        listing_for_bytecode_field,
        variables_text_for_field,
    )
except ImportError:  # pragma: no cover - only when wraithguard/ is absent
    listing_for_bytecode_field = None
    variables_text_for_field = None

try:
    import tkinter as tk
    from tkinter import messagebox, ttk

    # tkinter.filedialog's API, but the desktop's own dialog on Linux (dark, hidden files)
    from wraithguard.gui import filedlg as filedialog
except ImportError:
    sys.exit(
        "tkinter isn't available in this Python install.\n"
        "On Debian/Ubuntu: sudo apt install python3-tk\n"
        "On Windows/Mac's python.org installers, tkinter is included by default."
    )


def resource_path(relative_path: str) -> str:
    """Get absolute path to resource, works for development and PyInstaller bundles."""
    try:
        base_path = sys._MEIPASS  # type: ignore[attr-defined]  # set only in a PyInstaller bundle
    except Exception:  # noqa: BLE001 -- _MEIPASS absent unless bundled
        # os.path, not pathlib, on purpose: this runs during window init and must
        # never raise. os.path.abspath is a pure string op; Path.resolve() hits
        # the filesystem -- it follows symlinks (breaking the icon when the script
        # is reached via a link) and can raise on some Windows paths, which the
        # caller's broad except then swallows, leaving every window icon-less.
        base_path = os.path.dirname(os.path.abspath(__file__))  # noqa: PTH120, PTH100
    return os.path.join(base_path, relative_path)  # noqa: PTH118


def _icon_photo_image(ico_path: str) -> tk.PhotoImage | None:
    """Load the .ico's largest PNG frame as a Tk PhotoImage.

    The byte-level parsing lives in :mod:`wraithguard.images.ico` -- it has
    no tkinter dependency, so it is covered by the hermetic test suite,
    unlike this module. See that module's docstring for *why* this exists at
    all: Tk's ``iconbitmap`` only loads a real .ico on Windows.

    Args:
        ico_path: Path to the .ico file.

    Returns:
        A PhotoImage built from the largest PNG frame found, or ``None`` when
        the file is missing, unreadable, holds no PNG-encoded frame, or Tk
        itself rejects the bytes.
    """
    try:
        with Path(ico_path).open("rb") as fh:
            data = fh.read()
    except OSError:
        return None

    png_bytes = largest_png_frame(data)
    if png_bytes is None:
        return None
    try:
        return tk.PhotoImage(data=png_bytes)
    except tk.TclError:
        return None


# Inline HTML rendering for the cell map is optional. tkinterweb is PREFERRED --
# its HtmlFrame.load_file reads the map from disk (bounded memory) and renders the
# SVG grid; tkhtmlview only takes an in-memory string and can't draw SVG, so it's
# a last resort. Without either, the map is written to a file and opened in a
# browser. (pip install tkinterweb  -- recommended for the in-app window.)
#: Why each optional viewer backend is unavailable, if it is. Recorded rather
#: than discarded: "pywebview=False" in the trace says a page opened in the
#: browser but not *why*, and the difference between "not installed in the
#: build environment" and "bundled but its DLLs did not come with it" is the
#: whole diagnosis. Every entry here is a swallowed exception that would
#: otherwise have to be reproduced by hand.
VIEWER_IMPORT_ERRORS: dict[str, str] = {}

HTMLViewer: Any = None
try:
    from tkinterweb import HtmlFrame as _HtmlFrame

    HTMLViewer = _HtmlFrame  # supports load_file + SVG
except Exception as _exc:  # noqa: BLE001
    # optional 3rd-party import; a broken install must not kill startup
    VIEWER_IMPORT_ERRORS["tkinterweb"] = f"{type(_exc).__name__}: {_exc}"
    try:
        from tkhtmlview import HTMLScrolledText as _HTMLScrolledText

        HTMLViewer = _HTMLScrolledText
    except Exception as _exc2:  # noqa: BLE001
        # optional 3rd-party import; a broken install must not kill startup
        VIEWER_IMPORT_ERRORS["tkhtmlview"] = f"{type(_exc2).__name__}: {_exc2}"
        HTMLViewer = None

# The Tauri viewer shell is the BEST in-app option: it's a real native
# webview (WebView2 / WebKit), so it renders the SVG map + tabs exactly like
# a browser. It's a separate binary, not a Python import, and it's always
# launched in its own process (see _open_cell_map_pywebview) so it doesn't
# fight tkinter's mainloop. Detected here by presence on disk, not by
# import; used first if present. The name HAVE_PYWEBVIEW is kept (rather
# than renamed) to minimise the diff against callers that branch on it --
# it now means "the embedded native viewer is available" regardless of
# which binary provides it.
try:
    from wraithguard.viewer_launch import viewer_binary

    HAVE_PYWEBVIEW = viewer_binary() is not None
except Exception as _exc3:  # noqa: BLE001
    # resource_path() must never raise, but a broken install must not kill
    # startup even if it somehow does
    VIEWER_IMPORT_ERRORS["wraithguard-viewer"] = f"{type(_exc3).__name__}: {_exc3}"
    HAVE_PYWEBVIEW = False


_TRACE_REQUEST = None  # set by main() from --trace; None = use env var / off

# app_base_dir() moved to wraithguard/gui/__init__.py in the 3.0 split; it is
# re-imported below (after the sys.path fix-up) so every call site here and in
# the mixins resolves the same cached answer.

# abspath/dirname deliberately, not Path.resolve(): resolve() follows
# symlinks, and this file is routinely run through one (MO2 junctions, a
# symlinked tools folder). Resolving would put the WRONG directory on
# sys.path and the engine import below would fail.
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))  # noqa: PTH100, PTH120
try:
    import wraithguard_toolkit as core
except ImportError as e:
    sys.exit(f"Couldn't import wraithguard_toolkit.py -- make sure it's in the same folder.\n({e})")

# The gettext marker. Imported after the sys.path fix-up above, which is what
# makes the package findable; hence the E402. Every user-facing literal in this
# file is wrapped in _() so `tools/make_pot.py` can extract it -- with no
# catalogue installed this returns the English string unchanged.
from wraithguard import _, ngettext  # noqa: E402
from wraithguard.configurator.cfglines import (  # noqa: E402
    extract_data_path_value,
    normalize_data_path,
)

# ---------------------------------------------------------------------------
# GUI support package (split out per CODE_REVIEW.md §16/§9.2, 3.0). Bodies
# moved verbatim; every name is re-imported here so the rest of this module --
# and the smoke-test instructions that reference these names -- are unchanged.
# Imported after the sys.path fix-up, like the gettext marker above.
# ---------------------------------------------------------------------------
from wraithguard.gui import (  # noqa: E402
    HAVE_DND,
    HELP_DOCUMENTS,
    TkinterDnD,
    app_base_dir,
    case_insensitive_filetypes,
    dnd_ready,
    doc_path,
    open_in_browser,
    rtl,
    trace_first_fire,
)
from wraithguard.gui.cellpreview import CellPreviewMixin  # noqa: E402
from wraithguard.gui.conflicts import ConflictWindowsMixin  # noqa: E402
from wraithguard.gui.editorlink import EditorLinkMixin, record_cache  # noqa: E402
from wraithguard.gui.journalview import JournalViewMixin  # noqa: E402
from wraithguard.gui.luaview import LuaViewMixin  # noqa: E402
from wraithguard.gui.patchwin import PatchBuilderMixin  # noqa: E402
from wraithguard.gui.pluginview import PluginViewMixin  # noqa: E402
from wraithguard.gui.removemaster import RemoveMasterMixin  # noqa: E402
from wraithguard.gui.t3 import Tes3cmdMixin  # noqa: E402
from wraithguard.gui.theme import (  # noqa: E402
    _THEME_REQUIRED,
    DARK,
    THEME_PRESETS,
    _mix_hex,
    _normalize_hex,
    apply_dark_theme,
    apply_titlebar_theme,
    parse_theme_file,
    restyle_widget_tree,
    set_active_chrome,
    style_plain_widget,
)
from wraithguard.gui.widgets import (  # noqa: E402
    DragReorderListbox,
    PathField,
    QueueWriter,
    add_tooltip,
    attach_typeahead,
    group_separator,
    once_per_idle,
    repaint_after_resize,
)
from wraithguard.plugins import PluginFileIndex  # noqa: E402
from wraithguard.viz.serve import publish_html_file  # noqa: E402

# The cell map is coverage only and needs nothing from viz/'s page builders:
# the conflict views are built by the Conflicts window (see
# wraithguard/gui/conflicts.py), which imports the four it uses directly.
# Housekeeping and the documentation renderer are the two exceptions -- pruning
# generated pages and showing the shipped help are both app-level concerns, so
# they live here. Optional like every other extra: without them the tidy-up is
# skipped and the Help button reports that it has nothing to show.
try:
    from wraithguard.viz import docs as viz_docs, housekeeping as viz_housekeeping
except ImportError:  # pragma: no cover - only when viz/ is absent
    viz_docs = None  # type: ignore[assignment]
    viz_housekeeping = None  # type: ignore[assignment]
from wraithguard.tracing import set_trace_file, trace  # noqa: E402


def _app_version() -> str:
    """Return the running build's version string, or ``?`` if unknown."""
    try:
        from wraithguard import __version__

        return str(__version__)
    except Exception:  # noqa: BLE001
        # a version stamp must never be the thing that stops the app starting
        return "?"


def _build_stamp() -> str:
    """Identify which build is running: frozen exe vs source, plus its mtime.

    Exists because a stale .exe presents exactly like a code bug -- new source
    on disk, old behaviour on screen, and nothing in the log to tell them
    apart. Comparing this timestamp against the source tree settles it.
    """
    from datetime import datetime as _dt

    frozen = bool(getattr(sys, "frozen", False))
    target = Path(sys.executable) if frozen else Path(__file__)
    try:
        # Local clock: the build stamp is compared by eye against the user's
        # own file timestamps in Explorer.
        mtime = _dt.fromtimestamp(target.stat().st_mtime).strftime(  # noqa: DTZ006
            "%Y-%m-%d %H:%M:%S"
        )
    except OSError:
        mtime = "?"
    return f"frozen={frozen} built={mtime} path={target.name}"


# ---------------------------------------------------------------------------
# rule-file list: an ordered listbox (priority = order, last = highest,
# matching wraithguard_toolkit's own --rules semantics) with add/remove/reorder
# controls and its own drop target
# ---------------------------------------------------------------------------


class ReorderPanel:
    """A titled drag-reorderable list with Move Up/Down, Reset and disable support."""

    # "touched by this sort" row highlight. Deliberately a warm amber rather
    # than a blue -- blue was both low-contrast against the dark field bg and
    # easily confused with the blue selection highlight (#094771). Amber on
    # near-black reads clearly and never collides with the selection color.
    HIGHLIGHT: ClassVar[dict[str, str]] = {"background": "#8a0808", "foreground": "#ffe8c2"}

    # NORMAL/DISABLED are methods rather than ClassVars: reading DARK at class
    # definition time would freeze the startup palette, and a runtime theme
    # switch would then restyle rows with stale colors.
    @staticmethod
    def _row_normal() -> dict[str, str]:
        return {"background": DARK["field_bg"], "foreground": DARK["fg"]}

    @staticmethod
    def _row_disabled() -> dict[str, str]:
        # dimmer than fg_dim: mixed toward the field background (under the
        # default theme this lands within 1/255 of the old hardcoded #6a6a6a)
        return {
            "background": DARK["field_bg"],
            "foreground": _mix_hex(DARK["fg_dim"], DARK["field_bg"], 0.44),
        }

    # rows with an active problem (e.g. missing/mis-ordered master): vivid
    # purple with white text -- the only strong hue not already meaning
    # something in this app (red = touched by this sort, gold = your mods on
    # the cell map, navy = selection, grey = disabled, orange = log warnings),
    # and it pops against all of them
    ERROR: ClassVar[dict[str, str]] = {"background": "#8e24aa", "foreground": "#ffffff"}
    DISABLE_PREFIX = "✗ "  # "X " marker shown on opted-out rows

    def __init__(
        self,
        parent: tk.Misc,
        title: str,
        reset_label: str = "Reset to Computed Order",
        listbox_tooltip: str | None = None,
    ) -> None:
        """Build a titled reorderable panel inside ``parent``."""
        frame = ttk.LabelFrame(parent, text=title)
        frame.pack(fill="both", expand=True)
        frame.columnconfigure(0, weight=1)
        frame.rowconfigure(0, weight=1)
        self.frame = frame

        list_frame = ttk.Frame(frame)
        list_frame.grid(row=0, column=0, sticky="nsew", padx=8, pady=8)
        list_frame.columnconfigure(0, weight=1)
        list_frame.rowconfigure(0, weight=1)

        # extended selection so several rows can be opted out at once
        # (Ctrl/Cmd-click and Shift-click to multi-select); dragging still
        # reorders the single row under the cursor
        self.listbox = DragReorderListbox(
            list_frame,
            height=8,
            selectmode="extended",
            activestyle="dotbox",
            exportselection=False,
            on_reorder=self._restyle,
        )
        style_plain_widget(self.listbox)
        self.listbox.grid(row=0, column=0, sticky="nsew")
        self.listbox.bind("<Double-Button-1>", self._on_double, add="+")
        # type-to-jump: click the list, then just start typing a plugin name;
        # the panel title shows what you've typed
        attach_typeahead(
            self.listbox,
            strip=self._strip,
            feedback=lambda buf, _f=frame, _t=title: _f.configure(  # type: ignore[misc]
                text=(_t + (f"   [find: {buf}]" if buf else ""))
            ),
        )
        if listbox_tooltip:
            add_tooltip(self.listbox, listbox_tooltip)
        scroll = ttk.Scrollbar(list_frame, orient="vertical", command=self.listbox.yview)
        scroll.grid(row=0, column=1, sticky="ns")
        self.listbox.configure(yscrollcommand=scroll.set)

        btns = ttk.Frame(frame)
        btns.grid(row=0, column=1, sticky="n", padx=(0, 8), pady=8)
        up_btn = ttk.Button(btns, text=_("Move Up"), command=lambda: self.move(-1))
        up_btn.pack(fill="x", pady=2)
        add_tooltip(
            up_btn,
            _(
                "Move the selected row(s) one position earlier. Works with a multi-"
                "selection; you can also drag a contiguous block up with the mouse."
            ),
        )
        down_btn = ttk.Button(btns, text=_("Move Down"), command=lambda: self.move(1))
        down_btn.pack(fill="x", pady=2)
        add_tooltip(
            down_btn,
            _(
                "Move the selected row(s) one position later. Works with a multi-"
                "selection; you can also drag a contiguous block down with the mouse."
            ),
        )
        toggle_btn = ttk.Button(btns, text=_("Disable / Enable"), command=self.toggle_selected)
        toggle_btn.pack(fill="x", pady=(10, 2))
        add_tooltip(
            toggle_btn,
            _(
                "Opt the selected row in or out of the load order. Disabled rows are dimmed and "
                "marked, and are left out of Export: a custom item is simply not inserted, and an "
                "item already in your openmw.cfg gets a removeContent/removeData entry in the "
                "emitted TOML so it's durably removed. Double-click a row to toggle it too."
            ),
        )
        reset_btn = ttk.Button(btns, text=reset_label, command=self.reset)
        reset_btn.pack(fill="x", pady=(2, 2))
        add_tooltip(
            reset_btn,
            _(
                "Discard any manual dragging and restore the order from the last Sort "
                "(your disable/enable choices are kept)."
            ),
        )

        self._original_order: list[str] = []
        self._highlight_lower: set[str] = set()
        # rows flagged with an active problem (bright red)
        self._error_lower: set[str] = set()
        self._disabled: set[str] = set()  # real item texts the user has opted out

    def load(
        self,
        items: Sequence[str],
        highlighted_items: Collection[str] = (),
        disabled_items: Collection[str] = (),
    ) -> None:
        """Populate the list after a successful Sort.

        Remembers
        it (for Reset), which items render highlighted, and which are disabled.
        disabled_items lets a re-Sort carry the previous opt-outs forward for
        any item still present.
        """
        self._original_order = list(items)
        self._highlight_lower = {str(x).lower() for x in highlighted_items}
        self._error_lower = set()
        present = set(items)
        self._disabled = {str(d) for d in disabled_items if str(d) in present}
        self._refill(self._original_order)

    def set_errors(self, items: Collection[str]) -> None:
        """Flag rows with an active problem (e.g.

        a missing master) -- they render bright red until the next load().
        """
        self._error_lower = {str(x).lower() for x in (items or ())}
        self._restyle()

    def reset(self) -> None:
        """Restore the computed order, discarding manual edits."""
        self._refill(self._original_order)

    def _display(self, real: str) -> str:
        return self.DISABLE_PREFIX + real if real in self._disabled else real

    def _strip(self, display: str) -> str:
        if display.startswith(self.DISABLE_PREFIX):
            return display[len(self.DISABLE_PREFIX) :]
        return display

    def _refill(self, items: Sequence[str]) -> None:
        self.listbox.delete(0, "end")
        for real in items:
            self.listbox.insert("end", self._display(real))
        self._restyle()

    def _restyle(self) -> None:
        trace_first_fire("plugin list restyle")
        """Apply per-row colors: disabled = dim, else problem rows = bright
        red, else highlighted, else normal. Explicit on every row so
        toggling/dragging stays consistent."""
        for i, disp in enumerate(self.listbox.get(0, "end")):
            real = self._strip(disp)
            if real in self._disabled:
                self.listbox.itemconfig(i, **self._row_disabled())
            elif real.lower() in self._error_lower:
                self.listbox.itemconfig(i, **self.ERROR)
            elif real.lower() in self._highlight_lower:
                self.listbox.itemconfig(i, **self.HIGHLIGHT)
            else:
                self.listbox.itemconfig(i, **self._row_normal())

    def _on_double(self, _event: tk.Event) -> str:
        self.toggle_selected()
        return "break"

    def toggle_selected(self) -> None:
        """Opt the selected row(s) in/out.

        With several rows selected: if any is currently enabled, disable them all;
        otherwise enable them all (so a bulk click has one predictable outcome). A
        single row just flips.
        """
        sel = list(self.listbox.curselection())
        if not sel:
            return
        reals = [self._strip(self.listbox.get(i)) for i in sel]
        disable = any(r not in self._disabled for r in reals)
        for i, real in zip(sel, reals):
            if disable:
                self._disabled.add(real)
            else:
                self._disabled.discard(real)
            self.listbox.delete(i)
            self.listbox.insert(i, self._display(real))  # replace in place, index unchanged
        self._restyle()
        for i in sel:
            self.listbox.selection_set(i)
        self.listbox.see(sel[0])

    def move(self, direction: int) -> None:
        """Move all selected rows one step up or down.

        Direction < 0 is up, > 0 is down;
        together, preserving their order and selection. Blocked if the leading
        selected row is already at the edge.
        """
        sel = sorted(self.listbox.curselection())
        if not sel:
            return
        size = self.listbox.size()
        if direction < 0:
            if sel[0] <= 0:
                return
            for idx in sel:  # ascending, so each swaps up cleanly
                t = self.listbox.get(idx)
                self.listbox.delete(idx)
                self.listbox.insert(idx - 1, t)
            new_sel = [i - 1 for i in sel]
        else:
            if sel[-1] >= size - 1:
                return
            for idx in reversed(sel):  # descending for a down-move
                t = self.listbox.get(idx)
                self.listbox.delete(idx)
                self.listbox.insert(idx + 1, t)
            new_sel = [i + 1 for i in sel]
        self._restyle()
        self.listbox.selection_clear(0, "end")
        for i in new_sel:
            self.listbox.selection_set(i)
        self.listbox.see(new_sel[0] if direction < 0 else new_sel[-1])

    def get_order(self) -> list[str]:
        """All rows, in current order, real text (opt-out marker stripped)."""
        return [self._strip(x) for x in self.listbox.get(0, "end")]

    def get_enabled(self) -> list[str]:
        """Only the rows that are still enabled, in order."""
        return [r for r in self.get_order() if r not in self._disabled]

    def get_disabled(self) -> set[str]:
        """Return the rows the user opted out of."""
        return set(self._disabled)

    def has_order(self) -> bool:
        """Report whether the panel currently holds an order."""
        return self.listbox.size() > 0


class PluginOrderPanel(ReorderPanel):
    """The plugin load-order panel."""

    def __init__(self, parent: tk.Misc) -> None:
        """Build the panel inside ``parent``."""
        super().__init__(
            parent,
            title=_(
                "Plugin load order -- drag to override mlox (red = touched by this sort, "
                "purple = master problem)"
            ),
            reset_label="Reset to mlox Order",
            listbox_tooltip="The content= load order mlox computed, after '1. Sort'. Drag rows to "
            "manually override it before Exporting -- red rows are the ones "
            "mlox actually inserted or moved; PURPLE rows have a missing or "
            "mis-ordered master (see the MASTER CHECK section in the log); "
            "unhighlighted rows were already in openmw.cfg and left where they "
            "were. Select row(s) and click Disable/Enable (or double-click) to "
            "opt them out of the load order.",
        )


class DataPathOrderPanel(ReorderPanel):
    """Same idea as PluginOrderPanel but for data= folder paths.

    Populated when a Sort was run with 'Sort data= paths too' checked (the
    computed, managed order). On an *empty* sort it instead shows the cfg's
    existing data= lines so they can still be edited/removed, mirroring the
    plugin panel's base-order fallback; on a normal plugin-only sort with
    data-path sorting off it stays empty (see App._sort_finished).
    """

    def __init__(self, parent: tk.Misc) -> None:
        """Build the panel inside ``parent``."""
        super().__init__(
            parent,
            title=_("Data path order -- drag to adjust (highlighted = your custom paths)"),
            reset_label="Reset to Computed Order",
            listbox_tooltip="The data= folder order, populated after '1. Sort' if 'Sort data= paths "
            "too' is checked. Drag rows to manually adjust before Exporting -- "
            "highlighted rows are the custom data paths this sort manages (newly "
            "inserted, OR already in openmw.cfg from a prior configurator run); "
            "unhighlighted rows are base mod-list paths left as-is. Select row(s) and "
            "click Disable/Enable (or double-click) to opt them out. Stays empty if "
            "data-path sorting was off.",
        )


# ---------------------------------------------------------------------------
# main application
# ---------------------------------------------------------------------------


#: Marker for an ``[ANY ...]`` group in the rule maker's plugin list. A list
#: entry is either a plugin filename or one of these; nothing else is a valid
#: entry, so the prefix doubles as the discriminator when the rule is built.
RM_ANY_PREFIX = "ANY: "

#: One line per rule, in the guidelines' own terms. Shown under the picker so
#: choosing a rule type does not require having read the guidelines first.
RULE_KIND_HELP: dict[str, str] = {
    "Order": "These plugins load in this order, first one first. The rule most "
    "people need, and the one the guidelines steer you towards.",
    "NearStart": "Pull each plugin toward the START of the load order. "
    "Discouraged -- prefer [Order] to place plugins against each other.",
    "NearEnd": "Pull each plugin toward the END. Also discouraged, and it means "
    "'closer to the end where possible', not 'last'.",
    "Note": "Print a message when the listed expressions are all true. The "
    "general-purpose rule.",
    "Requires": "The first plugin needs the second to be present. Warns, in red, "
    "when it is not.",
    "Conflict": "Warns, in yellow, when any two of the listed plugins are active " "at once.",
    "Patch": "A mutual dependency: the patch is pointless without what it "
    "patches, and what it patches wants the patch. Warns both ways.",
}


def _action_button(
    bar: tk.Misc,
    text: str,
    cmd: Callable[[], None],
    tip: str,
    state: str = "normal",
    pad: tuple[int, int] = (0, 6),
    style: str = "",
    width: int = 0,
) -> ttk.Button:
    """Pack one left-aligned action button with its tooltip.

    Module-level rather than a closure inside the action bar: two row builders
    now share it, and a helper that has to be passed between them is a helper
    that belongs outside both.

    Args:
        bar: The row frame to pack into.
        text: The button label.
        cmd: The callback.
        tip: Hover text explaining what the button does and what it touches.
        state: ``"normal"`` or ``"disabled"``; the scans start disabled until a
            Sort has produced a plugin list for them to read.
        pad: Horizontal padding, used to group related buttons visually.
        style: A ttk style name (e.g. ``"Accent.TButton"`` for a primary
            action); empty leaves the default button style.
        width: Fixed button width in text units, or ``0`` to size to the label.
            Used to give the leading button of each row an equal width so the
            group divider that follows lines up between the two rows.

    Returns:
        The button, so the caller can keep a handle for enabling it later.
    """
    button = ttk.Button(bar, text=text, command=cmd, state=state)
    if style:
        button.configure(style=style)
    if width:
        button.configure(width=width)
    button.pack(side="left", padx=pad)
    add_tooltip(button, tip)
    return button


# The two action rows each open with a pair of buttons, then a group divider:
# row 1 is "1. Sort" / "2. Export", row 2 is "Merge Lands" / "Merge Settings".
# Giving each column the same width on both rows (wide enough for the longer
# row-2 labels) makes that first divider line up vertically between the rows.
_LEAD_COL1_WIDTH = 12  # fits "Merge Lands"
_LEAD_COL2_WIDTH = 15  # fits "Merge Settings"
_LEAD_PAD = (0, 6)  # identical on all four so the divider lands at the same x


class App(
    Tes3cmdMixin,
    RemoveMasterMixin,
    ConflictWindowsMixin,
    JournalViewMixin,
    LuaViewMixin,
    PatchBuilderMixin,
    PluginViewMixin,
    CellPreviewMixin,
    EditorLinkMixin,
):
    """The main application window."""

    # Colors for the log panel's tags now come from the selected syntax
    # highlighting theme (THEME_PRESETS / imported custom themes) rather than
    # a fixed palette -- see _log_tag_style, _apply_log_theme. section/error
    # stay bold across every theme since that's the thing you most need to
    # be able to spot at a glance; everything else is a plain-weight color.

    @staticmethod
    def _log_tag_style(theme: Mapping[str, Any]) -> dict[str, dict[str, Any]]:
        return {
            "section": {"foreground": theme["section"], "font": ("TkFixedFont", 10, "bold")},
            "warn": {"foreground": theme["warn"]},
            "error": {"foreground": theme["error"], "font": ("TkFixedFont", 10, "bold")},
            "ok": {"foreground": theme["ok"]},
            "inserted": {
                "foreground": theme["inserted"]
            },  # a plugin/path this sort inserted or moved
            "dim": {"foreground": theme["dim"]},
        }

    def _schedule_ui(
        self, delay_ms: int, func: Callable[..., Any], *args: Any  # noqa: ANN401
    ) -> None:
        """Marshal a worker-thread callback onto the UI thread, without raising.

        Every background worker (scan, sort, export, merge, lint, backups,
        rule/plugin-order downloads, plugin-view judging) ends by handing its
        result back with ``root.after(...)``. That call needs a live
        interpreter that is actually pumping its event loop to receive a
        cross-thread request; if the window has already been torn down (the
        app closing while a worker is still running) or -- in headless tests
        -- no ``mainloop()`` ever ran, it raises instead of queuing. Either
        way there is nothing left to update, so this drops the callback
        rather than crashing a daemon thread.

        Args:
            delay_ms: Milliseconds to wait before running ``func``.
            func: The callable to run on the UI thread.
            *args: Positional arguments for ``func``.
        """
        try:
            self.root.after(delay_ms, func, *args)
        except (RuntimeError, tk.TclError):
            LOG.debug("dropped a worker callback: window is gone")

    def __init__(self, root: tk.Tk) -> None:
        """Build the whole application UI on ``root``."""
        self.root = root
        root.title("Wraithguard Toolkit")
        # Safely attempt to load the window icon. default= (not the plain
        # positional form) is what makes every Toplevel this app opens later
        # -- the rule maker, conflict windows, viewers, etc. -- inherit it
        # automatically instead of falling back to Tk's generic feather icon;
        # the positional form only sets it for this one window.
        try:
            icon_path = resource_path("wraithguard_toolkit_icon.ico")
            if os.path.exists(icon_path):  # noqa: PTH110 -- pairs with resource_path's abspath
                # Windows: Tk special-cases .ico loading here, and this is
                # the only call that also sets the taskbar icon there. When
                # it succeeds, this IS the icon -- iconphoto below must not
                # also run and potentially override a perfectly good result
                # with whatever Tk's separate PNG-decode path produces.
                iconbitmap_ok = True
                try:
                    self.root.iconbitmap(default=icon_path)
                except tk.TclError:
                    iconbitmap_ok = False  # expected on Linux -- XBM-only there
                if not iconbitmap_ok:
                    # Linux, incl. Steam Deck's Desktop Mode: iconbitmap's
                    # .ico attempt above is silently a no-op, so this is what
                    # actually puts an icon on the window and in the
                    # taskbar there. default=True mirrors iconbitmap's
                    # default= above: every Toplevel this app opens later
                    # inherits it too.
                    photo = _icon_photo_image(icon_path)
                    if photo is not None:
                        self.root.iconphoto(True, photo)
                        # Tk drops a PhotoImage's pixels once nothing in
                        # Python still references it, even though the window
                        # keeps pointing at it -- a local-only reference here
                        # would let the icon go blank the moment __init__
                        # returns.
                        self._icon_photo = photo
        except Exception as e:  # noqa: BLE001 -- window icon is cosmetic; never block launch
            # Log or silently bypass if the icon file is missing
            print(f"Warning: Could not load window icon: {e}", file=sys.stderr)
        root.geometry("1320x900")
        root.minsize(1000, 620)

        self.log_queue = queue.Queue()
        self.worker_running = False
        self._log_group_tag: str | None = None
        self._current_plan = None
        # in-memory scan result (when not saved to a file)
        self._scanned_subset_lines: list[str] | None = None
        # Folders and plugins the user added by hand (button or drag-drop)
        # because the scan's folder-name heuristic missed them -- e.g. an OpenMW
        # Lua mod using a non-standard VFS layout, or a stray plugin outside a
        # scanned folder. Merged into the subset at Sort time, additive to
        # whatever other subset source is active. Session-scoped, like an
        # in-memory scan.
        self._manual_data_dirs: list[str] = []
        self._manual_plugins: list[str] = []
        self._tes3conv_override = None  # user-set path to tes3conv (for field-level diffs)
        # Where "Merge Lands" writes. Remembered, because the folder chosen by
        # "wherever most of the load order lives" is a mod's own directory on a
        # per-mod install -- which works, but buries the output somewhere the
        # user did not pick and would lose if that mod were uninstalled.
        self._merged_lands_out: str | None = None
        self._tes3cmd_override = None  # user-set path to tes3cmd (frontend window)
        # The OpenMW install whose Lua API the Lua scripts window checks against, when
        # the one found on its own is not it (the window's "OpenMW install...").
        self._openmw_install: str | None = None
        self._conf_session = None
        self._conf_paths = {}
        # reused disk-backed Tes3ConvSession
        self._session: core.Tes3ConvSession | None = None
        self._keep_json = False

        # Announce the running build in the Log panel on EVERY start, with no
        # flag required. "Am I actually running the build I just made?" should
        # be answerable by looking at the window, not by finding a trace file
        # -- which for a frozen .exe lives next to the .exe, not next to the
        # source, and is therefore easy to collect a stale copy of.
        self.log_queue.put(f"Wraithguard Toolkit {_app_version()} -- {_build_stamp()}\n")

        # Trace is OFF by default. It's turned on by the --trace flag (set by main()
        # into _TRACE_REQUEST) or the MLOX_SUBSET_TRACE env var -- either can name a
        # log file; otherwise wraithguard_toolkit_trace.log is written next to the app.
        # Enabled BEFORE theming and widget construction, deliberately: the theme
        # startup path calls trace_first_fire(), whose once-per-session labels
        # would otherwise be consumed while tracing was still off -- and then a
        # later theme switch would emit no [smoke] line at all, which reads as
        # exactly the dead-callback failure the markers exist to expose.
        try:
            req = (
                _TRACE_REQUEST
                if _TRACE_REQUEST is not None
                else os.environ.get("MLOX_SUBSET_TRACE")
            )
            if req:
                if isinstance(req, str) and req.lower() not in ("1", "true", "yes", "on", ""):
                    path: str | Path = req
                else:
                    path = app_base_dir() / "wraithguard_toolkit_trace.log"
                set_trace_file(path)
                trace("GUI started")
                # Build stamp, first thing after the header. A frozen .exe is
                # easy to rebuild-and-forget, and a stale one is otherwise
                # indistinguishable from a code bug: you get the old behaviour
                # with the new source sitting right there. Version + the source
                # file's mtime pin exactly which build is running.
                trace(f"build: version={_app_version()} {_build_stamp()}")
                trace(
                    f"viewers: frozen={bool(getattr(sys, 'frozen', False))} "
                    f"pywebview={HAVE_PYWEBVIEW} "
                    f"HTMLViewer={HTMLViewer.__module__ + '.' + HTMLViewer.__name__ if HTMLViewer else None} "
                    f"load_file={HTMLViewer is not None and hasattr(HTMLViewer, 'load_file')}"
                )
                # The line above says a viewer is missing; these say why it is
                # missing. Without them the only way to tell "never installed"
                # from "bundled but its DLLs did not come too" is to rebuild
                # and guess, which is how this cost an evening.
                for _name, _why in VIEWER_IMPORT_ERRORS.items():
                    trace(f"viewers: {_name} unavailable -- {_why}")
                # absolute path: for a frozen .exe this is next to the .exe,
                # which is NOT where the source tree's copy lives
                self.log_queue.put(f"[trace] writing debug trace to: {Path(path).resolve()}\n")
        except Exception:  # noqa: BLE001
            # enabling tracing must never be the thing that breaks startup
            pass

        self._custom_log_themes: dict[str, dict] = {}  # name -> theme dict, imported by the user
        self._load_custom_log_themes()
        self.log_theme_var = tk.StringVar(value="Dark (default)")
        # the saved theme must drive the chrome palette *before* the ttk.Style
        # is configured and any widget is built, or the whole GUI would come up
        # in the default dark chrome and only the log would match the theme.
        # _load_settings() can't run this early (it sets widget variables), so
        # just the theme name is read ahead of it; _load_settings() re-reads it
        # later to the same value.
        saved_theme = self._saved_log_theme_name()
        if saved_theme is not None and saved_theme in self._theme_names():
            self.log_theme_var.set(saved_theme)
        set_active_chrome(
            self._resolve_theme(self.log_theme_var.get()) or THEME_PRESETS["Dark (default)"]
        )
        self.style = apply_dark_theme(root)

        self._build_widgets()
        self._load_settings()
        self._apply_log_theme(self.log_theme_var.get(), announce=False)
        self.root.protocol("WM_DELETE_WINDOW", self._on_close)
        self.root.after(80, self._poll_log_queue)
        self.root.after(2000, self._watch_viewer_plan)

    # -- settings persistence ----------------------------------------------

    def _settings_file(self) -> Path:
        return app_base_dir() / "wraithguard_toolkit_settings.json"

    def _saved_log_theme_name(self) -> str | None:
        """Just the saved theme name, readable before any widget exists."""
        try:
            d = json.loads(self._settings_file().read_text(encoding="utf-8"))
        except (OSError, ValueError):  # unreadable file / bad JSON
            return None
        name = d.get("log_theme")
        return name if isinstance(name, str) else None

    def _gather_settings(self) -> dict[str, Any]:
        return {
            "cfg": self.cfg_var.get(),
            "plugin_order_yml": self.plugin_order_yml_var.get(),
            "tes3conv": self._tes3conv_override or "",
            "merged_lands_out": self._merged_lands_out or "",
            "exclude": self.exclude_var.get(),
            "tes3cmd": self._tes3cmd_override or "",
            "openmw_install": self._openmw_install or "",
            "keep_json": self.keep_json_var.get(),
            "merged_lands_verbose": self.merged_lands_verbose_var.get(),
            "cleanup_html": self.cleanup_html_var.get(),
            "log_theme": self.log_theme_var.get(),
        }

    def _load_settings(self) -> None:
        import json

        try:
            d = json.loads(self._settings_file().read_text(encoding="utf-8"))
        except (OSError, ValueError):  # unreadable file / bad JSON
            return
        for k, var in (
            ("cfg", self.cfg_var),
            ("plugin_order_yml", self.plugin_order_yml_var),
            ("exclude", self.exclude_var),
        ):
            if isinstance(d.get(k), str):
                var.set(d[k])
        for k, bvar in (
            ("keep_json", self.keep_json_var),
            ("merged_lands_verbose", self.merged_lands_verbose_var),
            ("cleanup_html", self.cleanup_html_var),
        ):
            if isinstance(d.get(k), bool):
                bvar.set(d[k])
        if d.get("tes3conv"):
            self._tes3conv_override = d["tes3conv"]
        if d.get("merged_lands_out"):
            self._merged_lands_out = d["merged_lands_out"]
        if d.get("tes3cmd"):
            self._tes3cmd_override = d["tes3cmd"]
        if isinstance(d.get("openmw_install"), str) and d["openmw_install"]:
            self._openmw_install = d["openmw_install"]
        if isinstance(d.get("log_theme"), str) and d["log_theme"] in self._theme_names():
            self.log_theme_var.set(d["log_theme"])

    def _save_settings(self) -> None:
        """Write the settings, keeping the keys this window no longer has.

        The sort's fields (``rules``, ``customizations``, ``list_name`` ...) left this window
        for the viewer's Load order page, which opens on them from this file
        (``toolscore::settings``); rewriting the file with only this window's keys would lose
        them.
        """
        import json

        try:
            kept = json.loads(self._settings_file().read_text(encoding="utf-8"))
        except (OSError, ValueError):  # no file yet / bad JSON
            kept = {}
        if not isinstance(kept, dict):
            kept = {}
        try:
            self._settings_file().write_text(
                json.dumps({**kept, **self._gather_settings()}, indent=2), encoding="utf-8"
            )
        except (OSError, TypeError, ValueError):  # unwritable path / unserialisable value
            pass

    def _on_close(self) -> None:
        self._save_settings()
        s = getattr(self, "_session", None)
        if s is not None:
            try:
                s.cleanup()  # removes the temp JSON dump (no-op if 'keep' was set)
            except Exception:  # noqa: BLE001
                # close path: an escape here would stop the window being destroyed
                pass
        self._tidy_generated_views()
        self.root.destroy()

    def _tidy_generated_views(self) -> None:
        """Prune old generated HTML views, unless the user opted out.

        Runs on exit rather than per-generation so a session's pages stay
        available for comparison while the app is open. Only files this tool
        wrote (a known stem plus a timestamp) are candidates, never anything
        saved under a name the user chose.

        Failures are swallowed: this is on the close path, and an exception
        here would leave the window undestroyed -- a tidy-up that prevents the
        app from closing is far worse than an untidy folder.
        """
        if viz_housekeeping is None or not self.cleanup_html_var.get():
            return
        try:
            removed = viz_housekeeping.prune_generated(app_base_dir())
            summary = viz_housekeeping.describe(removed)
            if summary:
                trace(f"housekeeping: {summary}")
        except Exception:  # noqa: BLE001 - see above; never block the close
            trace("housekeeping: prune FAILED:\n" + traceback.format_exc())

    # -- layout ------------------------------------------------------------

    def _paned(self, parent: tk.Misc, orient: str, bg: str | None = None) -> tk.PanedWindow:
        """Build a themed tk.PanedWindow with a draggable grip.

        Plain tk, not ttk (ttk's has no visible grip), styled to
        match the dark theme. The default square handle is turned off; a
        hamburger-style grip is overlaid instead by _attach_hamburger_grip().
        """
        return tk.PanedWindow(
            parent,
            orient=orient,  # type: ignore[arg-type]
            sashwidth=8,
            sashrelief="flat",
            showhandle=False,
            bd=0,
            background=bg or DARK["border"],
            sashpad=0,
        )

    def _attach_hamburger_grip(self, paned: tk.Misc, orient: str) -> None:
        """Overlay a hamburger-style (three-line) draggable grip on a sash.

        Centred on the
        single sash of a two-pane PanedWindow. Cross-platform: cursor names are
        tried in order and any failure is ignored, and if the sash geometry
        can't be read the grip just hides itself -- the sash stays draggable
        either way, so this is purely a nicer-looking handle, never load-bearing.
        """
        horizontal = orient == "horizontal"  # horizontal paned -> vertical sash
        long_px, thick_px = 34, 12
        w = thick_px if horizontal else long_px
        h = long_px if horizontal else thick_px
        grip = tk.Canvas(
            paned,
            width=w,
            height=h,
            bg=DARK["btn_bg"],
            highlightthickness=1,
            highlightbackground=DARK["border"],
            bd=0,
            takefocus=0,
        )
        # marks this as a pane-divider grip so theme.py's restyle walk only
        # repaints grips to btn_bg -- without this, every plain tk.Canvas in
        # the app (e.g. the scrolling controls_canvas) gets mistaken for one
        grip._is_paned_grip = True  # type: ignore[attr-defined]
        if horizontal:  # three vertical lines (drag left/right)
            for x in (w // 2 - 3, w // 2, w // 2 + 3):
                grip.create_line(x, 5, x, h - 5, fill=DARK["fg_dim"])
        else:  # three horizontal lines (drag up/down)
            for y in (h // 2 - 3, h // 2, h // 2 + 3):
                grip.create_line(5, y, w - 5, y, fill=DARK["fg_dim"])
        for cur in (
            ("sb_h_double_arrow" if horizontal else "sb_v_double_arrow"),
            "fleur",
            "hand2",
            "",
        ):
            try:
                grip.configure(cursor=cur)
                break
            except tk.TclError:
                continue

        def reposition(_event: tk.Event | None = None) -> None:
            try:
                x, y = paned.sash_coord(0)  # type: ignore[attr-defined]
                half = int(paned.cget("sashwidth")) // 2
            except (tk.TclError, IndexError, TypeError, ValueError):
                grip.place_forget()
                return
            if horizontal:
                grip.place(x=x + half, rely=0.5, anchor="center")
            else:
                grip.place(relx=0.5, y=y + half, anchor="center")

        def on_drag(event: tk.Event) -> None:
            try:
                if horizontal:
                    paned.sash_place(  # type: ignore[attr-defined]
                        0, max(1, event.x_root - paned.winfo_rootx()), 1
                    )
                else:
                    paned.sash_place(  # type: ignore[attr-defined]
                        0, 1, max(1, event.y_root - paned.winfo_rooty())
                    )
            except tk.TclError:
                pass
            reposition()

        reposition_idle = once_per_idle(paned, reposition)
        late: list[str] = []

        def reposition_soon(_event: tk.Event | None = None) -> None:
            # On a resize (esp. maximize/restore/fullscreen) the <Configure>
            # event fires before the PanedWindow has moved its sash, so reading
            # sash_coord() right now returns the OLD position and the grip lands
            # in the wrong place until you nudge it. Defer to the idle pass so we
            # read the sash position AFTER layout settles. A second delayed pass
            # catches window managers that relayout in more than one step.
            # Both coalesced: a drag sends one <Configure> a pixel, and scheduling
            # two callbacks for each piled up behind the drag.
            reposition_idle()
            if late:
                paned.after_cancel(late.pop())
            late.append(paned.after(60, reposition))

        grip.bind("<B1-Motion>", on_drag)
        paned.bind("<Configure>", reposition_soon, add="+")
        paned.bind("<B1-Motion>", reposition_idle, add="+")  # follow a direct sash drag
        paned.bind("<ButtonRelease-1>", lambda e: reposition(), add="+")
        paned.after(200, reposition)

    def _build_widgets(self) -> None:
        outer = ttk.Frame(self.root, padding=8)
        outer.pack(fill="both", expand=True)

        # main split: sorting panels on the left, everything else on the right
        main_pane = self._paned(outer, "horizontal")
        main_pane.pack(fill="both", expand=True)

        # LEFT: two stacked, independently resizable draggable-order panels
        left_pane = self._paned(main_pane, "vertical")
        main_pane.add(left_pane, minsize=280, width=380, stretch="always")

        plugin_frame = ttk.Frame(left_pane, padding=(0, 0, 6, 0))
        self.order_panel = PluginOrderPanel(plugin_frame)
        left_pane.add(plugin_frame, minsize=120, stretch="always")

        data_frame = ttk.Frame(left_pane, padding=(0, 6, 6, 0))
        self.data_order_panel = DataPathOrderPanel(data_frame)
        left_pane.add(data_frame, minsize=120, stretch="always")

        # RIGHT: the rest of the program (inputs/options/actions on top, log
        # below), also independently resizable against each other
        right_pane = self._paned(main_pane, "vertical")
        main_pane.add(right_pane, minsize=420, width=760, stretch="always")

        # Wrap controls_frame in a container with a scrollbar
        controls_container = ttk.Frame(right_pane)
        controls_container.columnconfigure(0, weight=1)
        controls_container.rowconfigure(0, weight=1)
        right_pane.add(controls_container, minsize=360)

        # Create a canvas with a scrollbar for the controls
        #
        # bg matches the chrome background directly. (Previously this looked
        # up "TFrame" background from the live ttk style instead of reading
        # DARK["bg"], on the theory that the two could drift apart -- in
        # practice that lookup could return a stale/wrong ttk state value
        # depending on theme, which is worse than just trusting DARK, the
        # single source of truth every other plain-tk widget in the app
        # already reads from.)
        canvas_bg = DARK["bg"]
        controls_canvas = tk.Canvas(controls_container, bg=canvas_bg, bd=0, highlightthickness=0)

        controls_scroll = ttk.Scrollbar(
            controls_container, orient="vertical", command=controls_canvas.yview
        )
        controls_canvas.configure(yscrollcommand=controls_scroll.set, yscrollincrement=24)
        controls_scroll.grid(row=0, column=1, sticky="ns")
        controls_canvas.grid(row=0, column=0, sticky="nsew")

        # Create a frame inside the canvas for the controls
        controls_frame = ttk.Frame(controls_canvas, padding=(6, 0, 0, 0))
        self._build_controls(controls_frame)

        # Configure the canvas to scroll the frame
        # Once per idle pass, not per event: dragging a divider sends one <Configure>
        # a pixel, and each would relay out the whole form (the slow pane drag on Linux).
        controls_frame.bind(
            "<Configure>",
            once_per_idle(
                controls_canvas,
                lambda: controls_canvas.configure(scrollregion=controls_canvas.bbox("all")),
            ),
        )
        self.controls_window = controls_canvas.create_window(
            (0, 0), window=controls_frame, anchor="nw"
        )
        controls_canvas.bind(
            "<Configure>",
            once_per_idle(
                controls_canvas,
                lambda: controls_canvas.itemconfig(
                    self.controls_window, width=controls_canvas.winfo_width()
                ),
            ),
        )
        # Linux: black blocks on the buttons after going fullscreen otherwise.
        repaint_after_resize(controls_canvas, self.controls_window)

        # Bind mousewheel to scroll the canvas -- only while the pointer is
        # actually over it. bind_all with no unbind (the previous version)
        # hijacks the wheel for every widget in the app, permanently, the
        # moment this frame is built. Button-4/5 are X11's wheel events (incl.
        # Steam Deck desktop mode); they carry no `delta` at all, unlike
        # <MouseWheel> (Windows/Mac), so both need handling. All three go
        # through "units" scrolling, which relies on the yscrollincrement set
        # above -- Tk silently no-ops unit scrolling when it's left at 0.
        def _controls_wheel(event: tk.Event) -> None:
            if event.num == 4:
                controls_canvas.yview_scroll(-1, "units")
            elif event.num == 5:
                controls_canvas.yview_scroll(1, "units")
            else:
                controls_canvas.yview_scroll(-1 * (event.delta // 120), "units")

        def _bind_controls_wheel(_event: object = None) -> None:
            controls_canvas.bind_all("<MouseWheel>", _controls_wheel)
            controls_canvas.bind_all("<Button-4>", _controls_wheel)
            controls_canvas.bind_all("<Button-5>", _controls_wheel)

        def _unbind_controls_wheel(_event: object = None) -> None:
            controls_canvas.unbind_all("<MouseWheel>")
            controls_canvas.unbind_all("<Button-4>")
            controls_canvas.unbind_all("<Button-5>")

        controls_canvas.bind("<Enter>", _bind_controls_wheel)
        controls_canvas.bind("<Leave>", _unbind_controls_wheel)

        log_container = ttk.Frame(right_pane, padding=(6, 6, 0, 0))
        self._build_log(log_container)
        right_pane.add(log_container, minsize=120, stretch="always")

        # overlay hamburger-style drag grips on each sash (purely cosmetic; the
        # sashes themselves stay draggable if the overlay can't render)
        self._attach_hamburger_grip(main_pane, "horizontal")
        self._attach_hamburger_grip(left_pane, "vertical")
        self._attach_hamburger_grip(right_pane, "vertical")

    def _build_controls(self, top: tk.Misc) -> None:
        """Build the input, option and action area that sits above the log.

        One builder per panel rather than one flat list. This was 413 lines of
        sequential widget construction, which is exactly the shape of code that
        makes a one-line change -- adding a button, moving a checkbox -- a nervous
        edit in the middle of a wall of text.

        Row numbers are still assigned here, in one place, so the vertical order
        of the panels stays readable at a glance.

        Args:
            top: The container frame.
        """
        top.columnconfigure(1, weight=1)
        start_row = self._build_dnd_note(top)
        self.cfg_var = tk.StringVar()
        self.plugin_order_yml_var = tk.StringVar()
        PathField(
            top,
            "openmw.cfg:",
            start_row,
            self.cfg_var,
            filetypes=(("openmw.cfg", "*.cfg"), ("All files", "*.*")),
            tooltip=_(
                "The openmw.cfg the Load order page opens on, and the tools here read their "
                "data folders from."
            ),
        )
        PathField(
            top,
            "plugin-order.yml:",
            start_row + 1,
            self.plugin_order_yml_var,
            filetypes=(("YAML", "*.yml *.yaml"), ("All files", "*.*")),
            tooltip=_(
                "MOMW's plugin-order.yml, for the tes3cmd window's needs-cleaning list. The "
                "Load order page downloads it (Update plugin-order.yml)."
            ),
        )
        self._build_tool_options(top, start_row + 2)
        self._build_action_bar(top, start_row + 3)

    def _build_tool_options(self, top: tk.Misc, row: int) -> None:
        """The options the tools here read: what to leave out of the scans, and housekeeping.

        Args:
            top: The container frame.
            row: The row the options occupy.
        """
        opts = ttk.LabelFrame(top, text=_("Options"))
        opts.grid(row=row, column=0, columnspan=3, sticky="ew", pady=(8, 0))
        opts.columnconfigure(1, weight=1)
        self.exclude_var = tk.StringVar()
        self.keep_json_var = tk.BooleanVar(value=False)
        self.merged_lands_verbose_var = tk.BooleanVar(value=False)
        self.cleanup_html_var = tk.BooleanVar(value=True)
        excl_lbl = ttk.Label(opts, text=_("Exclude from conflict / cell scans:"))
        excl_lbl.grid(row=0, column=0, sticky="w", padx=(6, 6), pady=(4, 2))
        excl = ttk.Entry(opts, textvariable=self.exclude_var)
        excl.grid(row=0, column=1, sticky="ew", padx=(0, 6), pady=(4, 2))
        add_tooltip(
            excl,
            _(
                "Glob patterns (comma or newline separated) for plugins the conflict, cell-map "
                "and lint scans skip, e.g. s3lightfixes* *delta* *grass*."
            ),
        )
        flags = ttk.Frame(opts)
        flags.grid(row=1, column=0, columnspan=2, sticky="w", padx=(6, 6), pady=(2, 4))
        for text, var, command in (
            (_("Keep tes3conv JSON dump"), self.keep_json_var, self._on_keep_json_toggle),
            (_("Tidy old HTML views"), self.cleanup_html_var, None),
            (_("Verbose Merged Lands log"), self.merged_lands_verbose_var, None),
        ):
            ttk.Checkbutton(flags, text=text, variable=var, command=command).pack(
                side="left", padx=(0, 12)
            )

    def _build_dnd_note(self, top: tk.Misc) -> int:
        """Explain the missing drag-and-drop when it is not available.

        Asks whether drops will actually *work*, not whether the Python package
        imported: those differ when the tkdnd Tcl side is missing, and the
        banner has to describe the window the user is looking at.

        Args:
            top: The container frame.

        Returns:
            The row the first real control should occupy -- one lower when the
            note is shown, so every panel below shifts with it.
        """
        if dnd_ready(top):
            return 0
        note = ttk.Label(
            top,
            foreground=DARK["fg_dim"],
            # Not "tkinterdnd2 not installed": it may well be installed, with
            # its tkdnd Tcl library missing or unloadable. Saying so sends
            # people to reinstall a package they already have.
            text=_(
                "Drag & drop is unavailable (tkinterdnd2 or its tkdnd library "
                "is missing) -- use the Browse buttons below."
            ),
        )
        note.grid(row=0, column=0, columnspan=3, sticky="w", pady=(0, 6))
        return 1

    def _build_action_bar(self, top: tk.Misc, row: int) -> None:
        """Build the two rows of buttons under the options.

        Sort computes the plan (and never writes anything); Export writes using
        whatever order the panels on the left are currently showing. Primary and
        read-only analysis actions go on the first row with the status label
        trailing, tools on the second.

        Args:
            top: The container frame.
            row: The row the action area occupies.
        """
        action_area = ttk.Frame(top)
        action_area.grid(row=row, column=0, columnspan=3, sticky="ew", pady=(8, 0))

        row1 = ttk.Frame(action_area)
        row1.pack(fill="x")
        row2 = ttk.Frame(action_area)
        row2.pack(fill="x", pady=(4, 0))
        self._build_primary_actions(row1)
        self._build_tool_actions(row2)

    def _build_primary_actions(self, row1: tk.Misc) -> None:
        """Build the first button row: the run itself, plus the read-only scans.

        Everything after Export starts disabled: they all need a sorted, enabled
        plugin list to work from, and offering them before a Sort has run would
        only produce a confusing empty result.

        Args:
            row1: The frame holding the first row.
        """
        self.sort_button = _action_button(
            row1,
            "1. Load order",
            self.on_open_load_order,
            "Open the Load order page in the viewer: Sort and Export happen there now -- the "
            "orders, the warnings, the rule files, the rule maker, the downloads. Its Export "
            "writes the order these tools work from.",
            pad=_LEAD_PAD,
            style="Accent.TButton",
            width=_LEAD_COL1_WIDTH,
        )
        self.export_button = _action_button(
            row1,
            "2. Take the order",
            self.on_take_viewer_plan,
            "Load the order the Load order page last exported into the panels on the left, "
            "for the tools here (Conflicts, Cell Map, Merge Lands, Lint, Cell Preview). It is "
            "picked up by itself whenever the page exports; this reads it again.",
            pad=_LEAD_PAD,
            style="Accent.TButton",
            width=_LEAD_COL2_WIDTH,
        )
        # Run (Sort/Export) | Analyse (the read-only scans).
        group_separator(row1)
        self.conflicts_button = _action_button(
            row1,
            "Check Conflicts",
            lambda: self.on_open_conflicts("records"),
            "Open the Conflicts page in the viewer on the records two or more plugins of the "
            "order define (the last wins): each record's fields across its plugins, coloured "
            "by what each plugin does to them, the patch steps beside them, the same scan by "
            "plugin (merge a plugin's fields in one go), and the map. "
            "Read-only until you queue something for the patch. Runs after a Sort.",
            state="disabled",
        )
        # The two conflict scans sit together; the Cell Map (a different kind of
        # view) follows them.
        self.resource_button = _action_button(
            row1,
            "Resource Conflicts",
            lambda: self.on_open_conflicts("files"),
            "Open the Conflicts page in the viewer on the loose files (meshes, textures, "
            "scripts...) two or more data folders provide. In OpenMW the LATER data folder "
            "wins, so reorder the data paths to change the winner. Meshes that lose collision "
            "or animation are marked; a texture compares with what it replaced.",
            state="disabled",
        )
        self.cellmap_button = _action_button(
            row1,
            "Cell Map",
            self.on_cell_map,
            "Build a 'modmapper'-style cell map from the sorted, enabled plugins: an "
            "exterior-cell SVG heatmap (brighter = more mods; click a cell to jump to its "
            "list entry) plus exterior/interior cell lists, showing which mods touch which "
            "cells (your custom ones get a gold outline). A 'Focus on mod' dropdown dims "
            "everything one mod doesn't touch and lists its co-editors. The map is written "
            "to a timestamped cell_map file "
            "and shown in an in-app window if pywebview or tkinterweb is installed, otherwise "
            "in your browser. Read-only.",
            state="disabled",
        )
        self.cellpreview_button = _action_button(
            row1,
            "Cell Preview",
            self.on_cell_preview,
            "Place a cell's objects in the 3D mesh viewer. Pick an interior cell "
            "by name or an exterior cell by grid; every reference is resolved to its winning "
            "object and world position across the sorted load order, the placed meshes are "
            "drawn (textured), and an audit prints what the load order does to the "
            "cell -- references a later plugin overrode, deleted or moved, and any meshes not "
            "found. Exterior cells also draw the terrain with its blended landscape textures, "
            "animated water, and the eight neighboring cells behind a toggle, under a Morrowind "
            "sky with time-of-day and weather controls; fly the camera with WASD. Read-only.",
            state="disabled",
        )

        self.status_var = tk.StringVar(value="Ready.")
        ttk.Label(row1, textvariable=self.status_var, foreground=DARK["fg_dim"], anchor="e").pack(
            side="right", padx=(12, 2)
        )

    def _build_tool_actions(self, row2: tk.Misc) -> None:
        """Build the second button row: plugin manipulation and file creation.

        The first row is the conflict/scan actions; this row is the tools that
        change or create files -- Merge Lands writes a merged plugin, Merge
        Settings writes a '.mergedlands.toml', tes3cmd cleans, Lint reads the
        sorted list. Merge Lands and Lint need a computed order and so start
        disabled; the rest work straight from files.

        Args:
            row2: The frame holding the second row.
        """
        self.mergedlands_button = _action_button(
            row2,
            "Merge Lands",
            self.on_merged_lands,
            "Build a 'Merged Lands.esp' from the sorted, enabled plugins. Where two mods "
            "reshape different vertices of the same exterior cell, a load order keeps only "
            "one and discards the other; this recovers both. Seams between cells are "
            "repaired and the terrain is conditioned so every height fits the format. "
            "WRITES A NEW PLUGIN into your Data Files -- your mods are never modified, and "
            "deleting the merged plugin restores your previous behaviour exactly. Load it "
            "LAST. Needs tes3conv.",
            state="disabled",
            pad=_LEAD_PAD,
            width=_LEAD_COL1_WIDTH,
        )
        self.merge_settings_button = _action_button(
            row2,
            "Merge Settings",
            self.on_create_merge_settings,
            "Write a '.mergedlands.toml' beside a plugin you pick, controlling how Merge "
            "Lands treats it: drop a layer (included = false) or force a conflict strategy "
            "(Overwrite/Ignore/Resolve/Curvature) per layer. Created with every option "
            "spelled out at its default for you to edit -- a plugin needs one only when a "
            "conflict makes it necessary. This is NOT the record patch maker in the "
            "conflict diff (which writes a patch plugin); it only writes this one settings "
            "file and never touches the plugin itself.",
            pad=_LEAD_PAD,
            width=_LEAD_COL2_WIDTH,
        )
        # Build (writes a plugin / settings file) | Checks (read-only) | Tools
        # (external tes3cmd, and backup management).
        group_separator(row2)
        self.lint_button = _action_button(
            row2,
            "Lint",
            lambda: self.on_open_tools("lint"),
            "Open the Tools page in the viewer on Lint: evil GMSTs (name AND value must match "
            "the known 72), the interior fog-density-0 'black void' bug, new interior cells "
            "with no pathgrid anywhere in the load order, expansion-function use without the "
            "expansion mastered, omwaddon/omwscripts twin mismatches, and your mods with "
            "blank author/description. Read-only.",
            state="disabled",
        )
        self.savecheck_button = _action_button(
            row2,
            "Save Check",
            lambda: self.on_open_tools("save"),
            "Open the Tools page in the viewer on Save Check: pick an OpenMW .omwsave and "
            "verify every content file it depends on is in the load order (OpenMW refuses a "
            "save whose plugins are missing), and see how much space each mod takes in it "
            "and how to shrink it; or compare two saves. Read-only.",
        )
        group_separator(row2)
        self.tes3cmd_button = _action_button(
            row2,
            "tes3cmd",
            lambda: self.on_open_tools("tes3cmd"),
            "Frontend for tes3cmd (distributed with the MOMW Tools Pack): clean plugins "
            "(staged with their masters so tes3cmd sees the full VFS), resync master "
            "sizes in-app ([MASTER SIZE] notes), or view headers. Uses the compiled "
            "tes3cmd.exe; the pure-perl script also works if perl is installed. "
            "Modifying commands keep backups. (No multipatch: OpenMW setups use "
            "delta-plugin for merged lists.)",
        )
        self.remove_master_button = _action_button(
            row2,
            "Remove Master",
            lambda: self.on_open_tools("masters"),
            "Open the Tools page in the viewer: strip a master from a plugin's header (references "
            "into it are dropped, later "
            "master indices shift). The fix for a plugin OpenMW won't load because it "
            "lists a master that's been renamed, merged into another file, or replaced by "
            "loose files / a pluginless version. Keeps a one-time .premaster.bak.",
        )
        self.backups_button = _action_button(
            row2,
            "Backups",
            lambda: self.on_open_tools("backups"),
            "Open the Tools page in the viewer on Backups: every backup left behind by this "
            "tool, tes3cmd and the Configurator (.preclean.bak, .masterfix.bak, .premaster.bak, "
            "name~1.esp, timestamped .bak / .backup copies), with restore and delete.",
        )

    def _build_log(self, log_container: tk.Misc) -> None:
        log_container.columnconfigure(0, weight=1)
        log_container.rowconfigure(0, weight=1)

        log_frame = ttk.LabelFrame(log_container, text=_("Log"))
        log_frame.grid(row=0, column=0, sticky="nsew")
        log_frame.rowconfigure(0, weight=1)
        log_frame.columnconfigure(0, weight=1)

        self.log_text = tk.Text(
            log_frame,
            wrap="word",
            font=("TkFixedFont", 10),
            state="disabled",
            background=DARK["log_bg"],
            foreground=DARK["fg"],
            insertbackground=DARK["fg"],
            selectbackground=DARK["select"],
            relief="flat",
            borderwidth=0,
            highlightbackground=DARK["border"],
            highlightthickness=1,
        )
        self.log_text.grid(row=0, column=0, sticky="nsew", padx=(6, 0), pady=6)

        log_scroll = ttk.Scrollbar(
            log_frame,
            orient="vertical",
            command=self.log_text.yview,
        )
        log_scroll.grid(row=0, column=1, sticky="ns", padx=(0, 6), pady=6)

        self.log_text.configure(yscrollcommand=log_scroll.set)
        add_tooltip(
            self.log_text,
            _(
                "Full output from the last Sort and/or Export. Color key: green = a plugin/path "
                "this sort inserted or moved, orange = a heads-up (mlox warning, or a rule your "
                "curated cfg order overrode), blue = a section header, bright red = an error worth "
                "checking. Plain text = frozen base rows left untouched. Colors follow the "
                "syntax highlighting theme picked below."
            ),
        )
        # tags get their real colors from _apply_log_theme once the theme is
        # known (see __init__) -- configure with placeholders now so nothing
        # is left unconfigured if something logs before that call
        for tag, cfg in self._log_tag_style(THEME_PRESETS["Dark (default)"]).items():
            self.log_text.tag_configure(tag, **cfg)

        log_btns = ttk.Frame(log_frame)
        log_btns.grid(row=1, column=0, sticky="ew", padx=6, pady=(0, 6))
        clear_btn = ttk.Button(log_btns, text=_("Clear Log"), command=self.clear_log)
        clear_btn.pack(side="left")
        add_tooltip(
            clear_btn, _("Clear the log panel. Doesn't affect the order panels or any files.")
        )
        save_btn = ttk.Button(log_btns, text=_("Save Log As..."), command=self.save_log)
        save_btn.pack(side="left", padx=(8, 0))
        add_tooltip(save_btn, _("Save the current log contents to a text file."))

        ttk.Label(log_btns, text=_("Theme:")).pack(side="left", padx=(16, 4))
        self.theme_combo = ttk.Combobox(
            log_btns,
            textvariable=self.log_theme_var,
            state="readonly",
            width=18,
            values=self._theme_names(),
        )
        self.theme_combo.pack(side="left")
        self.theme_combo.bind(
            "<<ComboboxSelected>>", lambda e: self._apply_log_theme(self.log_theme_var.get())
        )
        add_tooltip(
            self.theme_combo,
            _(
                "Color theme for the whole GUI: window/button colors, syntax "
                "highlighting here in the Log panel, and the field-diff JSON viewer "
                "(Check Conflicts -> double-click a field). Switching re-themes "
                "everything immediately, including any windows already open. Built-in: "
                "Dracula, Monokai, Atom One Dark, Gruvbox Dark, plus anything you've "
                "imported."
            ),
        )
        import_theme_btn = ttk.Button(
            log_btns, text=_("Import Theme..."), command=self._import_log_theme
        )
        import_theme_btn.pack(side="left", padx=(8, 0))
        add_tooltip(
            import_theme_btn,
            _(
                "Import a custom theme from a JSON file (background/foreground/select/"
                "section/warn/error/ok/inserted/dim as hex colors, plus an optional "
                '"chrome" object for explicit window/button colors) or a base16 scheme '
                "file (.yaml/.yml/.json with base00..base0F -- e.g. from the atelierbram/"
                "syntax-highlighting or chriskempson/base16 scheme repos). Window colors "
                "not given explicitly are derived from the background. Imported themes are "
                "saved and appear in the dropdown from then on."
            ),
        )

        self.help_btn = ttk.Button(log_btns, text=_("Help"), command=self.on_help)
        self.help_btn.pack(side="left", padx=(4, 0))
        add_tooltip(
            self.help_btn,
            _(
                "Open the program's own documentation -- the Quick start (what to click, in "
                "order) or the Read me (every option, in detail) -- rendered as a readable "
                "page with a contents sidebar. Works offline; nothing is downloaded."
            ),
        )

    # -- log panel syntax highlighting themes --------------------------------

    def _custom_themes_file(self) -> Path:
        return app_base_dir() / "log_themes.json"

    def _load_custom_log_themes(self) -> None:
        try:
            raw = json.loads(self._custom_themes_file().read_text(encoding="utf-8"))
        except (OSError, ValueError):  # unreadable file / bad JSON
            return
        if not isinstance(raw, dict):
            return
        for name, theme in raw.items():
            try:
                for field in _THEME_REQUIRED:
                    theme[field] = _normalize_hex(theme[field])
                self._custom_log_themes[str(name)] = theme
            except (KeyError, TypeError, ValueError):  # missing key / non-dict entry / bad hex
                continue  # skip a corrupted entry rather than lose the rest

    def _save_custom_log_themes(self) -> None:
        try:
            self._custom_themes_file().write_text(
                json.dumps(self._custom_log_themes, indent=2), encoding="utf-8"
            )
        except (OSError, TypeError, ValueError):  # unwritable path / unserialisable value
            pass

    def _theme_names(self) -> list[str]:
        return list(THEME_PRESETS.keys()) + sorted(self._custom_log_themes.keys())

    def _resolve_theme(self, name: str) -> dict | None:
        return THEME_PRESETS.get(name) or self._custom_log_themes.get(name)

    def _apply_log_theme(self, name: str, announce: bool = True) -> None:
        theme = self._resolve_theme(name)
        if theme is None:
            name = "Dark (default)"
            theme = THEME_PRESETS[name]
            self.log_theme_var.set(name)
        # keep the chrome palette in step with the syntax theme, then re-theme
        # every live widget (the main window and all open Toplevels)
        set_active_chrome(theme)
        trace_first_fire("theme -> chrome palette update (set_active_chrome)")
        # per-switch (not just first-fire): theme changes are rare and
        # user-initiated, and the name identifies *which* palette is active
        trace(f"[theme] chrome palette now follows: {name}")
        # never let a re-apply problem (e.g. a platform/build-specific Tk quirk)
        # stop the log theme itself from applying below
        try:
            self._reapply_chrome()
        except Exception:  # noqa: BLE001
            # diagnostic guard: must not raise onward
            trace("[theme] re-apply pass failed:\n" + traceback.format_exc())
        log_text = getattr(self, "log_text", None)
        if log_text is None or not log_text.winfo_exists():
            return
        log_text.configure(
            background=theme["background"],
            foreground=theme["foreground"],
            insertbackground=theme["foreground"],
            selectbackground=theme["select"],
        )
        for tag, cfg in self._log_tag_style(theme).items():
            log_text.tag_configure(tag, **cfg)
        if announce:
            self.status_var.set(_("Log syntax highlighting: %(name)s") % {"name": name})

    def _reapply_chrome(self) -> None:
        """Re-theme the live GUI after the chrome palette changed.

        Three passes, because three different mechanisms own the colors:
        1. apply_dark_theme() re-configures the (live) ttk.Style and option
           database, which instantly restyles every ttk widget everywhere.
        2. restyle_widget_tree() walks the real widget tree from the main
           window -- open Toplevels are its children, so dialogs, the tes3cmd
           window etc. are reached -- fixing the plain-tk widgets that
           ttk.Style can't touch.
        3. The reorder panels' per-row itemconfig colors are re-applied.
        """
        apply_dark_theme(self.root)
        count = restyle_widget_tree(self.root)
        for panel in (getattr(self, "order_panel", None), getattr(self, "data_order_panel", None)):
            if panel is not None:
                try:
                    panel._restyle()
                except tk.TclError:
                    pass
        trace_first_fire("theme runtime re-apply walk (restyle_widget_tree)")
        trace(f"[theme] re-applied chrome to {count} plain-tk widgets (ttk covered by Style)")

    def _import_log_theme(self) -> None:
        path = filedialog.askopenfilename(
            title=_("Import syntax highlighting theme"),
            filetypes=case_insensitive_filetypes(
                (("Theme files", "*.json *.yaml *.yml"), ("All files", "*.*"))
            ),
        )
        if not path:
            return
        try:
            name, theme = parse_theme_file(path)
        except (OSError, ValueError) as e:
            # parse_theme_file's documented contract, plus read errors
            messagebox.showerror(
                _("Import failed"), _("Couldn't import that theme:\n\n%(error)s") % {"error": e}
            )
            return
        base_name, existing = name, self._theme_names()
        n = 2
        while name in existing:
            name = f"{base_name} ({n})"
            n += 1
        self._custom_log_themes[name] = theme
        self._save_custom_log_themes()
        self.theme_combo.configure(values=self._theme_names())
        self.log_theme_var.set(name)
        self._apply_log_theme(name, announce=False)
        self.status_var.set(_("Imported and applied theme: %(name)s") % {"name": name})
        messagebox.showinfo(
            _("Theme imported"),
            _('Imported "%(name)s" and set it as the active log theme.') % {"name": name},
        )

    # -- log handling --------------------------------------------------------

    def _tag_for_line(self, line: str) -> str:
        stripped = line.strip()
        if stripped.startswith("=" * 5) or (
            stripped
            and stripped == stripped.upper()
            and any(c.isalpha() for c in stripped)
            and len(stripped) < 70
            and not stripped.startswith("[")
        ):
            return "section"
        # [CONFLICT] and an internal drift warning are active problems worth
        # flagging in bright red; [REQUIRES]/generic WARNING:/NOTE: are
        # milder heads-ups and stay orange
        if "[CONFLICT]" in line or "INTERNAL WARNING" in line:
            return "error"
        if "MISMATCH:" in line or "PREVIEW ABORTED" in line:
            return "error"
        if "VERIFIED:" in line:
            return "ok"
        # a missing or out-of-order master breaks the game at launch -- red
        if "[MISSING MASTER]" in line or "[MASTER ORDER]" in line or "[MISSING SAVE DEP]" in line:
            return "error"
        if any(k in line for k in ("Traceback", "ERROR", "Error:")):
            return "error"
        if any(
            k in line
            for k in (
                "[REQUIRES]",
                "[NOTE]",
                "WARNING:",
                "NOTE:",
                "[MASTER SIZE]",
                "[FOGBUG]",
                "[EVLGMST]",
                "[NO PATHGRID]",
                "[HEADER]",
                "[EXP-DEP]",
                "[TWIN]",
                "[STALE]",
                "[REDUNDANT]",
                "[ORPHAN]",
                "[NEEDS CLEANING]",
                "[LIST ORDER]",
                # skipped-rule summary (mlox order overridden by the curated cfg)
                "ordering rule(s) not applied",
                "mlox wanted",
            )
        ):
            return "warn"
        if line.startswith("* ["):  # conflict report line involving your custom mods
            return "warn"
        if "<-- inserted" in line:  # 'content=X  <-- inserted/moved' / 'data=...  <-- inserted'
            return "inserted"
        if any(k in line for k in ("Wrote ", "written:    yes")):
            return "ok"
        if line.startswith("---"):
            return "dim"
        return ""

    def _append_log(self, text: str) -> None:
        self.log_text.configure(state="normal")
        for line in text.splitlines(keepends=True):
            stripped = line.strip("\n")
            if not stripped:
                self._log_group_tag = None
                self.log_text.insert("end", line)
                continue
            if line.startswith("    ") and self._log_group_tag:
                # indented continuation line (e.g. "Needed by:", "Caused by:")
                # -- inherit the color of the warning it belongs to
                tag = self._log_group_tag
            else:
                tag = self._tag_for_line(line)
                if tag in ("warn", "error"):
                    self._log_group_tag = tag
            self.log_text.insert("end", line, (tag,) if tag else ())
        self.log_text.see("end")
        self.log_text.configure(state="disabled")

    def _poll_log_queue(self) -> None:
        drained = []
        try:
            while True:
                drained.append(self.log_queue.get_nowait())
        except queue.Empty:
            pass
        if drained:
            self._append_log("".join(drained))
        self.root.after(80, self._poll_log_queue)

    def clear_log(self) -> None:
        """Empty the log panel."""
        self.log_text.configure(state="normal")
        self.log_text.delete("1.0", "end")
        self.log_text.configure(state="disabled")
        self._log_group_tag = None

    def save_log(self) -> None:
        """Write the log panel's contents to a file the user picks."""
        path = filedialog.asksaveasfilename(
            defaultextension=".log",
            filetypes=case_insensitive_filetypes((("Log files", "*.log"), ("All files", "*.*"))),
        )
        if not path:
            return
        Path(path).write_text(self.log_text.get("1.0", "end"), encoding="utf-8")

    # -- run -----------------------------------------------------------------

    # -- manual subset additions (folders / plugins the scan missed) --------

    # -- the Load order page (the viewer) ------------------------------------

    def _viewer_plan_file(self) -> Path:
        """Where the Load order page writes the order on Export, for the tools here."""
        return app_base_dir() / "wraithguard_loadorder_plan.json"

    def on_open_load_order(self) -> None:
        """Open the viewer on its Load order page (Sort and Export happen there)."""
        if not self._open_cell_viewer(loadorder=True):
            messagebox.showerror(
                _("Load order"),
                _("The viewer (wraithguard-viewer) could not be started."),
            )

    def on_open_conflicts(self, tab: str) -> None:
        """Open the viewer on its Conflicts page, on ``tab`` (``records``, ``files``, ``plugins``)."""
        if not self._open_cell_viewer(conflicts=tab):
            messagebox.showerror(
                _("Conflicts"),
                _("The viewer (wraithguard-viewer) could not be started."),
            )

    def on_open_tools(self, tool: str) -> None:
        """Open the viewer on its Tools page, on ``tool`` (``lint``, ...)."""
        if not self._open_cell_viewer(tools=tool):
            messagebox.showerror(
                _("Tools"),
                _("The viewer (wraithguard-viewer) could not be started."),
            )

    def on_take_viewer_plan(self) -> None:
        """Load the order the Load order page last exported into the panels."""
        path = self._viewer_plan_file()
        if not path.is_file():
            messagebox.showinfo(
                _("Load order"),
                _("No order yet: open the Load order page, Sort, then Export."),
            )
            return
        self._take_viewer_plan(path)

    def _take_viewer_plan(self, path: Path) -> None:
        """Read the page's plan file into a plan the tools here work from."""
        import json

        try:
            data = json.loads(path.read_text(encoding="utf-8"))
            plan = self._plan_from_viewer(data)
        except (OSError, ValueError, KeyError, TypeError) as exc:
            self.status_var.set(
                _("Could not read the Load order page's order: %(error)s") % {"error": exc}
            )
            return
        self._viewer_plan_mtime = path.stat().st_mtime_ns
        if plan.get("cfg") and not self.cfg_var.get().strip():
            self.cfg_var.set(plan["cfg"])
        self._sort_finished(
            plan,
            _("Took the Load order page's order: %(count)d plugin(s).")
            % {"count": len(plan.get("final_order") or [])},
        )

    @staticmethod
    def _plan_from_viewer(data: Mapping[str, Any]) -> dict[str, Any]:
        """The plan the tools here read, from the Load order page's plan file.

        The cfg's own lines and positions are read again, so Cell Preview renders the setup
        as it does after a sort here.
        """
        cfg = Path(str(data["cfg"]))
        lines, content_positions, content_order, data_positions, data_order = core.read_cfg(cfg)
        new = {str(x) for x in data.get("newDataLines") or []}
        return {
            "cfg": str(cfg),
            "subset": list(data.get("subset") or []),
            "lines": lines,
            "content_positions": content_positions,
            "data_positions": data_positions,
            "data_order": data_order,
            "final_order": list(data["finalOrder"]),
            "data_result": [(line, line in new, None) for line in data.get("dataLines") or []],
            "base_order_names": [name for name, _line in content_order],
            "master_problem_plugins": list(data.get("masterProblems") or []),
            "orphan_data_paths": list(data.get("orphanData") or []),
            "data_inserts": [
                {"value": v, "after": None, "before": None}
                for v in data.get("customDataDirs") or []
            ],
            "raw_toml_data_inserts": [],
            "new_groundcover": list(data.get("newGroundcover") or []),
            "scan_dirs": list(data.get("scanDirs") or []),
            "custom_data_dirs": list(data.get("customDataDirs") or []),
            "predicate_warnings": [],
            "yml_warnings": [],
            "conflicts": [],
        }

    def _watch_viewer_plan(self) -> None:
        """Take the page's order whenever its Export writes a new one."""
        try:
            path = self._viewer_plan_file()
            if path.is_file() and not self.worker_running:
                mtime = path.stat().st_mtime_ns
                if mtime != getattr(self, "_viewer_plan_mtime", None):
                    self._take_viewer_plan(path)
        except OSError:
            pass
        self.root.after(2000, self._watch_viewer_plan)

    def _cfg_snapshot(self) -> tuple[int, int] | None:
        """(mtime_ns, size) of the cfg file, for drift detection."""
        try:
            st = Path(self.cfg_var.get().strip()).stat()
            return (st.st_mtime_ns, st.st_size)
        except OSError:
            return None

    def _sort_finished(self, plan: dict | None, status: str) -> None:
        self.worker_running = False
        self.sort_button.configure(state="normal")
        self.status_var.set(status)
        self._current_plan = plan
        self._cfg_at_sort = self._cfg_snapshot()  # detect drift before Export
        # carry the previous opt-outs forward across a re-Sort
        prev_disabled_p = self.order_panel.get_disabled()
        prev_disabled_d = self.data_order_panel.get_disabled()
        final_order = (plan or {}).get("final_order") or []
        # An empty sort (nothing in the subset) still returns the current load
        # order so it can be edited here -- e.g. to remove entries and Export --
        # rather than leaving a blank panel with nothing to act on.
        panel_order = final_order or (plan or {}).get("base_order_names") or []
        self.order_panel.load(
            panel_order, (plan or {}).get("subset") or [], disabled_items=prev_disabled_p
        )
        # plugins with a missing / mis-ordered master render bright red
        self.order_panel.set_errors((plan or {}).get("master_problem_plugins") or [])

        data_result = (plan or {}).get("data_result") or []
        data_lines = [line for line, _, _ in data_result]
        # On an empty sort (no plugins to place) the run manages no data= paths
        # either, so fall back to the cfg's existing data= lines -- exactly as the
        # plugin panel falls back to the base order -- so they can be edited
        # (removed) here too. Nothing is highlighted: none of them are ours.
        if not data_lines and not final_order:
            data_lines = list((plan or {}).get("data_order") or [])
        # Highlight every data= path that's OURS -- both genuinely new inserts
        # AND ones that are already in openmw.cfg because a prior
        # momw-configurator run baked them in (e.g. SetBonus/SkillFramework
        # added via the customizations TOML). Gating on is_new alone would leave
        # those un-highlighted even though they're part of what this sort
        # manages; matching them against this run's data-path inputs mirrors how
        # the plugin panel highlights all subset plugins, not just brand-new ones.
        user_norms = {
            normalize_data_path(d["value"]) for d in ((plan or {}).get("data_inserts") or [])
        }
        # Orphan data= paths pulled from the cfg (Pull unmanaged (orphan)...) are
        # ours to manage too, so highlight them alongside the declared inserts.
        # They live in data_order, not data_result (never re-inserted), so the
        # highlight is computed over data_lines below rather than data_result.
        user_norms |= {
            normalize_data_path(v) for v in ((plan or {}).get("orphan_data_paths") or [])
        }
        user_norms.discard("")
        new_norms = {
            normalize_data_path(extract_data_path_value(line) or "")
            for line, is_new, _ in data_result
            if is_new
        }
        new_norms.discard("")

        def _is_ours(line: str) -> bool:
            p = normalize_data_path(extract_data_path_value(line) or "")
            return bool(p) and (p in new_norms or p in user_norms)

        highlight_lines = [line for line in data_lines if _is_ours(line)]
        self.data_order_panel.load(data_lines, highlight_lines, disabled_items=prev_disabled_d)

        # If there's a load order or data= paths loaded (from a real sort OR an
        # empty sort's fallback), every analysis action should be available --
        # they all run over whatever the panels currently hold, so a fresh sort
        # is not a precondition; the same holds for an order taken from the page, which
        # re-enables them whenever a plan exists.
        have_data = bool(panel_order or data_lines)
        self.export_button.configure(state="normal")
        self.conflicts_button.configure(state="normal" if have_data else "disabled")
        self.cellmap_button.configure(state="normal" if have_data else "disabled")
        self.cellpreview_button.configure(state="normal" if have_data else "disabled")
        self.mergedlands_button.configure(state="normal" if have_data else "disabled")
        self.resource_button.configure(state="normal" if have_data else "disabled")
        self.lint_button.configure(state="normal" if have_data else "disabled")

    # -- conflict detection --------------------------------------------------

    def _apply_exclusions(self, order: Sequence[str]) -> list[str]:
        """Drop plugins matching the user's exclude patterns.

        From the Options field;
        logs what was skipped.
        """
        import re as _re

        pats = [p for p in _re.split(r"[,\n]", self.exclude_var.get()) if p.strip()]
        kept, excl = core.filter_plugins(order, pats)
        if excl:
            self.log_queue.put(
                f"\n  Excluded {len(excl)} plugin(s) by your filter: "
                + ", ".join(excl[:12])
                + (" ..." if len(excl) > 12 else "")
                + "\n"
            )
        return kept

    def _on_keep_json_toggle(self) -> None:
        """Return the folder the tes3conv JSON dump lives in.

        Always the same 'tes3conv_json' folder so it's
        reused within a run; this checkbox only decides whether it's KEPT on close
        or removed. Flipping it just updates the live session's keep flag.
        """
        keep = bool(self.keep_json_var.get())
        self._keep_json = keep
        s = getattr(self, "_session", None)
        if s is not None:
            s.keep = keep
        dest = self._tes3conv_json_dir()
        self.status_var.set(
            f"Keeping tes3conv JSON dump in: {dest}"
            if keep
            else f"tes3conv JSON dump ({dest}) will be removed on close."
        )

    def _get_session(self, conv: str | None) -> core.Tes3ConvSession | None:
        """Return the shared disk-backed conflict session, creating it if needed.

        The Rust backend (:class:`core.NativeEspSession`) whenever it is installed:
        it reads plugins in process and keeps each one's record keys and cells in
        memory while the file is unchanged, with no JSON on disk. ``conv`` (a
        tes3conv executable) is used only when the backend is missing. The session
        is reused across scans. Called from a worker thread; self._keep_json is
        snapshotted on the main thread.
        """
        keep = bool(getattr(self, "_keep_json", False))
        s = getattr(self, "_session", None)
        # The Rust backend when installed; tes3conv only without it. Reuse the
        # current session when it is the same engine.
        native = core.native_backend()
        same = (
            isinstance(s, core.NativeEspSession)
            if native
            else (not isinstance(s, core.NativeEspSession) and getattr(s, "exe", None) == conv)
        )
        if s is not None and same:
            s.keep = keep  # same dump folder -> just track keep
            return s
        if s is not None:  # engine changed -> retire the old one
            try:
                s.cleanup()
            except Exception:  # noqa: BLE001
                # retiring a replaced engine session; failure must not block the new one
                pass
        s = core.open_record_session(
            conv,
            dump_dir=str(self._tes3conv_json_dir()),
            keep=keep,
            disk_cache=record_cache(),
        )
        self._session = s
        return s

    def _ensure_conflict_session(self, conv: str | None = None) -> bool:
        """Make the field-diff data ready without a Check Conflicts run first.

        Establishes the shared tes3conv session and the plugin -> path map that
        the field diff, tree view, patch builder and plugin view all read. Both
        are cheap: the map is a last-wins path lookup per plugin
        (:func:`core.plugin_paths`), and the JSON itself is still generated
        lazily -- one plugin at a time, when a field is actually read -- so this
        opens nothing it does not have to. It exists so those features, and
        anything wanting field data after a Merge Lands run, do not depend on
        the conflict scan having populated the session as a side effect.

        Must be called on the UI thread: it reads the order panel and the cfg
        entry.

        Args:
            conv: The tes3conv path, or ``None`` to locate it from the cfg.

        Returns:
            ``True`` when a session and paths are available, ``False`` when
            tes3conv could not be found.
        """
        if conv is None:
            cfg = self.cfg_var.get().strip()
            cfg_dir = str(Path(cfg).parent) if cfg else None
            conv = core.find_tes3conv(explicit=self._tes3conv_override, extra_dirs=[cfg_dir])
        session = self._get_session(conv)
        if session is None:
            return False
        order = self._apply_exclusions(self.order_panel.get_enabled())
        dirs = [str(d) for d in (self._plan_scan_dirs() or [])]
        self._conf_paths = core.plugin_paths(order, PluginFileIndex(dirs))
        self._conf_session = session
        return True

    def _plan_scan_dirs(self) -> list[str]:
        """Return every folder the scans should search for this run.

        The cfg's data=
        dirs plus the pending custom data paths (scan / customizations TOML)
        that aren't in the cfg yet -- so Check Conflicts / Cell Map / Resource
        Conflicts can see your custom mods BEFORE the cfg is written.
        """
        plan = self._current_plan or {}
        return list(plan.get("scan_dirs") or [])

    # -- cell map (modmapper) ------------------------------------------------

    def on_cell_map(self) -> None:
        """Build the cell-coverage map, in a worker."""
        if self.worker_running or not self._current_plan:
            return
        order = self._apply_exclusions(self.order_panel.get_enabled())
        if not order:
            return
        dirs = self._plan_scan_dirs()
        subset = self._current_plan.get("subset") or []
        self._keep_json = self.keep_json_var.get()
        self.worker_running = True
        for b in (
            self.sort_button,
            self.export_button,
            self.conflicts_button,
            self.cellmap_button,
            self.cellpreview_button,
            self.mergedlands_button,
            self.resource_button,
        ):
            b.configure(state="disabled")
        self.status_var.set(_("Building cell map..."))
        threading.Thread(
            target=self._cellmap_worker, args=(order, dirs, subset), daemon=True
        ).start()

    # -- merged lands ----------------------------------------------------------

    def on_merged_lands(self) -> None:
        """Build a merged landscape plugin, after confirming the write."""
        if self.worker_running or not self._current_plan:
            return
        # The viewer's Tools page, Merge Lands (port step 4.4, landcore::service), on the
        # sorted setup; the run below stays for when the viewer cannot start, until Tk goes.
        if self._open_cell_viewer(tools="mergelands"):
            return
        order = self._apply_exclusions(self.order_panel.get_enabled())
        if not order:
            return

        cfg_dir = (
            str(Path(self.cfg_var.get().strip()).parent) if self.cfg_var.get().strip() else None
        )
        # The Rust backend reads the terrain and writes the merged plugin in process;
        # tes3conv is only used when that backend is not installed.
        conv = (
            None
            if core.native_backend()
            else core.find_tes3conv(explicit=self._tes3conv_override, extra_dirs=[cfg_dir])
        )

        folders = self._merged_lands_dirs()
        chosen = self._merged_lands_target(folders, order)
        if chosen is None:
            return

        # This is the only thing in the toolkit that writes a plugin, so the
        # confirmation states exactly what will and will not happen rather than
        # asking a vague "are you sure". Three answers rather than two, so the
        # folder can be changed on any run without hunting for a setting:
        # yes writes, no re-opens the browser, cancel abandons.
        while True:
            target = chosen / land_service.DEFAULT_NAME
            existing = (
                _("\n\nThis REPLACES the existing %(name)s.") % {"name": land_service.DEFAULT_NAME}
                if target.exists()
                else ""
            )
            answer = messagebox.askyesnocancel(
                _("Build Merged Lands.esp?"),
                _(
                    "A new plugin will be written to:\n%(path)s\n\n"
                    "It carries terrain only -- no references, objects or scripts -- so "
                    "everything placed in those cells stays where it is. Your mods are NOT "
                    "modified, and deleting the merged plugin restores your previous "
                    "behaviour completely.\n\n"
                    "Load it LAST, and back up your saves before playing with it the first "
                    "time.%(existing)s\n\n"
                    "Yes: write it here.   No: choose a different folder."
                )
                % {"path": target, "existing": existing},
            )
            if answer is None:
                return
            if answer:
                break
            picked = self._ask_merged_lands_folder(chosen, folders)
            if picked is None:
                return
            chosen = picked

        self.worker_running = True
        for b in (
            self.sort_button,
            self.export_button,
            self.conflicts_button,
            self.cellmap_button,
            self.cellpreview_button,
            self.mergedlands_button,
            self.resource_button,
        ):
            b.configure(state="disabled")
        # Read the Tk var here, on the UI thread; the worker runs off it and must
        # not touch a widget variable itself.
        self._merged_lands_verbose = bool(self.merged_lands_verbose_var.get())
        self.status_var.set(_("Merging land..."))
        threading.Thread(
            target=self._merged_lands_worker,
            args=(folders, order, conv, target),
            daemon=True,
        ).start()

    def _merged_lands_dirs(self) -> list[Path]:
        """Every data folder the plan scans, in search order.

        An OpenMW load order is composed from many ``data=`` directories -- one
        per mod is normal -- so a single folder is not enough to find the
        plugins in. Searching all of them is what lets a master installed
        somewhere other than Data Files be read at all.

        Returns:
            The existing scan directories, in order.
        """
        return [Path(directory) for directory in self._plan_scan_dirs() if Path(directory).is_dir()]

    def _merged_lands_target(self, folders: list[Path], order: list[str]) -> Path | None:
        """Settle where the merged plugin is written, asking if need be.

        The folder is remembered between runs, so the usual answer is one
        confirmation. It is still shown every time rather than assumed: this is
        the only thing in the toolkit that writes a plugin, and a merged file
        that quietly appears somewhere other than last time is worse than one
        extra click.

        Args:
            folders: The data folders being searched.
            order: The enabled plugins, in load order.

        Returns:
            The folder to write into, or ``None`` if the user backed out.
        """
        saved = Path(self._merged_lands_out) if self._merged_lands_out else None
        if saved is not None and not saved.is_dir():
            # The folder was remembered and has since gone. Say so rather than
            # silently writing somewhere else.
            messagebox.showinfo(
                _("Folder not found"),
                _("%(path)s no longer exists. Please choose where to write.") % {"path": saved},
            )
            saved = None
        target = saved or self._merged_lands_home(folders, order)
        if target is None:
            messagebox.showwarning(
                _("Data Files not found"),
                _(
                    "Could not work out which folder holds your plugins, so "
                    "there is no sensible place to put the merged plugin. "
                    "Use 'Merged Lands folder...' to choose one."
                ),
            )
            target = self._ask_merged_lands_folder(None, folders)
            if target is None:
                return None
        return target

    def _ask_merged_lands_folder(
        self, start: Path | None, folders: list[Path] | None = None
    ) -> Path | None:
        """Ask for the output folder and remember it.

        Args:
            start: Where the browser should open, if anywhere.
            folders: The data folders the game actually reads. Used only to
                warn when the chosen folder is not one of them.

        Returns:
            The chosen folder, or ``None`` if the dialog was dismissed.
        """
        picked = filedialog.askdirectory(
            title=_("Where should Merged Lands.esp be written?"),
            initialdir=str(start) if start else None,
            mustexist=True,
        )
        if not picked:
            return None
        chosen = Path(picked)
        self._warn_if_not_a_data_path(chosen, folders)
        self._merged_lands_out = str(chosen)
        self._save_settings()
        return chosen

    def _warn_if_not_a_data_path(self, chosen: Path, folders: list[Path] | None) -> None:
        """Say so when the game will not look in the chosen folder.

        A plugin outside every ``data=`` path is invisible to OpenMW: the merge
        succeeds, the file is written, and nothing happens in game. That is a
        confusing failure to debug, and the fix is one line of ``openmw.cfg``,
        so it is worth saying at the moment the folder is chosen.

        Told rather than refused: a perfectly reasonable order of operations is
        to make the folder, point the toolkit at it, and add the ``data=`` line
        afterwards.

        Args:
            chosen: The folder picked.
            folders: The data folders being searched, if known.
        """
        if not folders:
            return
        try:
            resolved = chosen.resolve()
            known = {folder.resolve() for folder in folders}
        except OSError:
            return
        if resolved in known:
            return
        messagebox.showinfo(
            _("Not a data folder"),
            _(
                "%(path)s is not one of the folders OpenMW reads, so it will not "
                "find the merged plugin there.\n\n"
                'Add this line to openmw.cfg:\n\ndata="%(path)s"\n\n'
                "and put 'content=%(name)s' last."
            )
            % {"path": resolved, "name": land_service.DEFAULT_NAME},
        )

    def _merged_lands_home(self, folders: list[Path], order: list[str]) -> Path | None:
        """Choose where the merged plugin should be written.

        Args:
            folders: The data folders being searched.
            order: The enabled plugins, in load order.

        Returns:
            The folder holding the most of the load order, or ``None`` when no
            folder holds any of it. The merged plugin goes where the bulk of
            the game lives rather than in whichever mod folder sorted first.
        """
        best: Path | None = None
        best_count = 0
        for folder in folders:
            count = sum(1 for name in order if (folder / name).is_file())
            if count > best_count:
                best, best_count = folder, count
        return best

    def _merged_lands_worker(
        self, data_files: list[Path], order: list[str], converter: str | None, target: Path
    ) -> None:
        """Run the merge off the UI thread."""
        writer = QueueWriter(self.log_queue)
        result = None
        try:
            with redirect_stdout(writer.as_stream()), redirect_stderr(writer.as_stream()):
                print("\n" + "=" * 70)
                print(_(" MERGE LANDS"))
                print("=" * 70)
                result = land_service.build_merged_lands(
                    data_files=data_files,
                    load_order=order,
                    converter=converter,
                    output=target,
                    # A tes3conv session's record-key sidecars say which plugins
                    # have terrain, so the ~90% that do not are never converted.
                    # The native reader writes none and needs none.
                    sidecars=None if converter is None else self._tes3conv_json_dir(),
                    report=lambda line: print("  " + line),
                    verbose=getattr(self, "_merged_lands_verbose", False),
                )
            status = (
                _("Merge produced nothing to write.")
                if result.output is None
                else _("Merged Lands written: %(cells)d cell(s).") % {"cells": result.cells_written}
            )
        except land_service.MergeServiceError as exc:
            writer.write(f"\nMerge failed: {exc}\n")
            status = _("Merge failed -- see log.")
        except Exception:  # noqa: BLE001
            # worker top level: reports the traceback into the log panel
            writer.write("\nERROR: merge failed:\n" + traceback.format_exc())
            status = _("Merge failed -- see log.")
        finally:
            self._schedule_ui(0, self._merged_lands_finished, result, status)

    def _merged_lands_finished(self, result: object, status: str) -> None:
        """Re-enable the UI and report where the plugin went."""
        self.worker_running = False
        self.sort_button.configure(state="normal")
        for b in (
            self.export_button,
            self.conflicts_button,
            self.cellmap_button,
            self.cellpreview_button,
            self.mergedlands_button,
            self.resource_button,
        ):
            b.configure(state="normal" if self._current_plan else "disabled")
        self.status_var.set(status)
        output = getattr(result, "output", None)
        if output is not None:
            # The merge is done and its plugins are known; make the field-diff
            # data ready so inspecting a record does not need a separate Check
            # Conflicts run. The tes3conv JSON is still spooled lazily, per
            # plugin, the first time a field is actually read.
            self._ensure_conflict_session()
        if output is not None:
            # The later plugin wins a contested vertex by default (Overwrite), so
            # "settled far from intent" -- a blend outcome -- is only meaningful
            # for the vertices a .mergedlands.toml asked to Resolve. Surface it
            # only when there were any, rather than reporting "0 settled far" as
            # though blending had run.
            major = getattr(result, "major", 0)
            detail = _(
                "%(contested)d vertex/vertices were edited by more than one mod; "
                "the later one in your load order won each."
            ) % {"contested": getattr(result, "contested", 0)}
            if major:
                detail += " " + _(
                    "%(major)d were blended far from a mod's intent where a "
                    ".mergedlands.toml asked to Resolve."
                ) % {"major": major}
            messagebox.showinfo(
                _("Merged Lands built"),
                _(
                    "Written to:\n%(path)s\n\n"
                    "Enable it and place it LAST in your load order.\n\n"
                    "%(cells)d land record(s), %(textures)d land texture(s).\n%(detail)s"
                )
                % {
                    "path": output,
                    "cells": getattr(result, "cells_written", 0),
                    "textures": getattr(result, "textures_written", 0),
                    "detail": detail,
                },
            )

    def on_create_merge_settings(self) -> None:
        """Open the ``.mergedlands.toml`` editor for a plugin the user picks.

        The per-plugin Merge Lands control file -- *not* the record patch maker
        in the conflict diff, which writes a patch plugin. Needs no sort: the
        user picks any plugin and then chooses, per layer, whether to include
        its edits and how to resolve conflicts, in a dialog. Merge Lands still
        runs without one, so this is offered rather than required.
        """
        # The viewer's Tools page, Merge settings (port step 4.4, landcore::meta); the dialog
        # below stays for when the viewer cannot start, until Tk goes (4.5).
        if self._open_cell_viewer(tools="mergesettings"):
            return
        folders = self._merged_lands_dirs()
        picked = filedialog.askopenfilename(
            title=_("Pick a plugin to write a .mergedlands.toml for"),
            initialdir=str(folders[0]) if folders else "",
            filetypes=case_insensitive_filetypes(
                (
                    (_("Morrowind plugins"), "*.esp *.esm *.omwaddon *.omwgame"),
                    (_("All files"), "*.*"),
                )
            ),
        )
        if not picked:
            return
        self._open_merge_settings_editor(Path(picked))

    def _open_merge_settings_editor(self, plugin: Path) -> None:
        """Build the per-layer ``.mergedlands.toml`` editor for one plugin.

        Seeds the controls from an existing sidecar when there is one -- so this
        edits rather than blanks it -- and falls back to a fresh ``Patch`` with
        every layer at its permissive default.

        Args:
            plugin: The plugin the sidecar will sit beside.
        """
        base: land_meta.PluginMeta | None = None
        existing = land_meta.meta_path_for(plugin)
        if existing.is_file():
            try:
                base = land_meta.load_meta(plugin)
            except land_meta.MetaError as exc:
                messagebox.showwarning(
                    _("Existing .mergedlands.toml could not be read"),
                    _("%(name)s will be replaced from defaults:\n\n%(err)s")
                    % {"name": existing.name, "err": exc},
                )

        win = tk.Toplevel(self.root)
        apply_titlebar_theme(win)
        self._ms_win = win
        self._ms_plugin = plugin
        win.title(_("Merge settings: %(name)s") % {"name": plugin.name})
        win.configure(bg=DARK["bg"])
        win.geometry("720x640")
        win.minsize(620, 560)

        top = ttk.Frame(win, padding=10)
        top.pack(fill="both", expand=True)
        top.columnconfigure(0, weight=1)

        ttk.Label(
            top,
            text=_(
                "Choose how %(name)s's landscape edits merge. Uncheck a layer to keep this "
                "plugin from touching it; set a strategy for how its edits resolve where they "
                "collide with another mod. The defaults behave as if there were no file."
            )
            % {"name": plugin.name},
            foreground=DARK["fg_dim"],
            wraplength=680,
            justify="left",
        ).grid(row=0, column=0, sticky="w", pady=(0, 8))

        mt_row = ttk.Frame(top)
        mt_row.grid(row=1, column=0, sticky="w", pady=(0, 8))
        ttk.Label(mt_row, text=_("meta_type:")).pack(side="left")
        self._ms_meta_type = tk.StringVar(
            value=base.meta_type if base is not None else land_meta.META_PATCH
        )
        mt_combo = ttk.Combobox(
            mt_row,
            textvariable=self._ms_meta_type,
            values=list(land_meta.META_TYPES),
            state="readonly",
            width=14,
        )
        mt_combo.pack(side="left", padx=(6, 0))
        mt_combo.bind("<<ComboboxSelected>>", lambda _e: self._ms_refresh_preview())
        add_tooltip(
            mt_combo,
            _(
                "Patch = a hand-written settings file (the usual choice). Auto = default "
                "behaviour. MergedLands marks a plugin the tool generated so a re-run ignores "
                "it -- do not set that on an ordinary mod."
            ),
        )

        grid = ttk.LabelFrame(top, text=_("Layers"), padding=8)
        grid.grid(row=2, column=0, sticky="ew", pady=(0, 8))
        grid.columnconfigure(2, weight=1)
        ttk.Label(grid, text=_("include"), foreground=DARK["fg_dim"]).grid(row=0, column=0)
        ttk.Label(grid, text=_("layer"), foreground=DARK["fg_dim"]).grid(
            row=0, column=1, sticky="w"
        )
        ttk.Label(grid, text=_("conflict strategy"), foreground=DARK["fg_dim"]).grid(
            row=0, column=2, sticky="w", padx=(12, 0)
        )

        # Literal _() calls so the strings are extracted for translation; keyed
        # by the same names as land_meta.LAYER_NAMES.
        blurbs = {
            "height_map": _("Vertex heights -- and the normals stored with them."),
            "vertex_colors": _("Baked vertex colors (terrain lighting)."),
            "texture_indices": _("Which land texture paints each square."),
            "world_map_data": _("The low-resolution world-map heightmap."),
        }
        self._ms_included: dict[str, tk.BooleanVar] = {}
        self._ms_strategy: dict[str, tk.StringVar] = {}
        self._ms_strategy_combos: dict[str, ttk.Combobox] = {}
        for row, name in enumerate(land_meta.LAYER_NAMES, start=1):
            settings = base.settings_for(name) if base is not None else land_meta.MergeSettings()
            inc = tk.BooleanVar(value=settings.included)
            strat = tk.StringVar(value=land_meta.strategy_display_name(settings.conflict_strategy))
            self._ms_included[name] = inc
            self._ms_strategy[name] = strat
            ttk.Checkbutton(grid, variable=inc, command=partial(self._ms_on_include, name)).grid(
                row=row, column=0
            )
            label = ttk.Label(grid, text=name)
            label.grid(row=row, column=1, sticky="w")
            add_tooltip(label, blurbs.get(name, ""))
            combo = ttk.Combobox(
                grid,
                textvariable=strat,
                values=list(land_meta.STRATEGY_NAMES),
                state="readonly",
                width=12,
            )
            combo.grid(row=row, column=2, sticky="w", padx=(12, 0))
            combo.bind("<<ComboboxSelected>>", lambda _e: self._ms_refresh_preview())
            self._ms_strategy_combos[name] = combo

        ttk.Label(
            top,
            foreground=DARK["fg_dim"],
            wraplength=680,
            justify="left",
            text=_(
                "Auto = later plugin wins for entries it changed · Overwrite = this plugin "
                "wins collisions · Ignore = keep the earlier edit · Resolve = blend "
                "conflicting numbers · Curvature = structure-weighted blend (ours)."
            ),
        ).grid(row=3, column=0, sticky="w", pady=(0, 8))

        ttk.Label(top, text=_("Preview:"), foreground=DARK["fg_dim"]).grid(
            row=4, column=0, sticky="w"
        )
        preview = tk.Text(
            top,
            height=10,
            wrap="none",
            background=DARK["log_bg"],
            foreground=DARK["fg"],
            relief="flat",
            highlightthickness=1,
            highlightbackground=DARK["border"],
        )
        preview.grid(row=5, column=0, sticky="nsew")
        top.rowconfigure(5, weight=1)
        self._ms_preview = preview

        btns = ttk.Frame(top)
        btns.grid(row=6, column=0, sticky="e", pady=(8, 0))
        ttk.Button(btns, text=_("Cancel"), command=win.destroy).pack(side="right")
        ttk.Button(btns, text=_("Write .mergedlands.toml"), command=self._ms_write).pack(
            side="right", padx=(0, 8)
        )

        for name in land_meta.LAYER_NAMES:
            self._ms_on_include(name)  # sync each strategy dropdown's enabled state
        self._ms_refresh_preview()

    def _ms_on_include(self, name: str) -> None:
        """Enable a layer's strategy dropdown only while the layer is included.

        A dropped layer (``included = false``) contributes nothing, so there is
        no collision for a strategy to resolve; greying the dropdown says so.

        Args:
            name: The layer whose checkbox changed.
        """
        included = self._ms_included[name].get()
        self._ms_strategy_combos[name].configure(state="readonly" if included else "disabled")
        self._ms_refresh_preview()

    def _ms_build_meta(self) -> land_meta.PluginMeta:
        """Assemble a :class:`PluginMeta` from the current control values.

        Returns:
            The settings the dialog currently describes.
        """
        layers = {
            name: land_meta.MergeSettings(
                included=self._ms_included[name].get(),
                conflict_strategy=land_meta.strategy_from_name(self._ms_strategy[name].get()),
            )
            for name in land_meta.LAYER_NAMES
        }
        return land_meta.PluginMeta(meta_type=self._ms_meta_type.get(), layers=layers)

    def _ms_refresh_preview(self) -> None:
        """Redraw the read-only TOML preview from the current controls."""
        body = land_meta.render_meta(self._ms_build_meta(), explicit=True)
        self._ms_preview.configure(state="normal")
        self._ms_preview.delete("1.0", "end")
        self._ms_preview.insert("1.0", body)
        self._ms_preview.configure(state="disabled")

    def _ms_write(self) -> None:
        """Write the sidecar from the current controls and close the editor.

        The editor was seeded from any existing file, so writing over it is the
        intended outcome and needs no second confirmation.
        """
        try:
            written = land_meta.write_settings(self._ms_plugin, self._ms_build_meta())
        except land_meta.MetaError as exc:
            messagebox.showerror(_("Could not write .mergedlands.toml"), str(exc))
            return
        self.status_var.set(_("Wrote %(name)s.") % {"name": written.name})
        self._ms_win.destroy()
        messagebox.showinfo(
            _("Merge settings written"),
            _("Wrote:\n%(path)s\n\nRun Merge Lands to apply it.") % {"path": str(written)},
        )

    def _cellmap_file(self) -> Path:
        """Timestamped, writable location for the generated map.

        Named ``cell_map_<stamp>.html`` like the other generated views so
        exit-time housekeeping can prune old ones (newest few kept). A map the
        user *saves* keeps the plain ``cell_map.html`` name and is never a
        housekeeping candidate -- the un-timestamped file is one they asked for.
        """
        from datetime import datetime

        stamp = datetime.now().strftime("%Y%m%d_%H%M%S")  # noqa: DTZ005 - local clock
        return app_base_dir() / f"cell_map_{stamp}.html"

    def _cellmap_worker(self, order: list[str], dirs: list[str], subset: list[str]) -> None:
        writer = QueueWriter(self.log_queue)
        path = None
        trace(f"cell map: start, {len(order)} plugin(s)")
        try:
            with redirect_stdout(writer.as_stream()), redirect_stderr(writer.as_stream()):
                index = PluginFileIndex(dirs)
                cfg_dir = (
                    str(Path(self.cfg_var.get().strip()).parent)
                    if self.cfg_var.get().strip()
                    else None
                )
                conv = core.find_tes3conv(explicit=self._tes3conv_override, extra_dirs=[cfg_dir])
                session = self._get_session(conv)
                print("\n" + "=" * 70)
                print(_(" CELL MAP"))
                print("=" * 70)
                print(
                    _("  Engine: %(engine)s")
                    % {
                        "engine": (
                            _("native esp reader")
                            if isinstance(session, core.NativeEspSession)
                            else "tes3conv"
                        )
                    }
                )
                cov = core.build_cell_coverage(order, index, subset_names=subset, session=session)
                trace(
                    f"cell map: coverage built, {len(cov['exterior'])} ext, {len(cov['interior'])} int"
                )
                # The cell map answers coverage only. Conflicts are reached from
                # the Conflicts window, which builds the standalone conflict map
                # directly -- one conflict view, no duplicated generation path.
                html = core.generate_cell_map_html(cov)
                trace(f"cell map: html built, {len(html)} bytes")
                # Write straight to disk and drop the string -- the map is viewed
                # FROM the file (browser / tkinterweb load_file), never rendered
                # from an in-memory 2MB+ string (that path OOM'd tkhtmlview).
                path = str(self._cellmap_file())
                try:
                    Path(path).write_text(html, encoding="utf-8")
                except OSError:
                    # script dir not writable (e.g. a read-only install) -> temp
                    import tempfile

                    fd, path = tempfile.mkstemp(prefix="cell_map_", suffix=".html")
                    os.close(fd)
                    Path(path).write_text(html, encoding="utf-8")
                del html
                trace(f"cell map: written to {path}")
                print(
                    _(
                        "  %(exterior)d exterior + %(interior)d interior cell(s) "
                        "touched across %(scanned)d plugin(s)."
                    )
                    % {
                        "exterior": len(cov["exterior"]),
                        "interior": len(cov["interior"]),
                        "scanned": cov["scanned"],
                    }
                )
                print(_("  Map written to: %(path)s") % {"path": path})
            status = _("Cell map ready (%(exterior)d exterior, %(interior)d interior).") % {
                "exterior": len(cov["exterior"]),
                "interior": len(cov["interior"]),
            }
        except Exception:  # noqa: BLE001
            # worker top level: reports the traceback into the log panel
            writer.write("\nERROR: cell map failed:\n" + traceback.format_exc())
            status = "Cell map failed -- see log."
        finally:
            self._schedule_ui(0, self._cellmap_finished, path, status)

    def _cellmap_finished(self, path: str | None, status: str) -> None:
        self.worker_running = False
        self.sort_button.configure(state="normal")
        for b in (
            self.export_button,
            self.conflicts_button,
            self.cellmap_button,
            self.cellpreview_button,
            self.resource_button,
            self.lint_button,
        ):
            b.configure(state="normal" if self._current_plan else "disabled")
        self.status_var.set(status)
        if not path:
            return
        self._last_cell_file = path
        # Optional override: MLOX_MAP_VIEWER = pywebview | tkinterweb | browser.
        # ("pywebview" selects the embedded native viewer -- see HAVE_PYWEBVIEW.)
        # Handy when the viewer binary is present but broken on this machine --
        # force tkinterweb/browser instead.
        force = (os.environ.get("MLOX_MAP_VIEWER") or "").strip().lower()
        can_tkweb = HTMLViewer is not None and hasattr(HTMLViewer, "load_file")
        if force == "browser":
            trace("cell map: viewer = browser (forced)")
            self._open_cell_map_browser()
            return
        if force == "tkinterweb" and can_tkweb:
            trace("cell map: viewer = tkinterweb (forced)")
            self._show_cell_map_window(path)
            return
        if force == "pywebview" and HAVE_PYWEBVIEW:
            trace("cell map: viewer = pywebview (forced)")
            self._open_cell_map_embedded(path)
            return
        # Auto: prefer pywebview (real OS webview), then tkinterweb's load_file,
        # then the browser. tkhtmlview can't draw SVG, so it's never used here.
        if HAVE_PYWEBVIEW:
            trace("cell map: viewer = pywebview (embedded)")
            self._open_cell_map_embedded(path)
        elif can_tkweb:
            trace("cell map: viewer = tkinterweb (in-app window)")
            self._show_cell_map_window(path)
        else:
            trace("cell map: viewer = browser (no pywebview/tkinterweb available)")
            self._open_cell_map_browser()
            self.status_var.set(
                status + "  (opened in browser - pip install pywebview " "for an in-app window)"
            )

    def _open_cell_map_embedded(self, path: str | Path) -> None:
        """Open the cell map in pywebview, over loopback rather than ``file://``.

        The written page is served on ``127.0.0.1`` first and the webview is
        handed that URL -- exactly what :meth:`open_html_in_app` does for the
        other views. Some webviews (the Steam Deck's, various sandboxed ones)
        refuse a ``file://`` page, so serving it is what makes the embedded
        window reliable. Falls back to the file path if no loopback port binds.

        Args:
            path: The written cell-map HTML file.
        """
        url = self._serve_html_file(path)
        self._open_cell_map_pywebview(url or path, "Cell Map")

    def _open_cell_map_pywebview(self, path: str | Path, title: str = "Cell Map") -> None:
        """Show the map in an embedded native window (the Tauri viewer shell).

        Spawns ``wraithguard-viewer`` -- a small standalone binary, not a
        Python process -- pointed at the URL. Replaces the old pywebview
        child process (see git history: it re-invoked this script with
        ``--show-map`` and called ``webview.create_window``/``webview.start()``
        there). That approach needed its OWN process because pywebview's
        Qt/pythonnet backends aren't free-threading-safe and ``webview.start()``
        wants the main thread; this one needs its own process only because
        it's a different binary entirely. Nothing here touches a Python
        interpreter, so there's no GIL/free-threading concern to work around
        any more -- contrast with the removed ``PYTHON_GIL=1`` child_env line
        this function used to set.
        """
        # A URL is passed through untouched; only a real path is absolutised.
        # abspath() on "http://127.0.0.1:1/x" produces a nonsense local path,
        # which the child would then fail to open for reasons that look
        # nothing like the actual cause.
        ap = str(path) if is_view_url(path) else os.path.abspath(path)  # noqa: PTH100
        url = ap if is_view_url(ap) else view_uri(ap)
        # IMPORTANT: only CREATE_NO_WINDOW here (suppresses a console flash) --
        # do NOT use the SW_HIDE startupinfo from _no_window_kwargs(): that
        # STARTUPINFO is inherited by the child's FIRST window, which would
        # hide the viewer window itself (it spawns but never shows). That was
        # the bug with the old pywebview child; the same caution still
        # applies to this one.
        # (wraithguard.viewer_launch: where the viewer is and what it needs -- the
        # Flatpak's viewer, the bundled WebView2 runtime, CREATE_NO_WINDOW.)
        from wraithguard.viewer_launch import viewer_binary, viewer_popen_kwargs

        viewer_bin = viewer_binary()
        if viewer_bin is None:
            trace("cell map: viewer binary not found")
            self._open_cell_map_browser()
            return
        cmd = [viewer_bin, url, title]
        try:
            trace(f"cell map: launching viewer: {cmd}")
            proc = subprocess.Popen(cmd, **viewer_popen_kwargs())  # type: ignore[call-overload]
        except (OSError, ValueError):  # Popen: missing exe or bad argv
            trace("cell map: viewer launch FAILED:\n" + traceback.format_exc())
            self._open_cell_map_browser()
            return

        def _check_viewer_alive() -> None:
            # A real webview window stays open until the user closes it. Exiting
            # non-zero this fast means webview init failed on this machine --
            # most commonly WebView2 missing or deliberately removed -- so fall
            # back exactly the way viewer-shell's own main.rs comment assumes we
            # do. No prompt, no reinstall attempt: the browser fallback already
            # renders everything (three.js included) with no WebView2 involved,
            # so it is the right destination for that machine, not a downgrade.
            rc = proc.poll()
            if rc is not None and rc != 0:
                trace(f"cell map: viewer exited immediately (code {rc}) -- falling back")
                self._open_cell_map_browser()

        self._schedule_ui(700, _check_viewer_alive)

    def open_html_in_app(self, path: str | Path, title: str) -> None:
        """Show a generated HTML page in-app, falling back the same way the map does.

        The chain is pywebview (a real OS webview, so canvas and SVG both work)
        -> tkinterweb -> the browser. It mirrors the cell map's chain
        deliberately: those pages are read in the same sitting, and a view that
        opened somewhere else would be its own surprise.

        The browser is the last resort rather than the default, which is the
        whole point -- but it is still a *real* resort, because tkinterweb
        cannot run the canvas the terrain and nav views draw on.

        Args:
            path: The written HTML file, or a URL for a page that is served
                rather than written -- the 3D mesh viewer runs over loopback.
            title: Window title.
        """
        already_url = is_view_url(path)
        file_path = None if already_url else Path(path)
        self._last_view_file = str(path)
        can_tkweb = HTMLViewer is not None and hasattr(HTMLViewer, "load_file")

        # Loopback first: some platforms refuse a file:// page -- the Steam
        # Deck's browser and various sandboxed webviews among them -- so the
        # self-contained view is served over loopback and pywebview / the
        # browser are handed the URL. tkinterweb is the exception: it renders a
        # file in-process with no file:// URL involved, so it keeps the file and
        # works even where an OS browser would not open one.
        if HAVE_PYWEBVIEW:
            url = str(path) if already_url else self._serve_html_file(path)
            trace(f"view: pywebview for {url or file_path}")
            self._open_cell_map_pywebview(url or path, title or "View")
            return
        if can_tkweb and file_path is not None:
            trace(f"view: tkinterweb for {file_path.name}")
            self._show_html_window(file_path, title)
            return
        url = str(path) if already_url else self._serve_html_file(path)
        if url is not None:
            # Last resort browser, but on loopback rather than file://.
            trace(f"view: browser for {url} (loopback)")
            self._open_url_in_browser(url)
            return
        if file_path is not None:
            trace(f"view: browser for {file_path.name} (file://, no loopback port)")
            self._open_file_in_browser(file_path)

    def _serve_html_file(self, path: str | Path) -> str | None:
        """Serve a written HTML view over loopback, or ``None`` to keep the file.

        Thin wrapper over :func:`wraithguard.viz.serve.publish_html_file` with
        the shared viewer server, so a generated page is opened over loopback on
        platforms that refuse ``file://`` and falls back to the written file
        when no port can be bound.

        Args:
            path: The written HTML file.

        Returns:
            A ``http://127.0.0.1`` URL, or ``None`` to use the file.
        """
        return publish_html_file(self._viewer_server(), path)

    def _open_url_in_browser(self, url: str) -> None:
        """Open a URL in the user's browser.

        Args:
            url: The address to open.
        """
        open_in_browser(url)

    def _open_file_in_browser(self, path: str | Path) -> None:
        """Open any local file in the user's browser.

        Args:
            path: The file to open.
        """
        target = Path(path)
        if not target.exists():
            return
        try:
            open_in_browser(target.resolve().as_uri())
        except (OSError, ValueError):
            pass

    def _open_html_in_browser_loopback(self, path: str | Path) -> None:
        """Open a written HTML page in the browser, over loopback where possible.

        What every in-app view's "Open in browser" button should use. ``file://``
        is refused by some platforms (the Steam Deck's browser among them), so
        the page is served over loopback and the browser is handed the
        ``http://127.0.0.1`` URL; only when no port can be bound does it fall back
        to ``file://``.

        Args:
            path: The written HTML file.
        """
        if not path or not Path(path).exists():
            return
        url = self._serve_html_file(path)
        if url is not None:
            self._open_url_in_browser(url)
            return
        self._open_file_in_browser(path)

    def _show_html_window(self, path: str | Path, title: str) -> None:
        """Render a page in an in-app tkinterweb window.

        Args:
            path: The HTML file.
            title: Window title.
        """
        win = tk.Toplevel(self.root)
        apply_titlebar_theme(win)
        win.title(title)
        win.configure(bg=DARK["bg"])
        win.geometry("1280x860")
        bar = ttk.Frame(win, padding=6)
        bar.pack(fill="x")
        ttk.Button(
            bar,
            text=_("Open in browser"),
            # Loopback first: file:// is refused on some platforms (the Steam
            # Deck), so the help page is served over loopback and the browser
            # gets the URL, exactly as the cell-map window's button does.
            command=lambda: self._open_html_in_browser_loopback(path),
        ).pack(side="left")
        ttk.Button(bar, text=_("Close"), command=win.destroy).pack(side="right")
        try:
            viewer = HTMLViewer(win)
            viewer.pack(fill="both", expand=True)
            viewer.load_file(str(path))
        except Exception:  # noqa: BLE001 - third-party widget; the browser still works
            ttk.Label(
                win,
                foreground="#ffb454",
                padding=8,
                text=_(
                    "(inline render failed -- use 'Open in browser'. The 3D and "
                    "nav views need a real browser engine.)"
                ),
            ).pack(anchor="w")

    def _show_cell_map_window(self, path: str | Path) -> None:
        win = getattr(self, "_cellmap_win", None)
        if win is not None and win.winfo_exists():
            win.destroy()
        win = tk.Toplevel(self.root)
        apply_titlebar_theme(win)
        self._cellmap_win = win
        win.title("Viz Window")
        win.configure(bg=DARK["bg"])
        win.geometry("1000x720")
        bar = ttk.Frame(win, padding=6)
        bar.pack(fill="x")
        ttk.Button(bar, text=_("Save HTML..."), command=self._save_cell_map).pack(side="left")
        ttk.Button(bar, text=_("Open in browser"), command=self._open_cell_map_browser).pack(
            side="left", padx=(8, 0)
        )
        ttk.Button(bar, text=_("Close"), command=win.destroy).pack(side="right")
        try:
            viewer = HTMLViewer(win)
            viewer.pack(fill="both", expand=True)
            viewer.load_file(path)  # reads from disk, not an in-memory string
        except Exception:  # noqa: BLE001
            # third-party HTML widget; falls back to the browser
            ttk.Label(
                win,
                foreground="#ffb454",
                padding=8,
                text=_(
                    "(inline render failed - use 'Open in browser' for the full map)",
                ),
            ).pack(anchor="w")

    def _open_cell_map_browser(self) -> None:
        """Open the last cell map in the browser, over loopback where possible.

        The explicit "Open in browser" button. Prefers a loopback URL for the
        same reason :meth:`open_html_in_app` does -- file:// is refused on some
        platforms (the Steam Deck) -- and falls back to file:// only when no
        loopback port can be bound.
        """
        p = getattr(self, "_last_cell_file", None)
        if not p or not Path(p).exists():
            return
        self._open_html_in_browser_loopback(p)

    def _save_cell_map(self) -> None:
        src = getattr(self, "_last_cell_file", None)
        if not src or not Path(src).exists():
            return
        out = filedialog.asksaveasfilename(
            title=_("Save cell map"),
            defaultextension=".html",
            initialfile="cell_map.html",
            filetypes=case_insensitive_filetypes((("HTML files", "*.html"), ("All files", "*.*"))),
        )
        if not out:
            return
        try:
            import shutil

            if os.path.abspath(out) != os.path.abspath(src):  # noqa: PTH100
                shutil.copyfile(src, out)
            self.status_var.set(_("Cell map saved: %(path)s") % {"path": out})
        except OSError as e:
            messagebox.showerror(_("Save failed"), str(e))

    # -- resource (VFS) conflicts --------------------------------------------

    # -- download sources dialog ---------------------------------------------

    # -- mlox user-rules maker -----------------------------------------------

    # -- rule assembly ------------------------------------------------------

    # -- MOMW order-file updaters --------------------------------------------
    #
    # plugin-order.yml and its three siblings (groundcover-, bsa-, data-path-)
    # download the same way: confirm, fetch on a worker thread so the UI never
    # freezes, and report into a dialog. One helper, four one-line handlers --
    # each names its file, its path/URL vars, and the validating updater.

    # -- savegame dependency check -------------------------------------------

    def on_save_check(self) -> None:
        """Check a savegame's content files against the current order."""
        order = (
            self.order_panel.get_enabled()
            if self.order_panel.has_order()
            else list((self._current_plan or {}).get("base_order_names") or [])
        )
        if not order:
            self.status_var.set(_("Run '1. Sort' first so there's a load order to check against."))
            return
        start = Path.home() / "Documents" / "My Games" / "OpenMW"
        p = filedialog.askopenfilename(
            title=_("Choose an OpenMW save"),
            initialdir=str(start if start.is_dir() else (self._cfg_dir() or ".")),
            filetypes=case_insensitive_filetypes(
                (("OpenMW saves", "*.omwsave"), ("All files", "*.*"))
            ),
        )
        if not p:
            return
        files, missing, err = core.check_savegame_against_order(p, order)
        writer = QueueWriter(self.log_queue)
        writer.write("\n" + "=" * 70 + f"\n SAVEGAME CHECK: {Path(p).name}\n" + "=" * 70 + "\n")
        if err:
            writer.write(f"  ERROR: {err}\n")
            self.status_var.set(_("Save check failed: %(error)s") % {"error": err})
            return
        if files is None:  # err is None here, so files is set; said, not assumed
            raise RuntimeError("savegame check returned no files and no error")
        writer.write(f"  Save depends on {len(files)} content file(s).\n")
        if missing:
            for m in missing:
                writer.write(
                    f"\n[MISSING SAVE DEP] '{m}' is required by this save but not in "
                    f"the current load order -- OpenMW will refuse to load it.\n"
                )
            self.status_var.set(
                ngettext(
                    "Save check: %(count)d missing dependency! See the log.",
                    "Save check: %(count)d missing dependencies! See the log.",
                    len(missing),
                )
                % {"count": len(missing)}
            )
            messagebox.showwarning(
                _("Save Check"),
                f"{Path(p).name} depends on {len(missing)} content file(s) that are NOT in "
                f"the current load order:\n\n  "
                + "\n  ".join(missing[:12])
                + ("\n  ..." if len(missing) > 12 else "")
                + "\n\nOpenMW will refuse to load this save. Re-enable those plugins (or don't "
                "export this order if you want to keep playing that character).",
            )
        else:
            writer.write("  All dependencies present -- this save is safe with this order.\n")
            self.status_var.set(
                _("Save check: all %(count)d dependencies present.") % {"count": len(files)}
            )
            messagebox.showinfo(
                _("Save Check"),
                _(
                    "%(file)s: all %(count)d content files it needs are in "
                    "the current load order. Safe to export."
                )
                % {"file": Path(p).name, "count": len(files)},
            )

    # -- backup manager ------------------------------------------------------

    def on_help(self) -> None:
        """Offer the shipped documentation, one entry per document.

        A menu rather than a button each: the row is already full, and the set
        of documents is a list that may grow.
        """
        trace_first_fire("on_help")
        # The viewer's Help page (port step 4.4); the menu below stays for when the viewer
        # cannot start, since the help has to work when everything else is broken.
        if self._open_cell_viewer(docs=HELP_DOCUMENTS[0]):
            return
        # Literal calls, not a lookup: the extractor reads the source, so
        # `_(variable)` produces a string no translator ever sees.
        labels = {
            "QUICKSTART.md": _("Quick start"),
            "README.md": _("Read me"),
            "MLOX_RULES.md": _("Writing mlox rules"),
        }
        menu = tk.Menu(
            self.root,
            tearoff=0,
            bg=DARK["log_bg"],
            fg=DARK["fg"],
            activebackground=DARK["select"],
            activeforeground=DARK["fg"],
            bd=0,
            relief="flat",
            activeborderwidth=0,
        )
        for filename in HELP_DOCUMENTS:
            label = labels.get(filename, filename)
            menu.add_command(
                label=label,
                command=partial(self._open_document, label, filename),
            )
        try:
            button = self.help_btn
            menu.tk_popup(button.winfo_rootx(), button.winfo_rooty() + button.winfo_height())
        finally:
            menu.grab_release()

    def _open_document(self, label: str, filename: str) -> None:
        """Render one Markdown document and show it.

        The page is written with a stable name and overwritten each time, so the
        help does not join the timestamped views that accumulate -- there is only
        ever one current copy of a document, and keeping older renders of an
        unchanged file would be clutter with no question it answers.

        Args:
            label: The document's display name, used as the window title.
            filename: The Markdown filename to look up.
        """
        source = doc_path(filename) if viz_docs is not None else None
        if source is None:
            messagebox.showinfo(
                _("Documentation not found"),
                _(
                    "%(name)s was not found next to the program.\n\n"
                    "It ships alongside the app; if this is a build made without it, "
                    "the same document is in the project folder."
                )
                % {"name": filename},
            )
            return
        try:
            text = source.read_text(encoding="utf-8", errors="replace")
            page = viz_docs.docs_page(label, text, source_name=filename)
            out = app_base_dir() / f"help_{source.stem.lower()}.html"
            out.write_text(page, encoding="utf-8")
        except OSError as exc:
            trace(f"help: rendering {filename} FAILED:\n" + traceback.format_exc())
            messagebox.showerror(
                _("Could not open documentation"),
                _("%(name)s could not be rendered:\n%(error)s") % {"name": filename, "error": exc},
            )
            return
        trace(f"help: rendered {filename} -> {out}")
        self.open_html_in_app(out, f"{label} - Wraithguard Toolkit")

    def _rule_and_yml_dirs(self) -> list[str]:
        """Folders holding the mlox rule files and the plugin-order.yml.

        The rule updater and the plugin-order.yml updater both drop a
        timestamped ``.bak`` next to the file they replace, and those files
        live outside the data paths -- typically in an mlox folder of their own.
        A backup scan of the data dirs and the cfg alone therefore never sees
        them, which is why updating rules looked like it kept no backup. These
        folders are added to the scan so those backups show up with the rest.
        """
        roots: list[str] = []
        yml = self.plugin_order_yml_var.get().strip()
        if yml:
            parent = str(Path(yml).parent)
            if parent and parent != ".":
                roots.append(parent)
        return roots

    def on_backups(self) -> None:
        """Scan for backup files and open the restore/delete window."""
        if self.worker_running:
            return
        # De-dupe while preserving order: the rule/yml folders can coincide with
        # a data path, and scanning one root twice would list every hit twice.
        dirs = list(dict.fromkeys([*self._plan_scan_dirs(), *self._rule_and_yml_dirs()]))
        cfg = self.cfg_var.get().strip() or None
        if not dirs and not cfg:
            self.status_var.set(_("Set openmw.cfg (or run a Sort) so I know where to look."))
            return
        self.worker_running = True
        self.status_var.set(_("Scanning for backups..."))
        threading.Thread(target=self._backups_worker, args=(dirs, cfg), daemon=True).start()

    def _backups_worker(self, dirs: list[str], cfg: str | None) -> None:
        try:
            found = core.scan_backups(dirs, cfg_path=cfg)
            status = ngettext(
                "Found %(count)d backup file.", "Found %(count)d backup files.", len(found)
            ) % {"count": len(found)}
        except Exception as e:  # noqa: BLE001
            # worker top level: status line carries the failure
            found, status = [], _("Backup scan failed: %(error)s") % {"error": e}
        finally:
            self.worker_running = False
        self._schedule_ui(0, self._show_backups_window, found, status)

    def _show_backups_window(self, found: Sequence[Any], status: str) -> None:
        self.status_var.set(status)
        win = getattr(self, "_bk_win", None)
        if win is not None and win.winfo_exists():
            win.destroy()
        win = tk.Toplevel(self.root)
        apply_titlebar_theme(win)
        self._bk_win = win
        win.title("Backups")
        win.configure(bg=DARK["bg"])
        win.geometry("900x480")
        top = ttk.Frame(win, padding=10)
        top.pack(fill="both", expand=True)
        top.columnconfigure(0, weight=1)
        top.rowconfigure(1, weight=1)
        ttk.Label(
            top,
            text=ngettext(
                "%(count)d backup file. Restore copies the backup over its "
                "original; Delete removes the backup file itself.",
                "%(count)d backup files. Restore copies the backup over its "
                "original; Delete removes the backup file itself.",
                len(found),
            )
            % {"count": len(found)},
            foreground=DARK["fg_dim"],
        ).grid(row=0, column=0, columnspan=2, sticky="w")
        lb = tk.Listbox(top, selectmode="extended", exportselection=False, activestyle="dotbox")
        style_plain_widget(lb)
        attach_typeahead(lb)
        lb.grid(row=1, column=0, sticky="nsew", pady=8)
        sc = ttk.Scrollbar(top, orient="vertical", command=lb.yview)
        sc.grid(row=1, column=1, sticky="ns", pady=8)
        lb.configure(yscrollcommand=sc.set)
        self._bk_rows = found
        for bpath, orig, kind in found:
            tag = "" if (orig and Path(orig).exists()) else "   [original missing]"
            lb.insert("end", f"[{kind}]  {bpath}{tag}")
        self._bk_list = lb

        btns = ttk.Frame(top)
        btns.grid(row=2, column=0, sticky="w")

        def _selected() -> list[Any]:
            return [self._bk_rows[i] for i in lb.curselection()]

        def _restore() -> None:
            sel = [r for r in _selected() if r[1]]
            if not sel:
                return
            if not messagebox.askyesno(
                _("Restore"),
                ngettext(
                    "Copy %(count)d backup over its original (overwriting it)?",
                    "Copy %(count)d backups over their originals (overwriting them)?",
                    len(sel),
                )
                % {"count": len(sel)},
                parent=win,
            ):
                return
            import shutil as _sh

            ok = fail = 0
            for bpath, orig, _k in sel:
                try:
                    _sh.copy2(bpath, orig)
                    ok += 1
                except OSError:
                    fail += 1
            self.status_var.set(
                ngettext(
                    "Restored %(count)d backup%(failed)s. Re-run '1. Sort' to refresh checks.",
                    "Restored %(count)d backups%(failed)s. Re-run '1. Sort' to refresh checks.",
                    ok,
                )
                % {
                    "count": ok,
                    "failed": (_(", %(fail)d failed") % {"fail": fail}) if fail else "",
                }
            )

        def _delete() -> None:
            sel = _selected()
            if not sel:
                return
            if not messagebox.askyesno(
                _("Delete"),
                ngettext(
                    "Permanently delete %(count)d backup file?",
                    "Permanently delete %(count)d backup files?",
                    len(sel),
                )
                % {"count": len(sel)},
                parent=win,
            ):
                return
            ok = 0
            for bpath, _o, _k in sel:
                try:
                    Path(bpath).unlink()
                    ok += 1
                except OSError:
                    pass
            self.status_var.set(
                ngettext(
                    "Deleted %(count)d backup file.",
                    "Deleted %(count)d backup files.",
                    ok,
                )
                % {"count": ok}
            )
            win.destroy()
            self.on_backups()

        ttk.Button(btns, text=_("Restore Selected"), command=_restore).pack(
            side="left", padx=(0, 6)
        )
        ttk.Button(btns, text=_("Delete Selected"), command=_delete).pack(side="left", padx=(0, 6))
        ttk.Button(
            btns,
            text=_("Refresh"),
            command=lambda: (win.destroy(), self.on_backups()),  # type: ignore[func-returns-value]
        ).pack(side="left")
        ttk.Button(btns, text=_("Close"), command=win.destroy).pack(side="right")

    # -- lint (tes3lint-style native checks) ---------------------------------

    def on_lint(self) -> None:
        """Run the tes3lint-style checks, in a worker."""
        if self.worker_running or not self._current_plan:
            return
        order = self._apply_exclusions(self.order_panel.get_enabled())
        if not order:
            return
        dirs = self._plan_scan_dirs()
        subset = self._current_plan.get("subset") or []
        self.worker_running = True
        for b in (
            self.sort_button,
            self.export_button,
            self.conflicts_button,
            self.cellmap_button,
            self.cellpreview_button,
            self.resource_button,
            self.lint_button,
        ):
            b.configure(state="disabled")
        self.status_var.set(_("Linting plugins..."))
        threading.Thread(target=self._lint_worker, args=(order, dirs, subset), daemon=True).start()

    def _lint_worker(self, order: list[str], dirs: list[str], subset: list[str]) -> None:
        writer = QueueWriter(self.log_queue)
        stats: dict = {}
        try:
            with redirect_stdout(writer.as_stream()), redirect_stderr(writer.as_stream()):
                print("\n" + "=" * 70)
                print(_(" LINT (tes3lint-style checks, native)"))
                print("=" * 70)
                index = PluginFileIndex(dirs)
                subset_origins = {str(s).lower(): "your mod" for s in subset}
                warnings, stats = core.lint_plugins(
                    order, index, subset_names=subset, origins=subset_origins
                )
                print(
                    _(
                        "  Scanned %(scanned)d plugin(s); %(cells)d interior cell(s), "
                        "%(grids)d interior pathgrid(s)."
                    )
                    % {
                        "scanned": stats.get("scanned", 0),
                        "cells": stats.get("interior_cells", 0),
                        "grids": stats.get("pathgrids", 0),
                    }
                )
                if warnings:
                    for w in warnings:
                        print(f"\n{w}")
                else:
                    print(_("\n  No lint findings. Clean bill of health."))
            n = stats.get("warnings", 0)
            status = (
                ngettext(
                    "Lint: %(count)d finding. See the log.",
                    "Lint: %(count)d findings. See the log.",
                    n,
                )
                % {"count": n}
                if n
                else _("Lint: no findings.")
            )
        except Exception:  # noqa: BLE001
            # worker top level: reports the traceback into the log panel
            writer.write("\nERROR: lint failed:\n" + traceback.format_exc())
            status = "Lint failed -- see log."
        finally:
            self._schedule_ui(0, self._lint_finished, status)

    def _lint_finished(self, status: str) -> None:
        self.worker_running = False
        self.sort_button.configure(state="normal")
        for b in (
            self.export_button,
            self.conflicts_button,
            self.cellmap_button,
            self.cellpreview_button,
            self.resource_button,
            self.lint_button,
        ):
            b.configure(state="normal" if self._current_plan else "disabled")
        self.status_var.set(status)

    # -- tes3cmd frontend ----------------------------------------------------

    T3_COMMANDS = (
        (
            "clean",
            "clean -- remove junk (dup records, junk cells, evil GMSTs); runs in a "
            "staged Data Files with the plugin's masters, so tes3cmd sees the full "
            "VFS; skipped if a master can't be found; backup kept",
        ),
        (
            "sync",
            "resync master sizes -- IN-APP, VFS-aware (fixes [MASTER SIZE] notes; "
            "tes3cmd's own sync writes empty sizes on OpenMW multi-folder setups)",
        ),
        ("header", "header -- show author / description / masters (read-only)"),
        (
            "custom",
            "custom -- run any tes3cmd command (type it in 'command', e.g. "
            "'dump --type CELL' or 'fixcells'); runs in a staged Data Files with "
            "the plugin's masters so tes3cmd sees the full VFS; the plugin is "
            "appended; a modifying command's changes are copied back with a backup",
        ),
        # NO multipatch: it needs the ENTIRE load order in one flat Data Files
        # dir, which can't be faked safely for a multi-GB OpenMW setup -- and
        # OpenMW/MOMW users get merged leveled lists from delta-plugin instead.
    )
    # "custom" may or may not modify (the user's command decides), so it is treated
    # as modifying: it confirms first and copies back only when bytes changed.
    T3_MODIFIES: ClassVar[set[str]] = {"clean", "sync", "custom"}
    # NEVER cleaned, no exceptions: cleaning the vanilla masters -- even a
    # careful GMST-preserving clean -- rewrites record bytes that other
    # content depends on byte-for-byte, and causes in-game failures.
    T3_NEVER_CLEAN: ClassVar[set[str]] = {"morrowind.esm", "tribunal.esm", "bloodmoon.esm"}

    def _cfg_dir(self) -> str | None:
        c = self.cfg_var.get().strip()
        return str(Path(c).parent) if c else None

    def _refill_res_tree(self) -> None:
        tree = getattr(self, "_res_tree", None)
        if tree is None or not tree.winfo_exists():
            return
        only = self._res_subset_only.get()
        shown = [c for c in self._all_res if c.get("involves_subset") or not only]
        query = getattr(self, "_res_search_var", None)
        if query is not None:
            shown = search_rows(shown, query.get(), ("path", "winner"))
        self._res_shown = shown
        tree.delete(*tree.get_children())
        for i, c in enumerate(self._res_shown):
            star = "★" if c["involves_subset"] else ""
            tags = ("sub",) if c["involves_subset"] else ()
            # "!" only when reading the mesh actually found something. An
            # unreadable mesh gets "?" -- distinct on purpose, because "we
            # could not read this" and "this loses collision" send a user to
            # completely different places.
            finding = c.get("mesh")
            mark = ""
            if finding is not None:
                if getattr(finding, "worth_reporting", False):
                    mark = "!"
                elif getattr(finding, "unreadable", ""):
                    mark = "?"
            tree.insert(
                "",
                "end",
                iid=str(i),
                tags=tags,
                values=(star, mark, c["path"], len(c["providers"]), c["winner"]),
            )

    def _is_custom(self, plugin: str) -> bool:
        """Report whether this plugin is one of the user's custom mods.

        True when it is in the scanned subset,
        as opposed to a curated-list plugin.
        """
        return str(plugin).lower() in getattr(self, "_conf_subset_lower", set())

    @staticmethod
    # Any: formats an arbitrary decoded TES3 field value for display.
    def _fmt_val(v: Any) -> str:  # noqa: ANN401
        if v is None:
            return "-"
        if isinstance(v, list):
            # short lists of scalars (e.g. a grid [x, y]) show inline; long ones
            # or lists of objects (e.g. references) get a count + hint to expand
            if all(not isinstance(x, (dict, list)) for x in v):
                s = str(v)
                if len(s) <= 140:
                    return s
            return f"[{len(v)} item(s)]  - double-click to view"
        s = str(v)
        return s if len(s) <= 140 else s[:137] + "…  (double-click)"

    def _populate_field_diff(self, conflict: Mapping[str, Any]) -> None:
        ftree = getattr(self, "_conf_ftree", None)
        if ftree is None or not ftree.winfo_exists():
            return
        plugins = conflict["plugins"]
        cols = ["field"] + [f"p{i}" for i in range(len(plugins))]
        ftree.configure(columns=cols)
        ftree.heading("field", text="Field")
        ftree.column("field", width=240, anchor="w", stretch=False)
        for i, p in enumerate(plugins):
            star = "★ " if self._is_custom(p) else ""  # ★ marks your custom mods
            suffix = "  (wins)" if i == len(plugins) - 1 else ""
            ftree.heading(f"p{i}", text=f"{star}{p}{suffix}")
            ftree.column(f"p{i}", width=210, anchor="w", stretch=True)
        rtl.apply_rtl_to_treeview(ftree)
        ftree.delete(*ftree.get_children())
        if self._conf_session is None:
            ftree.insert(
                "",
                "end",
                values=["(set a tes3conv binary for field-level diffs)"] + [""] * len(plugins),
            )
            return
        read = self.read_fields_now(conflict)
        if read is None:
            # Busy, or unreadable. Either way the panel says so rather than
            # blocking the UI thread until whatever holds the session lets go.
            ftree.insert("", "end", values=["(busy or unavailable)"] + [""] * len(plugins))
            return
        keys, per, diff = read
        self._conf_fdiff = {"plugins": plugins, "per": per}  # for the expand popup
        # The record's own id doubles as its cell label for the visualisations
        # ("(43, -45)" for landscape, "Balmora (-3, -2)" for a path grid), so
        # the generated page can say which cell it is showing.
        self._conf_record_label = str(conflict.get("id") or "")
        # The record type ("Landscape", "PathGrid", ...) so the detail window
        # can say what each field is in the file format, not just in the JSON.
        self._conf_record_type = str(conflict.get("type") or "")
        # Red-or-not was the wrong resolution. "These disagree" reads the same
        # whether four plugins agree with the winner or one plugin's work is
        # being thrown away, and only the second is worth opening. Each row is
        # now coloured by what is actually happening to that field.
        judged = {status.key: status for status in field_statuses(keys, per, plugins)}
        for k in keys:
            row = [k] + [self._fmt_val(per[p].get(k)) for p in plugins]
            status = judged.get(k)
            tag = ALL_TAGS[status.overall][0] if status else ""
            tags = (tag,) if tag else ()
            if not tags and k in diff:
                tags = ("diff",)
            ftree.insert("", "end", iid=k, values=row, tags=tags)
        if not keys:
            ftree.insert("", "end", values=["(no fields / identical)"] + [""] * len(plugins))

    @staticmethod
    def _disassemble_bytecode_field(
        value: str | bytes, source_text: str | None = None
    ) -> str | None:
        """Disassembly text for a tes3conv 'bytecode' field, or None.

        Thin delegation: the logic lives in wraithguard.mwscript so it can be
        unit-tested without a display. Returns None only when the package is
        unavailable, in which case the caller shows the raw value as before.
        """
        if listing_for_bytecode_field is None:
            return None
        return listing_for_bytecode_field(value, source_text)


def is_view_url(target: str | Path) -> bool:
    """Whether a view target is already a URL rather than a file to convert.

    The 3D mesh viewer is *served* over loopback, so what reaches the viewer
    chain is an ``http://127.0.0.1:...`` address. Every other visualisation is
    a file. Passing a URL through ``Path(...).as_uri()`` mangles it into
    something no viewer can open, which is why this is asked before converting
    rather than after something fails.

    Args:
        target: A file path or a URL.

    Returns:
        ``True`` when it should be handed to a viewer unchanged.
    """
    return str(target).startswith(("http://", "https://"))


def view_uri(target: str | Path) -> str:
    """Turn a view target into something a webview can load.

    Args:
        target: A file path or a URL.

    Returns:
        The URL to open.
    """
    return str(target) if is_view_url(target) else Path(target).resolve().as_uri()


def main() -> None:
    """Parse arguments and start the GUI."""
    import argparse

    ap = argparse.ArgumentParser(description="Wraithguard Toolkit (GUI)")
    ap.add_argument(
        "--trace",
        nargs="?",
        const=True,
        default=None,
        metavar="LOGFILE",
        help="Write a debug trace log for troubleshooting. Off by default. "
        "Pass --trace for the default log (wraithguard_toolkit_trace.log next "
        "to the app), or --trace PATH to choose the file.",
    )
    args, _unknown = ap.parse_known_args()
    global _TRACE_REQUEST
    _TRACE_REQUEST = args.trace
    root = TkinterDnD.Tk() if HAVE_DND else tk.Tk()
    # theming (including the native titlebar) happens inside App.__init__
    # (not here), because the saved theme name has to be loaded first so the
    # chrome -- OS titlebar included -- comes up already themed
    App(root)
    root.mainloop()


if __name__ == "__main__":
    main()
