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

    def _write_viewer_setup(self) -> Path | None:
        """Write the setup the viewer opens: the cfg as the sort panels hold it now.

        What Export would write for the current rows (enabled plugins and data paths,
        in their panel order, plus any groundcover the plan declares), rendered in
        memory and saved beside Wraithguard's settings -- the real openmw.cfg is not
        touched.

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
        out = app_base_dir() / "wraithguard_cellviewer_setup.cfg"
        out.write_text("\n".join(lines) + "\n", encoding="utf-8", newline="")
        return out

    def _open_cell_viewer(
        self, cell: str | None = None, meshes: list[dict[str, object]] | None = None
    ) -> bool:
        """Launch the shell's cell viewer on the current setup; False if it cannot start.

        Args:
            cell: A cell to open straight away -- ``"x,y"`` for an exterior,
                ``"int:<name>"`` for an interior -- or None for the picker.
            meshes: Open the mesh viewer instead: ``[{"path", "label"}]``, a path
                being a load-order path or one mod's own file on disk; optionally
                ``"inspect"`` (the block panel, :func:`wraithguard.nif.inspect.inspect_mesh`)
                and ``"edit"`` (``{"url", "filename"}``, the Save handler).

        Returns:
            Whether the viewer launched.
        """
        import subprocess

        exe = self._viewer_shell_binary()
        if exe is None:
            print("wraithguard-viewer not found.")
            return False
        try:
            setup = self._write_viewer_setup()
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
        try:
            extra_file.write_text(
                json.dumps({"subset": subset, "meshes": meshes or []}), encoding="utf-8"
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
