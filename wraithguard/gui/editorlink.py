"""The viewer's Editor mode, Wraithguard's half: records, edits and the patch pool.

Mixed into ``App``. The cell viewer has an Editor mode laid out like the Construction
Set (``viewer-shell/ui/src/50_wg_editor.js``); it draws, and asks this, over the
loopback server, for everything else:

- ``editRecord`` ``{tag, id, plugins?}`` -> the record dialog (:meth:`.EditorSession.view`);
- ``editSet`` ``{tag, id, plugins?, path, value}`` -> queue a typed value, and the dialog;
- ``editRevert`` ``{tag, id, plugins?, path?}`` -> drop a change (or all of a record's);
- ``editRef`` ``{cell, origin, refr, plugins?}`` -> the reference dialog
  (:meth:`.EditorSession.ref_view`), ``cell`` the cell's record key;
- ``editRefSet`` ``{..., path, value}`` / ``editRefRevert`` ``{..., path?}`` -> change a
  placed object, or drop the change; the patch carries it as merge_to_master does;
- ``editPlace`` ``{cell, tag, id, translation, rotation?, plugins?, defined?}`` -> a new
  reference to a record, in the patch (``plugins``: the cell's; ``defined``: the
  record's definers); ``editNew`` / ``editNewSet`` / ``editNewRemove`` ``{cell, uid,
  ...}`` -> its dialog, a change, or taking it back out;
- ``editDuplicate`` ``{tag, id, plugins?, newId}`` -> a copy of a record under a new
  id, made by the patch; its dialog (``editSet`` changes it, ``editRevert`` without a
  path removes it);
- ``editPending`` ``{}`` -> everything the patch would carry;
- ``editReview`` ``{}`` -> open the Patch Builder here, to review and write.

The edits go into the same :class:`~wraithguard.patch.queue.PatchQueue` the conflict
viewer fills, so one Patch Builder reviews and writes both. The server answers on its
own thread and the queue belongs to the Tk thread: reading plugins happens on the
server's thread, and every touch of the queue is run on the Tk thread
(:meth:`EditorLinkMixin._on_ui_wait`), which matters on free-threaded Python, where the
two really do run at once.

Copyright (c) 2026 StaticNation.
"""

from __future__ import annotations

import json
import threading
from typing import TYPE_CHECKING, Any

from wraithguard.gui import app_base_dir
from wraithguard.i18n import gettext as _
from wraithguard.logging_setup import get_logger
from wraithguard.viz.serve import Payload

if TYPE_CHECKING:
    import tkinter as tk
    from collections.abc import Callable
    from pathlib import Path

    from wraithguard.patch.editor import EditorSession, Found, FoundRef
    from wraithguard.patch.queue import PatchQueue
    from wraithguard.patch.refedit import NewRef
    from wraithguard.viz.serve import ViewerServer

LOG = get_logger(__name__)

#: How long a request waits for the Tk thread (a modal dialog open there holds it).
_UI_WAIT_S = 20.0


def journal_path() -> Path:
    """Where the patch pool is journalled (see :func:`.patch.editor.save_queue`)."""
    return app_base_dir() / "wraithguard_patch_journal.json"


class EditorLinkMixin:
    """The viewer editor's loopback endpoints (mixed into ``App``)."""

    if TYPE_CHECKING:
        status_var: tk.StringVar

        def _schedule_ui(
            self, delay_ms: int, func: Callable[..., Any], *args: Any  # noqa: ANN401
        ) -> None: ...
        def patch_queue(self) -> PatchQueue: ...  # noqa: D102
        def refresh_patch_views(self) -> None: ...  # noqa: D102
        def show_patch_builder(self) -> None: ...  # noqa: D102

    # -- the session --------------------------------------------------------------------

    def _editor_for(self, setup: Path) -> EditorSession:
        """The editor session for the setup the viewer was launched on.

        A new one when the setup changed (another sort, other plugins); the queue
        and its journal are shared either way.

        Args:
            setup: The cfg the viewer opens.

        Returns:
            The session.
        """
        from wraithguard.patch.editor import EditorSession, plugins_from_cfg

        plugins = plugins_from_cfg(setup)
        current: EditorSession | None = getattr(self, "_editor_session", None)
        if current is None or current.order != [n for n, _p in plugins]:
            current = EditorSession(plugins, self.patch_queue(), journal_path())
            self._editor_session = current
        return current

    def editor_links(self, server: ViewerServer, setup: Path) -> dict[str, str]:
        """Register the editor's endpoints for a viewer about to launch.

        Args:
            server: The loopback server.
            setup: The cfg the viewer opens (its load order is what is edited).

        Returns:
            Link name -> URL, for the launch's extra file.
        """
        self._editor_for(setup)
        return {
            "editRecord": server.register_post("wg_edit_record", self._on_edit_record),
            "editSet": server.register_post("wg_edit_set", self._on_edit_set),
            "editRevert": server.register_post("wg_edit_revert", self._on_edit_revert),
            "editRef": server.register_post("wg_edit_ref", self._on_edit_ref),
            "editRefSet": server.register_post("wg_edit_ref_set", self._on_edit_ref_set),
            "editRefRevert": server.register_post("wg_edit_ref_revert", self._on_edit_ref_revert),
            "editDuplicate": server.register_post("wg_edit_duplicate", self._on_edit_duplicate),
            "editPlace": server.register_post("wg_edit_place", self._on_edit_place),
            "editNew": server.register_post("wg_edit_new", self._on_edit_new),
            "editNewSet": server.register_post("wg_edit_new_set", self._on_edit_new_set),
            "editNewRemove": server.register_post("wg_edit_new_remove", self._on_edit_new_remove),
            "editPending": server.register_post("wg_edit_pending", self._on_edit_pending),
            "editReview": server.register_post("wg_edit_review", self._on_edit_review),
        }

    # -- running on the Tk thread -------------------------------------------------------

    def _on_ui_wait(self, fn: Callable[[], Any]) -> Any:  # noqa: ANN401 - fn's result
        """Run ``fn`` on the Tk thread and wait for its result (or its exception).

        Args:
            fn: What to run.

        Returns:
            Its result.

        Raises:
            TimeoutError: When the Tk thread does not get to it in time.
        """
        done = threading.Event()
        box: dict[str, Any] = {}

        def run() -> None:
            """Run it, keeping the result or the error for the waiting thread."""
            try:
                box["value"] = fn()
            except Exception as exc:  # noqa: BLE001 - re-raised on the waiting thread
                box["error"] = exc
            finally:
                done.set()

        self._schedule_ui(0, run)
        if not done.wait(_UI_WAIT_S):
            raise TimeoutError(_("Wraithguard is busy (a dialog is open?) - try again"))
        if "error" in box:
            raise box["error"]
        return box.get("value")

    # -- the endpoints ------------------------------------------------------------------

    def _edit_request(self, body: bytes) -> tuple[dict[str, Any], EditorSession, Found]:
        """Parse a request naming a record, and find the record (on this thread).

        Args:
            body: ``{tag, id, plugins?, ...}``.

        Returns:
            ``(request, session, record)``.

        Raises:
            ValueError: For a bad request, no session, or a record no plugin has.
        """
        from wraithguard.gui.record_link import parse_request

        tag, rid = parse_request(body)
        req = json.loads(body.decode("utf-8"))
        session: EditorSession | None = getattr(self, "_editor_session", None)
        if session is None:
            raise ValueError(_("The editor is not connected to a load order - reopen the viewer"))
        plugins = req.get("plugins")
        if plugins is not None and not (
            isinstance(plugins, list) and all(isinstance(p, str) for p in plugins)
        ):
            raise ValueError("bad plugins")
        found = session.find(tag, rid, plugins or None)
        if found is None:
            raise ValueError(
                _("%(id)s is not a record any plugin of this load order defines") % {"id": rid}
            )
        return req, session, found

    @staticmethod
    def _json(value: object) -> Payload:
        """An answer as JSON.

        Args:
            value: What to send.

        Returns:
            The payload.
        """
        return Payload(json.dumps(value).encode("utf-8"), "application/json")

    def _on_edit_record(self, body: bytes) -> Payload:
        """``editRecord``: the record dialog's contents.

        Args:
            body: ``{tag, id, plugins?}``.

        Returns:
            :meth:`.EditorSession.view`, as JSON.
        """
        _req, session, found = self._edit_request(body)
        return self._json(self._on_ui_wait(lambda: session.view(found)))

    def _on_edit_set(self, body: bytes) -> Payload:
        """``editSet``: queue a typed value for one field.

        Args:
            body: ``{tag, id, plugins?, path, value}``.

        Returns:
            The record dialog after the change.
        """
        req, session, found = self._edit_request(body)
        path = req.get("path")
        if not isinstance(path, str) or not path:
            raise ValueError("bad path")

        def change() -> dict[str, Any]:
            """Change the queue and redraw the Patch Builder."""
            session.set_field(found, path, req.get("value"))
            self.refresh_patch_views()
            return session.view(found)

        return self._json(self._on_ui_wait(change))

    def _on_edit_duplicate(self, body: bytes) -> Payload:
        """``editDuplicate``: a copy of a record under a new id, made by the patch.

        Args:
            body: ``{tag, id, plugins?, newId}``.

        Returns:
            The copy's record dialog.
        """
        req, session, found = self._edit_request(body)
        new_id = req.get("newId")
        if not isinstance(new_id, str):
            raise ValueError("bad newId")

        def change() -> dict[str, Any]:
            """Queue the copy and redraw the Patch Builder."""
            copy = session.duplicate(found, new_id)
            self.refresh_patch_views()
            return session.view(copy)

        return self._json(self._on_ui_wait(change))

    def _on_edit_revert(self, body: bytes) -> Payload:
        """``editRevert``: drop a queued change, or all of a record's.

        Args:
            body: ``{tag, id, plugins?, path?}``.

        Returns:
            The record dialog after the change.
        """
        req, session, found = self._edit_request(body)
        path = req.get("path")

        def change() -> dict[str, Any]:
            """Change the queue and redraw the Patch Builder."""
            session.revert(found, path if isinstance(path, str) and path else None)
            self.refresh_patch_views()
            return session.view(found)

        return self._json(self._on_ui_wait(change))

    def _ref_request(self, body: bytes) -> tuple[dict[str, Any], EditorSession, FoundRef]:
        """Parse a request naming a placed object, and find it (on this thread).

        Args:
            body: ``{cell, origin, refr, plugins?, ...}``.

        Returns:
            ``(request, session, reference)``.

        Raises:
            ValueError: For a bad request, no session, or a reference no plugin has.
        """
        try:
            req = json.loads(body.decode("utf-8"))
        except (UnicodeDecodeError, ValueError):
            raise ValueError("bad request") from None
        if not isinstance(req, dict):
            raise ValueError("bad request")
        cell, origin, refr = req.get("cell"), req.get("origin"), req.get("refr")
        if not (isinstance(cell, str) and cell and isinstance(origin, str) and origin):
            raise ValueError("bad request")
        if not isinstance(refr, int) or isinstance(refr, bool) or refr < 0:
            raise ValueError("bad refr")
        plugins = req.get("plugins")
        if plugins is not None and not (
            isinstance(plugins, list) and all(isinstance(p, str) for p in plugins)
        ):
            raise ValueError("bad plugins")
        session: EditorSession | None = getattr(self, "_editor_session", None)
        if session is None:
            raise ValueError(_("The editor is not connected to a load order - reopen the viewer"))
        found = session.find_ref(cell, origin, refr, plugins or None)
        if found is None:
            raise ValueError(
                _("No plugin of this load order has reference %(ref)s in %(cell)s")
                % {"ref": f"{origin}:{refr}", "cell": cell}
            )
        return req, session, found

    def _on_edit_ref(self, body: bytes) -> Payload:
        """``editRef``: the reference dialog's contents.

        Args:
            body: ``{cell, origin, refr, plugins?}``.

        Returns:
            :meth:`.EditorSession.ref_view`, as JSON.
        """
        _req, session, found = self._ref_request(body)
        return self._json(self._on_ui_wait(lambda: session.ref_view(found)))

    def _on_edit_ref_set(self, body: bytes) -> Payload:
        """``editRefSet``: queue a change to a placed object.

        Args:
            body: ``{cell, origin, refr, plugins?, path, value}``.

        Returns:
            The reference dialog after the change.
        """
        req, session, found = self._ref_request(body)
        path = req.get("path")
        if not isinstance(path, str) or not path:
            raise ValueError("bad path")

        def change() -> dict[str, Any]:
            """Change the queue and redraw the Patch Builder."""
            session.set_ref_field(found, path, req.get("value"))
            self.refresh_patch_views()
            return session.ref_view(found)

        return self._json(self._on_ui_wait(change))

    def _on_edit_ref_revert(self, body: bytes) -> Payload:
        """``editRefRevert``: drop a change to a placed object, or all of them.

        Args:
            body: ``{cell, origin, refr, plugins?, path?}``.

        Returns:
            The reference dialog after the change.
        """
        req, session, found = self._ref_request(body)
        path = req.get("path")

        def change() -> dict[str, Any]:
            """Change the queue and redraw the Patch Builder."""
            session.revert_ref(found, path if isinstance(path, str) and path else None)
            self.refresh_patch_views()
            return session.ref_view(found)

        return self._json(self._on_ui_wait(change))

    @staticmethod
    def _names(value: object, what: str) -> list[str] | None:
        """A list of plugin names from a request, or None when absent."""
        if value is None:
            return None
        if not (isinstance(value, list) and all(isinstance(p, str) for p in value)):
            raise ValueError(f"bad {what}")
        return value or None

    def _editor(self) -> EditorSession:
        """The editor session, or a refusal when the viewer is not connected."""
        session: EditorSession | None = getattr(self, "_editor_session", None)
        if session is None:
            raise ValueError(_("The editor is not connected to a load order - reopen the viewer"))
        return session

    def _on_edit_place(self, body: bytes) -> Payload:
        """``editPlace``: a new reference to a record, into the patch.

        Args:
            body: ``{cell, tag, id, translation, rotation?, plugins?, defined?}``.

        Returns:
            The new reference's dialog (:meth:`.EditorSession.new_view`).
        """
        _req, session, found = self._edit_request(
            json.dumps(self._place_record(body)).encode("utf-8")
        )
        place = json.loads(body.decode("utf-8"))
        cell = place.get("cell")
        if not isinstance(cell, str) or not cell:
            raise ValueError("bad cell")
        cell_plugins = self._names(place.get("plugins"), "plugins")

        def change() -> dict[str, Any]:
            """Queue it and redraw the Patch Builder."""
            new = session.place(
                cell, found, place.get("translation"), place.get("rotation"), cell_plugins
            )
            self.refresh_patch_views()
            return session.new_view(new)

        return self._json(self._on_ui_wait(change))

    @staticmethod
    def _place_record(body: bytes) -> dict[str, Any]:
        """The record half of an ``editPlace`` request, as ``editRecord`` takes it."""
        try:
            req = json.loads(body.decode("utf-8"))
        except (UnicodeDecodeError, ValueError):
            raise ValueError("bad request") from None
        if not isinstance(req, dict):
            raise ValueError("bad request")
        out = {"tag": req.get("tag"), "id": req.get("id")}
        if req.get("defined") is not None:
            out["plugins"] = req.get("defined")
        return out

    def _new_request(self, body: bytes) -> tuple[dict[str, Any], EditorSession, NewRef]:
        """Parse a request naming a new reference, and find it in the patch pool.

        Args:
            body: ``{cell, uid, ...}``.

        Returns:
            ``(request, session, reference)``.

        Raises:
            ValueError: For a bad request, or a reference the pool no longer has.
        """
        try:
            req = json.loads(body.decode("utf-8"))
        except (UnicodeDecodeError, ValueError):
            raise ValueError("bad request") from None
        if not isinstance(req, dict):
            raise ValueError("bad request")
        cell, uid = req.get("cell"), req.get("uid")
        if not (isinstance(cell, str) and cell and isinstance(uid, str) and uid):
            raise ValueError("bad request")
        session = self._editor()
        new = self._on_ui_wait(lambda: session.find_new(cell, uid))
        if new is None:
            raise ValueError(_("That new reference is no longer in the patch"))
        return req, session, new

    def _on_edit_new(self, body: bytes) -> Payload:
        """``editNew``: a new reference's dialog.

        Args:
            body: ``{cell, uid}``.

        Returns:
            :meth:`.EditorSession.new_view`, as JSON.
        """
        _req, session, new = self._new_request(body)
        return self._json(session.new_view(new))

    def _on_edit_new_set(self, body: bytes) -> Payload:
        """``editNewSet``: change a field of a new reference.

        Args:
            body: ``{cell, uid, path, value}``.

        Returns:
            Its dialog after the change.
        """
        req, session, new = self._new_request(body)
        path = req.get("path")
        if not isinstance(path, str) or not path:
            raise ValueError("bad path")

        def change() -> dict[str, Any]:
            """Change the queue and redraw the Patch Builder."""
            now = session.set_new_field(new, path, req.get("value"))
            self.refresh_patch_views()
            return session.new_view(now)

        return self._json(self._on_ui_wait(change))

    def _on_edit_new_remove(self, body: bytes) -> Payload:
        """``editNewRemove``: take a new reference back out of the patch.

        Args:
            body: ``{cell, uid}``.

        Returns:
            ``ok``.
        """
        _req, session, new = self._new_request(body)

        def change() -> None:
            """Change the queue and redraw the Patch Builder."""
            session.remove_new(new)
            self.refresh_patch_views()

        self._on_ui_wait(change)
        return Payload(b"ok", "text/plain; charset=utf-8")

    def _on_edit_pending(self, _body: bytes) -> Payload:
        """``editPending``: everything the patch would carry.

        Returns:
            :meth:`.EditorSession.pending`, as JSON.
        """
        session: EditorSession | None = getattr(self, "_editor_session", None)
        if session is None:
            return self._json([])
        return self._json(self._on_ui_wait(session.pending))

    def _on_edit_review(self, _body: bytes) -> Payload:
        """``editReview``: open the Patch Builder, where the pool is reviewed and written.

        Returns:
            ``ok``.
        """
        self._schedule_ui(0, self.show_patch_builder)
        return Payload(b"ok", "text/plain; charset=utf-8")


__all__ = ["EditorLinkMixin", "journal_path"]
