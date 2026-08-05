"""Conflict windows: record/resource conflict scans, field diff, CSV export.

Split out of the ``App`` class in ``wraithguard_toolkit_gui.py`` as a mixin
(CODE_REVIEW.md §16/§9.2, 3.0). Method bodies are verbatim; ``App`` inherits
this class, so ``self`` is the running ``App`` instance and every attribute
reference resolves exactly as it did when the methods lived there.
"""

from __future__ import annotations

import itertools
import json
import threading
import tkinter as tk
import traceback
import webbrowser
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from tkinter import filedialog, messagebox, scrolledtext, ttk
from typing import TYPE_CHECKING, Any, ClassVar, Final, Literal

import wraithguard_toolkit as core
from wraithguard.gui import app_base_dir
from wraithguard.gui.theme import (
    DARK,
    THEME_PRESETS,
    _json_syntax_colors,
    apply_titlebar_theme,
    highlight_json_with_html,
    highlight_plain_text_with_html,
    style_json_syntax_tags,
)
from wraithguard.gui.widgets import QueueWriter, add_tooltip
from wraithguard.i18n import gettext as _, ngettext
from wraithguard.images.compare import Comparison, Verdict, compare_bytes, difference_image
from wraithguard.images.image import ImageError
from wraithguard.images.png import encode_png
from wraithguard.images.reader import browser_image, read_image
from wraithguard.images.viewer import Maps, build_compare_page
from wraithguard.logging_setup import get_logger
from wraithguard.nif import MeshAnalyser
from wraithguard.nif.geometry import block_tree, world_meshes
from wraithguard.nif.reader import NifParseError
from wraithguard.nif.textures import TextureResolver
from wraithguard.nif.vfs import read_mesh
from wraithguard.nif.viewer import build_viewer_page
from wraithguard.patch import FieldChoice, Selection
from wraithguard.patch.status import ConflictThis
from wraithguard.patch.summary import ALL_TAGS, Survey, row_tag_updates, survey
from wraithguard.plugins import PluginFileIndex
from wraithguard.viz.library import ViewerError, three_source
from wraithguard.viz.serve import Payload, ViewerServer

LOG_GUI = get_logger(__name__)

#: Past this many records the plugin summary asks first. Each one is a separate
#: tes3conv read, and a real load order produces tens of thousands of them.
SURVEY_WARN_AT: Final = 10000

#: The conflict list is recoloured a chunk at a time, handing control back to
#: the event loop between chunks, so tagging tens of thousands of rows fills in
#: over a second or two instead of freezing the window while one loop does them
#: all. RECOLOUR_CHUNK rows per turn, RECOLOUR_PACE_MS between turns -- small
#: enough that the window keeps answering the window manager, large enough that
#: the whole list still finishes quickly.
RECOLOUR_CHUNK: Final = 400
RECOLOUR_PACE_MS: Final = 1

#: How often the summary reports progress, in records.
#: How long a UI-thread read will wait for the tes3conv session before giving
#: up and saying so. Long enough to cover a normal plugin read, short enough
#: that the window never stops answering the window manager.
UI_READ_WAIT: Final = 2.0

if TYPE_CHECKING:
    import queue
    from collections.abc import Callable, Mapping, Sequence

# Compiled-script disassembly for the field-diff window. Optional, exactly as
# in the main module: without it the diff shows the raw base64 blob. Declared
# first so the ImportError fallback to None type-checks.
listing_for_bytecode_field: Callable[..., str] | None
variables_text_for_field: Callable[..., str] | None
try:
    from wraithguard.mwscript import (
        listing_for_bytecode_field,
        variables_text_for_field,
    )
except ImportError:  # pragma: no cover - only when mwscript/ is absent
    listing_for_bytecode_field = None
    variables_text_for_field = None

# Landscape / path-grid field decoding, and the format reference that explains
# what each field is. Optional on the same terms.
text_for_field: Callable[..., str | None] | None
describe_field: Callable[[str], str | None] | None
field_note: Callable[[str, str], str | None] | None
layout_text: Callable[[str], str | None] | None
try:
    from wraithguard.tes3fields import describe_field, text_for_field
    from wraithguard.tes3fields.annotate import field_note, layout_text
except ImportError:  # pragma: no cover - only when tes3fields/ is absent
    text_for_field = None
    describe_field = None
    field_note = None
    layout_text = None

# The HTML visualisations that don't depend on the explorer/cell-page/detail
# machinery: the direct conflict map and the per-field graph/difference/3D
# views. Optional on the same terms as above: without the package the
# windows lose their "Visualise" buttons and nothing else changes.
build_conflict_map: Callable[..., str] | None
build_height_delta: Callable[..., str] | None
build_pathgrid_graph: Callable[..., str] | None
build_terrain_3d: Callable[..., str] | None
try:
    from wraithguard.viz import (
        build_conflict_map,
        build_height_delta,
        build_pathgrid_graph,
        build_terrain_3d,
    )
except ImportError:  # pragma: no cover - only when viz/ is absent
    build_conflict_map = None
    build_height_delta = None
    build_pathgrid_graph = None
    build_terrain_3d = None

#: Extensions the resource-conflict tree offers "Compare Textures" for.
#: Matched on the filename, the same as the ".nif" check below it -- the
#: comparison itself sniffs the actual bytes, so a mislabelled file still
#: fails informatively rather than silently, but a button enabled by content
#: detection would mean reading every row's bytes just to draw the toolbar.
_TEXTURE_EXTENSIONS: Final[tuple[str, ...]] = (".dds", ".tga", ".bmp", ".png")


def _as_float(value: object) -> float:
    """Coerce a field value to a float, defaulting to zero.

    Field values come from scanned third-party plugins, so a height offset can
    legitimately be missing, null, or a string. None of those is worth an
    exception when the consequence is a uniformly shifted surface.

    Args:
        value: The raw field value.

    Returns:
        The value as a float, or ``0.0`` if it cannot be read as one.
    """
    try:
        return float(value)  # type: ignore[arg-type]
    except (TypeError, ValueError):
        return 0.0


class ConflictWindowsMixin:
    """The conflict/resource windows and their workers (mixed into ``App``)."""

    if TYPE_CHECKING:
        # The host contract -- see the equivalent block in gui/t3.py for why
        # this is declared rather than silenced.
        # tk.Tk, not tk.Misc: App is built on a real toplevel and these
        # windows call transient()/title()/geometry() on it, which live on Wm
        # and not on Misc. Declaring the weaker type here type-checked fine and
        # hid those calls from mypy entirely.
        root: tk.Tk
        # This mixin owns _conflict_win (it opens the window and assigns it).
        # Declared here with the same type the sibling mixins use so the three
        # host-contract blocks agree when App inherits all of them.
        _conflict_win: tk.Toplevel | None
        log_queue: queue.Queue
        status_var: tk.StringVar
        cfg_var: tk.StringVar
        log_theme_var: tk.StringVar
        keep_json_var: tk.BooleanVar
        sort_button: ttk.Button
        export_button: ttk.Button
        conflicts_button: ttk.Button
        cellmap_button: ttk.Button
        resource_button: ttk.Button
        lint_button: ttk.Button
        order_panel: Any
        _current_plan: dict | None
        worker_running: bool
        _res_shown: list
        _tes3conv_override: str | None
        _conf_paths: dict[str, str]
        _conf_scan_args: tuple
        _conf_session: core.Tes3ConvSession | None
        _shown_conflicts: list[dict]
        # Identity of the recolour pass currently painting the tree; a new pass
        # replaces it so a superseded one stops (see _recolour_conflict_tree).
        _recolour_token: object
        _patch_button: ttk.Button
        _conf_survey: Survey | None

        # Supplied by PluginViewMixin, which owns the plugin tree window.
        def show_plugin_view(self) -> None: ...  # noqa: D102

        # Supplied by PatchBuilderMixin, which owns the patch queue.
        def queue_field(  # noqa: D102
            self, record_type: str, key: str, choice: FieldChoice
        ) -> None: ...
        def queue_whole_record(self, selection: Selection) -> None: ...  # noqa: D102
        def refresh_patch_views(self) -> None: ...  # noqa: D102
        def show_patch_builder(self) -> None: ...  # noqa: D102

        def _apply_exclusions(self, names: list[str]) -> list[str]: ...
        def _attach_hamburger_grip(self, widget: tk.Misc, orient: str) -> None: ...
        def _disassemble_bytecode_field(
            self, value: str, source_text: str | None
        ) -> str | None: ...
        def _get_session(self, conv: str | None) -> core.Tes3ConvSession | None: ...
        def _is_custom(self, name: str) -> bool: ...
        def _paned(self, parent: tk.Misc, orient: str) -> tk.PanedWindow: ...
        def _plan_scan_dirs(self) -> list[str]: ...
        def _populate_field_diff(self, conflict: dict) -> None: ...
        def _refill_res_tree(self) -> None: ...
        def _resolve_theme(self, name: str) -> dict | None: ...
        def _set_tes3conv(self) -> None: ...

    def on_check_conflicts(self) -> None:
        """Scan the current (sorted, enabled) plugins for TES3 record conflicts.

        Runs in a worker since parsing every plugin can take a moment.
        """
        if self.worker_running or not self._current_plan:
            return
        order = self._apply_exclusions(self.order_panel.get_enabled())
        if not order:
            return
        dirs = self._plan_scan_dirs()
        subset = self._current_plan.get("subset") or []
        self._keep_json = self.keep_json_var.get()
        self._conf_subset_lower = {str(s).lower() for s in subset}  # your custom mods
        self._conf_scan_args = (order, dirs, subset)
        self._conf_singles = None  # stale from any previous scan; refetched on demand
        self._conf_other_singles = None
        self.worker_running = True
        self.sort_button.configure(state="disabled")
        self.export_button.configure(state="disabled")
        self.conflicts_button.configure(state="disabled")
        self.status_var.set(_("Scanning for conflicts..."))
        threading.Thread(
            target=self._conflicts_worker, args=(order, dirs, subset), daemon=True
        ).start()

    def _conflicts_worker(self, order: list[str], dirs: list[str], subset: list[str]) -> None:
        """Scan every plugin for record conflicts, off the UI thread.

        Args:
            order: The active load order, in order -- which decides who wins.
            dirs: The data directories to resolve plugin names against.
            subset: The user's own mods, marked with a star in the result.
        """
        writer = QueueWriter(self.log_queue)
        conflicts: list[dict] = []
        stats: dict = {}
        session = None
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
                print(_(" TES3 RECORD CONFLICTS (read-only)"))
                print("=" * 70)
                if session:
                    print(
                        _("  Engine: tes3conv (%(path)s) -- field-level diffs available.")
                        % {"path": conv}
                    )
                else:
                    print(
                        "  Engine: built-in parser (record-level). Point the Conflicts window at "
                        "a tes3conv binary for field-level diffs."
                    )
                conflicts, stats = core.detect_conflicts(
                    order, index, subset_names=subset, session=session
                )
                print(core.format_conflict_report(conflicts, stats, limit=200))
            n_sub = sum(1 for c in conflicts if c.get("involves_subset"))
            status = _(
                "Conflicts: %(count)d record(s), %(involved)d involving your mods. "
                "See the Conflicts window."
            ) % {"count": stats.get("conflicts", 0), "involved": n_sub}
        except Exception:  # noqa: BLE001
            # worker top level: reports the traceback into the log panel
            writer.write("\nERROR: conflict scan failed:\n" + traceback.format_exc())
            status = "Conflict scan failed -- see log."
        finally:
            self.root.after(0, self._conflicts_finished, conflicts, stats, session, status)

    def _conflicts_finished(
        self,
        conflicts: list[dict],
        stats: dict,
        session: core.Tes3ConvSession | None,
        status: str,
    ) -> None:
        """Re-enable the window and show the record-conflict results.

        Args:
            conflicts: What the scan found.
            stats: Counts for the header.
            session: The tes3conv session the scan opened, kept so the field
                diff can reuse it rather than converting every plugin again.
            status: The line to show in the status bar.
        """
        self.worker_running = False
        self.sort_button.configure(state="normal")
        self.export_button.configure(state="normal" if self._current_plan else "disabled")
        self.conflicts_button.configure(state="normal" if self._current_plan else "disabled")
        self.cellmap_button.configure(state="normal" if self._current_plan else "disabled")
        self.resource_button.configure(state="normal" if self._current_plan else "disabled")
        self.lint_button.configure(state="normal" if self._current_plan else "disabled")
        self.status_var.set(status)
        self._conf_session = session
        self._conf_paths = (stats or {}).get("paths", {})
        self._show_conflict_window(conflicts, stats)

    def on_resource_conflicts(self) -> None:
        """Scan the data folders for loose-file (VFS) conflicts, in a worker."""
        if self.worker_running or not self._current_plan:
            return
        dirs = self._plan_scan_dirs()
        if not dirs:
            self.status_var.set(_("No data= folders to scan."))
            return
        subset_dirs = self._current_plan.get("custom_data_dirs") or core.pending_custom_dirs(
            self._current_plan.get("raw_toml_data_inserts"), self._current_plan.get("data_inserts")
        )
        self.worker_running = True
        for b in (
            self.sort_button,
            self.export_button,
            self.conflicts_button,
            self.cellmap_button,
            self.resource_button,
        ):
            b.configure(state="disabled")
        self.status_var.set(_("Scanning data folders for file conflicts..."))
        threading.Thread(
            target=self._resource_worker, args=(dirs, subset_dirs), daemon=True
        ).start()

    def _resource_worker(self, dirs: list[str], subset_dirs: list[str]) -> None:
        """Scan the data folders for loose-file conflicts, off the UI thread.

        Args:
            dirs: Every data directory, in load order -- the later folder wins
                in OpenMW, so order is the answer here too.
            subset_dirs: The folders holding the user's own mods.
        """
        writer = QueueWriter(self.log_queue)
        conflicts: list[dict] = []
        stats: dict = {}
        try:
            with redirect_stdout(writer.as_stream()), redirect_stderr(writer.as_stream()):
                print("\n" + "=" * 70)
                print(_(" DATA-PATH RESOURCE (VFS) CONFLICTS"))
                print("=" * 70)
                conflicts, stats = core.detect_resource_conflicts(dirs, subset_dirs=subset_dirs)
                # Read the meshes that conflict *and* differ, so the report and
                # the tree can mark them. Without this the GUI had the whole
                # mesh reader available and showed none of it -- the scan-time
                # pass was wired into the command line only.
                mesh_stats = core.analyse_mesh_conflicts(conflicts)
                stats = {**stats, **{f"mesh_{k}": v for k, v in mesh_stats.items()}}
                print(core.format_resource_report(conflicts, stats, limit=200))
            status = _("Resource conflicts: %(count)d file(s). See the window.") % {
                "count": stats.get("conflicts", 0)
            }
        except Exception:  # noqa: BLE001
            # worker top level: reports the traceback into the log panel
            writer.write("\nERROR: resource scan failed:\n" + traceback.format_exc())
            status = "Resource scan failed -- see log."
        finally:
            self.root.after(0, self._resource_finished, conflicts, stats, status)

    def _resource_finished(self, conflicts: list[dict], stats: dict, status: str) -> None:
        """Re-enable the window and show the loose-file results.

        Args:
            conflicts: What the scan found.
            stats: Counts for the summary line.
            status: The line to show in the status bar.
        """
        self.worker_running = False
        self.sort_button.configure(state="normal")
        for b in (
            self.export_button,
            self.conflicts_button,
            self.cellmap_button,
            self.resource_button,
            self.lint_button,
        ):
            b.configure(state="normal" if self._current_plan else "disabled")
        self.status_var.set(status)
        self._show_resource_window(conflicts, stats)

    def _show_resource_window(self, conflicts: list[dict], stats: dict) -> None:
        """Open the loose-file conflict window.

        Args:
            conflicts: The conflicts to list.
            stats: Counts for the header.
        """
        self._all_res = conflicts
        win = getattr(self, "_res_win", None)
        if win is not None and win.winfo_exists():
            win.destroy()
        win = tk.Toplevel(self.root)
        self._res_win = win
        apply_titlebar_theme(win)
        win.title("Data-path Resource Conflicts")
        win.configure(bg=DARK["bg"])
        win.geometry("900x560")
        top = ttk.Frame(win, padding=8)
        top.pack(fill="x")
        n_sub = sum(1 for c in conflicts if c.get("involves_subset"))
        ttk.Label(
            top,
            text=_(
                "%(conflicts)d loose-file conflict(s) across "
                "%(dirs)d folder(s), %(files)d file(s) - "
                "%(involved)d involve your custom data paths (★). Later folder wins - "
                "reorder the data-path panel to change it."
            )
            % {
                "conflicts": stats.get("conflicts", 0),
                "dirs": stats.get("dirs", 0),
                "files": stats.get("files", 0),
                "involved": n_sub,
            },
        ).pack(side="left")
        self._res_subset_only = tk.BooleanVar(value=False)
        ttk.Checkbutton(
            top,
            text=_("Only my paths"),
            variable=self._res_subset_only,
            command=self._refill_res_tree,
        ).pack(side="right")
        # tree (top) and the detail panel (bottom) live in a draggable vertical
        # split, so the detail box can be resized -- grab the grip to grow it.
        body = self._paned(win, "vertical")
        body.pack(fill="both", expand=True, padx=8, pady=(0, 6))

        mid = ttk.Frame(body)
        cols = ("custom", "mesh", "path", "count", "winner")
        tree = ttk.Treeview(
            mid, columns=cols, show="headings", selectmode="browse", style="Conf.Treeview"
        )
        for c, txt, w in (
            ("custom", "★", 34),
            # A marked row is one where reading the mesh found something. It
            # earns a column rather than living only in the detail panel: the
            # whole value of the finding is triage, and a signal you have to
            # click every row to see is not triage.
            ("mesh", "!", 28),
            ("path", "File", 500),
            ("count", "#", 50),
            ("winner", "Winner (loads last)", 280),
        ):
            tree.heading(c, text=txt)
            tree.column(c, width=w, anchor="w", stretch=(c in ("path", "winner")))
        vsb = ttk.Scrollbar(mid, orient="vertical", command=tree.yview)
        tree.configure(yscrollcommand=vsb.set)
        tree.grid(row=0, column=0, sticky="nsew")
        vsb.grid(row=0, column=1, sticky="ns")
        mid.rowconfigure(0, weight=1)
        mid.columnconfigure(0, weight=1)
        tree.tag_configure("sub", foreground="#ff9b6b")
        self._res_tree = tree
        body.add(mid, minsize=120, stretch="always")

        detbox = ttk.Frame(body)
        detail = tk.Text(
            detbox,
            height=7,
            wrap="word",
            background=DARK["log_bg"],
            foreground=DARK["fg"],
            relief="flat",
            highlightthickness=1,
            highlightbackground=DARK["border"],
        )
        detail.pack(fill="both", expand=True)
        detail.insert(
            "1.0",
            _(
                "Select a file to see every folder that provides it, in load order.\n"
                "Selecting a mesh (.nif) also reads it and describes what each "
                "provider contains -- shapes, textures, collision, animation -- "
                "and what the winning mesh loses. Rows marked ! in the second "
                "column already have a finding."
            ),
        )
        detail.configure(state="disabled")
        body.add(detbox, minsize=70)
        self._attach_hamburger_grip(body, "vertical")

        def on_sel(_e: object = None) -> None:
            """Show the detail for whichever row is selected.

            Args:
                _e: The Tk event, unused -- present because this is bound.
            """
            sel = tree.selection()
            if not sel:
                return
            c = self._res_shown[int(sel[0])]
            lines = [
                c["path"],
                *(f"  {i + 1}. {p}" for i, p in enumerate(c["providers"])),
                f"Wins: {c['winner']}",
            ]
            # Meshes are read *here*, on selection, and never during the scan.
            # A large mod setup holds tens of thousands of meshes and the scan
            # has no idea which one anybody cares about; by the time a row is
            # clicked, it knows exactly.
            lines.extend(self._mesh_detail(c))
            is_mesh = str(c.get("path", "")).lower().endswith(".nif")
            for name in ("_res_view3d", "_res_export3d"):
                button = getattr(self, name, None)
                if button is not None:
                    button.configure(state="normal" if is_mesh else "disabled")
            is_texture = (
                str(c.get("path", "")).lower().endswith(_TEXTURE_EXTENSIONS)
                and len(c.get("providers", [])) >= 2
            )
            for name in ("_res_view_texture", "_res_export_texture"):
                button = getattr(self, name, None)
                if button is not None:
                    button.configure(state="normal" if is_texture else "disabled")
            detail.configure(state="normal")
            detail.delete("1.0", "end")
            detail.insert("1.0", "\n".join(lines))
            detail.configure(state="disabled")

        tree.bind("<<TreeviewSelect>>", on_sel)
        btns = ttk.Frame(win, padding=8)
        btns.pack(fill="x")
        ttk.Button(btns, text=_("Save report (CSV)..."), command=self._save_resource_csv).pack(
            side="left"
        )
        self._res_view3d = ttk.Button(
            btns, text=_("View in 3D"), command=self._open_mesh_viewer, state="disabled"
        )
        self._res_view3d.pack(side="left", padx=(8, 0))
        self._res_export3d = ttk.Button(
            btns, text=_("Export 3D file..."), command=self._export_mesh_viewer, state="disabled"
        )
        self._res_export3d.pack(side="left", padx=(4, 0))
        self._res_view_texture = ttk.Button(
            btns,
            text=_("Compare Textures"),
            command=self._open_texture_viewer,
            state="disabled",
        )
        self._res_view_texture.pack(side="left", padx=(8, 0))
        self._res_export_texture = ttk.Button(
            btns,
            text=_("Export comparison..."),
            command=self._export_texture_viewer,
            state="disabled",
        )
        self._res_export_texture.pack(side="left", padx=(4, 0))
        ttk.Button(btns, text=_("Close"), command=win.destroy).pack(side="right")
        self._refill_res_tree()

    def _mesh_detail(self, conflict: dict) -> list[str]:
        """Read the meshes behind one selected conflict.

        Kept off the scan path deliberately -- see the caller. Failures are
        shown rather than raised: a mod folder holds meshes for other engines
        and truncated downloads, and neither should close the window a user
        just opened.

        Args:
            conflict: The selected conflict entry.

        Returns:
            Lines to append to the detail panel, empty when it is not a mesh.
        """
        analyser = getattr(self, "_mesh_analyser", None)
        if analyser is None:
            analyser = MeshAnalyser()
            self._mesh_analyser = analyser
        try:
            return core.describe_mesh_detail(analyser, conflict)
        except OSError as exc:
            return [_("Could not read the meshes: %(error)s") % {"error": exc}]

    def _mesh_sides(self, conflict: dict) -> tuple[list[tuple[str, list]], list[list]]:
        """Read every provider of a mesh conflict.

        Args:
            conflict: The selected conflict entry.

        Returns:
            ``(label, meshes)`` pairs and the matching block trees. Both come
            from one parse per provider -- reading each file twice to get the
            geometry and then the structure would double the cost of opening
            a view for no reason.

        Raises:
            NifParseError: If a mesh cannot be parsed.
            OSError: If one cannot be read.
        """
        path = str(conflict.get("path", ""))
        sides: list[tuple[str, list]] = []
        trees: list[list] = []
        for provider in conflict["providers"]:
            folder = Path(str(provider))
            parsed = read_mesh(folder, path)
            sides.append((f"{folder.name} / {path}", world_meshes(parsed)))
            trees.append(block_tree(parsed))
        return sides, trees

    def _texture_resolver(self, conflict: dict) -> TextureResolver | None:
        """Build a texture index for the folders this scan covered.

        Textures are resolved across *all* the data folders, not just the one
        providing the mesh: a mesh in one mod routinely draws with a texture
        another mod ships, and resolving only within its own folder would show
        half a mod collection untextured.

        Args:
            conflict: The selected conflict, for its providers.

        Returns:
            A resolver, or ``None`` when there is nothing to index.
        """
        dirs = [Path(str(d)) for d in (self._plan_scan_dirs() or [])]
        if not dirs:
            dirs = [Path(str(p)) for p in conflict.get("providers", [])]
        if not dirs:
            return None
        cached = getattr(self, "_texture_index", None)
        key = tuple(str(d) for d in dirs)
        if cached is not None and cached[0] == key:
            return cached[1]
        resolver = TextureResolver(dirs)
        self._texture_index = (key, resolver)
        return resolver

    def _selected_mesh_conflict(self) -> dict | None:
        """The selected row, when it is a mesh.

        Returns:
            The conflict entry, or ``None``.
        """
        tree = getattr(self, "_res_tree", None)
        selection = tree.selection() if tree is not None else ()
        if not selection:
            return None
        conflict = self._res_shown[int(selection[0])]
        return conflict if str(conflict.get("path", "")).lower().endswith(".nif") else None

    def _viewer_server(self) -> ViewerServer | None:
        """The loopback server, started on first use and shared by every view.

        One server for the whole session -- the mesh viewer, the texture
        comparison, and whatever else ends up wanting a browser tab -- rather
        than one per feature.
        :meth:`~wraithguard.viz.serve.ViewerServer.publish_session` is what
        keeps them from colliding on it.

        Returns:
            The running server, or ``None`` when no socket could be bound --
            which happens on locked-down machines and is why a standalone
            export still exists for every view that has one.
        """
        server: ViewerServer | None = getattr(self, "_shared_viewer_server", None)
        if server is None:
            server = ViewerServer()
            self._shared_viewer_server = server
        if not server.running:
            try:
                server.start()
            except OSError as exc:
                LOG_GUI.warning("no loopback port for the viewer: %s", exc)
                return None
        return server

    def _three_js_url(self, server: ViewerServer) -> str:
        """The URL for the vendored three.js build.

        Published under a fixed, un-namespaced key deliberately: the bytes are
        the same build regardless of which view asks for them or how many
        times, so giving every session its own copy under its own namespace
        would only store duplicates of a file that never changes within a run
        -- worth avoiding once more than one kind of view wants it.

        Args:
            server: The running server.

        Returns:
            The URL three.js is served from.
        """
        return server.publish(
            "three.js", Payload(three_source().encode("utf-8"), "text/javascript")
        )

    def _open_mesh_viewer(self) -> None:
        """Show the selected mesh conflict in 3D.

        Served over loopback when a port can be bound: the page is a few
        kilobytes and three.js is fetched once and cached, instead of a
        multi-megabyte document rebuilt per view. Falls back to the standalone
        page, which is the same builder with the bytes carried inline.
        """
        conflict = self._selected_mesh_conflict()
        if conflict is None:
            return
        path = str(conflict.get("path", ""))
        try:
            sides, trees = self._mesh_sides(conflict)
            server = self._viewer_server()
            if server is None:
                self._open_html_view(
                    build_viewer_page(
                        sides, title=path, trees=trees, resolver=self._texture_resolver(conflict)
                    ),
                    "mesh_view",
                    _("Mesh view"),
                )
                return
            session = server.publish_session("mesh")
            counter = itertools.count()

            def sink(blob: bytes, content_type: str = "") -> dict[str, str]:
                """Spool one embedded asset to disk for the generated page.

                Args:
                    blob: The bytes to write.
                    content_type: Its MIME type, which picks the extension.

                Returns:
                    What the page should use to reference it.
                """
                kind = content_type or "application/octet-stream"
                suffix = "png" if content_type.startswith("image/") else "bin"
                key = f"g{next(counter)}.{suffix}"
                return {"url": session.publish(key, Payload(blob, kind))}

            page = build_viewer_page(
                sides,
                title=path,
                sink=sink,
                library_url=self._three_js_url(server),
                trees=trees,
                resolver=self._texture_resolver(conflict),
            )
            url = session.publish("index.html", Payload(page.encode("utf-8"), "text/html"))
        except (ViewerError, NifParseError, OSError) as exc:
            messagebox.showerror(_("Cannot show this mesh"), str(exc))
            return
        # Through the same chain as every other visualisation. The served page
        # is a URL rather than a file, which the chain now understands: the one
        # viewer it cannot use is tkinterweb, whose load_file cannot fetch, and
        # this page needs real requests for its geometry.
        opener = getattr(self, "open_html_in_app", None)
        if callable(opener):
            opener(url, _("Mesh view"))
        else:  # pragma: no cover - only if the mixin is used outside App
            webbrowser.open(url)
        self.status_var.set(_("Opened the 3D view for %(path)s") % {"path": path})

    def _export_mesh_viewer(self) -> None:
        """Write the selected mesh conflict as one standalone HTML file.

        The served page is smaller and quicker; this one survives being moved,
        kept or sent to someone, which the served page cannot.
        """
        conflict = self._selected_mesh_conflict()
        if conflict is None:
            return
        path = str(conflict.get("path", ""))
        target = filedialog.asksaveasfilename(
            title=_("Export the 3D view"),
            defaultextension=".html",
            initialfile=f"{Path(path).stem}_3d.html",
            filetypes=(("HTML files", "*.html"), ("All files", "*.*")),
        )
        if not target:
            return
        try:
            sides, trees = self._mesh_sides(conflict)
            page = build_viewer_page(
                sides, title=path, trees=trees, resolver=self._texture_resolver(conflict)
            )
            Path(target).write_text(page, encoding="utf-8")
        except (ViewerError, NifParseError, OSError) as exc:
            messagebox.showerror(_("Export failed"), str(exc))
            return
        self.status_var.set(_("Exported: %(path)s") % {"path": target})

    def _selected_texture_conflict(self) -> dict | None:
        """The selected row, when it is a texture with something to compare.

        Mirrors :meth:`_selected_mesh_conflict`. A conflict with fewer than
        two providers cannot happen from the scan itself, but is excluded
        explicitly rather than trusted, the same way :meth:`_texture_sides`
        does not trust it either.

        Returns:
            The conflict entry, or ``None``.
        """
        tree = getattr(self, "_res_tree", None)
        selection = tree.selection() if tree is not None else ()
        if not selection:
            return None
        conflict = self._res_shown[int(selection[0])]
        path = str(conflict.get("path", "")).lower()
        if not path.endswith(_TEXTURE_EXTENSIONS):
            return None
        if len(conflict.get("providers", [])) < 2:
            return None
        return conflict

    def _texture_provider_dirs(self, conflict: dict) -> tuple[Path, Path]:
        """The two providers a texture comparison actually needs.

        Unlike the mesh view -- which shows every provider at once because
        three.js has room for it -- a texture comparison is inherently a pair
        of images. With more than two providers this picks the winner (what
        actually loads) and the one immediately below it in load order (what
        it replaced), rather than every provider or the earliest one: that is
        the pair whose difference the winning file is actually responsible
        for.

        Args:
            conflict: The selected conflict entry.

        Returns:
            The overridden provider's folder, then the winner's.
        """
        providers = [Path(str(p)) for p in conflict["providers"]]
        return providers[-2], providers[-1]

    def _texture_sides(self, conflict: dict) -> tuple[tuple[str, bytes], tuple[str, bytes]]:
        """Read the two textures a comparison actually needs.

        Args:
            conflict: The selected conflict entry.

        Returns:
            ``(label, bytes)`` for the overridden provider, then the winner.

        Raises:
            OSError: If either file cannot be read.
        """
        path = str(conflict.get("path", ""))
        overridden_dir, winner_dir = self._texture_provider_dirs(conflict)
        overridden_bytes = (overridden_dir / path).read_bytes()
        winner_bytes = (winner_dir / path).read_bytes()
        return (
            (f"{overridden_dir.name} / {path}", overridden_bytes),
            (f"{winner_dir.name} / {path}", winner_bytes),
        )

    def _texture_maps(self, provider_dir: Path, reference: str) -> Maps:
        """Find one provider's own auxiliary maps for a texture.

        A resolver scoped to just this one folder, not the shared
        multi-folder one :meth:`_texture_resolver` builds for the mesh view.
        That one answers "what would actually load" across every data folder,
        merged -- right for a single 3D scene, wrong here: a side-by-side
        comparison wants each side's *own* normal/specular map, even when a
        later-loading mod's version would win the VFS. Showing the winner's
        map beside both textures would defeat the point of comparing them.

        Indexing one folder is cheap enough to do per side, per view -- it
        walks a single mod's ``textures/`` directory, not the whole scan.

        Args:
            provider_dir: The one data folder this side of the comparison
                came from.
            reference: The diffuse texture's own path.

        Returns:
            Suffix to displayable bytes and MIME type, for the maps this
            folder ships beside the texture. Empty when it ships none, which
            is the common case and is why the lit view's controls stay
            conditional.
        """
        vfs_dirs = self._plan_scan_dirs()
        if vfs_dirs:
            search_dirs = [Path(str(d)) for d in vfs_dirs]
            if provider_dir not in search_dirs:
                search_dirs.append(provider_dir)
        else:
            search_dirs = [provider_dir]
        resolver = TextureResolver(search_dirs)
        maps: Maps = {}
        for suffix, resolved in resolver.siblings(reference).items():
            data = resolver.read(resolved)
            if data is None:
                continue
            try:
                maps[suffix] = browser_image(data)
            except ImageError:
                continue
        return maps

    def _texture_compare_payload(self, conflict: dict) -> tuple[
        tuple[str, bytes, str],
        tuple[str, bytes, str],
        Comparison,
        tuple[bytes, str] | None,
        Maps,
        Maps,
        str,
    ]:
        """Read and compare the selected texture conflict.

        Shared by :meth:`_open_texture_viewer` and :meth:`_export_texture_viewer`
        so reading both files, comparing them and building a difference image
        happens once regardless of which one a person reaches for -- the same
        split :meth:`_mesh_sides` already makes for the mesh viewer.

        Args:
            conflict: The selected conflict entry.

        Returns:
            Everything :func:`~wraithguard.images.viewer.build_compare_page`
            needs except how to publish it: both displayable sides, the
            comparison outcome, a difference image when there is one, each
            side's own auxiliary maps, and the asset path.

        Raises:
            OSError: If either file cannot be read.
            ImageError: If either cannot be decoded.
        """
        path = str(conflict.get("path", ""))
        overridden_dir, winner_dir = self._texture_provider_dirs(conflict)
        (left_name, left_bytes), (right_name, right_bytes) = self._texture_sides(conflict)
        outcome = compare_bytes(left_bytes, right_bytes, reference=path)
        left_display, left_mime = browser_image(left_bytes)
        right_display, right_mime = browser_image(right_bytes)
        difference = None
        # worst_channel is only ever 0 when compare_bytes skipped pixel
        # measurement outright (identical bytes, a decode failure, a size
        # mismatch, or a pair too large to measure) -- every one of those
        # is also a case with no difference image to build.
        if outcome.verdict is Verdict.DIFFERENT and outcome.worst_channel > 0:
            diff_img = difference_image(read_image(left_bytes), read_image(right_bytes))
            difference = (encode_png(diff_img), "image/png")
        left_maps = self._texture_maps(overridden_dir, path)
        right_maps = self._texture_maps(winner_dir, path)
        return (
            (left_name, left_display, left_mime),
            (right_name, right_display, right_mime),
            outcome,
            difference,
            left_maps,
            right_maps,
            path,
        )

    def _open_texture_viewer(self) -> None:
        """Show the selected texture conflict: side by side, wipe, and difference.

        Served over loopback when a port can be bound, through the same
        shared server as the mesh viewer -- worth doing now that the wipe view
        wants three.js on effectively every comparison rather than only the
        ones with an auxiliary map, which makes three.js itself, not the
        textures, the payload worth not re-embedding on every open. Falls back
        to the standalone page, built the same way :meth:`_export_texture_viewer`
        always builds it, when no port could be bound.

        Unlike the mesh viewer, a missing three.js does not stop this one
        from opening: :func:`~wraithguard.images.viewer.build_compare_page`
        needs it for the lit view and the WebGL overlay only, and degrades to
        its CSS fallback on its own when it cannot be found -- see
        :func:`~wraithguard.images.viewer._inline_library`. Publishing it to
        the server gets the same treatment here, so a build that shipped
        without the library still gets a working, merely less capable, served
        page rather than an error dialog over a feature it was not using.
        """
        conflict = self._selected_texture_conflict()
        if conflict is None:
            return
        try:
            left, right, outcome, difference, left_maps, right_maps, path = (
                self._texture_compare_payload(conflict)
            )
            server = self._viewer_server()
            if server is None:
                page = build_compare_page(
                    left,
                    right,
                    outcome,
                    difference=difference,
                    title=path,
                    left_maps=left_maps,
                    right_maps=right_maps,
                )
                self._open_html_view(page, "texture_compare", _("Texture comparison"))
                return
            session = server.publish_session("texture")
            counter = itertools.count()

            def sink(blob: bytes, content_type: str = "") -> dict[str, str]:
                """Spool one embedded asset to disk for the generated page.

                Args:
                    blob: The bytes to write.
                    content_type: Its MIME type, which picks the extension.

                Returns:
                    What the page should use to reference it.
                """
                kind = content_type or "application/octet-stream"
                suffix = "png" if content_type.startswith("image/") else "bin"
                key = f"t{next(counter)}.{suffix}"
                return {"url": session.publish(key, Payload(blob, kind))}

            try:
                library_url = self._three_js_url(server)
            except ViewerError:
                library_url = ""
            page = build_compare_page(
                left,
                right,
                outcome,
                difference=difference,
                title=path,
                sink=sink,
                left_maps=left_maps,
                right_maps=right_maps,
                library_url=library_url,
            )
            url = session.publish("index.html", Payload(page.encode("utf-8"), "text/html"))
        except (OSError, ImageError) as exc:
            messagebox.showerror(_("Cannot show this texture"), str(exc))
            return
        opener = getattr(self, "open_html_in_app", None)
        if callable(opener):
            opener(url, _("Texture comparison"))
        else:  # pragma: no cover - only if the mixin is used outside App
            webbrowser.open(url)
        self.status_var.set(_("Opened the texture comparison for %(path)s") % {"path": path})

    def _export_texture_viewer(self) -> None:
        """Write the selected texture comparison as one standalone HTML file.

        Mirrors :meth:`_export_mesh_viewer`: the served page is smaller and
        quicker to open, this one survives being moved, kept or sent to
        someone, which the served page cannot.
        """
        conflict = self._selected_texture_conflict()
        if conflict is None:
            return
        path = str(conflict.get("path", ""))
        target = filedialog.asksaveasfilename(
            title=_("Export the texture comparison"),
            defaultextension=".html",
            initialfile=f"{Path(path).stem}_compare.html",
            filetypes=(("HTML files", "*.html"), ("All files", "*.*")),
        )
        if not target:
            return
        try:
            left, right, outcome, difference, left_maps, right_maps, path = (
                self._texture_compare_payload(conflict)
            )
            page = build_compare_page(
                left,
                right,
                outcome,
                difference=difference,
                title=path,
                left_maps=left_maps,
                right_maps=right_maps,
            )
            Path(target).write_text(page, encoding="utf-8")
        except (OSError, ImageError) as exc:
            messagebox.showerror(_("Export failed"), str(exc))
            return
        self.status_var.set(_("Exported: %(path)s") % {"path": target})

    def _save_resource_csv(self) -> None:
        """Write the loose-file conflicts to a CSV the user chooses."""
        if not getattr(self, "_all_res", None):
            return
        path = filedialog.asksaveasfilename(
            title=_("Save resource conflicts"),
            defaultextension=".csv",
            initialfile="resource_conflicts.csv",
            filetypes=(("CSV files", "*.csv"), ("All files", "*.*")),
        )
        if not path:
            return
        try:
            core.write_resource_csv(path, self._all_res)
            self.status_var.set(_("Saved: %(path)s") % {"path": path})
        except OSError as e:
            messagebox.showerror(_("Save failed"), str(e))

    def _show_conflict_window(self, conflicts: list[dict], stats: dict) -> None:
        """Open the record conflict window.

        Args:
            conflicts: The conflicts to list.
            stats: Counts for the header.
        """
        self._all_conflicts = conflicts
        win = getattr(self, "_conflict_win", None)
        if win is not None and win.winfo_exists():
            win.destroy()
        win = tk.Toplevel(self.root)
        self._conflict_win = win
        apply_titlebar_theme(win)
        win.title("TES3 Record Conflicts")
        win.configure(bg=DARK["bg"])
        win.geometry("980x680")

        top = ttk.Frame(win, padding=8)
        top.pack(fill="x")
        n_sub = sum(1 for c in conflicts if c.get("involves_subset"))
        ttk.Label(
            top,
            text=_(
                "%(conflicts)d conflicting record(s) across "
                "%(scanned)d plugin(s) - %(involved)d involve your custom mods "
                "(★). Winner = last loaded."
            )
            % {
                "conflicts": stats.get("conflicts", 0),
                "scanned": stats.get("scanned", 0),
                "involved": n_sub,
            },
        ).pack(side="left")
        self._conf_subset_only = tk.BooleanVar(value=False)
        ttk.Checkbutton(
            top,
            text=_("Only my mods"),
            variable=self._conf_subset_only,
            command=self._refill_conflict_tree,
        ).pack(side="right")
        self._include_singles_var = tk.BooleanVar(value=False)
        ttk.Checkbutton(
            top,
            text=_("Include my mods' non-conflicting records"),
            variable=self._include_singles_var,
            command=lambda: self._toggle_singles("mine"),
        ).pack(side="right", padx=(0, 12))
        self._other_singles_var = tk.BooleanVar(value=False)
        ttk.Checkbutton(
            top,
            text=_("Include other mods' non-conflicting records"),
            variable=self._other_singles_var,
            command=lambda: self._toggle_singles("other"),
        ).pack(side="right", padx=(0, 12))

        engine = (stats or {}).get("engine", "builtin")
        bar = ttk.Frame(win, padding=(8, 0))
        bar.pack(fill="x")
        ttk.Label(
            bar,
            foreground=(DARK["fg_dim"] if engine == "tes3conv" else "#ffb454"),
            text=(
                "Field-level diffs: ON (tes3conv)."
                if engine == "tes3conv"
                else "Field-level diffs: OFF - record-level only. Set a tes3conv binary, then re-check."
            ),
        ).pack(side="left")
        ttk.Button(bar, text=_("Set tes3conv..."), command=self._set_tes3conv).pack(
            side="left", padx=(8, 0)
        )

        panes = tk.PanedWindow(
            win,
            orient="vertical",
            bg=DARK["bg"],
            bd=0,
            sashwidth=6,
            sashrelief="flat",
            background=DARK["border"],
        )
        panes.pack(fill="both", expand=True, padx=8, pady=6)

        # --- conflicts table ---
        topf = ttk.Frame(panes)
        cols = ("custom", "type", "id", "count", "winner")
        tree = ttk.Treeview(
            topf, columns=cols, show="headings", selectmode="browse", style="Conf.Treeview"
        )
        for c, txt, w in (
            ("custom", "★", 34),
            ("type", "Type", 90),
            ("id", "Record", 380),
            ("count", "#", 40),
            ("winner", "Winner (loads last)", 280),
        ):
            tree.heading(c, text=txt)
            tree.column(c, width=w, anchor="w", stretch=(c in ("id", "winner")))
        vsb = ttk.Scrollbar(topf, orient="vertical", command=tree.yview)
        tree.configure(yscrollcommand=vsb.set)
        tree.grid(row=0, column=0, sticky="nsew")
        vsb.grid(row=0, column=1, sticky="ns")
        topf.rowconfigure(0, weight=1)
        topf.columnconfigure(0, weight=1)
        tree.tag_configure("sub", foreground="#ff9b6b")
        # Filled in by the plugin summary. Until it runs the list says only
        # that these records conflict, which is what it has always said.
        tree.tag_configure("status-unknown", foreground=DARK["fg"])
        tree.tag_configure("status-only-one", foreground=DARK["fg_dim"])
        tree.tag_configure("status-agree", foreground=DARK["fg_dim"])
        tree.tag_configure("status-benign", foreground="#e8c07d")
        tree.tag_configure("status-conflict", foreground="#ff6b6b")
        self._conf_tree = tree
        panes.add(topf, minsize=150, stretch="always")

        # --- field-level comparison (populated on record select) ---
        botf = ttk.Frame(panes)
        ttk.Label(
            botf,
            foreground=DARK["fg_dim"],
            text=_(
                "Field comparison for the selected record - RED = a plugin's edit is "
                "discarded · AMBER = overridden but nothing lost · ★ = your custom mod · "
                "last column wins · double-click a field for the full value:"
            ),
        ).grid(row=0, column=0, columnspan=2, sticky="w", pady=(2, 2))
        ftree = ttk.Treeview(botf, show="headings", selectmode="browse", style="Conf.Treeview")
        fvsb = ttk.Scrollbar(botf, orient="vertical", command=ftree.yview)
        fhsb = ttk.Scrollbar(botf, orient="horizontal", command=ftree.xview)
        ftree.configure(yscrollcommand=fvsb.set, xscrollcommand=fhsb.set)
        ftree.grid(row=1, column=0, sticky="nsew")
        fvsb.grid(row=1, column=1, sticky="ns")
        fhsb.grid(row=2, column=0, sticky="ew")
        botf.rowconfigure(1, weight=1)
        botf.columnconfigure(0, weight=1)
        ftree.tag_configure("diff", foreground="#ff6b6b")
        # Four outcomes, not two. The names and their meanings live in
        # wraithguard.patch.summary.ALL_TAGS so the wording can be tested
        # without a display; only the colours are chosen here.
        ftree.tag_configure("status-unknown", foreground=DARK["fg_dim"])
        ftree.tag_configure("status-only-one", foreground=DARK["fg_dim"])
        ftree.tag_configure("status-agree", foreground=DARK["fg"])
        ftree.tag_configure("status-benign", foreground="#e8c07d")
        ftree.tag_configure("status-conflict", foreground="#ff6b6b")
        ftree.bind("<Double-Button-1>", lambda _e: self._show_field_detail())
        add_tooltip(
            ftree,
            _(
                "Field-by-field diff of the selected record. Red = the plugins disagree; "
                "the last one in the load order wins.\n\n"
                "Double-click any row for the full value, one tab per plugin. Fields "
                "stored as binary are decoded rather than shown raw, so an edit reads "
                "as a change instead of a wall of base64:\n"
                "  \u2022 bytecode -- disassembled to named script instructions. Spans the "
                "disassembler cannot decode are printed as offset/hex/ASCII rather than "
                "guessed at, and a 'decoded: N%' header says how much was understood.\n"
                "  \u2022 variables -- the script's local variable names, in declaration order.\n"
                "  \u2022 landscape (vertex heights, normals, colors, textures, world map) "
                "-- decoded to one terrain row per line, so the diff shows which rows of "
                "the cell moved. Heights are reconstructed to absolute world units.\n"
                "  \u2022 path grid connections -- decoded to a per-point adjacency list "
                "(point -> its neighbours), so you can see which nodes were rewired."
            ),
        )
        self._conf_ftree = ftree
        panes.add(botf, minsize=120, stretch="always")

        tree.bind("<<TreeviewSelect>>", lambda _e: self._on_conflict_select())

        btns = ttk.Frame(win, padding=8)
        btns.pack(fill="x")
        ttk.Button(btns, text=_("Save report (CSV)..."), command=self._save_conflicts_csv).pack(
            side="left"
        )
        if self._conf_session is not None:
            ttk.Button(
                btns, text=_("Dump tes3conv JSON..."), command=self._dump_conflict_json
            ).pack(side="left", padx=(8, 0))
        if build_conflict_map is not None:
            cmap_button = ttk.Button(
                btns, text=_("Conflict map (direct)..."), command=self._show_conflict_map_direct
            )
            cmap_button.pack(side="left", padx=(8, 0))
            add_tooltip(
                cmap_button,
                _(
                    "Build and open a conflict map directly from the selected conflicts. "
                    "Shows which mods edit LAND records in each cell, with a breakdown of "
                    "terrain shape, NPC navigation, and cell record edits."
                ),
            )
        patch_add = ttk.Button(
            btns, text=_("Add record to patch..."), command=self._add_record_to_patch
        )
        patch_add.pack(side="left", padx=(8, 0))
        merge_field = ttk.Button(
            btns, text=_("Merge field..."), command=self._merge_field_into_patch
        )
        merge_field.pack(side="left", padx=(8, 0))
        add_tooltip(
            merge_field,
            _(
                "Take the selected field from a plugin of your choosing and keep the "
                "rest of the record as it is now -- for when one mod fixed one thing "
                "and another mod fixed something else in the same record."
            ),
        )
        add_tooltip(
            patch_add,
            _(
                "Choose which plugin's version of the selected record should win, and "
                "add it to a patch. Nothing is written yet, and no mod is ever modified: "
                "the patch is one new plugin that loads last."
            ),
        )
        self._patch_button = ttk.Button(
            btns, text=_("Patch Builder..."), command=self.show_patch_builder
        )
        self._patch_button.pack(side="left", padx=(8, 0))
        add_tooltip(
            self._patch_button,
            _(
                "Review and edit the records queued so far, then write them as one "
                "new plugin. Nothing is written until you say so."
            ),
        )
        view_button = ttk.Button(btns, text=_("Plugin view..."), command=self.show_plugin_view)
        view_button.pack(side="left", padx=(8, 0))
        add_tooltip(
            view_button,
            _(
                "The same scan with the load order's own shape restored: plugin, then "
                "kind of record, then record, with the record compared across every "
                "plugin that defines it.\n\n"
                "A flat list answers 'what conflicts'. This answers 'what does this mod "
                "change, and where does it lose', which is the question you are actually "
                "asking. Run 'Plugin summary' first to colour it in. Read-only."
            ),
        )
        if self._conf_session is not None:
            summary_button = ttk.Button(
                btns, text=_("Plugin summary..."), command=self._survey_conflicts
            )
            summary_button.pack(side="left", padx=(8, 0))
            add_tooltip(
                summary_button,
                _(
                    "Judge every conflict in the list and count the result per mod, "
                    "rather than per record.\n\n"
                    "A flat list of conflicts cannot answer the question that decides "
                    "load order: which of my mods is losing work? This compares each "
                    "record field by field and counts, for every plugin, how many "
                    "records it edits and does not get its way on.\n\n"
                    "Reads every conflicting record with tes3conv, so it takes a "
                    "moment on a large load order. Read-only."
                ),
            )
        ttk.Button(btns, text=_("Close"), command=win.destroy).pack(side="right")
        self.refresh_patch_views()
        self._refill_conflict_tree()

    def _session_lock(self) -> threading.Lock:
        """The lock guarding the tes3conv session.

        The session is one process answering one request at a time down one
        pipe. Two threads talking to it at once do not fail -- they interleave,
        and each gets the other's answer, which is the worst possible outcome
        because it looks like data.

        Holds are one request long, so a background scan and a click never
        block each other for more than a single record. That is why this is a
        lock rather than a refusal: refusing made the window feel broken while
        a summary ran.

        Returns:
            The lock, created on first use.
        """
        lock = getattr(self, "_conf_lock", None)
        if lock is None:
            lock = threading.Lock()
            self._conf_lock = lock
        return lock

    def read_fields_now(
        self, conflict: Mapping[str, Any]
    ) -> tuple[list[str], dict[str, dict[str, Any]], set[str]] | None:
        """Read one record's fields from the UI thread, without ever hanging.

        The UI thread must never wait on the session lock indefinitely. It did,
        once: a Plugin summary held the lock for its whole run, a click in the
        Plugin view asked for a record, and the application stopped answering
        the window manager -- which reports that as "not responding" and offers
        to kill it. That is the crash this exists to prevent.

        Reads are now short (one plugin each), so a brief wait is reasonable
        and a long one means something is wrong. Waiting a bounded moment and
        then saying so is honest; waiting forever is not.

        Args:
            conflict: The record, as the scanner reports it.

        Returns:
            ``(keys, per plugin values, differing keys)``, or ``None`` when the
            session is busy or the record cannot be read.
        """
        if self._conf_session is None:
            return None
        lock = self._session_lock()
        if not lock.acquire(timeout=UI_READ_WAIT):
            self.status_var.set(_("Busy reading plugins -- try that again in a moment."))
            return None
        try:
            return core.diff_record_fields(self._conf_session, conflict, self._conf_paths)
        except Exception:
            LOG_GUI.exception("could not read %s %s", conflict.get("type"), conflict.get("id"))
            return None
        finally:
            lock.release()

    # -- plugin-level summary ------------------------------------------------
    #
    # The judgement and the roll-up live in wraithguard.patch.summary, which
    # imports no widgets and is tested without a display. What is here is the
    # worker, the window, and nothing else.

    def _survey_conflicts(self) -> None:
        """Judge every listed conflict and summarise it per plugin."""
        if self.worker_running or self._conf_session is None:
            return
        rows = list(getattr(self, "_shown_conflicts", None) or [])
        if not rows:
            self.status_var.set(_("Nothing to summarise."))
            return
        if len(rows) > SURVEY_WARN_AT and not messagebox.askyesno(
            _("Judge every conflict?"),
            _(
                "This reads every plugin that defines any of these %(count)d records "
                "-- once each, not once per record -- and will take a few minutes. "
                "The checkboxes and filters above narrow the list first if you would "
                "rather summarise part of it.\n\nGo ahead?"
            )
            % {"count": len(rows)},
        ):
            return
        self.worker_running = True
        self._survey_total = len(rows)
        self._survey_seen = 0
        self.status_var.set(_("Judging %(count)d conflict(s)...") % {"count": len(rows)})
        threading.Thread(target=self._survey_worker, args=(rows,), daemon=True).start()

    def _survey_worker(self, rows: list[dict]) -> None:
        """Compare every conflict off the UI thread.

        Args:
            rows: The conflicts currently listed.
        """
        found: Survey | None = None
        error = ""
        try:
            # One JSON parse per plugin, not one per record. Reading each
            # record on its own re-parsed its plugins' JSON every time, which
            # on a real load order is hours of repeated work.
            read = core.batch_record_fields(
                self._conf_session,
                rows,
                self._conf_paths,
                self._survey_progress,
                # The summary only ever compares values for equality, and
                # holding 50,000 records' landscape blobs is how this runs
                # the machine out of memory.
                digest=True,
                # Per plugin read, never around the whole batch: holding it
                # for the length of a summary froze every other reader --
                # including the UI thread -- for as long as it ran.
                lock=self._session_lock(),
            )
            found = survey(rows, lambda c: read.get((c["type"], c["id"])))
        except Exception as exc:
            error = str(exc)
            LOG_GUI.exception("plugin summary failed")
        finally:
            self.root.after(0, lambda: self._survey_done(found, error))

    def _survey_progress(self, done: int, total: int) -> None:
        """Say how far through the plugins the read has got.

        Progress is counted in plugins because that is what the work is: each
        one is read once and every record wanted from it is taken in that pass.
        No sign of movement is indistinguishable from a hang, and the only
        thing worse than a slow scan is a slow scan that looks broken.

        Args:
            done: Plugins read so far.
            total: Plugins to read.
        """
        self.root.after(
            0,
            lambda: self.status_var.set(
                _("Reading plugin %(done)d of %(total)d...") % {"done": done, "total": total}
            ),
        )

    def _survey_done(self, found: Survey | None, error: str) -> None:
        """Show the summary, and recolour the conflict list with it.

        Args:
            found: The survey, or ``None`` if it failed.
            error: What went wrong, if anything.
        """
        self.worker_running = False
        if found is None:
            self.status_var.set(_("Could not summarise the conflicts."))
            messagebox.showerror(_("Summary failed"), error or _("Unknown error."))
            return
        self._conf_survey = found
        self._recolour_conflict_tree()
        self.status_var.set(
            _("Judged %(count)d record(s); %(losers)d plugin(s) losing work.")
            % {"count": len(found.records), "losers": len(found.losing_plugins)}
        )
        self._show_plugin_summary(found)

    def _recolour_conflict_tree(self) -> None:
        """Tag each listed record with what is actually happening to it.

        The list already said these records conflict. What it could not say is
        whether anything is being *lost*, which is the difference between a
        record worth opening and one that is merely popular.

        **Driven at a controlled pace.** A full-MOMW summary judges tens of
        thousands of rows, and tagging them all in one loop froze the window for
        as long as it took. Instead this applies :data:`RECOLOUR_CHUNK` rows,
        hands control back to the event loop for :data:`RECOLOUR_PACE_MS`, and
        continues -- so the colours fill in over a second or two and the window
        keeps answering throughout. A token supersedes any pass still running,
        so a filter change or a fresh summary starts clean rather than leaving
        two passes fighting over the same rows.
        """
        tree = getattr(self, "_conf_tree", None)
        found = getattr(self, "_conf_survey", None)
        if tree is None or found is None or not tree.winfo_exists():
            return

        updates = row_tag_updates(getattr(self, "_shown_conflicts", None) or [], found.records)
        token = object()
        self._recolour_token = token

        def paint(start: int) -> None:
            """Tag one chunk of rows, then reschedule for the next, or stop."""
            # A newer recolour (a filter, another summary) took over: stop, so
            # two paced passes never interleave on the same tree.
            if getattr(self, "_recolour_token", None) is not token:
                return
            if not tree.winfo_exists():
                return
            end = min(start + RECOLOUR_CHUNK, len(updates))
            for index, status, involves_subset in updates[start:end]:
                iid = str(index)
                if tree.exists(iid):
                    keep = ("sub",) if involves_subset else ()
                    tree.item(iid, tags=(*keep, ALL_TAGS[status][0]))
            if end < len(updates):
                self.root.after(RECOLOUR_PACE_MS, lambda: paint(end))

        paint(0)

    def _show_plugin_summary(self, found: Survey) -> None:
        """A window of per-plugin counts, worst first.

        Args:
            found: The survey to show.
        """
        win = tk.Toplevel(getattr(self, "_conflict_win", None) or self.root)
        apply_titlebar_theme(win)
        win.title(_("Plugin conflict summary"))
        win.configure(bg=DARK["bg"])
        win.geometry("880x520")

        ttk.Label(
            win,
            foreground=DARK["fg_dim"],
            padding=(8, 6, 8, 2),
            justify="left",
            text=_(
                "How each mod fares across the conflicts listed. 'Loses' counts records "
                "it edits where a later plugin overwrote the change with something "
                "different -- work that never reaches the game. 'Same as original' "
                "counts records it redefines without changing anything, which is usually "
                "removable but not always: an unchanged dialogue response may be there "
                "to hold a line's position in its topic."
            ),
            wraplength=850,
        ).pack(fill="x")

        frame = ttk.Frame(win, padding=8)
        frame.pack(fill="both", expand=True)
        cols = ("records", "loses", "wins", "same", "first")
        tree = ttk.Treeview(frame, columns=cols, show="tree headings", style="Conf.Treeview")
        tree.heading("#0", text=_("Plugin"))
        tree.column("#0", width=330, stretch=True)
        for name, title, width in (
            ("records", _("Records"), 90),
            ("loses", _("Loses"), 80),
            ("wins", _("Wins"), 80),
            ("same", _("Same as original"), 140),
            ("first", _("Defines first"), 110),
        ):
            tree.heading(name, text=title)
            tree.column(name, width=width, anchor="e", stretch=False)
        scroll = ttk.Scrollbar(frame, orient="vertical", command=tree.yview)
        tree.configure(yscrollcommand=scroll.set)
        tree.grid(row=0, column=0, sticky="nsew")
        scroll.grid(row=0, column=1, sticky="ns")
        frame.rowconfigure(0, weight=1)
        frame.columnconfigure(0, weight=1)
        tree.tag_configure("losing", foreground="#ff6b6b")
        tree.tag_configure("mine", foreground="#ff9b6b")

        ordered = sorted(
            found.plugins.values(),
            key=lambda entry: (-entry.losing, -sum(entry.counts.values()), entry.plugin.lower()),
        )
        for entry in ordered:
            counts = entry.counts
            tags: tuple[str, ...] = ()
            if entry.losing:
                tags = ("losing",)
            elif self._is_custom(entry.plugin):
                tags = ("mine",)
            tree.insert(
                "",
                "end",
                text=("★ " if self._is_custom(entry.plugin) else "") + entry.plugin,
                tags=tags,
                values=(
                    sum(counts.values()),
                    entry.losing or "",
                    counts.get(ConflictThis.CONFLICT_WINS, 0)
                    + counts.get(ConflictThis.OVERRIDE_WINS, 0)
                    or "",
                    entry.redundant or "",
                    counts.get(ConflictThis.MASTER, 0) or "",
                ),
            )

        note = ttk.Label(win, foreground=DARK["fg_dim"], padding=(8, 0))
        note.pack(fill="x")
        note.configure(
            text=_(
                "%(records)d record(s) judged, %(plugins)d plugin(s) involved"
                "%(unreadable)s. The conflict list behind this window is now "
                "coloured to match: red where an edit is discarded, amber where a "
                "record is overridden but nothing is lost."
            )
            % {
                "records": len(found.records),
                "plugins": len(found.plugins),
                "unreadable": (
                    _(", %(n)d could not be read") % {"n": found.unreadable}
                    if found.unreadable
                    else ""
                ),
            }
        )
        ttk.Button(win, text=_("Close"), command=win.destroy).pack(side="right", padx=8, pady=8)

    def _merge_field_into_patch(self) -> None:
        """Take the selected field from a chosen plugin, keeping the rest.

        The whole-record button answers "which side wins". This answers "this
        record, but with *this* field from somewhere else" -- which is what you
        want when one mod fixed the script and another retextured the mesh.
        """
        tree = getattr(self, "_conf_tree", None)
        ftree = getattr(self, "_conf_ftree", None)
        row = ftree.selection() if ftree else None
        sel = tree.selection() if tree else None
        if not sel or not row:
            messagebox.showinfo(
                _("Nothing selected"),
                _("Select a record above, then a field in the comparison below."),
            )
            return

        conflict = self._shown_conflicts[int(sel[0])]
        plugins = list(conflict.get("plugins") or [])
        path = str(row[0])
        if len(plugins) < 2:
            return

        chosen = self._ask_patch_winner(conflict, plugins, field=path)
        if chosen is None:
            return

        self.queue_field(
            str(conflict.get("type") or ""),
            str(conflict.get("id") or ""),
            FieldChoice(path=path, plugin=chosen),
        )
        self.show_patch_builder()

    def _add_record_to_patch(self) -> None:
        """Ask which plugin should win for the selected record, and remember it."""
        tree = getattr(self, "_conf_tree", None)
        sel = tree.selection() if tree else None
        if not sel:
            messagebox.showinfo(_("Nothing selected"), _("Select a conflicting record first."))
            return
        conflict = self._shown_conflicts[int(sel[0])]
        plugins = list(conflict.get("plugins") or [])
        if len(plugins) < 2:
            messagebox.showinfo(
                _("Nothing to choose"),
                _("Only one plugin defines this record, so there is nothing to patch."),
            )
            return

        chosen = self._ask_patch_winner(conflict, plugins)
        if chosen is None:
            return

        self.queue_whole_record(
            Selection(
                plugin=chosen,
                record_type=str(conflict.get("type") or ""),
                key=str(conflict.get("id") or ""),
            )
        )
        # Opened rather than merely counted: a queued decision is one you may
        # want to change, and the window is where that happens.
        self.show_patch_builder()

    def _ask_patch_winner(
        self, conflict: dict, plugins: list[str], field: str | None = None
    ) -> str | None:
        """Put the choice of winner to the user.

        Args:
            conflict: The selected conflict.
            plugins: Its plugins, in load order -- the last currently wins.
            field: The field being taken, when this is a field-level merge
                rather than a choice of whole record. Changes the wording only:
                what the caller does with the answer differs, not the question.

        Returns:
            The chosen plugin, or ``None`` if the dialog was dismissed.
        """
        # Parented to the *conflicts* window, not the main one. A modal that is
        # transient to a window the user is not looking at opens behind the one
        # they are, and grab_set() then swallows every click on it -- which is
        # indistinguishable from the application having frozen.
        parent = getattr(self, "_conflict_win", None)
        if parent is None or not parent.winfo_exists():
            parent = self.root
        win = tk.Toplevel(parent)
        win.title(_("Which version should win?"))
        win.transient(parent)
        apply_titlebar_theme(win)
        win.configure(bg=DARK["bg"])
        frame = ttk.Frame(win, padding=10)
        frame.pack(fill="both", expand=True)

        ttk.Label(
            frame,
            text=_("%(type)s  %(id)s")
            % {"type": conflict.get("type", ""), "id": conflict.get("id", "")},
        ).pack(anchor="w")
        ttk.Label(
            frame,
            foreground=DARK["fg_dim"],
            text=(
                _(
                    "Take %(field)s from this plugin, keeping the rest of the record "
                    "as it is now.\nYour mods are not modified."
                )
                % {"field": field}
                if field
                else _(
                    "The patch will carry this plugin's whole record and load last.\n"
                    "Your mods are not modified."
                )
            ),
        ).pack(anchor="w", pady=(0, 8))

        picked = tk.StringVar(value=plugins[-1])
        for name in plugins:
            label = f"{name}  ({_('currently wins')})" if name == plugins[-1] else name
            ttk.Radiobutton(frame, text=label, value=name, variable=picked).pack(anchor="w")

        answer: dict[str, str | None] = {"value": None}

        def accept() -> None:
            """Take the chosen plugin and close the dialog."""
            answer["value"] = picked.get()
            win.destroy()

        row = ttk.Frame(frame)
        row.pack(fill="x", pady=(10, 0))
        ttk.Button(
            row, text=_("Use this field") if field else _("Add to patch"), command=accept
        ).pack(side="left")
        ttk.Button(row, text=_("Cancel"), command=win.destroy).pack(side="right")

        # Placed over the window it belongs to before grabbing, so it is on
        # screen and focused by the time input is captured.
        win.update_idletasks()
        win.geometry(f"+{parent.winfo_rootx() + 60}+{parent.winfo_rooty() + 60}")
        win.lift()
        win.focus_force()
        # Closing with the window manager must release the grab, or the parent
        # stays dead. Cancel already does; this makes the X button agree.
        win.protocol("WM_DELETE_WINDOW", win.destroy)
        win.grab_set()
        parent.wait_window(win)
        return answer["value"]

    def _show_conflict_map_direct(self) -> None:
        """Build the conflict map off the main thread, then show it.

        Unlike the explorer, this shows ONLY conflicts (no sampled overview cells).
        Threaded for the same reason as the explorer.
        """
        conflicts = getattr(self, "_all_conflicts", None)
        if not conflicts or build_conflict_map is None or self.worker_running:
            return
        self.worker_running = True
        self.status_var.set(_("Building the conflict map..."))
        threading.Thread(
            target=self._conflict_map_worker, args=(list(conflicts),), daemon=True
        ).start()

    def _conflict_map_worker(self, conflicts: list[dict]) -> None:
        """Build the conflict map, then hand it to the UI thread to display.

        Args:
            conflicts: The conflict list to render.
        """
        markup: str | None = None
        error = ""
        cells: set[tuple[int, int]] = set()
        if build_conflict_map is None:  # pragma: no cover - guarded by caller too
            self.root.after(0, lambda: self._conflict_map_done(None, "viz unavailable", 0))
            return
        try:
            # Collect cells with conflicts for the status-bar count only --
            # build_conflict_map computes its own cell breakdown from
            # `conflicts` and does not take a `cells` argument.
            from wraithguard.viz import conflictmap as cmap_module

            cells = cmap_module.cells_with_conflicts(conflicts)
            markup = build_conflict_map(
                conflicts,
                subset_lower=getattr(self, "_conf_subset_lower", ()),
            )
        except Exception as exc:  # noqa: BLE001
            error = str(exc)
        self.root.after(0, lambda: self._conflict_map_done(markup, error, len(cells)))

    def _conflict_map_done(self, markup: str | None, error: str, cells: int) -> None:
        """Open the conflict map in the in-app viewer, or report the failure."""
        self.worker_running = False
        if markup is None:
            self.status_var.set(_("The conflict map could not be built."))
            messagebox.showerror(
                _("Could not build conflict map"),
                _("%(error)s") % {"error": error},
            )
            return
        self.status_var.set(_("Conflict map ready (%(cells)d cell(s)).") % {"cells": cells})
        self._open_html_view(markup, "conflict_map", _("Conflict Map"))

    def _open_html_view(self, markup: str, stem: str, title: str = "") -> None:
        """Write a generated page beside the app and show it in-app.

        The visualisations are plain files rather than embedded widgets so the
        rendering code stays free of Tk -- which is what lets the hermetic
        suite test it at all -- but they are *displayed* through the same
        viewer chain as the cell map (pywebview, then tkinterweb, then the
        browser), not flung straight into a browser.

        Args:
            markup: The complete HTML document.
            stem: Filename stem; a timestamp is appended so successive views do
                not overwrite each other mid-comparison.
            title: Window title for the in-app viewer.
        """
        from datetime import datetime

        stamp = datetime.now().strftime("%Y%m%d_%H%M%S")  # noqa: DTZ005 - local clock is correct
        path = app_base_dir() / f"{stem}_{stamp}.html"
        self._last_written_view = str(path)
        try:
            path.write_text(markup, encoding="utf-8")
        except OSError as exc:
            messagebox.showerror(
                _("Could not write the page"),
                _("Writing %(path)s failed: %(error)s") % {"path": path, "error": exc},
            )
            return
        # Prefer the in-app viewer, exactly as the cell map does.
        opener = getattr(self, "open_html_in_app", None)
        try:
            if callable(opener):
                opener(path, title or stem)
            else:  # pragma: no cover - only if the mixin is used outside App
                webbrowser.open(path.as_uri())
        except Exception as exc:  # noqa: BLE001 - a viewer failure is non-fatal
            # The file exists either way, so tell the user where it is rather
            # than losing the work to a viewer that would not launch.
            messagebox.showinfo(
                _("Saved, but could not open a viewer"),
                _("The page was written to %(path)s (%(error)s)") % {"path": path, "error": exc},
            )

    def _visualise_field(self, key: str, plugins: Sequence[str], per: Mapping[str, Any]) -> None:
        """Open the right visualisation for the selected field.

        Which view makes sense is a property of the field, so the button is
        contextual rather than a menu of mostly-inapplicable options.

        Args:
            key: The flattened field name the user selected.
            plugins: The plugins that touch this record, in load order.
            per: Field values per plugin.
        """
        winner = plugins[-1] if plugins else ""
        cell = str(getattr(self, "_conf_record_label", "") or "")

        def value(plugin: str, field: str, default: object = "") -> Any:  # noqa: ANN401
            """Read one field for one plugin, tolerating a missing record.

            Returns ``Any`` because a flattened tes3conv record holds whatever
            the field is -- base64 text for the grids, a list for ``points``, a
            float for the offset. Narrowing here would just move the cast.

            Args:
                plugin: The plugin to read from.
                field: The flattened field name.
                default: Returned when the plugin has no such field.

            Returns:
                The field value, or ``default``.
            """
            return (per.get(plugin) or {}).get(field, default)

        try:
            if key == "connections" and build_pathgrid_graph is not None:
                surfaces = {
                    p: (value(p, "connections"), value(p, "points", None))
                    for p in plugins
                    if value(p, "connections", None) is not None
                }
                if winner not in surfaces:
                    return
                markup = build_pathgrid_graph(surfaces, winner_name=winner, cell_label=cell)
                self._open_html_view(markup, "pathgrid")
                return
            if key == "vertex_heights.data" and build_height_delta is not None and len(plugins) > 1:
                surfaces = {
                    p: (value(p, key), _as_float(value(p, "vertex_heights.offset", 0.0)))
                    for p in plugins
                    if value(p, key, None) is not None
                }
                if winner in surfaces:
                    markup = build_height_delta(surfaces, winner_name=winner, cell_label=cell)
                    self._open_html_view(markup, "height_delta")
                return

            if key == "vertex_heights.data" and build_terrain_3d is not None:
                has_single_plugin = (
                    sum(1 for p in plugins if (per.get(p) or {}).get("vertex_heights.data")) == 1
                )
                if has_single_plugin:
                    self._show_terrain_3d(plugins, per)
        except Exception as exc:  # noqa: BLE001 - a bad record must not kill the window
            messagebox.showerror(_("Could not build the view"), _("%(error)s") % {"error": exc})

    def _show_terrain_3d(self, plugins: Sequence[str], per: Mapping[str, Any]) -> None:
        """Open the 3D surface for every plugin that has terrain here.

        Args:
            plugins: The plugins that touch this record, in load order.
            per: Field values per plugin.
        """
        if build_terrain_3d is None:
            return
        surfaces = {
            p: (
                (per.get(p) or {}).get("vertex_heights.data", ""),
                _as_float((per.get(p) or {}).get("vertex_heights.offset", 0.0)),
            )
            for p in plugins
            if (per.get(p) or {}).get("vertex_heights.data")
        }
        try:
            markup = build_terrain_3d(
                surfaces, cell_label=str(getattr(self, "_conf_record_label", "") or "")
            )
        except Exception as exc:  # noqa: BLE001 - a bad record must not kill the window
            messagebox.showerror(_("Could not build the view"), _("%(error)s") % {"error": exc})
            return
        self._open_html_view(markup, "terrain")

    def _add_format_reference_button(self, bar: ttk.Frame, record_type: str) -> None:
        """Offer the documented layout of the record being diffed.

        Only when the reference actually covers this record type, so the button
        can never open an empty window.

        Args:
            bar: The detail window's button row.
            record_type: tes3conv's ``"type"`` value for the record.
        """
        if layout_text is None or not record_type or layout_text(record_type) is None:
            return
        button = ttk.Button(
            bar,
            text=_("Format reference..."),
            command=lambda: self._show_format_reference(record_type),
        )
        button.pack(side="left", padx=(12, 0))
        add_tooltip(
            button,
            _(
                "Show what this kind of record is supposed to contain: every subrecord, "
                "whether the game requires it, how wide it is, and -- where the layout is "
                "documented -- the named fields inside it.\n\n"
                "A diff tells you what changed; this tells you what it was."
            ),
        )

    def _show_format_reference(self, record_type: str) -> None:
        """Open a window with the record type's documented layout.

        Args:
            record_type: tes3conv's ``"type"`` value for the record.
        """
        if layout_text is None:
            return
        text = layout_text(record_type)
        if text is None:
            return
        win = tk.Toplevel(self.root)
        apply_titlebar_theme(win)
        win.title(_("Format reference: %(type)s") % {"type": record_type})
        win.configure(bg=DARK["bg"])
        win.geometry("860x640")
        widget = scrolledtext.ScrolledText(
            win,
            wrap="word",
            font=("TkFixedFont", 10),
            # log_bg, matching every other read-only text pane in the app. NOT
            # "entry_bg": there is no such key, and asking for one raised a
            # KeyError *after* the Toplevel was created -- so the window opened,
            # blank, and the traceback went to stderr where nobody was looking.
            bg=DARK["log_bg"],
            fg=DARK["fg"],
            insertbackground=DARK["fg"],
        )
        widget.pack(fill="both", expand=True, padx=8, pady=8)
        widget.insert("1.0", text)
        widget.configure(state="disabled")

    def _add_field_view_buttons(
        self, bar: ttk.Frame, key: str, plugins: Sequence[str], per: Mapping[str, Any]
    ) -> None:
        """Add the visualisation buttons that apply to this field.

        Contextual by design: only the views that can say something about this
        field appear, so the bar never offers an action that would open an
        empty page.

        Args:
            bar: The detail window's button row.
            key: The flattened field name being shown.
            plugins: The plugins that touch this record, in load order.
            per: Field values per plugin.
        """
        has_terrain = sum(1 for p in plugins if (per.get(p) or {}).get("vertex_heights.data"))

        if key == "connections" and build_pathgrid_graph is not None:
            button = ttk.Button(
                bar,
                text=_("Show graph..."),
                command=lambda: self._visualise_field(key, plugins, per),
            )
            button.pack(side="left", padx=(12, 0))
            add_tooltip(
                button,
                _(
                    "Draw the path grid as a navigation graph, with the edges this "
                    "plugin added in green and the ones it removed in red.\n\n"
                    "A mod that only REMOVES edges has probably rebuilt its path grid "
                    "by accident -- that breaks NPC movement and nothing else reports it."
                ),
            )
            return

        if key == "vertex_heights.data" and build_height_delta is not None and len(plugins) > 1:
            button = ttk.Button(
                bar,
                text=_("Show difference..."),
                command=lambda: self._visualise_field(key, plugins, per),
            )
            button.pack(side="left", padx=(12, 0))
            add_tooltip(
                button,
                _(
                    "Decode both versions to absolute heights and subtract them: red "
                    "where the winner raised the ground, blue where it lowered it.\n\n"
                    "Comparing the raw values above is misleading. Heights are stored "
                    "as cumulative deltas, so moving ONE vertex changes every byte "
                    "after it and two nearly-identical cells look completely different."
                ),
            )

        if has_terrain and build_terrain_3d is not None:
            button = ttk.Button(
                bar,
                text=_("Show in 3D..."),
                command=lambda: self._show_terrain_3d(plugins, per),
            )
            button.pack(side="left", padx=(8, 0))
            add_tooltip(
                button,
                _(
                    "Draw the cell's terrain as a surface you can rotate, with each "
                    "plugin's version switchable in place. Drag to turn it.\n\n"
                    "Shading follows slope rather than height, which reads as terrain."
                ),
            )

    def _dump_conflict_json(self) -> None:
        """Write the tes3conv JSON for every scanned plugin to a chosen folder."""
        if self._conf_session is None or not self._conf_paths:
            return
        folder = filedialog.askdirectory(title=_("Dump tes3conv JSON to folder"))
        if not folder:
            return
        try:
            n = core.dump_tes3conv_json(
                self._conf_session, list(self._conf_paths.keys()), self._conf_paths, folder
            )
            self.status_var.set(
                ngettext(
                    "Wrote %(count)d JSON file to %(folder)s",
                    "Wrote %(count)d JSON files to %(folder)s",
                    n,
                )
                % {"count": n, "folder": folder}
            )
            if n:
                messagebox.showinfo(
                    _("JSON dumped"),
                    ngettext(
                        "Wrote %(count)d tes3conv JSON file to:\n%(folder)s",
                        "Wrote %(count)d tes3conv JSON files to:\n%(folder)s",
                        n,
                    )
                    % {"count": n, "folder": folder},
                )
            else:
                messagebox.showwarning(
                    _("Nothing written"),
                    _(
                        "No JSON was written. The tes3conv session may have been cleared - "
                        "re-run Check Conflicts, then dump again."
                    ),
                )
        except Exception as e:  # noqa: BLE001
            # user-facing dump; any failure becomes an error dialog
            messagebox.showerror(_("Dump failed"), str(e))

    def _on_conflict_select(self) -> None:
        """Show the field comparison for whichever record is selected."""
        tree = getattr(self, "_conf_tree", None)
        sel = tree.selection() if tree else None
        if not sel:
            return
        self._populate_field_diff(self._shown_conflicts[int(sel[0])])

    def _show_field_detail(self) -> None:
        """Pop up the full value of the selected field for each plugin.

        One tab per plugin, pretty-printed with JSON syntax highlighting (and,
        for text fields like book/dialogue content, the embedded HTML-ish
        markup broken out too). Uses whatever theme is picked next to the Log
        panel, so the two stay in sync. For long fields like 'references'
        that get truncated in the table.
        """
        ftree = getattr(self, "_conf_ftree", None)
        fd = getattr(self, "_conf_fdiff", None)
        if not ftree or not fd:
            return
        sel = ftree.selection()
        if not sel:
            return
        key = sel[0]
        plugins = fd["plugins"]
        per = fd["per"]
        theme = self._resolve_theme(self.log_theme_var.get()) or THEME_PRESETS["Dark (default)"]
        json_colors = _json_syntax_colors(theme)
        win = tk.Toplevel(self.root)
        apply_titlebar_theme(win)
        win.title(f"Field: {key}")
        win.configure(bg=DARK["bg"])
        win.geometry("1040x680")
        note = "last plugin wins · ★ orange = your custom mod"
        if key == "bytecode" and listing_for_bytecode_field is not None:
            note += " · shown disassembled; undecoded spans are printed as hex"
        elif key == "variables" and variables_text_for_field is not None:
            note += " · decoded to local variable names"
        elif describe_field is not None and (described := describe_field(key)):
            note += f" · {described}"
        record_type = str(getattr(self, "_conf_record_type", "") or "")
        if field_note is not None and (formatted := field_note(record_type, key)):
            # What this field is in the file itself, not in tes3conv's JSON:
            # the subrecord it comes from, its width, and whether the game
            # requires it.
            note += f" · {formatted}"
        ttk.Label(win, text=f"{key}   ({note})", padding=8).pack(anchor="w")
        bar = ttk.Frame(win, padding=(8, 0))
        bar.pack(fill="x")
        wrap_var = tk.BooleanVar(value=True)
        texts: list[tk.Text] = []

        def _apply_wrap() -> None:
            """Turn word wrap on or off in every tab at once."""
            w: Literal["word", "none"] = "word" if wrap_var.get() else "none"
            for st in texts:
                st.configure(state="normal")
                st.configure(wrap=w)
                st.configure(state="disabled")

        ttk.Checkbutton(bar, text=_("Word wrap"), variable=wrap_var, command=_apply_wrap).pack(
            side="left"
        )
        self._add_field_view_buttons(bar, key, plugins, per)
        self._add_format_reference_button(bar, record_type)
        ttk.Label(
            bar,
            text=_("Syntax highlighting: %(theme)s") % {"theme": self.log_theme_var.get()},
            foreground=DARK["fg_dim"],
        ).pack(side="right")
        nb = ttk.Notebook(win)
        nb.pack(fill="both", expand=True, padx=8, pady=(4, 8))
        custom_fg = "#ff9b6b"
        for i, p in enumerate(plugins):
            cust = self._is_custom(p)
            val = per[p].get(key, None)
            # A plain string field (book/dialogue text, mesh/icon/script
            # paths, ids, ...) is shown as its own raw content, not run
            # through json.dumps -- dumping it would wrap it in quotes and
            # JSON-escape every embedded " and \ (Morrowind book text is
            # full of pseudo-HTML like <FONT COLOR="000000">, which
            # json.dumps turns into <FONT COLOR=\"000000\">, all noise and
            # no benefit since nothing here gets re-parsed as JSON). Only
            # structured values (list/dict/number/etc.) still need JSON's
            # own formatting, so those still go through json.dumps.
            is_plain_string = isinstance(val, str)
            # A compiled-script field is base64, so showing it verbatim makes
            # every script edit look like a total rewrite. Disassemble instead.
            is_listing = False
            listing = None
            if is_plain_string and key == "bytecode":
                listing = self._disassemble_bytecode_field(val, per[p].get("text"))
            elif is_plain_string and key == "variables" and variables_text_for_field:
                # Same base64+zstd wrapping as bytecode; shown as names so the
                # diff says WHICH locals changed, not just that the blob did.
                listing = variables_text_for_field(val)
            elif is_plain_string and text_for_field is not None:
                # Landscape grids and path-grid edges are base64 too, and just
                # as unreadable: one moved vertex changes the whole string.
                # The whole record is passed because some of these fields only
                # mean something beside a sibling -- heights need their offset,
                # edges need their points.
                listing = text_for_field(key, val, per[p])
            if listing is not None:
                text, is_listing, is_plain_string = listing, True, False
            elif is_plain_string:
                text = str(val)
            else:
                try:
                    text = json.dumps(val, indent=2, ensure_ascii=False, default=str)
                except (TypeError, ValueError):
                    # default=str handles most; circular refs raise ValueError
                    text = repr(val)
            frame = ttk.Frame(nb)
            # colored per-plugin header inside the tab: orange = your custom mod
            ttk.Label(
                frame,
                text=(
                    ("★ " if cust else "")
                    + p
                    + ("   - your custom mod" if cust else "   - curated list")
                    + ("   ✓ wins" if i == len(plugins) - 1 else "")
                ),
                foreground=(custom_fg if cust else DARK["fg_dim"]),
                padding=(4, 4),
            ).pack(anchor="w")
            st = scrolledtext.ScrolledText(
                frame,
                wrap="word",
                font=("TkFixedFont", 10),
                background=theme["background"],
                foreground=theme["foreground"],
                insertbackground=theme["foreground"],
                selectbackground=theme["select"],
                relief="flat",
                highlightthickness=1,
                highlightbackground=DARK["border"],
            )
            st.pack(fill="both", expand=True)
            style_json_syntax_tags(st, json_colors)
            shown = text if val is not None else "(field not present in this plugin)"
            st.insert("1.0", shown)
            if val is not None and not is_listing:
                try:
                    if is_plain_string:
                        highlight_plain_text_with_html(st, text, json_colors)
                    else:
                        highlight_json_with_html(st, text, json_colors)
                except Exception:  # noqa: BLE001
                    # Highlighting is cosmetic. Never let it stop the value
                    # being shown -- the value is the thing the user opened
                    # this window for.
                    pass
            st.configure(state="disabled")
            texts.append(st)
            label = (p[:22] + "…") if len(p) > 24 else p
            tab = ("★ " if cust else "") + label + (" ✓" if i == len(plugins) - 1 else "")
            nb.add(frame, text=tab)
        ttk.Button(win, text=_("Close"), command=win.destroy).pack(pady=(0, 8))

    #: Per-checkbox config for the two non-conflicting-records toggles:
    #: which BooleanVar, which cache attribute, and which engine function.
    #: ``ClassVar`` because it is shared, read-only configuration -- never
    #: mutated per instance, which is what a bare mutable class attribute
    #: would invite.
    _SINGLES_KINDS: ClassVar[dict[str, dict[str, Any]]] = {
        "mine": {
            "var": "_include_singles_var",
            "cache": "_conf_singles",
            "fn": "list_subset_singles",
            "label": lambda: _("your mods'"),
        },
        "other": {
            "var": "_other_singles_var",
            "cache": "_conf_other_singles",
            "fn": "list_other_singles",
            "label": lambda: _("other mods'"),
        },
    }

    def _toggle_singles(self, kind: str) -> None:
        """Show/hide non-conflicting records (``kind`` "mine" or "other").

        Fetched once per scan and cached -- toggling a checkbox off and back
        on just re-filters, it doesn't re-scan. Deliberately never touches
        ``self._all_conflicts``: the conflict map and CSV export stay
        conflict-only, since a record one plugin defines alone isn't a
        conflict and doesn't belong in either.
        """
        cfg = self._SINGLES_KINDS[kind]
        var: tk.BooleanVar = getattr(self, cfg["var"])
        if not var.get():
            self._refill_conflict_tree()
            return
        if getattr(self, cfg["cache"], None) is not None:
            self._refill_conflict_tree()
            return
        scan_args = getattr(self, "_conf_scan_args", None)
        if scan_args is None or self.worker_running:
            var.set(False)
            return
        order, dirs, subset = scan_args
        if kind == "mine" and not subset:
            var.set(False)
            messagebox.showinfo(
                _("No custom mods"),
                _("You have no custom/subset mods configured, so there is nothing to list here."),
            )
            return
        self.worker_running = True
        self.status_var.set(
            _("Finding %(label)s non-conflicting records...") % {"label": cfg["label"]()}
        )
        threading.Thread(
            target=self._singles_worker, args=(order, dirs, subset, kind), daemon=True
        ).start()

    def _singles_worker(
        self, order: list[str], dirs: list[str], subset: list[str], kind: str
    ) -> None:
        """List records only one plugin defines, off the UI thread.

        Args:
            order: The active load order.
            dirs: The data directories to resolve names against.
            subset: The user's own mods.
            kind: Which toggle asked -- the user's own mods, or everything else.
        """
        writer = QueueWriter(self.log_queue)
        records: list[dict] = []
        error = ""
        try:
            with redirect_stdout(writer.as_stream()), redirect_stderr(writer.as_stream()):
                index = PluginFileIndex(dirs)
                fn = getattr(core, self._SINGLES_KINDS[kind]["fn"])
                records, _stats = fn(order, index, subset_names=subset, session=self._conf_session)
        except Exception:  # noqa: BLE001
            error = traceback.format_exc()
        self.root.after(0, self._singles_done, records, error, kind)

    def _singles_done(self, records: list[dict], error: str, kind: str) -> None:
        """Fold a non-conflicting-records scan back into the list.

        Args:
            records: What was found.
            error: What went wrong, if anything.
            kind: Which of the two "include singles" toggles asked for it.
        """
        self.worker_running = False
        cfg = self._SINGLES_KINDS[kind]
        var: tk.BooleanVar = getattr(self, cfg["var"])
        if error:
            var.set(False)
            self.status_var.set(_("Could not list records."))
            messagebox.showerror(_("Could not list records"), error)
            return
        setattr(self, cfg["cache"], records)
        self.status_var.set(
            _("Found %(count)d non-conflicting record(s) from %(label)s.")
            % {"count": len(records), "label": cfg["label"]()}
        )
        self._refill_conflict_tree()

    def _refill_conflict_tree(self) -> None:
        """Rebuild the conflict list from the current filters."""
        tree = getattr(self, "_conf_tree", None)
        if tree is None or not tree.winfo_exists():
            return
        only = self._conf_subset_only.get()
        rows = list(self._all_conflicts)
        if self._include_singles_var.get():
            rows += getattr(self, "_conf_singles", None) or []
        if self._other_singles_var.get():
            rows += getattr(self, "_conf_other_singles", None) or []
        self._shown_conflicts = [c for c in rows if c.get("involves_subset") or not only]
        tree.delete(*tree.get_children())
        for i, c in enumerate(self._shown_conflicts):
            star = "★" if c["involves_subset"] else ""
            tags = ("sub",) if c["involves_subset"] else ()
            tree.insert(
                "",
                "end",
                iid=str(i),
                tags=tags,
                values=(star, c["type"], c["id"], len(c["plugins"]), c["winner"]),
            )
        # Filtering rebuilds every row, which would drop the judgement colours.
        # Reapplying them here rather than at the call sites means a filter can
        # never silently un-say what the summary found.
        self._recolour_conflict_tree()

    def _save_conflicts_csv(self) -> None:
        """Write the record conflicts to a CSV the user chooses."""
        if not getattr(self, "_all_conflicts", None):
            return
        path = filedialog.asksaveasfilename(
            title=_("Save conflict report"),
            defaultextension=".csv",
            initialfile="tes3_conflicts.csv",
            filetypes=(("CSV files", "*.csv"), ("All files", "*.*")),
        )
        if not path:
            return
        try:
            core.write_conflict_csv(path, self._all_conflicts)
            self.status_var.set(_("Conflict report saved: %(path)s") % {"path": path})
        except OSError as e:
            messagebox.showerror(_("Save failed"), str(e))
