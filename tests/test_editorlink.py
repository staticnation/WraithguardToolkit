"""wraithguard.gui.editorlink: the viewer editor's endpoints, without a display.

A stand-in host runs "UI" work on its own thread, as Tk's would be, so the requests
(answered on the loopback server's thread) really do hand the queue across.
"""

from __future__ import annotations

import json
import queue as stdqueue
import threading
from typing import TYPE_CHECKING, Any

import pytest

from wraithguard.gui import editorlink
from wraithguard.gui.editorlink import EditorLinkMixin
from wraithguard.patch.editor import EditorSession
from wraithguard.patch.queue import PatchQueue

if TYPE_CHECKING:
    from pathlib import Path


class _Host(EditorLinkMixin):
    def __init__(self, tmp_path: Path) -> None:
        self._queue = PatchQueue()
        self.ui_threads: set[str] = set()
        self.refreshed = 0
        self.builder_opened = threading.Event()
        self._jobs: stdqueue.Queue = stdqueue.Queue()
        self._ui = threading.Thread(target=self._loop, name="fake-tk", daemon=True)
        self._ui.start()
        records = {
            "NPC_": [{"type": "Npc", "id": "fargoth", "name": "Fargoth", "data": {"level": 2}}]
        }
        self._editor_session = EditorSession(
            [("Morrowind.esm", tmp_path / "Morrowind.esm")],
            self._queue,
            journal=tmp_path / "journal.json",
            read=lambda _p, tag: records.get(tag, []),
        )

    def _loop(self) -> None:
        while True:
            fn, args = self._jobs.get()
            fn(*args)

    def _schedule_ui(self, _delay: int, fn: Any, *args: Any) -> None:
        self._jobs.put((fn, args))

    def patch_queue(self) -> PatchQueue:
        self.ui_threads.add(threading.current_thread().name)
        return self._queue

    def refresh_patch_views(self) -> None:
        self.ui_threads.add(threading.current_thread().name)
        self.refreshed += 1

    def show_patch_builder(self) -> None:
        self.builder_opened.set()


def _post(handler: Any, **body: Any) -> Any:
    return json.loads(handler(json.dumps(body).encode("utf-8")).body)


def test_record_set_revert_pending_review(tmp_path):
    host = _Host(tmp_path)
    view = _post(host._on_edit_record, tag="NPC_", id="FARGOTH", plugins=["Morrowind.esm"])
    assert view["id"] == "fargoth" and view["winner"] == "Morrowind.esm"
    view = _post(host._on_edit_set, tag="NPC_", id="fargoth", path="data.level", value="7")
    assert {f["path"]: f for f in view["fields"]}["data.level"]["queued"] == 7
    assert host.refreshed == 1 and host.ui_threads == {"fake-tk"}
    assert (tmp_path / "journal.json").is_file()
    assert _post(host._on_edit_pending)[0]["changes"] == [{"path": "data.level", "value": 7}]
    view = _post(host._on_edit_revert, tag="NPC_", id="fargoth", path="data.level")
    assert "queued" not in {f["path"]: f for f in view["fields"]}["data.level"]
    assert _post(host._on_edit_pending) == []
    assert host._on_edit_review(b"{}").body == b"ok"
    assert host.builder_opened.wait(5)


def test_refusals_come_back_as_errors(tmp_path):
    host = _Host(tmp_path)
    with pytest.raises(ValueError, match="not a record"):
        host._on_edit_record(json.dumps({"tag": "NPC_", "id": "nobody"}).encode())
    with pytest.raises(ValueError, match="cannot be changed"):
        host._on_edit_set(
            json.dumps({"tag": "NPC_", "id": "fargoth", "path": "id", "value": "x"}).encode()
        )
    with pytest.raises(ValueError):
        host._on_edit_record(b"not json")


def test_a_busy_ui_thread_times_out(tmp_path, monkeypatch):
    host = _Host(tmp_path)
    monkeypatch.setattr(editorlink, "_UI_WAIT_S", 0.2)
    gate = threading.Event()
    host._schedule_ui(0, gate.wait)  # a modal dialog holding the Tk thread
    with pytest.raises(TimeoutError):
        host._on_edit_pending(b"{}")
    gate.set()


def test_every_link_the_viewer_asks_for_is_registered(tmp_path):
    """The editor page asks Wraithguard by link name; each must be one editor_links gives.

    The viewer's own tests answer from a stand-in, so a link the page uses that Wraithguard
    never registered passes there and fails only in the app.
    """
    import re
    from pathlib import Path

    js = (Path(__file__).resolve().parent.parent / "viewer-shell/ui/src/50_wg_editor.js").read_text(
        encoding="utf-8"
    )
    # Every link name the page spells: asked directly, passed on, or read off links().
    used = set(re.findall(r"'(edit[A-Z]\w*)'", js)) | set(re.findall(r"links\(\)\.(\w+)", js))

    class Server:
        def register_post(self, name: str, fn: Any) -> str:
            return f"http://x/{name}"

    host = _Host(tmp_path)
    host._editor_for = lambda _setup: host._editor_session  # type: ignore[method-assign]
    links = host.editor_links(Server(), tmp_path / "openmw.cfg")  # type: ignore[arg-type]
    missing = sorted(n for n in used if n.startswith("edit") and n not in links)
    assert not missing, f"the viewer asks for links Wraithguard does not register: {missing}"
