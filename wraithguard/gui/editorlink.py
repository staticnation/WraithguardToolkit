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
- ``editUses`` ``{tag, id, plugins?}`` -> the record's Use Report
  (:meth:`.EditorSession.uses`: every record that names it, live or overridden);
- ``editReplace`` ``{tag, id, plugins?, newId}`` -> Search & Replace: every live use's
  field, and every placed reference, repointed to ``newId``, queued (``{changed, refs,
  scripts}``: scripts and dialogue results changed too, a script recompiled);
- ``editScript`` ``{tag, id, plugins?, text?}`` -> the Script Edit window: the source
  (the queued change, or ``text`` to check as typed), its checks, the compiler's messages
  and the compiled listing; saving is ``editSet`` on ``text`` (which compiles it and queues
  the bytecode with it);
- ``editFlowchart`` ``{tag, id, plugins?, text?}`` -> the script's control flow, opened in
  Wraithguard's chart window (:mod:`wraithguard.mwscript.flowchart`);
- ``editResult`` ``{tag, id, plugins?, text?}`` -> a dialogue response's result script
  compiled for its errors, against its speaker's script's locals;
- ``editTopics`` ``{}`` / ``editTopic`` ``{topic}`` -> the dialogue window: every topic,
  and a topic's responses in the order the engine reads them (a response is edited as a
  record, ``INFO``, in the record dialog);
- ``editDialogueFlow`` ``{topic}`` / ``editDialogueMap`` ``{topic, depth?}`` /
  ``editDialogueFlags`` ``{name?, kind?}`` / ``editDialoguePlay`` ``{speaker, topic?, choice?,
  cell?, disposition?, strict?}`` -> the dialogue window's other views
  (:mod:`wraithguard.patch.dialogue_graph`): a topic as a flow with its choice tree, the
  topics around one as a map, the variables and quests the dialogue writes and tests, and
  the game's dialogue window rehearsed for an NPC;
- ``editColumns`` ``{tag}`` -> the Object Window's columns for a record type, as the
  Construction Set has them (:mod:`wraithguard.patch.columns`);
- ``editInsert`` ``{tag, newId}`` -> a blank record of a type, made by the patch; its
  dialog;
- ``editNewResponse`` ``{topic, after}`` -> a response added to a topic after another (or
  at the top), made by the patch; its record dialog;
- ``editCopyTopic`` ``{topic, newId}`` -> a copy of a topic and its responses under a new
  name, made by the patch; the new topic as ``editTopic`` gives it;
- ``editPending`` ``{}`` -> everything the patch would carry;
- ``editReview`` ``{}`` -> open the Patch Builder here, to review and write;
- ``editBuildInfo`` ``{}`` -> the viewer's own Patch Builder (its "Build patch"
  panel): what the pool holds by kind, where the last patch went, the setup's data
  folders to write into, and the load order's plugin names;
- ``editBuildCheck`` ``{path}`` -> whether a chosen file exists (Append or Replace);
- ``editBuild`` ``{path, mode}`` -> write the pool as a patch, ``mode`` ``new``,
  ``append`` or ``replace``, on a thread of its own: ``{job}``, followed with
  ``editBuildStatus`` ``{job, from}`` -> ``{state, lines, result | error}`` (the build's
  report as it goes; the pool is cleared when it is written, as the Patch Builder does);
- ``editPreviewPatch`` ``{path}`` -> a new Cell Preview with the written patch loaded
  last.

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
import os
import threading
from pathlib import Path
from typing import TYPE_CHECKING, Any

from wraithguard.gui import app_base_dir
from wraithguard.i18n import gettext as _
from wraithguard.logging_setup import get_logger
from wraithguard.viz.serve import Payload

if TYPE_CHECKING:
    import tkinter as tk
    from collections.abc import Callable

    from wraithguard.patch.editor import EditorSession, Found, FoundRef
    from wraithguard.patch.queue import PatchQueue
    from wraithguard.patch.refedit import NewRef
    from wraithguard.patch.service import PatchResult
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
            self,
            delay_ms: int,
            func: Callable[..., Any],
            *args: Any,  # noqa: ANN401
        ) -> None: ...
        def patch_queue(self) -> PatchQueue: ...  # noqa: D102
        def refresh_patch_views(self) -> None: ...  # noqa: D102
        def show_patch_builder(self) -> None: ...  # noqa: D102
        def patch_summary(self) -> dict[str, int]: ...  # noqa: D102
        def patch_last_path(self) -> Path | None: ...  # noqa: D102
        def patch_default_name(self) -> str: ...  # noqa: D102
        def prepare_patch(self, target: Path, *, append: bool) -> dict[str, Any]: ...  # noqa: D102
        def run_patch(  # noqa: D102
            self, job: dict[str, Any], report: Callable[[str], Any] | None = None
        ) -> PatchResult: ...
        def finish_patch(self, result: PatchResult) -> PatchResult: ...  # noqa: D102
        def patch_written_note(self, result: PatchResult) -> str: ...  # noqa: D102

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

        self._editor_setup = setup
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
            "editInsert": server.register_post("wg_edit_insert", self._on_edit_insert),
            "editUses": server.register_post("wg_edit_uses", self._on_edit_uses),
            "editScript": server.register_post("wg_edit_script", self._on_edit_script),
            "editResult": server.register_post("wg_edit_result", self._on_edit_result),
            "editFlowchart": server.register_post("wg_edit_flowchart", self._on_edit_flowchart),
            "editTopics": server.register_post("wg_edit_topics", self._on_edit_topics),
            "editTopic": server.register_post("wg_edit_topic", self._on_edit_topic),
            "editColumns": server.register_post("wg_edit_columns", self._on_edit_columns),
            "editPathgrid": server.register_post("wg_edit_pathgrid", self._on_edit_pathgrid),
            "editPathgridSet": server.register_post(
                "wg_edit_pathgrid_set", self._on_edit_pathgrid_set
            ),
            "editDialogueFlow": server.register_post("wg_edit_dial_flow", self._on_dialogue_flow),
            "editDialogueMap": server.register_post("wg_edit_dial_map", self._on_dialogue_map),
            "editDialogueFlags": server.register_post(
                "wg_edit_dial_flags", self._on_dialogue_flags
            ),
            "editDialoguePlay": server.register_post("wg_edit_dial_play", self._on_dialogue_play),
            "editNewResponse": server.register_post(
                "wg_edit_new_response", self._on_edit_new_response
            ),
            "editCopyTopic": server.register_post("wg_edit_copy_topic", self._on_edit_copy_topic),
            "editReplace": server.register_post("wg_edit_replace", self._on_edit_replace),
            "editPlace": server.register_post("wg_edit_place", self._on_edit_place),
            "editNew": server.register_post("wg_edit_new", self._on_edit_new),
            "editNewSet": server.register_post("wg_edit_new_set", self._on_edit_new_set),
            "editNewRemove": server.register_post("wg_edit_new_remove", self._on_edit_new_remove),
            "editPending": server.register_post("wg_edit_pending", self._on_edit_pending),
            "editReview": server.register_post("wg_edit_review", self._on_edit_review),
            "editBuildInfo": server.register_post("wg_edit_build_info", self._on_edit_build_info),
            "editBuildCheck": server.register_post(
                "wg_edit_build_check", self._on_edit_build_check
            ),
            "editBuild": server.register_post("wg_edit_build", self._on_edit_build),
            "editBuildStatus": server.register_post(
                "wg_edit_build_status", self._on_edit_build_status
            ),
            "editPreviewPatch": server.register_post(
                "wg_edit_preview_patch", self._on_edit_preview_patch
            ),
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
        return Payload(json.dumps(value, default=_jsonable).encode("utf-8"), "application/json")

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

        value = req.get("value")
        script_text = found.record_type == "Script" and path == "text" and isinstance(value, str)
        if script_text:
            # Read the load order for the compiler here, off the Tk thread (once).
            session.script_compiler(self._on_ui_wait(session.pool_script_records))

        def change() -> dict[str, Any]:
            """Change the queue (a script's text with its bytecode) and redraw."""
            compiled = None
            if script_text:
                compiled = session.set_script_text(found, str(value))
            else:
                session.set_field(found, path, value)
            self.refresh_patch_views()
            out = session.view(found)
            if compiled is not None:
                out["compile"] = compiled
            return out

        return self._json(self._on_ui_wait(change))

    def _on_edit_topics(self, _body: bytes) -> Payload:
        """``editTopics``: every topic of the load order (read on the server's thread).

        Returns:
            :meth:`.EditorSession.topics`, as JSON.
        """
        return self._json(self._editor().topics())

    def _on_edit_topic(self, body: bytes) -> Payload:
        """``editTopic``: one topic's responses, in the order the engine reads them.

        Args:
            body: ``{topic}``.

        Returns:
            :meth:`.EditorSession.topic`, as JSON.
        """
        try:
            req = json.loads(body.decode("utf-8"))
        except (UnicodeDecodeError, ValueError):
            raise ValueError("bad request") from None
        topic = req.get("topic") if isinstance(req, dict) else None
        if not isinstance(topic, str) or not topic.strip():
            raise ValueError("bad topic")
        return self._json(self._editor().topic(topic))

    # -- the dialogue's other views ------------------------------------------------------

    def _dialogue_index(self) -> Any:  # noqa: ANN401 - a DialogueIndex
        """The whole load order's dialogue, read once (again when the pool's responses change).

        The pool's typed changes to responses, and the topics and responses it makes, are
        laid over the plugins', so a rehearsal says the line as it will be.
        """
        from wraithguard.patch.dialogue_graph import DialogueIndex
        from wraithguard.patch.merge import FieldValue

        session = self._editor()

        def snapshot() -> tuple[list[tuple[str, str, str, Any]], dict[str, dict[str, Any]]]:
            """The pool's made records and edits, read on the Tk thread."""
            queue = self.patch_queue()
            made = [
                (m.record_type, m.topic, m.key, dict(m.record))
                for m in queue.new_records
                if m.record_type in ("Dialogue", "DialogueInfo")
            ]
            edits: dict[str, dict[str, Any]] = {}
            for (record_type, key), choices in queue.fields.items():
                if record_type == "DialogueInfo":
                    for c in choices:
                        if isinstance(c, FieldValue):
                            edits.setdefault(key, {})[c.path] = c.value
            return made, edits

        made, edits = self._on_ui_wait(snapshot)
        sig = repr((made, sorted(edits.items()), tuple(session.order)))
        cached = getattr(self, "_dial_index", None)
        if cached is not None and cached[0] == sig:
            return cached[1]
        plugins = [(name, session._dialogue(name)) for name in session.order]
        index = DialogueIndex(plugins, made, edits)
        self._dial_index = (sig, index)
        return index

    def _on_edit_columns(self, body: bytes) -> Payload:
        """``editColumns``: a record type's own columns, for every record of it.

        Returns:
            :func:`.columns.table`, as JSON.
        """
        from wraithguard.patch.columns import table

        req = self._body(body)
        tag = req.get("tag")
        if not isinstance(tag, str) or not tag.strip():
            raise ValueError("bad tag")
        session = self._editor()
        tag = tag.strip().upper().ljust(4, "_")[:4]
        versions = [session._records(name, tag) for name in session.order]
        if tag != "ARMO":
            return self._json(table(tag, versions))
        from wraithguard.patch.armor import browser_table, settings_from

        settings = settings_from(session._records(name, "GMST") for name in session.order)
        out = table(tag, versions, settings)
        out["armorClass"] = browser_table(settings)
        return self._json(out)

    def _on_edit_pathgrid(self, body: bytes) -> Payload:
        """``editPathgrid``: a cell's path grid as the patch would write it.

        Args:
            body: ``{cell}`` - ``"x,y"`` or ``"int:<name>"``.

        Returns:
            :meth:`.EditorSession.pathgrid_view`, as JSON.
        """
        req = self._body(body)
        cell = req.get("cell")
        if not isinstance(cell, str) or not cell.strip():
            raise ValueError("bad cell")
        session = self._editor()
        return self._json(self._on_ui_wait(lambda: session.pathgrid_view(cell)))

    def _on_edit_pathgrid_set(self, body: bytes) -> Payload:
        """``editPathgridSet``: queue a cell's whole path grid, or drop it (``revert``).

        Args:
            body: ``{cell, points:[[x,y,z]], edges:[[a,b]]}`` (an exterior's points
                relative to its cell), or ``{cell, revert: true}``.

        Returns:
            :meth:`.EditorSession.pathgrid_view` after, as JSON.
        """
        req = self._body(body)
        cell = req.get("cell")
        if not isinstance(cell, str) or not cell.strip():
            raise ValueError("bad cell")
        session = self._editor()
        if req.get("revert"):

            def drop() -> dict[str, Any]:
                """Drop it and redraw."""
                out = session.revert_pathgrid(cell)
                self.refresh_patch_views()
                return out

            return self._json(self._on_ui_wait(drop))
        points, edges = req.get("points"), req.get("edges")
        if not isinstance(points, list) or not isinstance(edges, list):
            raise ValueError("points and edges are lists")

        def change() -> dict[str, Any]:
            """Queue it and redraw."""
            out = session.set_pathgrid(cell, points, edges)
            self.refresh_patch_views()
            return out

        return self._json(self._on_ui_wait(change))

    def _on_dialogue_flow(self, body: bytes) -> Payload:
        """``editDialogueFlow``: a topic's responses as a flow, with its choice tree.

        Returns:
            :meth:`.DialogueIndex.flow`, as JSON.
        """
        req = self._body(body)
        topic = req.get("topic")
        if not isinstance(topic, str) or not topic.strip():
            raise ValueError("bad topic")
        return self._json(self._dialogue_index().flow(topic))

    def _on_dialogue_map(self, body: bytes) -> Payload:
        """``editDialogueMap``: the topics, quests and variables around a topic.

        Returns:
            :meth:`.DialogueIndex.neighbourhood`, as JSON.
        """
        req = self._body(body)
        topic = req.get("topic")
        if not isinstance(topic, str) or not topic.strip():
            raise ValueError("bad topic")
        depth = req.get("depth") if isinstance(req.get("depth"), int) else 1
        return self._json(self._dialogue_index().neighbourhood(topic, depth))

    def _on_dialogue_flags(self, body: bytes) -> Payload:
        """``editDialogueFlags``: what the dialogue writes and tests (one, or the list).

        Returns:
            :meth:`.DialogueIndex.flags`, as JSON.
        """
        req = self._body(body)
        name = req.get("name") if isinstance(req.get("name"), str) else ""
        kind = req.get("kind") if isinstance(req.get("kind"), str) else ""
        return self._json(self._dialogue_index().flags(name, kind))

    def _on_dialogue_play(self, body: bytes) -> Payload:
        """``editDialoguePlay``: the game's dialogue window, rehearsed for an NPC.

        Returns:
            :meth:`.DialogueIndex.play` with ``speaker`` (the NPC as read), as JSON.

        Raises:
            ValueError: For an NPC no plugin defines.
        """
        from wraithguard.patch.dialogue_graph import npc_of

        req = self._body(body)
        speaker = req.get("speaker")
        if not isinstance(speaker, str) or not speaker.strip():
            raise ValueError(_("Choose who to talk to (an NPC's id)"))
        found = self._editor().find("NPC_", speaker.strip(), None) or self._editor().find(
            "CREA", speaker.strip(), None
        )
        if found is None:
            raise ValueError(
                _("%(id)s is not an NPC or creature of this load order") % {"id": speaker}
            )
        npc = npc_of(found.record)

        def num(key: str, default: int) -> int:
            """An integer from the request, or ``default`` when it is not a number."""
            value = req.get(key)
            return int(value) if isinstance(value, (int, float)) else default

        out = self._dialogue_index().play(
            npc,
            str(req.get("topic") or ""),
            choice=num("choice", 0),
            cell=str(req.get("cell") or ""),
            disposition=num("disposition", 50),
            strict=bool(req.get("strict")),
        )
        out["speaker"] = npc
        return self._json(out)

    @staticmethod
    def _body(body: bytes) -> dict[str, Any]:
        """A request's JSON object.

        Args:
            body: The request.

        Returns:
            The object.

        Raises:
            ValueError: When it is not a JSON object.
        """
        try:
            req = json.loads(body.decode("utf-8"))
        except (UnicodeDecodeError, ValueError):
            raise ValueError("bad request") from None
        if not isinstance(req, dict):
            raise ValueError("bad request")
        return req

    def _on_edit_new_response(self, body: bytes) -> Payload:
        """``editNewResponse``: a response added to a topic, in the patch.

        Args:
            body: ``{topic, after}`` (``after`` empty for the top).

        Returns:
            The response's record dialog.
        """
        req = self._body(body)
        topic, after = req.get("topic"), req.get("after", "")
        if not isinstance(topic, str) or not topic.strip() or not isinstance(after, str):
            raise ValueError("bad request")
        session = self._editor()

        def change() -> dict[str, Any]:
            """Queue the response and redraw the Patch Builder."""
            made = session.new_response(topic, after)
            self.refresh_patch_views()
            return session.view(made)

        return self._json(self._on_ui_wait(change))

    def _on_edit_copy_topic(self, body: bytes) -> Payload:
        """``editCopyTopic``: a topic and its responses copied under a new name.

        Args:
            body: ``{topic, newId}``.

        Returns:
            The new topic (:meth:`.EditorSession.topic`).
        """
        req = self._body(body)
        topic, new_id = req.get("topic"), req.get("newId")
        if not isinstance(topic, str) or not isinstance(new_id, str):
            raise ValueError("bad request")
        session = self._editor()

        def change() -> dict[str, Any]:
            """Queue the copy and redraw the Patch Builder."""
            out = session.copy_topic(topic, new_id)
            self.refresh_patch_views()
            return out

        return self._json(self._on_ui_wait(change))

    def _on_edit_script(self, body: bytes) -> Payload:
        """``editScript``: the Script Edit window's source, checks and listing.

        Args:
            body: ``{tag, id, plugins?, text?}``.

        Returns:
            :meth:`.EditorSession.script_view`, as JSON, with ``queued`` (whether a
            changed text waits in the pool).
        """
        req, session, found = self._edit_request(body)
        text = req.get("text")
        if text is not None and not isinstance(text, str):
            raise ValueError("bad text")

        def queued() -> object:
            """The text waiting in the pool for this script, or None."""
            view = session.view(found)
            return next((f.get("queued") for f in view["fields"] if f["path"] == "text"), None)

        waiting, extra = self._on_ui_wait(lambda: (queued(), session.pool_script_records()))
        source = text if text is not None else (waiting if isinstance(waiting, str) else None)
        out = session.script_view(found, source, session.script_compiler(extra))
        out["queued"] = isinstance(waiting, str)
        return self._json(out)

    def _on_edit_flowchart(self, body: bytes) -> Payload:
        """``editFlowchart``: a script's control flow, opened in Wraithguard's chart window.

        Args:
            body: ``{tag, id, plugins?, text?}`` - ``text`` to draw as typed.

        Returns:
            ``{"shown": bool}`` (False when this Wraithguard has no chart window).
        """
        from wraithguard.mwscript.flowchart import flowchart

        req, _session, found = self._edit_request(body)
        text = req.get("text")
        if not isinstance(text, str):
            text = str(found.record.get("text") or "")
        chart = flowchart(text)
        heading = f"{found.key} - " + _("flowchart")
        show = getattr(self, "_lua_show_chart", None)
        if not callable(show):
            return self._json({"shown": False})
        self._on_ui_wait(lambda: show(heading, chart, _("Script flowchart")))
        return self._json({"shown": True})

    def _on_edit_result(self, body: bytes) -> Payload:
        """``editResult``: a dialogue response's result script, compiled for its errors.

        Args:
            body: ``{tag, id, plugins?, text?}`` - ``text`` to check as typed.

        Returns:
            :meth:`.EditorSession.result_check`, as JSON.
        """
        req, session, found = self._edit_request(body)
        text = req.get("text")
        if text is not None and not isinstance(text, str):
            raise ValueError("bad text")

        def current() -> tuple[dict[str, Any], list[Any]]:
            """The response's result and speaker as the pool has them, and its records."""
            view = session.view(found)
            values = {
                f["path"]: f.get("queued", f["value"])
                for f in view["fields"]
                if f["path"] in ("script_text", "speaker_id")
            }
            return values, session.pool_script_records()

        values, extra = self._on_ui_wait(current)
        source = text if text is not None else str(values.get("script_text") or "")
        compiler = session.script_compiler(extra)
        return self._json(
            session.result_check(source, str(values.get("speaker_id") or ""), compiler)
        )

    def _on_edit_uses(self, body: bytes) -> Payload:
        """``editUses``: the record's Use Report (read on the server's thread).

        Args:
            body: ``{tag, id, plugins?}``.

        Returns:
            :meth:`.EditorSession.uses`, as JSON.
        """
        _req, session, found = self._edit_request(body)
        return self._json(session.uses(found))

    def _on_edit_replace(self, body: bytes) -> Payload:
        """``editReplace``: Search & Replace - the live uses repointed to another record.

        Args:
            body: ``{tag, id, plugins?, newId}``.

        Returns:
            ``{changed, refs, scripts}``: records and references changed, the references
            among them, and the scripts and dialogue results changed with them (a script
            recompiled as it is queued).
        """
        req, session, found = self._edit_request(body)
        new_id = req.get("newId")
        if not isinstance(new_id, str) or not new_id.strip():
            raise ValueError("bad newId")
        plan, _report = session.replace_plan(found, new_id.strip())

        def change() -> int:
            """Queue the plan and redraw the Patch Builder."""
            n = session.replace_uses(plan)
            self.refresh_patch_views()
            return n

        changed = self._on_ui_wait(change)
        return self._json(
            {
                "changed": changed,
                "refs": sum(1 for p in plan if not isinstance(p, tuple)),
                "scripts": sum(
                    1
                    for p in plan
                    if isinstance(p, tuple)
                    and p[1] in ("text", "script_text")
                    and p[0].record_type in ("Script", "DialogueInfo")
                ),
            }
        )

    def _on_edit_insert(self, body: bytes) -> Payload:
        """``editInsert``: a blank record of a type under a new id, made by the patch.

        Args:
            body: ``{tag, newId}``.

        Returns:
            The record's dialog.
        """
        try:
            req = json.loads(body.decode("utf-8"))
        except (UnicodeDecodeError, ValueError):
            raise ValueError("bad request") from None
        tag, new_id = (req.get("tag"), req.get("newId")) if isinstance(req, dict) else (None, None)
        if not isinstance(tag, str) or not isinstance(new_id, str):
            raise ValueError("bad request")
        session = self._editor()

        def change() -> dict[str, Any]:
            """Queue the record and redraw the Patch Builder."""
            made = session.insert(tag, new_id)
            self.refresh_patch_views()
            return session.view(made)

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

    # -- writing the patch from the viewer ---------------------------------------------

    def _patch_folders(self) -> list[str]:
        """The setup's data folders, the last (highest priority) first.

        Where a patch the viewer writes would usually go.

        Returns:
            The folders that exist, as text.
        """
        setup = getattr(self, "_editor_setup", None)
        if setup is None:
            return []
        from wraithguard.lua.scan import read_cfg_lua

        try:
            dirs, _content = read_cfg_lua(setup)
        except OSError:
            return []
        out: list[str] = []
        for d in reversed(dirs):
            text = str(d)
            if d.is_dir() and text not in out:
                out.append(text)
        return out

    def _on_edit_build_info(self, _body: bytes) -> Payload:
        """``editBuildInfo``: what the viewer's "Build patch" panel starts from.

        Returns:
            ``{summary, last, suggested, folders, defaultName, order}``.
        """
        summary, last = self._on_ui_wait(lambda: (self.patch_summary(), self.patch_last_path()))
        folders = self._patch_folders()
        name = self.patch_default_name()
        suggested = str(last) if last else (str(Path(folders[0]) / name) if folders else "")
        session: EditorSession | None = getattr(self, "_editor_session", None)
        return self._json(
            {
                "summary": summary,
                "last": str(last) if last else None,
                "suggested": suggested,
                "folders": folders,
                "defaultName": self.patch_default_name(),
                "order": list(session.order) if session is not None else [],
            }
        )

    @staticmethod
    def _patch_target(body: bytes) -> tuple[dict[str, Any], Path]:
        """A request naming where a patch goes, checked.

        Args:
            body: ``{path, ...}``.

        Returns:
            ``(request, path)``.

        Raises:
            ValueError: For a path that is not a plugin's, not absolute, or in a folder
                that does not exist.
        """
        req = json.loads(body.decode("utf-8"))
        raw = req.get("path")
        if not isinstance(raw, str) or not raw.strip():
            raise ValueError(_("Choose where the patch goes"))
        target = Path(raw.strip())
        if not target.is_absolute():
            raise ValueError(_("The patch needs a full path, folder and all"))
        if target.suffix.lower() not in (".esp", ".esm", ".omwaddon"):
            raise ValueError(_("A patch is a plugin: name it .esp (or .omwaddon)"))
        if not target.parent.is_dir():
            raise ValueError(_("%(folder)s is not a folder") % {"folder": target.parent})
        return req, target

    def _on_edit_build_check(self, body: bytes) -> Payload:
        """``editBuildCheck``: whether the chosen file is there already.

        Returns:
            ``{path, exists, inOrder}``: ``inOrder`` when a plugin of the load order has
            its name (a patch written before, most likely - or a mod).
        """
        _req, target = self._patch_target(body)
        session: EditorSession | None = getattr(self, "_editor_session", None)
        names = {n.lower() for n in (session.order if session is not None else [])}
        return self._json(
            {
                "path": str(target),
                "exists": target.exists(),
                "inOrder": target.name.lower() in names,
            }
        )

    def _on_edit_build(self, body: bytes) -> Payload:
        """``editBuild``: write the pool as a patch, on a thread of its own.

        The pool is gathered on the Tk thread (refused there, as the Patch Builder
        refuses, with the reason), the plugins read and the patch written on the build's
        own thread - long for a big load order, longer than one request may wait - and
        the pool cleared on the Tk thread once it is written.

        Returns:
            ``{job}``, for ``editBuildStatus``.

        Raises:
            ValueError: For a bad path, a build already running, a file that exists
                with no ``append`` or ``replace``, or what the Patch Builder refuses.
        """
        req, target = self._patch_target(body)
        mode = req.get("mode") or "new"
        if mode not in ("new", "append", "replace"):
            raise ValueError("bad mode")
        if mode == "new" and target.exists():
            raise ValueError(_("%(name)s exists: choose Append or Replace") % {"name": target.name})
        running = getattr(self, "_build_state", None)
        if running is not None and running["state"] == "running":
            raise ValueError(_("A patch is being written already"))
        # What the Patch Builder refuses (nothing queued, no reader, a merge the scan
        # lost) comes back as the request's error, its reason the page shows.
        job = self._on_ui_wait(lambda: self.prepare_patch(target, append=mode == "append"))
        state: dict[str, Any] = {
            "job": (running or {}).get("job", 0) + 1,
            "state": "running",
            "lines": [],
            "result": None,
            "error": None,
        }
        self._build_state = state

        def work() -> None:
            """Read, write, then spend the pool (on the Tk thread)."""
            try:
                result = self.run_patch(job, report=state["lines"].append)
                self._on_ui_wait(lambda: self.finish_patch(result))
                state["result"] = {
                    "output": str(result.output) if result.output else None,
                    "records": result.records,
                    "carried": result.carried,
                    "masters": list(result.masters),
                    "remapped": result.remapped,
                    "note": self.patch_written_note(result),
                }
                state["state"] = "done"
            except Exception as exc:  # noqa: BLE001 - said to the viewer, which shows it
                LOG.warning("the viewer's patch build failed: %s", exc)
                state["error"] = str(exc)
                state["state"] = "failed"

        threading.Thread(target=work, name="wg-patch-build", daemon=True).start()
        return self._json({"job": state["job"]})

    def _on_edit_build_status(self, body: bytes) -> Payload:
        """``editBuildStatus``: how the build is going.

        Args:
            body: ``{job, from}``: the job, and how many report lines the page has.

        Returns:
            ``{job, state, lines, result, error}`` - ``lines`` from ``from`` on.

        Raises:
            ValueError: For a job that is not the current one.
        """
        req = json.loads(body.decode("utf-8"))
        state = getattr(self, "_build_state", None)
        if state is None or req.get("job") != state["job"]:
            raise ValueError(_("No such patch build"))
        start = req.get("from") or 0
        if not isinstance(start, int) or start < 0:
            start = 0
        return self._json(
            {
                "job": state["job"],
                "state": state["state"],
                "lines": state["lines"][start:],
                "total": len(state["lines"]),
                "result": state["result"],
                "error": state["error"],
            }
        )

    def _on_edit_preview_patch(self, body: bytes) -> Payload:
        """``editPreviewPatch``: a new Cell Preview with a written patch loaded last.

        Returns:
            ``ok``.

        Raises:
            ValueError: For a path that is not a written plugin, or no viewer to open.
        """
        _req, target = self._patch_target(body)
        if not target.is_file():
            raise ValueError(_("%(name)s is not written yet") % {"name": target.name})
        preview = getattr(self, "preview_plugin", None)
        if preview is None:
            raise ValueError(_("Cell Preview cannot be opened from here"))
        self._schedule_ui(0, preview, target)
        return Payload(b"ok", "text/plain; charset=utf-8")


__all__ = ["EditorLinkMixin", "journal_path"]


def _jsonable(value: object) -> object:
    """What ``json`` cannot write itself: plugin bytes (as their text), sets, paths.

    Raises:
        TypeError: For anything else, as ``json.dumps`` would.
    """
    if isinstance(value, (bytes, bytearray, memoryview)):
        from wraithguard.mwscript.compiler import plugin_text

        return plugin_text(bytes(value))
    if isinstance(value, (set, frozenset, tuple)):
        return list(value)
    if isinstance(value, os.PathLike):
        return os.fspath(value)
    raise TypeError(f"Object of type {type(value).__name__} is not JSON serializable")
