"""The Lua scripts window: the load order's OpenMW Lua scripts, read and checked.

Opened from the Conflicts window. It scans the openmw.cfg the main window points at
(:func:`wraithguard.lua.scan.scan_cfg`) on a worker thread, then shows:

- left, the scripts in load order, each with its flags and how many findings it has,
  under a first row for the findings about the load order as a whole;
- right, for the selected script: its source with Lua syntax highlighting (from the
  same tokenizer the checks use), its syntax tree, and its findings - double-click
  a finding to go to its line.

Read-only: nothing here writes.

Copyright (c) 2026 StaticNation.
"""

from __future__ import annotations

import tempfile
import threading
import tkinter as tk
from pathlib import Path
from tkinter import messagebox, ttk
from typing import TYPE_CHECKING, Any

from wraithguard.gui import open_in_browser
from wraithguard.gui.theme import (
    DARK,
    THEME_PRESETS,
    _json_syntax_colors,
    apply_titlebar_theme,
)
from wraithguard.gui.widgets import add_tooltip
from wraithguard.i18n import gettext as _
from wraithguard.logging_setup import get_logger
from wraithguard.lua.flowchart import flowchart, flowchart_html
from wraithguard.lua.lexer import LuaSyntaxError, tokenize
from wraithguard.lua.report import all_findings, render
from wraithguard.lua.scan import scan_cfg
from wraithguard.viz.serve import Payload

if TYPE_CHECKING:
    from collections.abc import Callable

    from wraithguard.lua.analysis import Finding
    from wraithguard.lua.parser import Node
    from wraithguard.lua.scan import LuaScan, ScriptRecord

LOG = get_logger(__name__)

_LOAD_ORDER_IID = "__load_order__"
_FUNCTION_KINDS = frozenset({"Function", "LocalFunction", "FunctionStat"})
_SEVERITY_ORDER = {"error": 0, "warn": 1, "info": 2}
#: Token kind -> highlight tag.
_TOKEN_TAGS = {
    "keyword": "lua_keyword",
    "string": "lua_string",
    "number": "lua_number",
    "comment": "lua_comment",
    "op": "lua_op",
}
#: Above this many characters a script is shown without highlighting (Tk slows down).
_HIGHLIGHT_LIMIT = 400_000


def _span(line: int, col: int, text: str) -> tuple[str, str]:
    """Tk text indexes for a token.

    Args:
        line: Its 1-based line.
        col: Its 1-based column.
        text: Its text (may span lines).

    Returns:
        ``(start, end)`` indexes.
    """
    nl = text.count("\n")
    if nl:
        end = f"{line + nl}.{len(text) - text.rfind(chr(10)) - 1}"
    else:
        end = f"{line}.{col - 1 + len(text)}"
    return f"{line}.{col - 1}", end


def _code_view(frame: ttk.Frame) -> tk.Text:
    """A read-only, scrolling, monospaced text view for source code.

    Args:
        frame: Its frame (it fills it).

    Returns:
        The text widget.
    """
    view = tk.Text(
        frame,
        wrap="none",
        font=("TkFixedFont", 10),
        bg=DARK["field_bg"],
        fg=DARK["fg"],
        insertbackground=DARK["fg"],
        selectbackground=DARK["select"],
        borderwidth=0,
        undo=False,
    )
    ysb = ttk.Scrollbar(frame, orient="vertical", command=view.yview)
    xsb = ttk.Scrollbar(frame, orient="horizontal", command=view.xview)
    view.configure(yscrollcommand=ysb.set, xscrollcommand=xsb.set)
    ysb.pack(side="right", fill="y")
    xsb.pack(side="bottom", fill="x")
    view.pack(side="left", fill="both", expand=True)
    return view


class LuaViewMixin:
    """The Lua scripts window (mixed into ``App``)."""

    if TYPE_CHECKING:  # pragma: no cover - declarations for the host class
        root: tk.Tk
        cfg_var: tk.StringVar
        log_theme_var: tk.StringVar
        _conflict_win: tk.Toplevel | None

        def _schedule_ui(
            self, delay_ms: int, func: Callable[..., Any], *args: Any  # noqa: ANN401
        ) -> None: ...
        def _resolve_theme(self, name: str) -> dict | None: ...

    # -- the window ---------------------------------------------------------------

    def show_lua_view(self) -> None:
        """Open the Lua scripts window (or raise it), and scan the current openmw.cfg."""
        win = getattr(self, "_lua_win", None)
        if win is not None and win.winfo_exists():
            win.lift()
            win.focus_force()
            return
        cfg = Path(self.cfg_var.get().strip()) if self.cfg_var.get().strip() else None
        if cfg is None or not cfg.is_file():
            messagebox.showwarning(
                _("No openmw.cfg"),
                _("Choose your openmw.cfg in the main window first: the scripts come from it."),
            )
            return

        win = tk.Toplevel(getattr(self, "_conflict_win", None) or self.root)
        self._lua_win = win
        apply_titlebar_theme(win)
        win.title(_("Lua scripts"))
        win.configure(bg=DARK["bg"])
        win.geometry("1400x820")
        win.minsize(900, 500)

        top = ttk.Frame(win, padding=(8, 6, 8, 2))
        top.pack(fill="x")
        self._lua_status = tk.StringVar(value=_("Reading the load order's scripts..."))
        ttk.Label(top, textvariable=self._lua_status, foreground=DARK["fg_dim"]).pack(
            side="left", fill="x", expand=True
        )
        ttk.Button(top, text=_("Rescan"), command=lambda: self._lua_scan(cfg)).pack(side="right")
        ttk.Button(top, text=_("Copy report"), command=self._lua_copy_report).pack(
            side="right", padx=(0, 6)
        )
        flow = ttk.Button(top, text=_("Flowchart"), command=self._lua_flowchart)
        flow.pack(side="right", padx=(0, 6))
        add_tooltip(
            flow,
            _(
                "The control flow of the function selected in the Syntax tree tab (or of "
                "the whole script), as a flowchart in your browser: branches, loops, "
                "returns, breaks and gotos. Needs an internet connection the first time, "
                "for the chart library."
            ),
        )
        self._lua_show_info = tk.BooleanVar(value=True)
        ttk.Checkbutton(
            top, text=_("Show notes"), variable=self._lua_show_info, command=self._lua_refill
        ).pack(side="right", padx=(0, 12))

        panes = ttk.PanedWindow(win, orient="horizontal")
        panes.pack(fill="both", expand=True, padx=8, pady=4)

        left = ttk.Frame(panes)
        nav = ttk.Treeview(
            left, show="tree headings", columns=("flags", "found"), style="Conf.Treeview"
        )
        nav.heading("#0", text=_("Script"))
        nav.column("#0", width=380, stretch=True)
        nav.heading("flags", text=_("Flags"))
        nav.column("flags", width=150, stretch=False)
        nav.heading("found", text=_("Findings"))
        nav.column("found", width=70, anchor="e", stretch=False)
        nav.tag_configure("error", foreground=DARK.get("error", "#e06c75"))
        nav.tag_configure("warn", foreground=DARK.get("warn", "#e5c07b"))
        nav.tag_configure("missing", foreground=DARK["fg_dim"])
        nav_scroll = ttk.Scrollbar(left, orient="vertical", command=nav.yview)
        nav.configure(yscrollcommand=nav_scroll.set)
        nav.pack(side="left", fill="both", expand=True)
        nav_scroll.pack(side="right", fill="y")
        panes.add(left, weight=2)
        nav.bind("<<TreeviewSelect>>", lambda _e: self._lua_select())
        self._lua_nav = nav

        right = ttk.PanedWindow(panes, orient="vertical")
        panes.add(right, weight=5)
        book = ttk.Notebook(right)
        right.add(book, weight=4)

        src_frame = ttk.Frame(book)
        src = _code_view(src_frame)
        book.add(src_frame, text=_("Source"))
        self._lua_book = book
        self._lua_src_frame = src_frame
        self._lua_src = src
        self._lua_style_source(src)
        # Teal: the .tl a mod ships beside its compiled .lua, when it does.
        teal_frame = ttk.Frame(book)
        self._lua_teal = _code_view(teal_frame)
        book.add(teal_frame, text=_("Teal source"))
        self._lua_style_source(self._lua_teal)

        tree_frame = ttk.Frame(book)
        ast = ttk.Treeview(
            tree_frame, show="tree headings", columns=("line",), style="Conf.Treeview"
        )
        ast.heading("#0", text=_("Syntax tree"))
        ast.column("#0", width=600, stretch=True)
        ast.heading("line", text=_("Line"))
        ast.column("line", width=60, anchor="e", stretch=False)
        ast_scroll = ttk.Scrollbar(tree_frame, orient="vertical", command=ast.yview)
        ast.configure(yscrollcommand=ast_scroll.set)
        ast.pack(side="left", fill="both", expand=True)
        ast_scroll.pack(side="right", fill="y")
        ast.bind("<<TreeviewOpen>>", lambda _e: self._lua_expand_ast())
        ast.bind("<Double-1>", lambda _e: self._lua_goto(self._lua_ast_line()))
        book.add(tree_frame, text=_("Syntax tree"))
        self._lua_ast = ast
        self._lua_ast_nodes: dict[str, Node] = {}

        found_frame = ttk.Frame(right)
        found = ttk.Treeview(
            found_frame,
            show="headings",
            columns=("sev", "code", "line", "msg"),
            height=8,
            style="Conf.Treeview",
        )
        for col, title, width, stretch in (
            ("sev", _("Severity"), 70, False),
            ("code", _("Check"), 170, False),
            ("line", _("Line"), 60, False),
            ("msg", _("What it means"), 700, True),
        ):
            found.heading(col, text=title)
            found.column(col, width=width, stretch=stretch)
        found.tag_configure("error", foreground=DARK.get("error", "#e06c75"))
        found.tag_configure("warn", foreground=DARK.get("warn", "#e5c07b"))
        found_scroll = ttk.Scrollbar(found_frame, orient="vertical", command=found.yview)
        found.configure(yscrollcommand=found_scroll.set)
        found.pack(side="left", fill="both", expand=True)
        found_scroll.pack(side="right", fill="y")
        found.bind("<Double-1>", lambda _e: self._lua_goto_finding())
        right.add(found_frame, weight=1)
        self._lua_found = found

        self._lua_result: LuaScan | None = None
        self._lua_records: dict[str, ScriptRecord] = {}
        self._lua_scan(cfg)

    # -- scanning -------------------------------------------------------------------

    def _lua_scan(self, cfg: Path) -> None:
        """Scan ``cfg`` on a worker thread, then fill the window.

        Args:
            cfg: The openmw.cfg.
        """
        self._lua_status.set(_("Reading the load order's scripts..."))

        def work() -> None:
            """Scan, then hand the result (or the error) to the UI thread."""
            try:
                result = scan_cfg(cfg)
            except Exception as exc:  # shown to the user, never fatal
                LOG.exception("Lua scan failed")
                self._schedule_ui(0, self._lua_failed, str(exc))
                return
            self._schedule_ui(0, self._lua_filled, result)

        threading.Thread(target=work, name="wg-lua-scan", daemon=True).start()

    def _lua_failed(self, message: str) -> None:
        """Say the scan failed.

        Args:
            message: The error.
        """
        if self._lua_alive():
            self._lua_status.set(_("The scan failed: ") + message)

    def _lua_alive(self) -> bool:
        """Is the window still open?

        Returns:
            True when it is.
        """
        win = getattr(self, "_lua_win", None)
        return bool(win is not None and win.winfo_exists())

    def _lua_filled(self, result: LuaScan) -> None:
        """Take a finished scan.

        Args:
            result: The scan.
        """
        if not self._lua_alive():
            return
        self._lua_result = result
        found = all_findings(result)
        counts = {sev: sum(1 for _p, f in found if f.severity == sev) for sev in _SEVERITY_ORDER}
        sources = {
            "scripts": len(result.scripts),
            "files": len(result.omwscripts),
            "lual": len(result.lual_files),
        }
        what = _("%(scripts)d scripts from %(files)d .omwscripts file(s) and %(lual)d addon(s)")
        tally = _("%(error)d errors, %(warn)d warnings, %(info)d notes")
        self._lua_status.set(f"{what % sources}  -  {tally % counts}")
        self._lua_refill()

    def _lua_findings_for(self, path: str) -> list[Finding]:
        """The findings shown for a script ("" for the load order).

        Args:
            path: The script's path, or "".

        Returns:
            Its findings, most serious first, notes left out when hidden.
        """
        result = self._lua_result
        if result is None:
            return []
        show_info = self._lua_show_info.get()
        keep = {"error", "warn", "info"} if show_info else {"error", "warn"}
        out = [f for p, f in all_findings(result) if p == path and f.severity in keep]
        out.sort(key=lambda f: (_SEVERITY_ORDER.get(f.severity, 3), f.line))
        return out

    def _lua_refill(self) -> None:
        """(Re)build the script list from the last scan."""
        result = self._lua_result
        nav = self._lua_nav
        nav.delete(*nav.get_children())
        self._lua_records = {}
        if result is None:
            return
        own = self._lua_findings_for("")
        nav.insert(
            "",
            "end",
            iid=_LOAD_ORDER_IID,
            text=_("(the load order)"),
            values=("", len(own)),
            tags=(own[0].severity,) if own else (),
        )
        for i, rec in enumerate(result.scripts):
            iid = f"s{i}"
            self._lua_records[iid] = rec
            found = self._lua_findings_for(rec.path)
            tags: tuple[str, ...] = (found[0].severity,) if found else ()
            if rec.file is None:
                tags = (*tags, "missing")
            nav.insert(
                "",
                "end",
                iid=iid,
                text=rec.path,
                values=(", ".join(rec.flags), len(found) or ""),
                tags=tags,
            )
        nav.selection_set(_LOAD_ORDER_IID)

    # -- the selected script ----------------------------------------------------------

    def _lua_select(self) -> None:
        """Show the selected script's source, tree and findings."""
        sel = self._lua_nav.selection()
        if not sel:
            return
        rec = self._lua_records.get(sel[0])
        path = rec.path if rec else ""
        self._lua_fill_findings(path)
        self._lua_fill_source(rec)
        self._lua_fill_ast(rec)

    def _lua_fill_findings(self, path: str) -> None:
        """List a script's findings.

        Args:
            path: The script ("" for the load order).
        """
        found = self._lua_found
        found.delete(*found.get_children())
        for f in self._lua_findings_for(path):
            found.insert(
                "",
                "end",
                values=(f.severity, f.code, f.line or "", f.message),
                tags=(f.severity,),
            )

    def _lua_fill_source(self, rec: ScriptRecord | None) -> None:
        """Show a script's source, highlighted.

        Args:
            rec: The script, or None for the load order (then the full report).
        """
        src = self._lua_src
        src.configure(state="normal")
        src.delete("1.0", "end")
        if rec is None or rec.file is None:
            self._lua_fill_teal(None)
        if rec is None:
            if self._lua_result is not None:
                src.insert("1.0", render(self._lua_result, info=self._lua_show_info.get()))
            src.configure(state="disabled")
            return
        path = rec.file
        if path is None:
            src.insert("1.0", _("No data folder has this script."))
            src.configure(state="disabled")
            return
        try:
            text = path.read_text(encoding="utf-8", errors="replace")
        except OSError as exc:
            src.insert("1.0", str(exc))
            src.configure(state="disabled")
            return
        src.insert("1.0", text)
        if len(text) <= _HIGHLIGHT_LIMIT:
            self._lua_highlight(src, text)
        src.configure(state="disabled")
        self._lua_fill_teal(path)

    def _lua_fill_teal(self, path: Path | None) -> None:
        """Show the Teal source beside a script (``x.tl`` next to ``x.lua``), if any.

        Args:
            path: The script's file, or None.
        """
        view = self._lua_teal
        view.configure(state="normal")
        view.delete("1.0", "end")
        teal = path.with_suffix(".tl") if path is not None else None
        if teal is None or not teal.is_file():
            view.insert("1.0", _("No Teal source (.tl) beside this script."))
        else:
            try:
                text = teal.read_text(encoding="utf-8", errors="replace")
            except OSError as exc:
                text = str(exc)
            view.insert("1.0", text)
            if len(text) <= _HIGHLIGHT_LIMIT:
                self._lua_highlight(view, text, teal=True)
        view.configure(state="disabled")

    def _lua_style_source(self, src: tk.Text) -> None:
        """Set a code view's highlight colours from the log panel's syntax theme.

        Args:
            src: The text widget.
        """
        theme_name = self.log_theme_var.get() if hasattr(self, "log_theme_var") else ""
        theme = self._resolve_theme(theme_name) or THEME_PRESETS["Dark (default)"]
        colors = _json_syntax_colors(theme)
        src.tag_configure("lua_keyword", foreground=colors["keyword"])
        src.tag_configure("lua_string", foreground=colors["string"])
        src.tag_configure("lua_number", foreground=colors["number"])
        src.tag_configure("lua_comment", foreground=colors["punct"])
        src.tag_configure("lua_op", foreground=colors["punct"])
        src.tag_configure("lua_goto", background=DARK["select"])

    def _lua_highlight(self, src: tk.Text, text: str, teal: bool = False) -> None:
        """Colour a code view's tokens.

        Args:
            src: The text widget.
            text: The source (already inserted).
            teal: It is Teal.
        """
        try:
            tokens = tokenize(text, comments=True, teal=teal)
        except LuaSyntaxError:
            return  # shown plain; the findings say where the syntax error is
        for tok in tokens:
            tag = _TOKEN_TAGS.get(tok.kind)
            if tag:
                start, end = _span(tok.line, tok.col, tok.text)
                src.tag_add(tag, start, end)

    def _lua_goto(self, line: int) -> None:
        """Show the source at a line, marked.

        Args:
            line: The 1-based line (0 does nothing).
        """
        if line <= 0:
            return
        src = self._lua_src
        src.tag_remove("lua_goto", "1.0", "end")
        src.tag_add("lua_goto", f"{line}.0", f"{line}.end")
        src.see(f"{line}.0")
        self._lua_book.select(self._lua_src_frame)

    def _lua_goto_finding(self) -> None:
        """Go to the line of the double-clicked finding."""
        sel = self._lua_found.selection()
        if sel:
            line = self._lua_found.item(sel[0], "values")[2]
            self._lua_goto(int(line) if str(line).isdigit() else 0)

    # -- the syntax tree -----------------------------------------------------------------

    def _lua_fill_ast(self, rec: ScriptRecord | None) -> None:
        """Show a script's syntax tree, top level open, the rest on demand.

        Args:
            rec: The script, or None.
        """
        ast = self._lua_ast
        ast.delete(*ast.get_children())
        self._lua_ast_nodes = {}
        tree = rec.info.tree if rec is not None and rec.info is not None else None
        if tree is None:
            return
        self._lua_ast_add("", tree)

    def _lua_ast_add(self, parent: str, node: Node) -> None:
        """Add one node, with a placeholder child when it has children.

        Args:
            parent: The parent item.
            node: The node.
        """
        iid = self._lua_ast.insert(parent, "end", text=node.label(), values=(node.line,))
        self._lua_ast_nodes[iid] = node
        if node.children:
            self._lua_ast.insert(iid, "end", text="...")

    def _lua_expand_ast(self) -> None:
        """Fill an opened node's children in (once)."""
        ast = self._lua_ast
        iid = ast.focus()
        node = self._lua_ast_nodes.get(iid)
        kids = ast.get_children(iid)
        if node is None or not kids or ast.item(kids[0], "text") != "...":
            return
        ast.delete(*kids)
        for child in node.children:
            self._lua_ast_add(iid, child)

    def _lua_ast_line(self) -> int:
        """The line of the selected tree node.

        Returns:
            Its line, or 0.
        """
        node = self._lua_ast_nodes.get(self._lua_ast.focus())
        return node.line if node is not None else 0

    def _lua_flowchart(self) -> None:
        """Open the selected function's (or the script's) flowchart in the browser."""
        sel = self._lua_nav.selection()
        rec = self._lua_records.get(sel[0]) if sel else None
        tree = rec.info.tree if rec is not None and rec.info is not None else None
        if rec is None or tree is None:
            self._lua_status.set(_("Select a script that parses first."))
            return
        node = self._lua_ast_nodes.get(self._lua_ast.focus())
        if node is None or node.kind not in _FUNCTION_KINDS:
            node = tree  # no function selected: the whole script
        target = node
        name = target.value or (_("function at line ") + str(target.line))
        heading = rec.path if target is tree else f"{rec.path} - {name}"
        page = flowchart_html(heading, [(heading, flowchart(target))])
        title = _("Lua flowchart")
        # In the app's own HTML window over the shared loopback server, as every other
        # page the toolkit shows; a temporary file when no port can be bound.
        server_of = getattr(self, "_viewer_server", None)
        server = server_of() if callable(server_of) else None
        opener = getattr(self, "open_html_in_app", None)
        if server is not None and callable(opener):
            session = server.publish_session("luaflow")
            url = session.publish("index.html", Payload(page.encode("utf-8"), "text/html"))
            opener(url, title)
            return
        fallback = getattr(self, "_open_html_view", None)
        if callable(fallback):
            fallback(page, "lua_flowchart", title)
            return
        out = Path(tempfile.gettempdir()) / "wraithguard_lua_flowchart.html"
        out.write_text(page, encoding="utf-8")
        open_in_browser(out.as_uri())

    def _lua_copy_report(self) -> None:
        """Put the text report on the clipboard."""
        if self._lua_result is None:
            return
        text = render(self._lua_result, info=self._lua_show_info.get())
        self.root.clipboard_clear()
        self.root.clipboard_append(text)
        self._lua_status.set(_("Report copied."))


__all__ = ["LuaViewMixin"]
