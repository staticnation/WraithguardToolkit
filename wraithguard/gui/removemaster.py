"""Remove- or rename-a-master window: pick a plugin, pick a master, act.

Split out of ``App`` as a mixin, like the tes3cmd front-end. The heavy lifting is
:func:`wraithguard.esp.remove_master_from_bytes` /
:func:`wraithguard.esp.rename_master_in_bytes`; this is only the window, a
dry-run preview into the main log, and guarded applies that keep a one-time
backup. Both fix a plugin OpenMW won't load because it lists a master it can't
find: *remove* it when the master is no longer needed (merged away, or now
provided by loose files / a pluginless version), or *rename* it when the same
master was simply renamed on disk.
"""

from __future__ import annotations

import shutil
import threading
import tkinter as tk
import traceback
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from tkinter import filedialog, messagebox, ttk
from typing import TYPE_CHECKING, Any

from wraithguard.esp import read_header, remove_master_from_bytes, rename_master_in_bytes
from wraithguard.gui import case_insensitive_filetypes
from wraithguard.gui.theme import DARK, apply_titlebar_theme
from wraithguard.gui.widgets import QueueWriter, add_tooltip
from wraithguard.i18n import gettext as _, ngettext

if TYPE_CHECKING:
    import queue
    from collections.abc import Callable


class RemoveMasterMixin:
    """The remove/rename-master window and its workers (mixed into ``App``)."""

    if TYPE_CHECKING:
        # The host contract -- these live on ``App`` (see the tes3cmd mixin for
        # the same pattern). Declared so mypy can check the half that is here.
        root: tk.Tk
        log_queue: queue.Queue
        status_var: tk.StringVar
        sort_button: ttk.Button
        worker_running: bool

        def _schedule_ui(
            self, delay_ms: int, func: Callable[..., Any], *args: Any  # noqa: ANN401
        ) -> None: ...

    def on_remove_master_window(self) -> None:
        """Open (or raise) the remove-master window."""
        win = getattr(self, "_rmw_win", None)
        if win is not None and win.winfo_exists():
            win.lift()
            return
        win = tk.Toplevel(self.root)
        self._rmw_win = win
        apply_titlebar_theme(win)
        win.title(_("Remove / Rename Master"))
        win.configure(bg=DARK["bg"])
        win.geometry("660x260")
        top = ttk.Frame(win, padding=10)
        top.pack(fill="both", expand=True)
        top.columnconfigure(1, weight=1)

        # --- plugin picker -------------------------------------------------
        ttk.Label(top, text=_("Plugin:")).grid(row=0, column=0, sticky="w")
        self._rmw_plugin_var = tk.StringVar()
        plugin_entry = ttk.Entry(top, textvariable=self._rmw_plugin_var, state="readonly")
        plugin_entry.grid(row=0, column=1, sticky="ew", padx=6)
        ttk.Button(top, text=_("Browse..."), command=self._rmw_browse).grid(row=0, column=2)

        # --- master picker -------------------------------------------------
        ttk.Label(top, text=_("Master:")).grid(row=1, column=0, sticky="w", pady=(8, 0))
        self._rmw_master_var = tk.StringVar()
        self._rmw_master_combo = ttk.Combobox(
            top, textvariable=self._rmw_master_var, state="readonly", values=()
        )
        self._rmw_master_combo.grid(row=1, column=1, columnspan=2, sticky="ew", padx=6, pady=(8, 0))
        add_tooltip(
            self._rmw_master_combo,
            _(
                "The master to strip from this plugin's header. References into it are "
                "dropped; references into masters listed after it shift up one slot. "
                "Use this when a master was renamed, merged into another file, or is now "
                "provided by loose files / a pluginless version -- OpenMW refuses to load "
                "a plugin whose header names a master it can't find."
            ),
        )

        # --- actions -------------------------------------------------------
        actions = ttk.Frame(top)
        actions.grid(row=2, column=0, columnspan=3, sticky="ew", pady=(14, 0))
        self._rmw_preview_btn = ttk.Button(
            actions, text=_("Preview"), command=self._rmw_preview, state="disabled"
        )
        self._rmw_preview_btn.pack(side="left")
        add_tooltip(
            self._rmw_preview_btn,
            _("Report to the main Log what removing this master would drop -- changes nothing."),
        )
        self._rmw_apply_btn = ttk.Button(
            actions, text=_("Remove Master"), command=self._rmw_apply, state="disabled"
        )
        self._rmw_apply_btn.pack(side="left", padx=(6, 0))
        add_tooltip(
            self._rmw_apply_btn,
            _("Rewrite the plugin without this master, keeping a one-time .premaster.bak."),
        )
        self._rmw_rename_btn = ttk.Button(
            actions, text=_("Rename to..."), command=self._rmw_rename, state="disabled"
        )
        self._rmw_rename_btn.pack(side="left", padx=(6, 0))
        add_tooltip(
            self._rmw_rename_btn,
            _(
                "Point this master at a differently-named file (references are kept, only "
                "the header entry and its recorded size change). Use it when a master was "
                "renamed on disk. Keeps a one-time .premaster.bak."
            ),
        )
        ttk.Button(actions, text=_("Close"), command=win.destroy).pack(side="right")

    def _rmw_browse(self) -> None:
        """Pick a plugin and load its master list into the combo box."""
        path = filedialog.askopenfilename(
            title=_("Choose a plugin"),
            filetypes=case_insensitive_filetypes(
                (
                    ("Morrowind plugins", "*.esp"),
                    ("Morrowind masters", "*.esm"),
                    ("OpenMW add-ons", "*.omwaddon"),
                    ("All files", "*.*"),
                )
            ),
        )
        if not path:
            return
        try:
            header = read_header(Path(path).read_bytes())
        except (OSError, ValueError) as error:
            messagebox.showerror(
                _("Remove Master"),
                _("Could not read '%(name)s':\n%(error)s")
                % {"name": Path(path).name, "error": error},
                parent=self._rmw_win,
            )
            return
        self._rmw_plugin_var.set(path)
        masters = [name for name, _size in header.masters]
        self._rmw_master_combo.configure(values=masters)
        if masters:
            self._rmw_master_var.set(masters[0])
            self._rmw_preview_btn.configure(state="normal")
            self._rmw_apply_btn.configure(state="normal")
            self._rmw_rename_btn.configure(state="normal")
        else:
            self._rmw_master_var.set("")
            self._rmw_preview_btn.configure(state="disabled")
            self._rmw_apply_btn.configure(state="disabled")
            self._rmw_rename_btn.configure(state="disabled")
            messagebox.showinfo(
                _("Remove Master"),
                _("'%(name)s' has no masters to remove.") % {"name": Path(path).name},
                parent=self._rmw_win,
            )

    def _rmw_selection(self) -> tuple[str, str] | None:
        """The chosen ``(plugin_path, master_name)``, or ``None`` if incomplete."""
        plugin = self._rmw_plugin_var.get()
        master = self._rmw_master_var.get()
        if not plugin or not master:
            return None
        return plugin, master

    def _rmw_preview(self) -> None:
        """Dry-run the removal and write the report to the main log."""
        selection = self._rmw_selection()
        if selection is None:
            return
        plugin, master = selection
        writer = QueueWriter(self.log_queue)
        try:
            _new_bytes, report = remove_master_from_bytes(Path(plugin).read_bytes(), master)
        except (OSError, ValueError) as error:
            writer.write(f"\nERROR: {error}\n")
            return
        with redirect_stdout(writer.as_stream()):
            print("\n" + "=" * 70)
            print(_(" REMOVE MASTER (preview): %(master)s") % {"master": master})
            print("=" * 70)
            self._rmw_print_report(report, Path(plugin).name)
            print(_("  (preview only -- nothing was written)"))

    def _rmw_apply(self) -> None:
        """Confirm, then rewrite the plugin without the chosen master on a worker."""
        if self.worker_running:
            return
        selection = self._rmw_selection()
        if selection is None:
            return
        plugin, master = selection
        if not messagebox.askyesno(
            _("Remove Master"),
            _(
                "Remove master '%(master)s' from '%(name)s'?\n\nReferences into it are "
                "dropped and later master indices shift; a one-time .premaster.bak of the "
                "original is kept. Run Preview first to see what would be dropped."
            )
            % {"master": master, "name": Path(plugin).name},
            parent=self._rmw_win,
        ):
            return
        self.worker_running = True
        self._rmw_apply_btn.configure(state="disabled")
        self._rmw_rename_btn.configure(state="disabled")
        self.sort_button.configure(state="disabled")
        self.status_var.set(_("Removing master..."))
        threading.Thread(target=self._rmw_worker, args=(plugin, master), daemon=True).start()

    def _rmw_rename(self) -> None:
        """Ask for the new master file, confirm, then rename on a worker."""
        if self.worker_running:
            return
        selection = self._rmw_selection()
        if selection is None:
            return
        plugin, old_master = selection
        new_path = filedialog.askopenfilename(
            title=_("Point '%(master)s' at which file?") % {"master": old_master},
            filetypes=case_insensitive_filetypes(
                (
                    ("Morrowind masters", "*.esm"),
                    ("Morrowind plugins", "*.esp"),
                    ("OpenMW add-ons", "*.omwaddon"),
                    ("All files", "*.*"),
                )
            ),
        )
        if not new_path:
            return
        new_name = Path(new_path).name
        try:
            new_size = Path(new_path).stat().st_size
        except OSError as error:
            messagebox.showerror(
                _("Rename Master"),
                _("Could not read '%(name)s':\n%(error)s") % {"name": new_name, "error": error},
                parent=self._rmw_win,
            )
            return
        if not messagebox.askyesno(
            _("Rename Master"),
            _(
                "Point master '%(old)s' at '%(new)s' in '%(name)s'?\n\nReferences are kept "
                "(only the header entry and its recorded size change); a one-time "
                ".premaster.bak of the original is kept."
            )
            % {"old": old_master, "new": new_name, "name": Path(plugin).name},
            parent=self._rmw_win,
        ):
            return
        self.worker_running = True
        self._rmw_apply_btn.configure(state="disabled")
        self._rmw_rename_btn.configure(state="disabled")
        self.sort_button.configure(state="disabled")
        self.status_var.set(_("Renaming master..."))
        threading.Thread(
            target=self._rmw_rename_worker,
            args=(plugin, old_master, new_name, new_size),
            daemon=True,
        ).start()

    def _rmw_worker(self, plugin: str, master: str) -> None:
        """Do the removal off the UI thread: back up, rewrite, report."""
        writer = QueueWriter(self.log_queue)
        status = _("Remove master: done.")
        try:
            with redirect_stdout(writer.as_stream()), redirect_stderr(writer.as_stream()):
                print("\n" + "=" * 70)
                print(_(" REMOVE MASTER: %(master)s") % {"master": master})
                print("=" * 70)
                source = Path(plugin)
                new_bytes, report = remove_master_from_bytes(source.read_bytes(), master)
                if not report.removed:
                    print(
                        _("  '%(master)s' is not a master of '%(name)s' -- nothing to do.")
                        % {"master": master, "name": source.name}
                    )
                    status = _("Remove master: not a master of that plugin.")
                    return
                backup = source.with_name(source.name + ".premaster.bak")
                if not backup.exists():
                    shutil.copy2(source, backup)
                source.write_bytes(new_bytes)
                self._rmw_print_report(report, source.name)
                print(
                    _("  Rewrote %(name)s (backup: %(backup)s).")
                    % {"name": source.name, "backup": backup.name}
                )
                status = _("Removed '%(master)s'. Re-run '1. Sort' to refresh checks.") % {
                    "master": master
                }
        except Exception:  # noqa: BLE001 -- report the traceback into the log panel
            writer.write("\nERROR: remove master failed:\n" + traceback.format_exc())
            status = _("Remove master failed -- see log.")
        finally:
            self._schedule_ui(0, self._rmw_finished, status)

    def _rmw_rename_worker(
        self, plugin: str, old_master: str, new_name: str, new_size: int
    ) -> None:
        """Do the rename off the UI thread: back up, rewrite the header, report."""
        writer = QueueWriter(self.log_queue)
        status = _("Rename master: done.")
        try:
            with redirect_stdout(writer.as_stream()), redirect_stderr(writer.as_stream()):
                print("\n" + "=" * 70)
                print(
                    _(" RENAME MASTER: %(old)s -> %(new)s") % {"old": old_master, "new": new_name}
                )
                print("=" * 70)
                source = Path(plugin)
                new_bytes, report = rename_master_in_bytes(
                    source.read_bytes(), old_master, new_name, new_size
                )
                if not report.renamed:
                    print(
                        _("  '%(master)s' is not a master of '%(name)s' -- nothing to do.")
                        % {"master": old_master, "name": source.name}
                    )
                    status = _("Rename master: not a master of that plugin.")
                    return
                backup = source.with_name(source.name + ".premaster.bak")
                if not backup.exists():
                    shutil.copy2(source, backup)
                source.write_bytes(new_bytes)
                remaining = ", ".join(report.remaining_masters) or _("(none)")
                print(_("  Masters now: %(list)s") % {"list": remaining})
                print(
                    _("  Rewrote %(name)s (backup: %(backup)s).")
                    % {"name": source.name, "backup": backup.name}
                )
                status = _("Renamed to '%(new)s'. Re-run '1. Sort' to refresh checks.") % {
                    "new": new_name
                }
        except ValueError as error:  # target already a master of the plugin
            writer.write(f"\n  {error}\n")
            status = _("Rename master: %(error)s") % {"error": error}
        except Exception:  # noqa: BLE001 -- report the traceback into the log panel
            writer.write("\nERROR: rename master failed:\n" + traceback.format_exc())
            status = _("Rename master failed -- see log.")
        finally:
            self._schedule_ui(0, self._rmw_finished, status)

    def _rmw_finished(self, status: str) -> None:
        """Re-enable the UI after a worker run and post the final status."""
        self.worker_running = False
        self.sort_button.configure(state="normal")
        win = getattr(self, "_rmw_win", None)
        if win is not None and win.winfo_exists():
            self._rmw_apply_btn.configure(state="normal")
            self._rmw_rename_btn.configure(state="normal")
        self.status_var.set(status)

    @staticmethod
    def _rmw_print_report(report: Any, plugin_name: str) -> None:  # noqa: ANN401 -- report type
        """Print a removal report: what dropped, and the masters that remain."""
        if report.references_dropped:
            print(
                ngettext(
                    "  %(count)d reference into the master was dropped:",
                    "  %(count)d references into the master were dropped:",
                    report.references_dropped,
                )
                % {"count": report.references_dropped}
            )
            for label, count in sorted(report.dropped_by_cell.items()):
                print(f"      {label}: {count}")
        else:
            print(_("  No references pointed into it -- a clean removal."))
        if report.references_remapped:
            print(
                _("  %(count)d reference(s) into later masters were renumbered.")
                % {"count": report.references_remapped}
            )
        remaining = ", ".join(report.remaining_masters) or _("(none)")
        print(_("  Masters remaining: %(list)s") % {"list": remaining})
