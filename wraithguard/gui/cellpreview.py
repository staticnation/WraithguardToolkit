"""The Cell Preview window: place a cell's objects in the 3D mesh viewer.

Mixed into ``App`` (like the tes3cmd and conflict windows). It resolves a cell
across the sorted load order -- every reference's winning object and world
transform -- groups the placements by model for *instanced* drawing (one mesh per
unique model plus a matrix per placement), and serves the result over the shared
loopback server so geometry and textures stream as blobs instead of inlining into
one document. Alongside the view it prints an audit of what the load order does
to the cell (overrides, deletions, moves, missing meshes).

The heavy lifting is pure and lives in :mod:`wraithguard.scene`; this file is only
the Tk glue: pick a cell, run it off the UI thread, print the audit, publish and
open the page. Meshes are textured through the same
:class:`~wraithguard.nif.textures.TextureResolver` the single-mesh view uses, and
drawn single-sided so an interior's near walls do not block the view. Interior and
exterior cells are both supported; exteriors show their statics, terrain (with its
blended landscape textures), animated water and, behind a toggle, the eight
neighbouring cells, all under a Morrowind sky with time-of-day and weather. The
view is read-only -- it is for checking a cell for conflicts without loading the
game, not for editing it.
"""

from __future__ import annotations

import itertools
import threading
import time
import traceback
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from tkinter import ttk
from typing import TYPE_CHECKING, Any

import wraithguard_toolkit as core
from wraithguard.esp.io import EspError
from wraithguard.gui import app_base_dir
from wraithguard.gui.theme import DARK, apply_titlebar_theme
from wraithguard.gui.widgets import QueueWriter, RadioButton, ToggleSwitch
from wraithguard.i18n import gettext as _
from wraithguard.nif.geometry import world_meshes
from wraithguard.nif.textures import TextureResolver
from wraithguard.nif.vfs import MeshVfs
from wraithguard.nif.viewer import build_cell_viewer_page, texture_bytes
from wraithguard.plugins import PluginFileIndex
from wraithguard.scene import (
    CellKey,
    LoadedPlugin,
    PluginParseCache,
    cell_label,
    cell_layers,
    object_provenance,
    preview_cell_instanced,
)
from wraithguard.tracing import trace
from wraithguard.viz.serve import Payload

if TYPE_CHECKING:
    import queue
    import tkinter as tk
    from collections.abc import Callable

    from wraithguard.nif.geometry import Mesh
    from wraithguard.nif.textures import Resolved
    from wraithguard.viz.serve import ViewerServer

#: OpenMW's binary content files -- the ones made of TES3 records. A load order
#: also lists ``.omwscripts`` (a plain-text list of Lua scripts, not records),
#: which :func:`~wraithguard.esp.plugin.read_plugin` cannot and should not parse;
#: they hold no cell content, so they are skipped without a word.
_CONTENT_SUFFIXES = frozenset({".esm", ".esp", ".omwaddon", ".omwgame"})

#: Largest texture dimension a cell preview decodes to, taken from a DDS mip.
#: Cell scale means many textures at once; 1K is ample for a preview and a
#: quarter the decode and memory of a 2K source.
_CELL_TEXTURE_MAX_DIM = 1024

#: Textures whose reference names them an atlas are left at full resolution --
#: an atlas packs many images into one sheet, so capping it would shrink every
#: packed tile, not just add a mip.
_ATLAS_HINT = "atlas"

#: Morrowind's per-weather sky textures, keyed by the viewer's weather names.
#: Each is served (lazily) so the viewer's Weather selector can swap the sky dome
#: and the water's reflection to the chosen weather; "clear" is the default hung
#: behind the scene at load. Resolved from the load order like any other texture,
#: so a texture pack's overrides win; a weather whose texture no data folder
#: provides simply falls back to the plain gradient sky.
_WEATHER_SKIES = {
    "clear": "tx_sky_clear.dds",
    "cloudy": "tx_sky_cloudy.dds",
    "foggy": "tx_sky_foggy.dds",
    "overcast": "tx_sky_overcast.dds",
    "storm": "tx_sky_stormy.dds",
    "ashstorm": "tx_sky_ashstorm.dds",
    "blight": "tx_sky_blight.dds",
    "snow": "tx_sky_snow.dds",
}

#: Morrowind's starfield, faded in behind the night sky as the sun drops.
_STAR_TEXTURE = "tx_stars.dds"


class CellPreviewMixin:
    """The Cell Preview window and its worker (mixed into ``App``)."""

    #: Cache of the last texture resolver, ``(dirs_key, resolver)``. Indexing a
    #: big collection's ``textures/`` trees takes tens of seconds; the folders do
    #: not change between previews, so it is built once and reused until they do.
    _cell_resolver_cache: tuple[tuple[str, ...], TextureResolver] | None = None

    #: Cache of the last mesh VFS index, ``(dirs_key, MeshVfs)``. Same reasoning:
    #: the merged ``meshes/`` index is built once and reused across previews so a
    #: mesh is a dict lookup, not a probe of every data folder.
    _cell_mesh_vfs_cache: tuple[tuple[str, ...], MeshVfs] | None = None

    #: Parsed plugins, reused across previews and (via sidecars) across launches,
    #: so a repeat preview re-parses only files that changed. Built lazily.
    _plugin_cache: PluginParseCache | None = None

    if TYPE_CHECKING:
        # The host contract -- these live on ``App``. Declared, not silenced, so
        # mypy checks the half that is here against what the host must provide.
        root: tk.Tk
        log_queue: queue.Queue
        status_var: tk.StringVar
        worker_running: bool
        _current_plan: dict | None
        cellpreview_button: ttk.Button
        sort_button: ttk.Button
        export_button: ttk.Button
        conflicts_button: ttk.Button
        cellmap_button: ttk.Button
        resource_button: ttk.Button

        def _apply_exclusions(self, order: list[str]) -> list[str]: ...
        def _plan_scan_dirs(self) -> list[str]: ...
        def _open_html_view(self, markup: str, stem: str, title: str = "") -> None: ...
        def _schedule_ui(
            self, delay_ms: int, func: Callable[..., Any], *args: Any  # noqa: ANN401
        ) -> None: ...

        # Shared with the conflict window's mesh viewer (both mixed into App):
        # the loopback server that lets the page fetch geometry/textures instead
        # of inlining them, and the in-app opener that shows a URL.
        def _viewer_server(self) -> ViewerServer | None: ...
        def open_html_in_app(self, path: str | Path, title: str) -> None:
            """Show a served URL or a file path in the in-app viewer chain."""

        def _three_js_url(self, server: ViewerServer) -> str: ...

    def on_cell_preview(self) -> None:
        """Ask for a cell, then build its preview in a worker."""
        if self.worker_running or not self._current_plan:
            return
        chosen = self._ask_cell()
        if chosen is None:
            return
        key, label, adjacent = chosen
        order = self._apply_exclusions(self.order_panel.get_enabled())  # type: ignore[attr-defined]
        if not order:
            return
        dirs = self._plan_scan_dirs()
        self.worker_running = True
        for button in (
            self.sort_button,
            self.export_button,
            self.conflicts_button,
            self.cellmap_button,
            self.cellpreview_button,
            self.resource_button,
        ):
            button.configure(state="disabled")
        self.status_var.set(_("Building cell preview..."))
        threading.Thread(
            target=self._cellpreview_worker,
            args=(order, dirs, key, label, adjacent),
            daemon=True,
        ).start()

    def prewarm_cell_preview(self) -> None:
        """Parse the load order into the cache in the background, after a Sort.

        Called when a Sort produces a plan: it reads and (filtered-)parses every
        enabled plugin off the UI thread and writes the sidecars, so the first
        Cell Preview afterwards skips straight to resolution instead of parsing the
        whole load order while the user waits. Best-effort and silent -- a preview
        that runs before it finishes simply parses whatever is not yet warm, and
        any unreadable file is left for the preview to report.
        """
        if self.worker_running or not self._current_plan:
            return
        try:
            order = self._apply_exclusions(self.order_panel.get_enabled())  # type: ignore[attr-defined]
            dirs = self._plan_scan_dirs()
        except Exception:  # noqa: BLE001 -- a pre-warm must never disturb the UI
            return
        if not order:
            return

        def work() -> None:
            """Parse every plugin into the cache off the UI thread; never raise."""
            try:
                cache = self._plugin_parse_cache()
                resolved = core.plugin_paths(order, PluginFileIndex(dirs))
                for name in order:
                    path = resolved.get(name)
                    if not path or Path(name).suffix.lower() not in _CONTENT_SUFFIXES:
                        continue
                    try:
                        cache.load(name, path)
                    except (OSError, ValueError, EspError):
                        pass  # the preview reports it if the user opens this cell
            except Exception:  # noqa: BLE001 -- never crash the app from a warmer
                trace("cell preview pre-warm failed")

        threading.Thread(target=work, name="cellpreview-prewarm", daemon=True).start()

    def _ask_cell(self) -> tuple[CellKey, str, bool] | None:
        """Modal: pick an interior cell by name or an exterior cell by grid.

        Returns:
            ``(CellKey, label, include_adjacent)`` or ``None`` if cancelled.
            ``include_adjacent`` is only ever true for an exterior cell.
        """
        import tkinter as tk

        win = tk.Toplevel(self.root)
        win.title(_("Cell to preview"))
        win.transient(self.root)
        win.resizable(False, False)
        win.configure(background=DARK["bg"])
        # The OS-drawn titlebar is not reached by ttk theming; sync it to the dark
        # chrome the way every other window in the app does.
        apply_titlebar_theme(win)
        frame = ttk.Frame(win, padding=12)
        frame.pack(fill="both", expand=True)
        mode = tk.StringVar(value="interior")
        name_var = tk.StringVar()
        x_var = tk.StringVar(value="0")
        y_var = tk.StringVar(value="0")
        adjacent_var = tk.BooleanVar(value=False)
        result: dict[str, tuple[CellKey, str, bool] | None] = {"value": None}

        from tkinter import messagebox

        RadioButton(frame, text=_("Interior cell"), value="interior", variable=mode).grid(
            row=0, column=0, columnspan=3, sticky="w"
        )
        ttk.Label(frame, text=_("Name:")).grid(row=1, column=0, sticky="e", padx=(16, 4), pady=2)
        name_entry = ttk.Entry(frame, textvariable=name_var, width=34)
        name_entry.grid(row=1, column=1, columnspan=2, sticky="w", pady=2)
        RadioButton(frame, text=_("Exterior cell"), value="exterior", variable=mode).grid(
            row=2, column=0, columnspan=3, sticky="w", pady=(8, 0)
        )
        ttk.Label(frame, text=_("Grid X, Y:")).grid(row=3, column=0, sticky="e", padx=(16, 4))
        x_entry = ttk.Entry(frame, textvariable=x_var, width=8)
        x_entry.grid(row=3, column=1, sticky="w")
        y_entry = ttk.Entry(frame, textvariable=y_var, width=8)
        y_entry.grid(row=3, column=2, sticky="w")
        # Neighbours are an exterior-only notion (an interior has no grid), so the
        # box lives under the exterior fields and applies only when that mode wins.
        adjacent_check = ToggleSwitch(
            frame,
            text=_("Include adjacent cells (8 neighbours)"),
            variable=adjacent_var,
        )
        adjacent_check.grid(row=4, column=0, columnspan=3, sticky="w", padx=(16, 0), pady=(4, 0))
        # Selecting a field selects its mode, so typing a grid (even a negative
        # one) and pressing Preview just works without also clicking the radio --
        # the silent no-op that made "-4, -9" look like it did nothing.
        name_entry.bind("<FocusIn>", lambda _e: mode.set("interior"))
        for entry in (x_entry, y_entry):
            entry.bind("<FocusIn>", lambda _e: mode.set("exterior"))

        def accept() -> None:
            """Build the key from the fields and close, or explain what is missing."""
            if mode.get() == "interior":
                name = name_var.get().strip()
                if not name:
                    messagebox.showinfo(
                        _("Which cell?"),
                        _("Enter an interior cell name, or pick a grid."),
                        parent=win,
                    )
                    return
                # An interior has no grid neighbours, so adjacency never applies.
                result["value"] = (CellKey(interior=name.lower()), name, False)
            else:
                try:
                    grid = (int(x_var.get().strip()), int(y_var.get().strip()))
                except ValueError:
                    messagebox.showinfo(
                        _("Which cell?"),
                        _("Enter whole numbers for the grid X and Y (negatives are fine)."),
                        parent=win,
                    )
                    return
                result["value"] = (
                    CellKey(grid=grid),
                    f"Wilderness ({grid[0]}, {grid[1]})",
                    adjacent_var.get(),
                )
            win.destroy()

        buttons = ttk.Frame(frame)
        buttons.grid(row=5, column=0, columnspan=3, sticky="e", pady=(12, 0))
        ttk.Button(buttons, text=_("Preview"), command=accept).pack(side="left", padx=4)
        ttk.Button(buttons, text=_("Cancel"), command=win.destroy).pack(side="left")
        win.grab_set()
        self.root.wait_window(win)
        return result["value"]

    def _plugin_parse_cache(self) -> PluginParseCache:
        """The parse cache, built once and reused across previews and launches.

        Persisted to ``plugin_cache`` sidecars beside the app's other spools, so a
        warmed cache (from a preview, or the pre-warm after a Sort) survives a
        restart and only re-reads a plugin the file itself has changed.
        """
        cache = self._plugin_cache
        if cache is None:
            cache = PluginParseCache(cache_dir=app_base_dir() / "plugin_cache")
            self._plugin_cache = cache
        return cache

    def _load_order_plugins(self, order: list[str], dirs: list[str]) -> list[LoadedPlugin]:
        """Parse every plugin in the order into a :class:`LoadedPlugin`.

        Reuses the parse cache, so a plugin unchanged since a previous preview (or
        since the pre-warm after the last Sort) is not re-read or re-parsed. The
        parse itself keeps only the records a preview uses (see
        :data:`~wraithguard.scene.PREVIEW_RECORD_TAGS`).

        Args:
            order: Enabled plugin filenames, in load order.
            dirs: The data folders to resolve them in.

        Returns:
            One :class:`LoadedPlugin` per plugin that could be read.
        """
        cache = self._plugin_parse_cache()
        plugins: list[LoadedPlugin] = []
        resolved = core.plugin_paths(order, PluginFileIndex(dirs))
        for name in order:
            path = resolved.get(name)
            if not path:
                continue
            if Path(name).suffix.lower() not in _CONTENT_SUFFIXES:
                continue  # .omwscripts and the like: not record files, no cells
            try:
                plugins.append(cache.load(name, path))
            except (OSError, ValueError, EspError) as exc:
                print(_("  skipped %(name)s: %(error)s") % {"name": name, "error": exc})
                continue
        return plugins

    def _cached_texture_resolver(self, dir_paths: list[Path]) -> TextureResolver | None:
        """The texture resolver for these folders, built once and cached.

        Args:
            dir_paths: The data folders, in load order.

        Returns:
            A resolver, or ``None`` when there are no folders.
        """
        if not dir_paths:
            return None
        key = tuple(str(d) for d in dir_paths)
        cached = self._cell_resolver_cache
        if cached is None or cached[0] != key:
            cached = (key, TextureResolver(list(dir_paths)))
            self._cell_resolver_cache = cached
        return cached[1]

    def _cached_mesh_vfs(self, dir_paths: list[Path]) -> MeshVfs:
        """The merged mesh index for these folders, built once and cached.

        Args:
            dir_paths: The data folders, in load order.

        Returns:
            A :class:`~wraithguard.nif.vfs.MeshVfs`.
        """
        key = tuple(str(d) for d in dir_paths)
        cached = self._cell_mesh_vfs_cache
        if cached is None or cached[0] != key:
            cached = (key, MeshVfs(dir_paths))
            self._cell_mesh_vfs_cache = cached
        return cached[1]

    @staticmethod
    def _resolved_label(plugins: list[LoadedPlugin], key: CellKey, fallback: str) -> str:
        """The cell's real display label from the load order, or ``fallback``.

        The picker builds an exterior label from the grid alone, so it cannot
        know a cell's name -- and many exteriors have one (Seyda Neen, Balmora,
        Vivec). This finds the cell records for ``key`` and takes its proper label
        (name, else region, else "Wilderness (x, y)").

        For an exterior the name is borrowed across all the layers, not just the
        last: a base master names Seyda Neen, and a later mod that only adds a
        reference there rewrites the cell with a blank name -- so taking the last
        record's name alone would lose it. Any layer's name is this same cell's
        name, so the most recent non-empty one wins.

        Args:
            plugins: The parsed load order.
            key: The cell being previewed.
            fallback: The picker's label, returned if no plugin defines the cell.

        Returns:
            The resolved label, or ``fallback``.
        """
        records = [record for _name, _masters, record in cell_layers(plugins, key) if record]
        if not records:
            return fallback
        if key.grid is None:
            return cell_label(records[-1])  # interior: its own name is definitive
        name = next((record.name for record in reversed(records) if record.name), "")
        named = name or records[-1].region or "Wilderness"
        gx, gy = key.grid
        return f"{named} ({gx}, {gy})"

    def _cellpreview_worker(
        self, order: list[str], dirs: list[str], key: CellKey, label: str, adjacent: bool
    ) -> None:
        """Off-thread: parse the order, resolve the cell, build and open the view.

        The cell is drawn instanced (one mesh per unique model plus a matrix per
        placement) and served over the shared loopback server, so geometry and
        textures stream as separate blobs instead of inlining into one document
        the in-app view cannot hold. Only when no loopback port can be bound does
        it fall back to a standalone page with the bytes carried inline.

        Args:
            order: Enabled plugin filenames, in load order.
            dirs: The data folders to resolve them in.
            key: The cell to preview.
            label: The cell's label, for the window title and log.
            adjacent: Whether to also assemble the eight neighbouring cells (an
                exterior only), drawn behind the viewer's "Adjacent cells" toggle.
        """
        writer = QueueWriter(self.log_queue)
        opened: tuple[str, str] | None = None  # ("url", url) served, or ("page", html)
        trace(f"cell preview: start, {label!r}, {len(order)} plugin(s)")
        try:
            with redirect_stdout(writer.as_stream()), redirect_stderr(writer.as_stream()):
                plugins = self._load_order_plugins(order, dirs)
                # The dialog only knows the grid, so it labels every exterior
                # "Wilderness"; now that the cell record is in hand, use its real
                # name -- many exteriors are named (Seyda Neen is (-2, -9)).
                label = self._resolved_label(plugins, key, label)
                print("\n" + "=" * 70)
                print(_(" CELL PREVIEW: %(label)s") % {"label": label})
                print("=" * 70)
                dir_paths = [Path(d) for d in dirs]
                clock = time.perf_counter

                mark = clock()
                mesh_key = tuple(str(d) for d in dir_paths)
                indexed = (
                    self._cell_mesh_vfs_cache is not None
                    and self._cell_mesh_vfs_cache[0] == mesh_key
                )
                print(f"  {'reusing' if indexed else 'building'} mesh index...")
                mesh_vfs = self._cached_mesh_vfs(dir_paths)
                print(f"  mesh index ready in {clock() - mark:.1f}s ({len(mesh_vfs)} meshes)")

                def load_mesh(model: str) -> list[Mesh] | None:
                    """Resolve a model path through the merged mesh index."""
                    parsed = mesh_vfs.read(f"meshes/{model}")
                    return world_meshes(parsed) if parsed is not None else None

                mark = clock()
                placements, audit, cell = preview_cell_instanced(
                    plugins, key, load_mesh, include_adjacent=adjacent
                )
                print(f"  resolved cell in {clock() - mark:.1f}s")
                self._print_audit(audit, cell, len(placements))
                if not cell.groups:
                    print(_("\n  Nothing to draw -- no placed meshes were found for this cell."))
                mark = clock()
                dirs_key = tuple(str(d) for d in dir_paths)
                cached = (
                    self._cell_resolver_cache is not None
                    and self._cell_resolver_cache[0] == dirs_key
                )
                print(
                    f"  {'reusing' if cached else 'indexing'} texture index"
                    f" ({len(dir_paths)} folders)..."
                )
                resolver = self._cached_texture_resolver(dir_paths)
                print(f"  texture index ready in {clock() - mark:.1f}s (decoded on demand)")
                groups = [(group.meshes, group.matrices) for group in cell.groups]
                focus_indices = {i for i, group in enumerate(cell.groups) if group.focus}
                ref_ids = [group.ref_ids for group in cell.groups]
                record_types = [group.record_type for group in cell.groups]
                adjacent_flags = [group.adjacent for group in cell.groups]
                # The audit numbers, for the viewer's cell-info panel (the same
                # figures the log prints, but shown in the page's side panel).
                cell_info = {
                    "label": label,
                    "stats": [
                        [_("references"), audit.references],
                        [_("placed"), audit.placed],
                        [_("no mesh record"), audit.no_mesh_record],
                        [_("editor markers"), audit.editor_markers],
                        [_("actors skipped"), audit.actors_skipped],
                        [_("overridden by later"), audit.overridden_by_later],
                        [_("deleted by later"), audit.deleted_by_later],
                        [_("moved"), audit.moved],
                        [_("meshes drawn"), cell.drawn],
                        [_("meshes not found"), len(cell.missing_models)],
                    ],
                    "missing": list(cell.missing_models),
                }
                # Per-object provenance for the "ori" readout: click a mesh to see
                # which plugins define and place it, and its winning texture.
                object_info = object_provenance(plugins, key)
                title = _("Cell preview: %(label)s") % {"label": label}
                # Interiors render one-sided so near walls cull away; an exterior
                # has no enclosing walls, and two-sided keeps the terrain visible
                # from any angle whichever way its faces happen to wind.
                interior = key.interior is not None
                mark = clock()
                server = self._viewer_server()
                if server is not None:
                    print("  serving cell over loopback...")
                    session = server.publish_session("cell")
                    tex_urls: dict[str, dict[str, str]] = {}
                    tex_counter = itertools.count()

                    def publish_texture(resolved: Resolved) -> dict[str, str] | None:
                        """Register a texture to decode on first fetch, not now.

                        Returns a URL whose bytes the server produces (a DDS
                        decode) only when the browser asks for it, deduped per
                        texture so the page opens on geometry and textures stream.
                        """
                        if not resolved.found or resolver is None:
                            return None
                        hit = tex_urls.get(resolved.reference)
                        if hit is not None:
                            return hit

                        cap = None if _ATLAS_HINT in resolved.reference else _CELL_TEXTURE_MAX_DIM

                        def producer(
                            res: Resolved = resolved, cap: int | None = cap
                        ) -> Payload | None:
                            """Decode the texture on demand (resolver-cached, mip-capped)."""
                            shown = texture_bytes(res, resolver, cap)
                            return Payload(shown[0], shown[1]) if shown else None

                        url = session.register_lazy(f"t{next(tex_counter)}.png", producer)
                        made = {"url": url}
                        tex_urls[resolved.reference] = made
                        return made

                    # An exterior's water reflects Morrowind's own sky, and the
                    # Weather selector can swap it: serve every weather's sky
                    # texture the same lazy way as any other (each only decodes if
                    # actually chosen), keyed by weather. Interiors get none.
                    sky_textures: dict[str, str] = {}
                    if not interior and resolver is not None:
                        for weather, tex in _WEATHER_SKIES.items():
                            made = publish_texture(resolver.resolve(tex))
                            if made:
                                sky_textures[weather] = made.get("url", "")
                    sky_url = sky_textures.get("clear", "")
                    star_url = ""
                    if not interior and resolver is not None:
                        star_made = publish_texture(resolver.resolve(_STAR_TEXTURE))
                        if star_made:
                            star_url = star_made.get("url", "")

                    # Geometry is inlined (base64) rather than served: instancing
                    # already makes it small -- one copy per unique model -- so a
                    # handful of KB in the page beats thousands of loopback fetches
                    # (which is what failed on a large cell). Only three.js and the
                    # textures are fetched, and the textures decode lazily.
                    page = build_cell_viewer_page(
                        label,
                        groups,
                        library_url=self._three_js_url(server),
                        resolver=resolver,
                        title=title,
                        single_sided=interior,
                        publish_texture=publish_texture,
                        focus_indices=focus_indices,
                        ref_ids=ref_ids,
                        record_types=record_types,
                        adjacent_flags=adjacent_flags,
                        sky_texture_url=sky_url,
                        sky_textures=sky_textures,
                        star_texture_url=star_url,
                        cell_info=cell_info,
                        object_info=object_info,
                    )
                    url = session.publish(
                        "index.html", Payload(page.encode("utf-8"), "text/html; charset=utf-8")
                    )
                    opened = ("url", url)
                    print(f"  served cell in {clock() - mark:.1f}s")
                else:
                    print("  no loopback port; building a standalone page...")
                    page = build_cell_viewer_page(
                        label,
                        groups,
                        resolver=resolver,
                        title=title,
                        single_sided=interior,
                        focus_indices=focus_indices,
                        ref_ids=ref_ids,
                        record_types=record_types,
                        adjacent_flags=adjacent_flags,
                        cell_info=cell_info,
                        object_info=object_info,
                    )
                    opened = ("page", page)
                    print(f"  built page in {clock() - mark:.1f}s ({len(page) / 1_048_576:.1f} MB)")
            status = _("Cell preview ready (%(drawn)d object(s) drawn).") % {"drawn": cell.drawn}
        except Exception:  # noqa: BLE001 -- worker top level reports into the log
            writer.write("\nERROR: cell preview failed:\n" + traceback.format_exc())
            status = _("Cell preview failed -- see log.")
            opened = None
        self._schedule_ui(0, self._cellpreview_finished, opened, label, status)

    @staticmethod
    def _print_audit(audit: Any, scene: Any, placement_count: int) -> None:  # noqa: ANN401
        """Print the cell audit as a stat block, mirroring the preview panel."""
        rows = [
            (_("references"), audit.references),
            (_("placed"), audit.placed),
            (_("no mesh record"), audit.no_mesh_record),
            (_("editor markers"), audit.editor_markers),
            (_("actors skipped"), audit.actors_skipped),
            (_("overridden by later"), audit.overridden_by_later),
            (_("deleted by later"), audit.deleted_by_later),
            (_("moved"), audit.moved),
            (_("meshes drawn"), scene.drawn),
            (_("meshes not found"), len(scene.missing_models)),
        ]
        for label, value in rows:
            print(f"  {label:.<24}{value:>6}")
        for missing in scene.missing_models[:20]:
            print(_("    missing mesh: %(model)s") % {"model": missing})

    def _cellpreview_finished(
        self, opened: tuple[str, str] | None, label: str, status: str
    ) -> None:
        """Back on the UI thread: re-enable buttons and open the view.

        Args:
            opened: ``("url", loopback_url)`` for the served page, ``("page",
                html)`` for the standalone fallback, or ``None`` on failure.
            label: The cell label, for the window title.
            status: The status-bar line.
        """
        self.worker_running = False
        self.sort_button.configure(state="normal")
        for button in (
            self.export_button,
            self.conflicts_button,
            self.cellmap_button,
            self.cellpreview_button,
            self.resource_button,
        ):
            button.configure(state="normal" if self._current_plan else "disabled")
        self.status_var.set(status)
        if opened is None:
            return
        kind, target = opened
        title = _("Cell preview: %(label)s") % {"label": label}
        if kind == "url":
            # The served page must fetch its geometry, so it needs a real viewer
            # (pywebview or the browser), never tkinterweb -- open_html_in_app
            # picks that chain, same as the conflict window's mesh view.
            opener = getattr(self, "open_html_in_app", None)
            if callable(opener):
                opener(target, title)
            else:  # pragma: no cover - only if the mixin is used outside App
                import webbrowser

                webbrowser.open(target)
        else:
            self._open_html_view(target, "cell_preview", title)
