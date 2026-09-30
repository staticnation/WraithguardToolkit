"""Cell Preview and the mesh viewer: launch the viewer shell on the current setup.

Mixed into ``App``. Both views are ``wraithguard-viewer --cell-viewer`` -- engine,
commands and page all live in that binary (viewer-shell/src/cellviewer,
viewer-shell/viewcore, viewer-shell/ui). Python writes the setup the viewer opens
(the cfg as the sort panels hold it now, not the real openmw.cfg) and launches it;
the cell map's "open in Cell Preview" reaches :meth:`CellPreviewMixin._on_open_cell_request`
over the loopback server.
"""

from __future__ import annotations

from pathlib import Path
from tkinter import messagebox
from typing import TYPE_CHECKING

import wraithguard_toolkit as core
from wraithguard.gui import app_base_dir
from wraithguard.i18n import gettext as _
from wraithguard.viz.serve import Payload

if TYPE_CHECKING:
    import tkinter as tk
    from collections.abc import Callable
    from typing import Any


class CellPreviewMixin:
    """Cell Preview and the mesh viewer (mixed into ``App``)."""

    if TYPE_CHECKING:
        # The host contract -- these live on ``App``.
        status_var: tk.StringVar
        worker_running: bool
        _current_plan: dict | None

        def _schedule_ui(
            self,
            delay_ms: int,
            func: Callable[..., Any],
            *args: Any,  # noqa: ANN401
        ) -> None: ...

    @staticmethod
    def _viewer_shell_binary() -> str | None:
        """The ``wraithguard-viewer`` binary (see :mod:`wraithguard.viewer_launch`)."""
        from wraithguard.viewer_launch import viewer_binary

        return viewer_binary()

    def _write_viewer_setup(self, extra_plugins: list[Path] | None = None) -> Path | None:
        """Write the setup the viewer opens: the cfg as the sort panels hold it now.

        What Export would write for the current rows (enabled plugins and data paths,
        in their panel order, plus any groundcover the plan declares), rendered in
        memory and saved beside Wraithguard's settings -- the real openmw.cfg is not
        touched.

        Args:
            extra_plugins: Plugins to load last on top of the order (a patch just
                written, to preview it): each one's folder is added as the last data
                path, so its file wins, and it is loaded after everything else.

        Returns:
            The written cfg, or None when there is no plan or openmw.cfg yet.
        """
        plan = self._current_plan
        cfg = getattr(self, "cfg_var", None)
        cfg_path = Path(cfg.get()) if cfg is not None and cfg.get() else None
        if not plan or cfg_path is None:
            return None
        order_panel = getattr(self, "order_panel", None)
        data_panel = getattr(self, "data_order_panel", None)
        final_order = order_panel.get_enabled() if order_panel is not None else None
        data_order = data_panel.get_enabled() if data_panel is not None else None
        lines = core.render_cfg_lines(plan, final_order or None, data_order or None)
        lines = core.viewer_setup_cfg(lines, cfg_path.parent)
        from wraithguard.gui.record_link import with_extra_plugins

        lines = with_extra_plugins(lines, extra_plugins or [])
        out = app_base_dir() / "wraithguard_cellviewer_setup.cfg"
        out.write_text("\n".join(lines) + "\n", encoding="utf-8", newline="")
        return out

    def _open_cell_viewer(
        self,
        cell: str | None = None,
        meshes: list[dict[str, object]] | None = None,
        extra_plugins: list[Path] | None = None,
    ) -> bool:
        """Launch the shell's cell viewer on the current setup; False if it cannot start.

        Args:
            cell: A cell to open straight away -- ``"x,y"`` for an exterior,
                ``"int:<name>"`` for an interior -- or None for the picker.
            meshes: Open the mesh viewer instead: ``[{"path", "label"}]``, a path
                being a load-order path or one mod's own file on disk; optionally
                ``"inspect"`` (the block panel, :func:`wraithguard.nif.inspect.inspect_mesh`)
                and ``"edit"`` (``{"url", "filename"}``, the Save handler).
            extra_plugins: Plugins loaded last on top of the order (see
                :meth:`_write_viewer_setup`).

        Returns:
            Whether the viewer launched.
        """
        import subprocess

        exe = self._viewer_shell_binary()
        if exe is None:
            print("wraithguard-viewer not found.")
            return False
        try:
            setup = self._write_viewer_setup(extra_plugins)
        except (OSError, KeyError, ValueError) as exc:
            print(f"Could not prepare the setup for the cell viewer ({exc}).")
            return False
        if setup is None:
            return False
        import json

        from wraithguard.gui.theme import DARK

        prefs = app_base_dir() / "wraithguard_cellviewer.profile.json"
        # Too long for a command line: the subset (your own mods), which the viewer's
        # cell map rings in orange.
        extra_file = app_base_dir() / "wraithguard_cellviewer_extra.json"
        extra: Path | None = extra_file
        subset = [str(n) for n in ((self._current_plan or {}).get("subset") or [])]
        # Where the viewer's object inspector sends "show this record in the conflict
        # viewer" (ConflictWindowsMixin._on_open_record_request), token and all.
        links: dict[str, str] = {}
        server_of = getattr(self, "_viewer_server", None)
        handler = getattr(self, "_on_open_record_request", None)
        server = server_of() if server_of is not None and handler is not None else None
        if server is not None and handler is not None:
            links["openRecord"] = server.register_post("wg_open_record", handler)
            # And where it asks, while it is open, for the next thing to show.
            links["poll"] = server.register_post("wg_viewer_poll", self._on_viewer_poll)
        try:
            extra_file.write_text(
                json.dumps({"subset": subset, "meshes": meshes or [], "links": links}),
                encoding="utf-8",
            )
        except OSError:
            extra = None
        cmd = [
            exe,
            "--cell-viewer",
            "--openmw-cfg",
            str(setup),
            "--prefs",
            str(prefs),
            "--title",
            _("Cell Preview"),
            # The toolkit's current chrome palette: the viewer wears the same theme.
            "--theme",
            json.dumps(DARK),
        ]
        if extra is not None:
            cmd += ["--extra", str(extra)]
        if cell:
            cmd += ["--cell", cell]
        if meshes:
            cmd += ["--mesh-view"]
            cmd[cmd.index("--title") + 1] = _("Mesh Viewer")
        try:
            from wraithguard.viewer_launch import viewer_popen_kwargs

            subprocess.Popen(cmd, **viewer_popen_kwargs())  # type: ignore[call-overload]  # noqa: S603
        except OSError as exc:
            print(f"Cell viewer failed to launch ({exc}).")
            return False
        self.status_var.set(_("Cell preview opened."))
        return True

    def preview_plugin(self, plugin: Path) -> None:
        """Open Cell Preview on the order with ``plugin`` loaded last (a patch just written).

        The viewer opens on the plugin's cells, reviewed for it: what it placed and
        changed, or the cells without it.

        Args:
            plugin: The plugin file.
        """
        from wraithguard.gui.record_link import plugin_spec

        if not self._open_cell_viewer(plugin_spec(plugin.name) or None, extra_plugins=[plugin]):
            messagebox.showerror(
                _("Cell Preview"),
                _("The cell viewer (wraithguard-viewer) could not be started."),
            )

    def _on_viewer_poll(self, body: bytes) -> Payload:
        """An open cell viewer asking for the next thing to show (``wg_viewer_poll``).

        Answers the oldest spec queued by :meth:`show_in_cell_preview`, or nothing, and
        remembers that a viewer is listening.

        Args:
            body: Empty.

        Returns:
            The spec, or an empty answer.
        """
        import time

        self._viewer_polled_at = time.monotonic()
        queue: list[str] = getattr(self, "_viewer_nav", None) or []
        spec = queue.pop(0) if queue else ""
        self._viewer_nav = queue
        return Payload(spec.encode("utf-8"), "text/plain; charset=utf-8")

    def show_in_cell_preview(self, spec: str) -> None:
        """Show ``spec`` (``find:<tag>:<id>``, ``x,y``, ``int:<name>``) in Cell Preview.

        An open viewer from this session (one that asked within the last few seconds)
        is sent there; otherwise a viewer is launched on it.

        Args:
            spec: What to show.
        """
        import time

        polled = getattr(self, "_viewer_polled_at", None)
        if polled is not None and time.monotonic() - polled < 5.0:
            queue: list[str] = getattr(self, "_viewer_nav", None) or []
            queue.append(spec)
            self._viewer_nav = queue
            self.status_var.set(_("Showing it in the open Cell Preview."))
            return
        if not self._open_cell_viewer(spec):
            messagebox.showerror(
                _("Cell Preview"),
                _("The cell viewer (wraithguard-viewer) could not be started."),
            )

    def _on_open_cell_request(self, body: bytes) -> Payload:
        """The cell map's "open in Cell Preview": launch the viewer at that cell.

        Registered on the loopback server (token-guarded like everything on it) as
        ``wg_open_cell``; the cell map page posts ``x,y`` or ``int:<name>``.

        Args:
            body: The cell spec.

        Returns:
            A short plain-text answer for the page.

        Raises:
            ValueError: For anything that is not a cell spec (the server answers 400).
        """
        import re

        spec = body.decode("utf-8", errors="replace").strip()
        if not re.fullmatch(r"-?\d{1,5},-?\d{1,5}|int:[^\r\n]{1,200}", spec):
            raise ValueError("not a cell")
        self._schedule_ui(0, self._open_cell_viewer, spec)
        return Payload(b"ok", "text/plain; charset=utf-8")

    def on_cell_preview(self) -> None:
        """Open the cell viewer on the current setup."""
        if self.worker_running:
            return
        if not self._open_cell_viewer():
            messagebox.showerror(
                _("Cell Preview"),
                _(
                    "The cell viewer (wraithguard-viewer) could not be started. "
                    "Sort first, and check that the viewer is installed beside Wraithguard."
                ),
            )
