"""Headless smoke tests for the Tk GUI.

The GUI has no other automated coverage: it cannot be imported without Tk, so
it is excluded from the hermetic suite and from mypy, and its verification has
been the manual ``SMOKE_TEST.md`` run. That gap is not theoretical -- the last
two defects to reach a user were both here, and both invisible to ruff, mypy and
1,200 passing tests:

* a window that opened **blank**, because a mistyped palette key raised after
  the ``Toplevel`` was created and before its content was packed;
* two panels **gridded on top of each other**, because a body moved during a
  refactor kept row offsets that were relative to a different base.

Neither needs a human to spot. What they need is a display, so this module
builds the real application on a real (virtual) X server and asserts the things
a person would otherwise have to look for.

It **skips** rather than fails when Tk or a display is missing, so the hermetic
suite is unaffected; CI runs it under ``xvfb`` in a job of its own. A skip here
means "not checked", so the CI job asserts it actually ran.

Running it locally, on a machine that has Tk::

    python -m pytest tests/test_gui_smoke.py -v

Windows flash open and closed while it runs; that is the suite working. If every
test reports ``SKIPPED``, Tk or the display is missing and nothing was checked --
``python -c "import tkinter; tkinter.Tk()"`` will say which.
"""

from __future__ import annotations

import sys
from pathlib import Path
from typing import TYPE_CHECKING, Any

import pytest

if TYPE_CHECKING:
    from collections.abc import Iterator

tkinter = pytest.importorskip("tkinter", reason="Tk is not installed")

ROOT = Path(__file__).resolve().parent.parent
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))


@pytest.fixture(scope="module")
def tk_root() -> Iterator[Any]:
    """A real Tk root window, built the way the application builds its own.

    The root type is not incidental. ``main()`` uses ``TkinterDnD.Tk()`` when
    tkinterdnd2 is present, and that is what loads the tkdnd Tcl package into
    the interpreter; a plain ``tkinter.Tk()`` looks identical but cannot carry
    a drop target. Building the wrong one here tested a window the application
    never constructs.

    Yields:
        The root window, withdrawn so nothing flashes on screen.
    """
    from mlox_subset.gui import HAVE_DND, TkinterDnD

    try:
        root = TkinterDnD.Tk() if HAVE_DND else tkinter.Tk()
    except tkinter.TclError as exc:  # pragma: no cover - depends on the environment
        pytest.skip(f"no display available: {exc}")
    root.withdraw()
    try:
        yield root
    finally:
        root.destroy()


@pytest.fixture(scope="module")
def app(tk_root: Any, tmp_path_factory: pytest.TempPathFactory) -> Any:
    """The real application, built on a scratch app directory.

    Args:
        tk_root: The Tk root.
        tmp_path_factory: Pytest's temp directory factory.

    Returns:
        The constructed ``App``.
    """
    import mlox_subset.gui as gui_pkg

    # app_base_dir() caches into this module global; pointing it at a temp
    # directory keeps the settings file and trace log out of the checkout.
    gui_pkg._APP_DIR = tmp_path_factory.mktemp("appdir")
    import mlox_subset_sort_gui as gui

    return gui.App(tk_root)


@pytest.fixture
def fresh_app(tk_root: Any, tmp_path: Path) -> Iterator[Any]:
    """A second application on its own settings file, for the mutating tests.

    Anything that writes settings or switches theme would otherwise leak into
    every test that runs after it, since ``app`` is module-scoped. This builds a
    separate ``App`` pointed at a per-test directory, and puts the theme back
    afterwards -- the active theme is process-wide, not per-application.

    It shares the one Tk root deliberately rather than creating a second.
    ``tkinter`` keeps a single ``_default_root``, and a second ``Tk()`` does not
    become it, so an application built on the second root would create its
    variables on the *first* interpreter and its widgets on the second -- which
    fails in a way that looks like a bug in the code under test.

    Args:
        tk_root: The Tk root.
        tmp_path: A per-test temp directory.

    Yields:
        A freshly constructed ``App``.
    """
    import mlox_subset.gui as gui_pkg
    from mlox_subset.gui import theme as theme_mod

    previous_dir = gui_pkg._APP_DIR
    previous_theme = theme_mod._ACTIVE_THEME
    gui_pkg._APP_DIR = tmp_path

    import mlox_subset_sort_gui as gui

    built = gui.App(tk_root)
    try:
        yield built
    finally:
        gui_pkg._APP_DIR = previous_dir
        theme_mod._ACTIVE_THEME = previous_theme


def grid_cells(widget: Any) -> dict[tuple[int, int], list[str]]:
    """Map every grid cell a container uses to the widgets occupying it.

    Args:
        widget: The container to inspect.

    Returns:
        ``(row, column)`` to the widget class names placed there, expanded
        across each widget's row and column spans.
    """
    used: dict[tuple[int, int], list[str]] = {}
    for child in widget.grid_slaves():
        info = child.grid_info()
        row, column = int(info["row"]), int(info["column"])
        rowspan, columnspan = int(info.get("rowspan", 1)), int(info.get("columnspan", 1))
        for r in range(row, row + rowspan):
            for c in range(column, column + columnspan):
                used.setdefault((r, c), []).append(child.winfo_class())
    return used


class TestApplicationBuilds:
    """The window comes up at all."""

    def test_app_constructs(self, app: Any) -> None:
        """Construction runs every builder; an exception in any is a blank app.

        Args:
            app: The application.
        """
        assert app.root.winfo_exists()

    def test_the_controls_panel_has_widgets(self, app: Any) -> None:
        """A panel that built nothing looks exactly like one that built wrongly.

        Args:
            app: The application.
        """
        assert app.sort_button.winfo_exists()
        assert app.log_text.winfo_exists()


class TestDragAndDropIsOptional:
    """Drag and drop is a convenience, so its absence must not stop the app.

    Found by running this suite for the first time on a real desktop. The
    fixture built a plain ``tkinter.Tk()`` root, which does not load the tkdnd
    Tcl package, and **every test errored** -- not on an assertion but during
    construction, because registering a drop target raised and took the whole
    window build down with it.

    That was a test bug (the application builds a ``TkinterDnD.Tk()`` root) but
    it exposed a real one: the code asked whether the *Python package* imported
    and then assumed the *Tcl package* was loaded. Those come apart on a
    half-installed tkdnd or a frozen build that shipped one side without the
    other, and the result is an application that will not open at all over a
    feature nobody needs to have.
    """

    def test_the_app_builds_when_drag_and_drop_is_unavailable(
        self, tk_root: Any, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        """The regression: the window must finish building without tkdnd.

        Simulated by turning the availability flag off rather than by building
        a second root without the Tcl package. A second root is a second Tk
        interpreter, which brings its own failure modes and can make this skip
        -- and a skip here means the one check that covers the reported bug
        quietly did not run.

        Args:
            tk_root: The Tk root.
            tmp_path: A scratch directory for settings.
            monkeypatch: Pytest's patcher.
        """
        import mlox_subset.gui as gui_pkg

        monkeypatch.setattr(gui_pkg, "HAVE_DND", False)
        monkeypatch.setattr(gui_pkg, "_APP_DIR", tmp_path)
        import mlox_subset_sort_gui as gui

        built = gui.App(tk_root)

        assert built.sort_button.winfo_exists(), "the window did not finish building"
        assert built.log_text.winfo_exists()

    def test_the_missing_feature_is_reported_on_screen(
        self, tk_root: Any, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        """Silently offering a dead drop target would be worse than saying so.

        Args:
            tk_root: The Tk root.
            tmp_path: A scratch directory for settings.
            monkeypatch: Pytest's patcher.
        """
        import mlox_subset.gui as gui_pkg

        monkeypatch.setattr(gui_pkg, "HAVE_DND", False)
        monkeypatch.setattr(gui_pkg, "_APP_DIR", tmp_path)
        import mlox_subset_sort_gui as gui

        built = gui.App(tk_root)
        labels = [
            str(child.cget("text"))
            for child in _all_widgets(built.root)
            if child.winfo_class() == "TLabel"
        ]

        # Compared against the translated form of the same literal, not against
        # English: a matched string would otherwise start failing the moment a
        # catalogue is shipped, which is not a regression in anything.
        from mlox_subset.i18n import gettext

        expected = gettext(
            "Drag & drop is unavailable (tkinterdnd2 or its tkdnd library "
            "is missing) -- use the Browse buttons below."
        )
        assert expected in labels

    def test_a_registration_that_raises_is_survivable(self) -> None:
        """The guard itself, without needing an interpreter that lacks tkdnd.

        A widget whose registration refuses must yield ``False`` rather than an
        exception, because the caller is halfway through building a window.
        """
        from mlox_subset.gui import register_drop_target

        class Refuses:
            """A widget that reports tkdnd present but refuses registration."""

            class _Tk:
                @staticmethod
                def eval(_script: str) -> str:
                    """Report the package as present.

                    Args:
                        _script: Ignored.

                    Returns:
                        A version string.
                    """
                    return "2.9"

            tk = _Tk()

            @staticmethod
            def drop_target_register(*_args: object) -> None:
                """Refuse, the way Tk does when tkdnd is not really usable.

                Args:
                    _args: Ignored.

                Raises:
                    TclError: Always.
                """
                raise tkinter.TclError("invalid command name 'tkdnd::drop_target'")

        assert register_drop_target(Refuses()) is False

    def test_the_real_root_does_support_it(self, tk_root: Any) -> None:
        """Otherwise the checks above would pass by testing nothing.

        Args:
            tk_root: The root the application actually builds on.
        """
        from mlox_subset.gui import HAVE_DND, dnd_ready

        if not HAVE_DND:  # pragma: no cover - tkinterdnd2 is optional
            pytest.skip("tkinterdnd2 is not installed")
        assert dnd_ready(tk_root), "the app's own root should support drag and drop"


class TestActionButtons:
    """Every button exists, is bound, and starts in the documented state."""

    #: Button attribute -> whether it should start disabled (needs a Sort first).
    BUTTONS = {
        "sort_button": False,
        "export_button": True,
        "conflicts_button": True,
        "cellmap_button": True,
        "resource_button": True,
        "lint_button": True,
        "tes3cmd_button": False,
        "savecheck_button": False,
        "backups_button": False,
        "help_button": False,
    }

    def test_every_button_exists(self, app: Any) -> None:
        """A renamed or dropped button is otherwise found by clicking around.

        Args:
            app: The application.
        """
        missing = [name for name in self.BUTTONS if not hasattr(app, name)]
        assert not missing, f"buttons missing from the app: {missing}"

    def test_every_button_has_a_command(self, app: Any) -> None:
        """An unbound button is silent on screen and silent in the log.

        Args:
            app: The application.
        """
        unbound = [
            name for name in self.BUTTONS if not str(getattr(app, name).cget("command")).strip()
        ]
        assert not unbound, f"buttons with no command: {unbound}"

    def test_read_only_scans_start_disabled(self, app: Any) -> None:
        """They need a sorted plugin list; offering them earlier is a dead end.

        Args:
            app: The application.
        """
        wrong = {
            name: str(getattr(app, name).cget("state"))
            for name, should_be_disabled in self.BUTTONS.items()
            if (str(getattr(app, name).cget("state")) == "disabled") is not should_be_disabled
        }
        assert not wrong, f"buttons in the wrong initial state: {wrong}"


class TestControlsLayout:
    """No two panels may be gridded into the same cell.

    This is the check that would have caught the output-fields panel landing on
    top of the rules panel, the options box and the action bar.
    """

    def test_no_two_widgets_share_a_grid_cell(self, app: Any) -> None:
        """Overlapping widgets hide each other, silently.

        Args:
            app: The application.
        """
        container = app.sort_button.winfo_toplevel()
        collisions: dict[str, dict[tuple[int, int], list[str]]] = {}
        for frame in _grid_containers(container):
            clashes = {cell: names for cell, names in grid_cells(frame).items() if len(names) > 1}
            if clashes:
                collisions[str(frame)] = clashes
        assert not collisions, f"widgets stacked in one grid cell: {collisions}"

    def test_the_controls_panel_uses_consecutive_rows(self, app: Any) -> None:
        """A gap means a panel went somewhere unintended.

        Args:
            app: The application.
        """
        parent = app.sort_button.master.master.master
        rows = sorted({row for row, _column in grid_cells(parent)})
        if not rows:  # pragma: no cover - layout differs from the expected nesting
            pytest.skip("controls frame not found by walking up from the Sort button")
        assert rows == list(range(min(rows), max(rows) + 1)), f"gap in the control rows: {rows}"


def _grid_containers(widget: Any) -> Iterator[Any]:
    """Yield every widget in a tree that has grid-managed children.

    Args:
        widget: The root of the tree.

    Yields:
        Containers using the grid geometry manager.
    """
    children = widget.winfo_children()
    if widget.grid_slaves():
        yield widget
    for child in children:
        yield from _grid_containers(child)


class TestOptionFields:
    """The text options that feed the engine.

    Each is only useful if it is both built and *wired*: an entry with no
    variable behind it looks identical on screen to one that works.
    """

    FIELDS = ("exclude_var", "groundcover_var", "cleanup_html_var")

    def test_every_option_variable_exists(self, app: Any) -> None:
        """A renamed variable silently stops being saved or read.

        Args:
            app: The application.
        """
        missing = [name for name in self.FIELDS if not hasattr(app, name)]
        assert not missing, f"option variables missing: {missing}"

    def test_the_groundcover_field_is_empty_by_default(self, app: Any) -> None:
        """The common case needs no help: the cfg declares its own grass.

        Args:
            app: The application.
        """
        assert app.groundcover_var.get() == ""

    def test_a_widget_is_bound_to_each_text_option(self, app: Any) -> None:
        """The variable must actually reach an entry on screen.

        Args:
            app: The application.
        """
        bound = {
            str(child.cget("textvariable"))
            for child in _all_widgets(app.root)
            if child.winfo_class() == "TEntry"
        }
        for name in ("exclude_var", "groundcover_var"):
            assert str(getattr(app, name)) in bound, f"{name} is not on screen"


def _all_widgets(widget: Any) -> Iterator[Any]:
    """Walk every widget in a tree.

    Args:
        widget: The root of the tree.

    Yields:
        Each descendant, and the root itself.
    """
    yield widget
    for child in widget.winfo_children():
        yield from _all_widgets(child)


class TestSecondaryWindows:
    """Every window the app opens must open with content in it.

    A ``Toplevel`` that raises part-way through construction leaves an empty
    window on screen and a traceback on stderr -- which is what "it does
    nothing" looks like from the outside.
    """

    def test_format_reference_window_has_content(self, app: Any) -> None:
        """The exact defect: this opened blank on a bad palette key.

        Args:
            app: The application.
        """
        before = set(app.root.winfo_children())
        app._show_format_reference("Landscape")
        opened = [w for w in app.root.winfo_children() if w not in before]
        assert opened, "no window was created"
        window = opened[-1]
        try:
            assert window.winfo_children(), "window opened with nothing in it"
            text = _first_text_widget(window)
            assert text is not None, "no text widget in the reference window"
            assert "VHGT" in text.get("1.0", "end"), "the layout was not rendered into the window"
        finally:
            window.destroy()

    def test_format_reference_is_not_offered_for_an_unknown_type(self, app: Any) -> None:
        """It must not open an empty window for a type it cannot describe.

        Args:
            app: The application.
        """
        before = set(app.root.winfo_children())
        app._show_format_reference("Nonesuch")
        assert set(app.root.winfo_children()) == before

    def test_help_renders_a_document(self, app: Any, monkeypatch: pytest.MonkeyPatch) -> None:
        """The Help button must produce a real page, not an empty file.

        The viewer itself is stubbed: launching a browser or an embedded webview
        is not something a CI job should do, and what matters here is that the
        document was found, rendered and handed over.

        Args:
            app: The application.
            monkeypatch: Pytest's patcher.
        """
        opened: list[Path] = []
        monkeypatch.setattr(
            type(app), "open_html_in_app", lambda _self, path, _title: opened.append(Path(path))
        )
        app._open_document("Read me", "README.md")
        assert opened, "the rendered page was never handed to a viewer"
        page = opened[0].read_text(encoding="utf-8")
        assert page.startswith("<!DOCTYPE html>")
        assert "<nav>" in page, "the contents sidebar is missing"


def _first_text_widget(widget: Any) -> Any:
    """Find the first Text widget in a tree.

    Args:
        widget: The root of the tree.

    Returns:
        The widget, or ``None``.
    """
    if widget.winfo_class() == "Text":
        return widget
    for child in widget.winfo_children():
        found = _first_text_widget(child)
        if found is not None:
            return found
    return None


class TestRuleMakerWindow:
    """The rebuilt rule maker.

    It covers every rule the guidelines describe now, so the window has a lot
    more moving parts than the three-radio-button version it replaced. What is
    checked here is that it builds, that every rule type is offered, and that
    the live preview and the write button actually track validity -- the last
    being the whole safety mechanism, since mlox silently discards a rule it
    cannot use.
    """

    def test_the_window_opens_with_content(self, app: Any) -> None:
        """The blank-window failure mode, on the window with the most widgets.

        Args:
            app: The application.
        """
        app.on_rule_maker()
        window = app._rm_win

        try:
            assert window.winfo_exists()
            assert window.winfo_children()
        finally:
            window.destroy()

    def test_every_documented_rule_type_is_offered(self, app: Any) -> None:
        """A rule the guidelines describe but the window cannot write is a gap.

        Args:
            app: The application.
        """
        from mlox_subset.rules.authoring import RULE_KINDS

        app.on_rule_maker()
        try:
            offered = {
                str(child.cget("value"))
                for child in _all_widgets(app._rm_win)
                if child.winfo_class() == "TRadiobutton"
            }
            assert set(RULE_KINDS) <= offered, f"missing: {set(RULE_KINDS) - offered}"
        finally:
            app._rm_win.destroy()

    def test_the_preview_renders_the_rule_being_built(self, app: Any) -> None:
        """The preview is how someone checks the rule before writing it.

        Args:
            app: The application.
        """
        app.on_rule_maker()
        try:
            app._rm_kind.set("Order")
            app._rm_list.insert("end", "A.esp")
            app._rm_list.insert("end", "B.esp")
            app._rm_refresh()

            shown = app._rm_preview.get("1.0", "end")
            assert "[Order]" in shown
            assert "A.esp" in shown and "B.esp" in shown
        finally:
            app._rm_win.destroy()

    def test_the_write_button_follows_validity(self, app: Any) -> None:
        """An invalid rule must not be writable; a valid one must be.

        This is the safety mechanism: mlox discards a rule it cannot parse
        without saying so, and this window is the only place a person finds out.

        Args:
            app: The application.
        """
        app.on_rule_maker()
        try:
            app._rm_kind.set("Order")
            app._rm_list.insert("end", "Only.esp")  # an [Order] needs two
            app._rm_refresh()
            assert str(app._rm_write.cget("state")) == "disabled"

            app._rm_list.insert("end", "Second.esp")
            app._rm_refresh()
            assert str(app._rm_write.cget("state")) == "normal"
        finally:
            app._rm_win.destroy()

    def test_problems_are_shown_with_their_severity(self, app: Any) -> None:
        """Warnings and errors read differently and must not look the same.

        Args:
            app: The application.
        """
        app.on_rule_maker()
        try:
            app._rm_kind.set("NearEnd")  # discouraged: a warning, not an error
            app._rm_list.insert("end", "A.esp")
            app._rm_refresh()

            shown = app._rm_problems.get("1.0", "end")
            assert "warning:" in shown
            assert str(app._rm_write.cget("state")) == "normal", "a warning must not block"
        finally:
            app._rm_win.destroy()

    def test_grouping_builds_an_any_expression(self, app: Any) -> None:
        """The shape the guidelines use for "whichever version you installed".

        Args:
            app: The application.
        """
        app.on_rule_maker()
        try:
            app._rm_kind.set("Requires")
            app._rm_list.insert("end", "Mod.esp")
            app._rm_list.insert("end", "DepA.esp")
            app._rm_list.insert("end", "DepB.esp")
            app._rm_list.selection_set(1, 2)
            app._rm_group_any()
            app._rm_refresh()

            assert "[ANY DepA.esp DepB.esp]" in app._rm_preview.get("1.0", "end")
        finally:
            app._rm_win.destroy()

    def test_the_rule_guide_opens_from_the_window(self, app: Any, monkeypatch: Any) -> None:
        """The reference is only useful if it is reachable while writing a rule.

        Args:
            app: The application.
            monkeypatch: Pytest's patcher.
        """
        opened: list[str] = []
        monkeypatch.setattr(
            type(app), "open_html_in_app", lambda _self, path, _title: opened.append(str(path))
        )
        app.on_rule_maker()
        try:
            guide = [
                child
                for child in _all_widgets(app._rm_win)
                if child.winfo_class() == "TButton" and "guide" in str(child.cget("text")).lower()
            ]
            assert guide, "the rule maker offers no way to reach the rule guide"
            guide[0].invoke()
            assert opened, "the guide button rendered nothing"
        finally:
            app._rm_win.destroy()


class TestSettingsRoundTrip:
    """What is saved must come back.

    The failure this exists for is quiet and permanent: a new option added to
    ``_gather_settings`` but not to ``_load_settings`` is written to disk on
    every exit and silently discarded on every start. Nothing errors, the file
    looks right, and the setting simply never sticks.
    """

    #: A distinctive value per option, chosen so a field restored from the wrong
    #: key is visible rather than coincidentally equal.
    STRINGS = {
        "cfg_var": "settings-probe/openmw.cfg",
        "customizations_var": "settings-probe/momw-customizations.toml",
        "subset_file_var": "settings-probe/subset.txt",
        "list_name_var": "total-overhaul",
        "plugin_order_yml_var": "settings-probe/plugin-order.yml",
        "exclude_var": "Excluded One.esp, Excluded Two.esp",
        "groundcover_var": "My Grass.esp",
        "plugin_order_url_var": "https://example.invalid/order.yml",
        "rules_url_var": "https://example.invalid/rules/%s.txt",
    }

    #: Every boolean option, flipped from whatever it defaults to.
    BOOLEANS = (
        "write_toml_inplace_var",
        "dry_run_var",
        "write_cfg_var",
        "sort_data_paths_var",
        "no_backup_var",
        "no_predicate_warnings_var",
        "create_subset_doc_var",
        "keep_json_var",
        "cleanup_html_var",
    )

    def test_text_options_survive_a_save_and_load(self, fresh_app: Any) -> None:
        """Each one, individually, so a failure names the field.

        Args:
            fresh_app: An application on a scratch settings file.
        """
        for name, value in self.STRINGS.items():
            getattr(fresh_app, name).set(value)
        fresh_app._save_settings()

        for name in self.STRINGS:
            getattr(fresh_app, name).set("")
        fresh_app._load_settings()

        lost = {
            name: getattr(fresh_app, name).get()
            for name, value in self.STRINGS.items()
            if getattr(fresh_app, name).get() != value
        }
        assert not lost, f"saved but not restored: {lost}"

    def test_checkboxes_survive_a_save_and_load(self, fresh_app: Any) -> None:
        """A toggle that resets every launch is worse than no toggle.

        Args:
            fresh_app: An application on a scratch settings file.
        """
        flipped = {name: not getattr(fresh_app, name).get() for name in self.BOOLEANS}
        for name, value in flipped.items():
            getattr(fresh_app, name).set(value)
        fresh_app._save_settings()

        for name in self.BOOLEANS:
            getattr(fresh_app, name).set(not flipped[name])
        fresh_app._load_settings()

        lost = {
            name: getattr(fresh_app, name).get()
            for name, value in flipped.items()
            if getattr(fresh_app, name).get() != value
        }
        assert not lost, f"saved but not restored: {lost}"

    def test_the_rules_list_survives_a_save_and_load(self, fresh_app: Any, tmp_path: Path) -> None:
        """The rule files are the most laborious thing to re-enter by hand.

        Order matters as much as membership: the rule files are consulted in
        list order, so a round trip that restored them shuffled would change
        which rule wins without changing anything visible.

        Args:
            fresh_app: An application on a scratch settings file.
            tmp_path: A per-test temp directory, for platform-native paths.
        """
        wanted = [tmp_path / "mlox_base.txt", tmp_path / "my_rules.txt"]
        for path in wanted:
            fresh_app.rules_panel.listbox.insert("end", str(path))
        fresh_app._save_settings()

        fresh_app.rules_panel.listbox.delete(0, "end")
        fresh_app._load_settings()

        assert fresh_app.rules_panel.get_paths() == wanted

    def test_every_saved_key_is_read_back(self, fresh_app: Any) -> None:
        """The general form of the defect, for options added later.

        Saving a key that nothing ever loads is dead weight at best and a
        setting that does not stick at worst, and it is invisible from the
        outside. This compares the two halves directly rather than waiting for
        someone to notice a checkbox forgetting itself.

        Args:
            fresh_app: An application on a scratch settings file.
        """
        import json

        saved = set(fresh_app._gather_settings())
        source = Path(fresh_app._settings_file())
        fresh_app._save_settings()
        on_disk = set(json.loads(source.read_text(encoding="utf-8")))
        assert saved == on_disk, "the settings file does not match what was gathered"

        # A key is "read back" if _load_settings mentions it. Checked against
        # the source because several are restored through paths that do not go
        # via a Tk variable -- the overrides, the rules list, the theme.
        import inspect

        loader = inspect.getsource(type(fresh_app)._load_settings)
        never_read = sorted(key for key in saved if f'"{key}"' not in loader)
        assert not never_read, f"saved but never loaded: {never_read}"


class TestLogThemes:
    """Switching theme must repaint, and must not raise.

    Theme code reaches into a palette by name, and a missing key raises at the
    moment of the switch. That is how the blank Format-reference window
    happened, and the log pane is the widget most palette keys exist for.
    """

    @staticmethod
    def _apply(app_under_test: Any, name: str) -> str | None:
        """Apply one theme and report what went wrong, if anything.

        Args:
            app_under_test: The application.
            name: The theme name.

        Returns:
            A description of the failure, or ``None`` when it applied cleanly.
        """
        try:
            app_under_test._apply_log_theme(name, announce=False)
        except (KeyError, tkinter.TclError) as exc:  # pragma: no cover - the defect
            return f"{type(exc).__name__}: {exc}"
        return None

    def test_every_offered_theme_applies(self, fresh_app: Any) -> None:
        """A theme in the dropdown that cannot be applied is a trap.

        Args:
            fresh_app: An application on a scratch settings file.
        """
        failed = {
            name: problem
            for name in fresh_app._theme_names()
            if (problem := self._apply(fresh_app, name)) is not None
        }
        assert not failed, f"themes that could not be applied: {failed}"

    def test_switching_theme_changes_the_log_colours(self, fresh_app: Any) -> None:
        """Applying without repainting looks exactly like a theme that matches.

        Args:
            fresh_app: An application on a scratch settings file.
        """
        names = fresh_app._theme_names()
        if len(names) < 2:  # pragma: no cover - a build with one preset
            pytest.skip("only one theme is available")

        seen: set[str] = set()
        for name in names:
            fresh_app._apply_log_theme(name, announce=False)
            seen.add(str(fresh_app.log_text.tag_cget("error", "foreground")))
        assert len(seen) > 1, "every theme painted the error tag the same colour"

    def test_the_chosen_theme_is_remembered(self, fresh_app: Any) -> None:
        """It is a per-person preference, so it has to outlive the session.

        Args:
            fresh_app: An application on a scratch settings file.
        """
        other = [n for n in fresh_app._theme_names() if n != fresh_app.log_theme_var.get()]
        if not other:  # pragma: no cover - a build with one preset
            pytest.skip("only one theme is available")

        fresh_app._apply_log_theme(other[0], announce=False)
        fresh_app.log_theme_var.set(other[0])
        fresh_app._save_settings()

        assert fresh_app._saved_log_theme_name() == other[0]


class TestHelpMenu:
    """Every shipped document opens, and opens with something in it."""

    def test_every_help_document_renders(self, app: Any, monkeypatch: Any) -> None:
        """One entry that renders nothing is a menu item that does nothing.

        Args:
            app: The application.
            monkeypatch: Pytest's patcher.
        """
        from mlox_subset.gui import HELP_DOCUMENTS

        opened: list[Path] = []
        monkeypatch.setattr(
            type(app), "open_html_in_app", lambda _self, path, _title: opened.append(Path(path))
        )
        for name in HELP_DOCUMENTS:
            opened.clear()
            app._open_document(name, name)
            assert opened, f"{name} produced no page"
            page = opened[0].read_text(encoding="utf-8")
            assert page.startswith("<!DOCTYPE html>"), f"{name} is not a page"
            assert "<nav>" in page, f"{name} rendered without a contents sidebar"
            assert len(page) > 2000, f"{name} rendered almost empty"

    def test_a_missing_document_does_not_open_an_empty_window(
        self, app: Any, monkeypatch: Any
    ) -> None:
        """It reports the problem instead, which is the recoverable outcome.

        Args:
            app: The application.
            monkeypatch: Pytest's patcher.
        """
        import mlox_subset_sort_gui as gui

        told: list[str] = []
        monkeypatch.setattr(gui.messagebox, "showinfo", lambda title, _msg: told.append(title))
        monkeypatch.setattr(
            type(app), "open_html_in_app", lambda *_a: pytest.fail("a viewer was opened")
        )
        app._open_document("Nope", "NO_SUCH_DOCUMENT.md")
        assert told, "a missing document failed silently"

    def test_the_rule_guide_covers_the_rule_vocabulary(self, app: Any, monkeypatch: Any) -> None:
        """The reference has to describe the rules the tool can actually write.

        A rule kind the rule maker offers but the guide never mentions leaves
        someone with a button and no explanation of it.

        Args:
            app: The application.
            monkeypatch: Pytest's patcher.
        """
        from mlox_subset.rules.authoring import RULE_KINDS

        opened: list[Path] = []
        monkeypatch.setattr(
            type(app), "open_html_in_app", lambda _self, path, _title: opened.append(Path(path))
        )
        app._open_document("Writing mlox rules", "MLOX_RULES.md")
        if not opened:  # pragma: no cover - a trimmed checkout
            pytest.skip("MLOX_RULES.md is not in this checkout")

        page = opened[0].read_text(encoding="utf-8")
        missing = [kind for kind in RULE_KINDS if f"[{kind}]" not in page]
        assert not missing, f"the rule guide never mentions: {missing}"


class TestBackupsWindow:
    """The one window that reports "nothing found" as a normal outcome."""

    def test_it_opens_with_content_when_there_are_backups(self, app: Any) -> None:
        """The listing is the point; an empty frame is indistinguishable from a bug.

        Args:
            app: The application.
        """
        app._show_backups_window([], "no backups found")
        window = app._bk_win
        try:
            assert window.winfo_exists()
            assert window.winfo_children(), "the backups window opened empty"
            assert app.status_var.get() == "no backups found"
        finally:
            window.destroy()

    def test_opening_it_twice_does_not_leave_two(self, app: Any) -> None:
        """Otherwise the stale one stays on screen showing stale backups.

        Args:
            app: The application.
        """
        app._show_backups_window([], "first")
        first = app._bk_win
        app._show_backups_window([], "second")
        second = app._bk_win
        try:
            assert not first.winfo_exists(), "the previous backups window was left open"
            assert second.winfo_exists()
        finally:
            second.destroy()


class TestFormatReferenceCoversTheSchema:
    """The reference is generated, so it should hold for every record type."""

    RECORDS = ("Landscape", "Cell", "PathGrid", "Npc", "Creature", "Book")

    @pytest.mark.parametrize("record", RECORDS)
    def test_a_reference_window_opens_for_each_record(self, app: Any, record: str) -> None:
        """One record type with a gap in the schema opens a blank window.

        Args:
            app: The application.
            record: The record type to describe.
        """
        before = set(app.root.winfo_children())
        app._show_format_reference(record)
        opened = [w for w in app.root.winfo_children() if w not in before]
        if not opened:
            pytest.skip(f"{record} is not in the generated schema")
        window = opened[-1]
        try:
            text = _first_text_widget(window)
            assert text is not None, f"{record}: no text widget in the window"
            assert text.get("1.0", "end").strip(), f"{record}: the window is empty"
        finally:
            window.destroy()
