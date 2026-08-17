"""The load order as a tree: plugin, kind of thing, thing.

**Why this and not the flat list.** The conflict list answers "what conflicts",
which is the right question exactly once -- when you want a count. Every
question after that is about a *mod*: what does this one change, where does it
lose, is it worth moving or patching. A flat list of 51,946 rows cannot be read
that way, and no amount of filtering turns a list of records into a picture of
a load order.

So this is the same data with the load order's own shape restored: file ->
record type -> record, with the record compared across every plugin that
defines it on the right. That is the layout xEdit established and yampt's
editor follows, and it is worth following because it matches how people
already think about their mods.

**Nothing is built or read until it is opened.** A single mod can edit tens of
thousands of cells, and Tk builds every row eagerly; inserting them up front
freezes the window. So a plugin's groups appear when the plugin is opened, its
records appear when the group is opened, and even then they are inserted in
batches with the event loop given a turn between them -- the tree stays usable
while a large group fills in behind it.

The judgement works the same way. Colouring a record means reading it with
tes3conv, which is far too slow to do for a whole load order up front, but
perfectly fast for the few hundred records you just expanded. So each group
judges itself in the background as it opens, and its rows take their colour as
the answers arrive.

**Colour carries two independent facts**, as it does everywhere in this
package: the text colour says what *this plugin* is doing to the record, and
the row's shade says whether anything is being lost overall. A plugin row takes
the worst of everything beneath it, so a branch worth opening looks like one
without being opened.

The tree structure follows ``model/nav_tree_model.hpp`` in yampt (MIT,
Rafał Wierzchoś): ``file_node_t`` -> ``type_group_t`` -> records, with the roll-up
taking the worst of each child on both axes.
"""

from __future__ import annotations

import threading
import tkinter as tk
from tkinter import ttk
from typing import TYPE_CHECKING, Any, Final

import wraithguard_toolkit as core
from wraithguard.gui.conflict_colors import THIS_TEXT, all_colors, this_text
from wraithguard.gui.theme import DARK, apply_titlebar_theme
from wraithguard.gui.widgets import add_tooltip
from wraithguard.i18n import gettext as _
from wraithguard.logging_setup import get_logger
from wraithguard.patch.align import align, alignable, label_for
from wraithguard.patch.status import ABSENT, ConflictThis, worst_this
from wraithguard.patch.summary import (
    ALL_TAGS,
    Branch,
    field_statuses,
    group_by_plugin,
    record_plugin_statuses,
    record_status,
)

if TYPE_CHECKING:
    from collections.abc import Callable, Mapping, Sequence

    from wraithguard.patch.summary import Survey

LOG: Final = get_logger(__name__)

#: How many record rows are inserted before the event loop is given a turn.
#: Tk builds rows eagerly, so a group of 30,000 inserted in one go locks the
#: window; in batches it fills in visibly and stays usable throughout.
INSERT_BATCH: Final = 200

#: How many records one opened group will judge in the background. Since each
#: plugin is now read once for the whole group rather than once per record,
#: this is a memory guard rather than a time one -- the values being compared
#: are held while the group is judged. Everything is still *listed*; only the
#: colouring stops here.
JUDGE_BUDGET: Final = 20000

#: How many plugin rows the index scan colours per event-loop tick. The work is
#: dict lookups over a compact index -- no I/O, no records held -- so this is a
#: UI-smoothness knob, not a memory one: a big load order colours over a second
#: or two of ticks rather than one blocking pass.
COLOUR_BATCH: Final = 64

#: Row-background tag marking the plugins that conflict with the selected one.
#: A *background* rather than a foreground, so it stacks with the verdict colours
#: (which are foregrounds) instead of replacing them: "related to what you
#: clicked" sits behind "what each plugin is doing".
CONFLICT_TAG: Final = "conflicts-with-selection"

#: The colour that background-marks them. **Purple, not blue** -- ktim816 asked
#: for blue, but the selection highlight is already blue (``#094771``), so a blue
#: mark would be near-indistinguishable from the selected row. Purple is a hue no
#: verdict foreground uses and clearly not the selection, and it is dark enough
#: that every verdict colour still reads on top of it: the mid-tone red (a losing
#: plugin) and the dim grey (an ignored one) both clear 4:1, the rest 6-7:1.
CONFLICT_BG: Final = "#512e5a"

#: Separator inside a tree row's id. ``\x00`` cannot occur in a plugin name, a
#: record type or a record id, so splitting on it cannot go wrong.
SEP: Final = "\x00"

#: Marks a node whose children have not been built yet.
PENDING: Final = "pending"


def this_tag(status: ConflictThis) -> str:
    """The tree tag for a per-plugin status.

    Args:
        status: What one plugin is doing.

    Returns:
        The tag name.
    """
    return f"this-{status.name.lower()}"


class PluginViewMixin:
    """A window showing the scan as a tree of plugins."""

    if TYPE_CHECKING:  # pragma: no cover - declarations for the host class
        # tk.Tk, not tk.Misc: App is built on a real toplevel and these
        # windows call transient()/title()/geometry() on it, which live on Wm
        # and not on Misc. Declaring the weaker type here type-checked fine and
        # hid those calls from mypy entirely.
        root: tk.Tk
        status_var: tk.StringVar
        _conflict_win: tk.Toplevel | None
        _conf_session: Any
        _conf_paths: dict[str, str]
        _shown_conflicts: list[dict]
        _conf_survey: Survey | None

        def _is_custom(self, name: str) -> bool: ...
        def _schedule_ui(
            self, delay_ms: int, func: Callable[..., Any], *args: Any  # noqa: ANN401
        ) -> None: ...
        def _fmt_val(self, value: Any) -> str: ...  # noqa: ANN401
        def _session_lock(self) -> threading.Lock: ...
        def read_fields_now(  # noqa: D102
            self, conflict: Mapping[str, Any]
        ) -> tuple[list[str], dict[str, dict[str, Any]], set[str]] | None: ...
        def _ensure_conflict_session(self, conv: str | None = ...) -> bool: ...
        def _show_field_value(
            self,
            key: str,
            plugins: Sequence[str],
            per: Mapping[str, Mapping[str, Any]],
            record_type: str = ...,
            record_label: str = ...,
        ) -> None: ...

    # -- the window ----------------------------------------------------

    def show_plugin_view(self) -> None:
        """Open the plugin tree, or raise it if it is already open."""
        win = getattr(self, "_plugin_win", None)
        if win is not None and win.winfo_exists():
            win.lift()
            win.focus_force()
            return

        win = tk.Toplevel(getattr(self, "_conflict_win", None) or self.root)
        self._plugin_win = win
        apply_titlebar_theme(win)
        win.title(_("Plugin view"))
        win.configure(bg=DARK["bg"])
        win.geometry("1280x760")

        ttk.Label(
            win,
            foreground=DARK["fg_dim"],
            padding=(8, 6, 8, 2),
            text=_(
                "Your load order as a tree. Open a plugin to see what it changes, and a "
                "record to compare it across every plugin that defines it. Records are "
                "read and judged as you open them. Read-only: nothing here writes."
            ),
        ).pack(fill="x")

        panes = ttk.PanedWindow(win, orient="horizontal")
        panes.pack(fill="both", expand=True, padx=8, pady=4)

        left = ttk.Frame(panes)
        nav = ttk.Treeview(left, show="tree headings", columns=("count",), style="Conf.Treeview")
        nav.heading("#0", text=_("Plugin / type / record"))
        nav.column("#0", width=380, stretch=True)
        nav.heading("count", text=_("#"))
        nav.column("count", width=70, anchor="e", stretch=False)
        nav_scroll = ttk.Scrollbar(left, orient="vertical", command=nav.yview)
        nav.configure(yscrollcommand=nav_scroll.set)
        nav.grid(row=0, column=0, sticky="nsew")
        nav_scroll.grid(row=0, column=1, sticky="ns")
        left.rowconfigure(0, weight=1)
        left.columnconfigure(0, weight=1)
        self._plugin_nav = nav
        panes.add(left, weight=1)

        right = ttk.Frame(panes)
        detail = ttk.Treeview(right, show="tree headings", style="Conf.Treeview")
        detail.heading("#0", text=_("Field"))
        detail.column("#0", width=260, stretch=False)
        detail_scroll = ttk.Scrollbar(right, orient="vertical", command=detail.yview)
        detail_across = ttk.Scrollbar(right, orient="horizontal", command=detail.xview)
        detail.configure(yscrollcommand=detail_scroll.set, xscrollcommand=detail_across.set)
        detail.grid(row=0, column=0, sticky="nsew")
        detail_scroll.grid(row=0, column=1, sticky="ns")
        detail_across.grid(row=1, column=0, sticky="ew")
        right.rowconfigure(0, weight=1)
        right.columnconfigure(0, weight=1)
        self._plugin_detail = detail
        panes.add(right, weight=2)

        # xEdit's convention (see conflict_colors): what one plugin does is the
        # text colour, and a record/field's overall status is the row
        # background. The nav tree carries per-plugin rows, so it takes the
        # text colours; the detail tree carries per-field rows judged across all
        # plugins, so those get the coloured backgrounds.
        for tree in (nav, detail):
            for status in THIS_TEXT:
                tree.tag_configure(this_tag(status), foreground=this_text(status))
            tree.tag_configure("dim", foreground=DARK["fg_dim"])
            for value, (name, _why) in ALL_TAGS.items():
                bg, fg = all_colors(value)
                if bg is not None:
                    tree.tag_configure(name, background=bg, foreground=fg)
                else:
                    tree.tag_configure(name, foreground=fg)
        # The conflict highlight only ever lands on plugin rows in the nav,
        # which carry no verdict background -- so its dark background layers
        # cleanly behind the row's verdict text colour (see CONFLICT_BG).
        nav.tag_configure(CONFLICT_TAG, background=CONFLICT_BG)

        nav.bind("<<TreeviewSelect>>", lambda _e: self._on_plugin_node())
        nav.bind("<<TreeviewOpen>>", lambda _e: self._on_plugin_open())
        # Double-click a field row for the full value, exactly as the conflict
        # window's field diff does -- long fields (references, scripts, the
        # base64 land blobs) are truncated in the columns here.
        detail.bind("<Double-Button-1>", lambda _e: self._on_plugin_detail_double())
        add_tooltip(detail, _("Double-click a field for its full value across every plugin."))
        add_tooltip(
            nav,
            _(
                "Colour says what this plugin is doing to the records beneath it:\n"
                "  blue = it defines them first\n"
                "  green = it changes them and nothing later disagrees\n"
                "  amber = it changes them, others disagreed, and it still wins\n"
                "  RED = it changes them and something later overrides the change\n"
                "  grey = it redefines them without changing anything\n\n"
                "Records are read as you expand them, so colour fills in a moment after "
                "a group opens. A plugin takes the worst of whatever has been judged "
                "beneath it."
            ),
        )

        self._plugin_rows: dict[tuple[str, str, str], str] = {}
        self._plugin_branches: dict[str, Branch] = {}
        self._fill_plugin_nav()
        # If a plugin summary has already judged the load order, colour the whole
        # tree from its index now -- no need to open a group to see a plugin's
        # colour. Harmless when no summary has run: the scan simply finds nothing.
        self._colour_tree_from_index()
        # If it has *not* run, build the index in the background so the colours
        # fill in on their own rather than only where a group is opened. It reads
        # every record once (carefully -- digests, per-plugin), which is slow on a
        # large order but populates the whole tree without a click, which is what
        # this view is for. Only started when idle and nothing has judged yet.
        self._start_background_colouring()

        ttk.Button(win, text=_("Close"), command=win.destroy).pack(side="right", padx=8, pady=8)
        self._plugin_lost_only = tk.BooleanVar(value=True)
        self._highlighted_plugins: set[str] = set()
        self._conflict_adjacency_cache: dict[bool, dict[str, set[str]]] = {}
        highlight_chk = ttk.Checkbutton(
            win,
            text=_("Highlight only conflicts that lose work"),
            variable=self._plugin_lost_only,
            command=self._on_highlight_mode_changed,
        )
        highlight_chk.pack(side="left", padx=8, pady=8)
        add_tooltip(
            highlight_chk,
            _(
                "Click a plugin to highlight the plugins it conflicts with, in purple.\n\n"
                "Checked (lost): only records where an edit is actually discarded -- the "
                "conflicts worth acting on. Needs a plugin summary, which opening this view "
                "starts on its own.\n\n"
                "Unchecked (broad): any record two plugins both touch, whether or not "
                "anything is lost."
            ),
        )

    def _start_background_colouring(self) -> None:
        """Kick a quiet plugin summary to colour the tree, if one has not run.

        No-op when a survey already exists (colours come from its index), when
        no conflict session is available to read with, or when a worker is
        already busy. The survey colours the tree itself on completion.
        """
        if getattr(self, "_conf_survey", None) is not None:
            return
        if getattr(self, "_conf_session", None) is None or getattr(self, "worker_running", False):
            return
        survey = getattr(self, "_survey_conflicts", None)
        if callable(survey):
            survey(quiet=True)

    # -- building the tree, one level at a time ------------------------

    def _fill_plugin_nav(self) -> None:
        """Insert the plugin rows. Nothing below them is built yet."""
        nav = self._plugin_nav
        nav.delete(*nav.get_children())
        self._plugin_rows.clear()
        rows = list(getattr(self, "_shown_conflicts", None) or [])
        order = [str(name) for name in getattr(self, "_plugin_order", None) or []]
        self._plugin_branches = {branch.plugin: branch for branch in group_by_plugin(rows, order)}

        for branch in self._plugin_branches.values():
            node = nav.insert(
                "",
                "end",
                iid=branch.plugin,
                text=("★ " if self._is_custom(branch.plugin) else "") + branch.plugin,
                values=(branch.records,),
            )
            nav.insert(node, "end", text=_("(opening...)"), tags=(PENDING, "dim"))

        self.status_var.set(
            _("Plugin view: %(n)d plugin(s). Open one to see what it changes.")
            % {"n": len(self._plugin_branches)}
            if self._plugin_branches
            else _("Nothing to show -- run a conflict scan first.")
        )

    def _on_plugin_open(self) -> None:
        """Build the children of whichever node was just expanded."""
        nav = self._plugin_nav
        node = nav.focus()
        if not node:
            return
        children = nav.get_children(node)
        if not children or PENDING not in nav.item(children[0], "tags"):
            return
        nav.delete(*children)
        if SEP in node:
            plugin, kind = node.split(SEP, 1)
            self._open_group(node, plugin, kind)
        else:
            self._open_plugin(node, node)

    def _open_plugin(self, node: str, plugin: str) -> None:
        """Insert one row per record type the plugin touches.

        Args:
            node: The plugin's row.
            plugin: Its file name.
        """
        branch = self._plugin_branches.get(plugin)
        if branch is None:
            return
        for kind, markers in sorted(branch.groups.items()):
            group = self._plugin_nav.insert(
                node, "end", iid=f"{plugin}{SEP}{kind}", text=kind, values=(len(markers),)
            )
            self._plugin_nav.insert(group, "end", text=_("(opening...)"), tags=(PENDING, "dim"))

    def _open_group(self, node: str, plugin: str, kind: str) -> None:
        """Insert every record of one type, in batches, then judge them.

        Args:
            node: The group's row.
            plugin: The plugin it belongs to.
            kind: The record type.
        """
        branch = self._plugin_branches.get(plugin)
        if branch is None:
            return
        markers = branch.groups.get(kind, [])
        self._insert_batch(node, plugin, markers, 0)

    def _insert_batch(
        self, node: str, plugin: str, markers: Sequence[tuple[str, str]], start: int
    ) -> None:
        """Insert one batch of record rows and schedule the next.

        Tk has no virtualised tree, so a group of tens of thousands has to be
        inserted for real -- but not all at once. Yielding to the event loop
        between batches is the difference between a window that fills in and a
        window that is frozen.

        Args:
            node: The group's row.
            plugin: The plugin it belongs to.
            markers: Every ``(type, key)`` in the group.
            start: Where this batch begins.
        """
        nav = self._plugin_nav
        if not nav.winfo_exists() or not nav.exists(node):
            return
        stop = min(start + INSERT_BATCH, len(markers))
        for marker in markers[start:stop]:
            iid = f"{plugin}{SEP}{marker[0]}{SEP}{marker[1]}"
            if nav.exists(iid):
                continue
            nav.insert(node, "end", iid=iid, text=marker[1])
            self._plugin_rows[(plugin, marker[0], marker[1])] = iid
        if stop < len(markers):
            self.root.after(1, lambda: self._insert_batch(node, plugin, markers, stop))
            return
        self._judge_group(plugin, markers)

    # -- judging what has been opened ----------------------------------

    def _judge_group(self, plugin: str, markers: Sequence[tuple[str, str]]) -> None:
        """Read and judge an opened group's records, off the UI thread.

        Args:
            plugin: The plugin whose rows are being coloured.
            markers: The records in the group.
        """
        if not markers:
            return
        # If the plugin summary has run, it already judged every record *per
        # plugin* and kept the verdicts as an index (Survey.verdicts). Colour
        # straight from that -- a dict lookup per row, no tes3conv, no records
        # held -- rather than reading the group again. The lazy read below is the
        # fallback for when no summary has been taken.
        survey = getattr(self, "_conf_survey", None)
        if survey is not None and survey.verdicts:
            self._colour_group_from_index(plugin, markers, survey.verdicts)
            return
        if self._conf_session is None:
            return
        budget = list(markers[:JUDGE_BUDGET])
        if len(markers) > JUDGE_BUDGET:
            self.status_var.set(
                _("Judging the first %(n)d of %(total)d record(s) in this group.")
                % {"n": JUDGE_BUDGET, "total": len(markers)}
            )
        threading.Thread(target=self._judge_worker, args=(plugin, budget), daemon=True).start()

    def _judge_worker(self, plugin: str, markers: Sequence[tuple[str, str]]) -> None:
        """Compare each record and hand the verdicts back in batches.

        Args:
            plugin: The plugin whose rows are being coloured.
            markers: The records to judge.
        """
        by_marker = {
            (str(entry.get("type") or ""), str(entry.get("id") or "")): entry
            for entry in getattr(self, "_shown_conflicts", None) or []
        }
        wanted = [by_marker[marker] for marker in markers if marker in by_marker]
        if not wanted:
            return
        try:
            read = core.batch_record_fields(
                self._conf_session,
                wanted,
                self._conf_paths,
                digest=True,
                lock=self._session_lock(),
            )
        except Exception:
            LOG.exception("could not read a group of %s", plugin)
            return

        verdicts: dict[tuple[str, str], ConflictThis] = {}
        for conflict in wanted:
            marker = (str(conflict["type"]), str(conflict["id"]))
            keys, per = read.get(marker, ([], {}))
            plugins = [str(name) for name in conflict["plugins"]]
            statuses = field_statuses(keys, per, plugins)
            verdict = record_plugin_statuses(statuses, plugins).get(plugin)
            if verdict is not None:
                verdicts[marker] = verdict
        if verdicts:
            self._schedule_ui(0, self._paint, plugin, verdicts)

    def _paint(self, plugin: str, verdicts: Mapping[tuple[str, str], ConflictThis]) -> None:
        """Colour the rows whose verdicts have come back, and roll them up.

        Args:
            plugin: The plugin whose rows these are.
            verdicts: ``(type, key)`` to its status.
        """
        nav = getattr(self, "_plugin_nav", None)
        if nav is None or not nav.winfo_exists():
            return
        for marker, status in verdicts.items():
            iid = self._plugin_rows.get((plugin, marker[0], marker[1]))
            if iid and nav.exists(iid):
                nav.item(iid, tags=(this_tag(status),))
        self._roll_up(plugin)

    def _roll_up(self, plugin: str) -> None:
        """Give a plugin and its groups the worst colour beneath them.

        Args:
            plugin: The plugin to recolour.
        """
        nav = self._plugin_nav
        if not nav.exists(plugin):
            return
        overall: list[ConflictThis] = []
        for group in nav.get_children(plugin):
            beneath = [
                status
                for child in nav.get_children(group)
                if (status := _tagged(nav.item(child, "tags"))) is not None
            ]
            if not beneath:
                continue
            worst = worst_this(beneath)
            nav.item(group, tags=(this_tag(worst),))
            overall.extend(beneath)
        if overall:
            nav.item(plugin, tags=(this_tag(worst_this(overall)),))

    # -- colouring the whole tree from the summary's index -------------

    def _colour_group_from_index(
        self,
        plugin: str,
        markers: Sequence[tuple[str, str]],
        verdicts: Mapping[tuple[str, str], Mapping[str, ConflictThis]],
    ) -> None:
        """Colour one opened group's rows from the summary index, no re-read.

        Args:
            plugin: The plugin whose rows these are.
            markers: The group's ``(type, key)`` records.
            verdicts: The survey's ``marker -> {plugin: status}`` index.
        """
        nav = getattr(self, "_plugin_nav", None)
        if nav is None or not nav.winfo_exists():
            return
        for mtype, key in markers:
            status = verdicts.get((mtype, key), {}).get(plugin)
            if status is None:
                continue
            iid = self._plugin_rows.get((plugin, mtype, key))
            if iid and nav.exists(iid):
                nav.item(iid, tags=(this_tag(status),))
        self._roll_up(plugin)

    def _colour_tree_from_index(self) -> None:
        """Colour every plugin row from the summary index, over several ticks.

        The index scan: once the plugin summary has judged the load order, this
        walks every plugin and gives it the worst colour of the records it
        touches -- so the tree shows which mods are losing work without a group
        being opened, and without reading anything again. Throttled across
        plugins so a large order colours smoothly rather than in one freeze, and
        it reads only the compact index, so it never holds a record.
        """
        survey = getattr(self, "_conf_survey", None)
        nav = getattr(self, "_plugin_nav", None)
        branches = getattr(self, "_plugin_branches", None)
        if survey is None or not survey.verdicts or nav is None or not nav.winfo_exists():
            return
        # A fresh survey makes the lost-mode conflict index stale: drop it so the
        # next highlight rebuilds from the new verdicts, and refresh the current
        # highlight in place if a plugin is selected (a survey finishing while a
        # plugin is selected should light its conflicts, not wait for a re-click).
        getattr(self, "_conflict_adjacency_cache", {}).pop(True, None)
        chosen = nav.selection()
        if chosen and SEP not in chosen[0] and chosen[0] in (branches or {}):
            self._highlight_conflicts(chosen[0])
        if branches:
            self._colour_plugins_batch(list(branches.values()), 0, survey.verdicts)

    def _colour_plugins_batch(
        self,
        branches: Sequence[Branch],
        start: int,
        verdicts: Mapping[tuple[str, str], Mapping[str, ConflictThis]],
    ) -> None:
        """Colour one batch of plugin rows, then schedule the next.

        Args:
            branches: Every plugin branch, in tree order.
            start: Where this batch begins.
            verdicts: The survey's ``marker -> {plugin: status}`` index.
        """
        nav = getattr(self, "_plugin_nav", None)
        if nav is None or not nav.winfo_exists():
            return
        stop = min(start + COLOUR_BATCH, len(branches))
        for branch in branches[start:stop]:
            seen = [
                status
                for markers in branch.groups.values()
                for marker in markers
                if (status := verdicts.get(marker, {}).get(branch.plugin)) is not None
            ]
            if seen and nav.exists(branch.plugin):
                nav.item(branch.plugin, tags=(this_tag(worst_this(seen)),))
        if stop < len(branches):
            self.root.after(1, lambda: self._colour_plugins_batch(branches, stop, verdicts))

    # -- conflict highlighting -----------------------------------------

    def _on_highlight_mode_changed(self) -> None:
        """Re-highlight the selected plugin when the lost/broad toggle flips."""
        nav = getattr(self, "_plugin_nav", None)
        if nav is None or not nav.winfo_exists():
            return
        chosen = nav.selection()
        if chosen and SEP not in chosen[0] and chosen[0] in getattr(self, "_plugin_branches", {}):
            self._highlight_conflicts(chosen[0])

    @staticmethod
    def _link_all(adjacency: dict[str, set[str]], plugins: Sequence[str]) -> None:
        """Record that every plugin in ``plugins`` conflicts with the others.

        Args:
            adjacency: The plugin -> {conflicting plugins} map, updated in place.
            plugins: The plugins that share one contested record.
        """
        for plugin in plugins:
            bucket = adjacency.setdefault(plugin, set())
            bucket.update(p for p in plugins if p != plugin)

    def _conflict_adjacency(self, *, lost_only: bool) -> dict[str, set[str]]:
        """Which plugins each plugin shares a contested record with.

        Built once per mode and cached. ``lost_only`` links plugins only through
        records where an edit is actually discarded (the survey's verdicts, so it
        is empty until the summary has run); the broad mode links them through
        any record two or more plugins both define.

        Args:
            lost_only: Whether to count only records that lose work.

        Returns:
            A plugin -> {conflicting plugins} map.
        """
        cache = getattr(self, "_conflict_adjacency_cache", None)
        if cache is None:
            cache = self._conflict_adjacency_cache = {}
        if lost_only in cache:
            return cache[lost_only]
        adjacency: dict[str, set[str]] = {}
        if lost_only:
            survey = getattr(self, "_conf_survey", None)
            verdicts = survey.verdicts.items() if survey is not None else ()
            for _marker, per_plugin in verdicts:
                if any(v is ConflictThis.CONFLICT_LOSES for v in per_plugin.values()):
                    self._link_all(adjacency, list(per_plugin))
        else:
            for row in getattr(self, "_shown_conflicts", None) or []:
                plugins = [str(p) for p in row.get("plugins") or []]
                if len(plugins) >= 2:
                    self._link_all(adjacency, plugins)
        cache[lost_only] = adjacency
        return adjacency

    def _clear_conflict_highlight(self) -> None:
        """Remove the highlight tag from whatever currently carries it."""
        nav = getattr(self, "_plugin_nav", None)
        highlighted = getattr(self, "_highlighted_plugins", None)
        if nav is None or not nav.winfo_exists() or not highlighted:
            self._highlighted_plugins = set()
            return
        for iid in highlighted:
            if nav.exists(iid):
                nav.item(iid, tags=tuple(t for t in nav.item(iid, "tags") if t != CONFLICT_TAG))
        self._highlighted_plugins = set()

    def _highlight_conflicts(self, plugin: str) -> None:
        """Background-mark every plugin that conflicts with ``plugin``.

        Clears the previous highlight first, so only one plugin's conflicts show
        at a time. The tag is added alongside the row's verdict tag rather than
        replacing it, so a highlighted plugin keeps its own colour.

        Args:
            plugin: The plugin whose conflicts to mark.
        """
        nav = getattr(self, "_plugin_nav", None)
        if nav is None or not nav.winfo_exists():
            return
        self._clear_conflict_highlight()
        lost_only = bool(getattr(self, "_plugin_lost_only", None) and self._plugin_lost_only.get())
        marked: set[str] = set()
        for iid in self._conflict_adjacency(lost_only=lost_only).get(plugin, set()):
            if nav.exists(iid):
                tags = nav.item(iid, "tags")
                if CONFLICT_TAG not in tags:
                    nav.item(iid, tags=(*tags, CONFLICT_TAG))
                marked.add(iid)
        self._highlighted_plugins = marked
        if marked:
            self.status_var.set(
                _("%(name)s conflicts with %(n)d plugin(s) (highlighted).")
                % {"name": plugin, "n": len(marked)}
            )
        elif lost_only and getattr(self, "_conf_survey", None) is None:
            self.status_var.set(
                _("Judging the load order -- conflicts that lose work highlight when it finishes.")
            )
        else:
            self.status_var.set(_("%(name)s conflicts with nothing here.") % {"name": plugin})

    # -- the record pane -----------------------------------------------

    def _on_plugin_node(self) -> None:
        """Show the selected record, or highlight a selected plugin's conflicts."""
        nav = self._plugin_nav
        chosen = nav.selection()
        if not chosen:
            return
        # A plugin row (no separator in its id) -> highlight who it conflicts
        # with; a record row (two separators) -> show the field comparison.
        if SEP not in chosen[0] and chosen[0] in getattr(self, "_plugin_branches", {}):
            self._highlight_conflicts(chosen[0])
        if chosen[0].count(SEP) != 2:
            return
        _plugin, kind, key = chosen[0].split(SEP, 2)
        conflict = next(
            (
                entry
                for entry in getattr(self, "_shown_conflicts", None) or []
                if str(entry.get("type") or "") == kind and str(entry.get("id") or "") == key
            ),
            None,
        )
        if conflict is not None:
            self._fill_plugin_detail(conflict)

    def _fill_plugin_detail(self, conflict: Mapping[str, Any]) -> None:
        """Compare one record across its plugins, expanding lists into entries.

        Args:
            conflict: The record, as the scanner reports it.
        """
        detail = self._plugin_detail
        plugins = [str(name) for name in conflict.get("plugins") or []]
        columns = [f"p{index}" for index in range(len(plugins))]
        detail.configure(columns=columns)
        for index, plugin in enumerate(plugins):
            star = "★ " if self._is_custom(plugin) else ""
            wins = _("  (wins)") if index == len(plugins) - 1 else ""
            detail.heading(f"p{index}", text=f"{star}{plugin}{wins}")
            detail.column(f"p{index}", width=220, anchor="w", stretch=True)
        detail.delete(*detail.get_children())

        if self._conf_session is None:
            # Fold in on-demand JSON: establish the session here rather than
            # depending on a prior Check Conflicts run (see
            # _ensure_conflict_session). The JSON is still spooled lazily.
            self._ensure_conflict_session()
        if self._conf_session is None:
            detail.insert("", "end", text=_("(set a tes3conv binary to compare fields)"))
            return
        read = self.read_fields_now(conflict)
        if read is None:
            detail.insert("", "end", text=_("(busy, or this record could not be read)"))
            return
        keys, per, _diff = read
        # Stash what a double-click needs to open the full value of a field --
        # and the record's own type and label, so a visualisation opened from
        # here names the right cell instead of the conflict window's selection.
        self._plugin_detail_fd = {
            "plugins": plugins,
            "per": per,
            "record_type": str(conflict.get("type") or ""),
            "record_label": str(conflict.get("id") or ""),
        }

        statuses = field_statuses(list(keys), per, plugins)
        judged = {status.key: status for status in statuses}
        for name in keys:
            status = judged.get(name)
            tag = ALL_TAGS[status.overall][0] if status else ""
            node = detail.insert(
                "",
                "end",
                text=name,
                tags=(tag,) if tag else (),
                values=[self._fmt_val((per.get(plugin) or {}).get(name)) for plugin in plugins],
            )
            self._expand_entries(detail, node, name, per, plugins)
        # The record's own verdict, now that it has been read anyway.
        marker = (str(conflict.get("type") or ""), str(conflict.get("id") or ""))
        for plugin, verdict in record_plugin_statuses(statuses, plugins).items():
            self._paint(plugin, {marker: verdict})
        LOG.debug("%s %s: %s", marker[0], marker[1], record_status(statuses).name)

    def _expand_entries(
        self,
        detail: ttk.Treeview,
        node: str,
        field: str,
        per: Mapping[str, Mapping[str, Any]],
        plugins: Sequence[str],
    ) -> None:
        """Add one child row per entry of a repeated field.

        Only for fields that genuinely hold entries. A landscape's ``grid`` is
        a coordinate written with list syntax, and expanding it entry by entry
        would say nothing.

        Args:
            detail: The tree to add to.
            node: The field's row.
            field: The field's name.
            per: Plugin name to that plugin's field values.
            plugins: The plugins, in load order.
        """
        if not any(alignable(field, (per.get(plugin) or {}).get(field)) for plugin in plugins):
            return
        columns = {plugin: (per.get(plugin) or {}).get(field) for plugin in plugins}
        for row in align(field, columns, plugins):
            detail.insert(
                node,
                "end",
                text=f"  {row.label}",
                tags=(ALL_TAGS[row.overall][0],),
                values=[_entry_text(value) for value in row.values],
            )

    def _on_plugin_detail_double(self) -> None:
        """Open the double-clicked field's full value across every plugin.

        Reuses the conflict window's field-value popup
        (:meth:`_show_field_value`), so the tree view gets the same
        syntax-highlighted, script-disassembled, per-plugin view. An expanded
        entry row resolves to its parent field, since the value shown is the
        whole field's.
        """
        detail = self._plugin_detail
        field_data = getattr(self, "_plugin_detail_fd", None)
        selection = detail.selection()
        if not selection or not field_data:
            return
        row = selection[0]
        # An expanded entry's parent is its field row; a field row has none.
        parent = detail.parent(row)
        field_row = parent or row
        key = str(detail.item(field_row, "text")).strip()
        if not key:
            return
        self._show_field_value(
            key,
            field_data["plugins"],
            field_data["per"],
            str(field_data.get("record_type", "")),
            str(field_data.get("record_label", "")),
        )


def _tagged(tags: Any) -> ConflictThis | None:  # noqa: ANN401 - Tk returns str or tuple
    """Read a status back off a row's tags.

    Args:
        tags: Whatever ``Treeview.item(..., "tags")`` returned.

    Returns:
        The status, or ``None`` for a row that has not been judged.
    """
    names = (tags,) if isinstance(tags, str) else tuple(tags or ())
    for status in ConflictThis:
        if this_tag(status) in names:
            return status
    return None


# Any: an entry is whatever tes3conv decoded.
def _entry_text(value: Any) -> str:  # noqa: ANN401
    """Render one aligned entry for a cell.

    Args:
        value: The entry, or :data:`~wraithguard.patch.status.ABSENT`.

    Returns:
        A short rendering. Absence reads as ``--`` rather than as blank,
        because a blank cell looks like an empty value.
    """
    if value is ABSENT:
        return "--"
    if isinstance(value, dict):
        return label_for("", value) or str(value)
    return str(value)
