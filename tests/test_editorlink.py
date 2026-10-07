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

    src = Path(__file__).resolve().parent.parent / "viewer-shell/ui/src"
    js = "\n".join(p.read_text(encoding="utf-8") for p in sorted(src.glob("5*_wg_*.js")))
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


class _Builder(_Host):
    """A host that also writes patches (the Patch Builder's steps, stood in for)."""

    def __init__(self, tmp_path: Path) -> None:
        super().__init__(tmp_path)
        self._last: Path | None = None
        self.finished: list[str] = []
        self.previewed = threading.Event()
        self.setup = tmp_path / "openmw.cfg"
        (tmp_path / "Data Files").mkdir()
        (tmp_path / "Mods").mkdir()
        self.setup.write_text(
            f'data="{tmp_path / "Data Files"}"\ndata="{tmp_path / "Mods"}"\ncontent=Morrowind.esm\n',
            encoding="utf-8",
        )
        self._editor_setup = self.setup

    def patch_summary(self) -> dict[str, int]:
        self.ui_threads.add(threading.current_thread().name)
        return {
            "records": len(self._queue),
            "whole": 0,
            "merged": len(self._queue.fields),
            "refs": 0,
            "new_refs": 0,
            "new_records": 0,
        }

    def patch_last_path(self) -> Path | None:
        return self._last

    @staticmethod
    def patch_default_name() -> str:
        return "Wraithguard Patch.esp"

    def prepare_patch(self, target: Path, *, append: bool) -> dict[str, Any]:
        self.ui_threads.add(threading.current_thread().name)
        if not len(self._queue):
            raise ValueError("Nothing is queued")
        return {"target": target, "append": append}

    def run_patch(self, job: dict[str, Any], report: Any = None) -> Any:
        from types import SimpleNamespace

        for line in ("reading", "writing"):
            report(line)
        job["target"].write_bytes(b"TES3")
        return SimpleNamespace(
            output=job["target"],
            records=1,
            carried=1 if job["append"] else 0,
            masters=["Morrowind.esm"],
            remapped=0,
        )

    def finish_patch(self, result: Any) -> Any:
        self.finished.append(threading.current_thread().name)
        self._queue.clear()
        self._last = result.output
        return result

    @staticmethod
    def patch_written_note(result: Any) -> str:
        return f"{result.records} written"

    def preview_plugin(self, plugin: Path) -> None:
        self.previewed.set()


def test_the_viewer_writes_the_patch(tmp_path):
    import time

    host = _Builder(tmp_path)
    info = _post(host._on_edit_build_info)
    assert info["summary"]["records"] == 0
    assert info["folders"][0].endswith("Mods") and info["suggested"].endswith(
        "Wraithguard Patch.esp"
    )
    assert info["order"] == ["Morrowind.esm"]
    target = tmp_path / "Mods" / "Patch.esp"
    with pytest.raises(ValueError, match="Nothing is queued"):
        host._on_edit_build(json.dumps({"path": str(target)}).encode())
    _post(host._on_edit_set, tag="NPC_", id="fargoth", path="data.level", value="7")
    assert _post(host._on_edit_build_check, path=str(target))["exists"] is False
    job = _post(host._on_edit_build, path=str(target), mode="new")["job"]
    status: dict[str, Any] = {}
    for _ in range(200):
        status = _post(host._on_edit_build_status, job=job, **{"from": 0})
        if status["state"] != "running":
            break
        time.sleep(0.02)
    assert status["state"] == "done", status
    assert status["lines"] == ["reading", "writing"]
    assert status["result"]["records"] == 1 and status["result"]["note"] == "1 written"
    assert host.finished == ["fake-tk"] and not len(host._queue)
    # It is there now: a new build must say whether to append or replace.
    _post(host._on_edit_set, tag="NPC_", id="fargoth", path="data.level", value="8")
    assert _post(host._on_edit_build_check, path=str(target))["exists"] is True
    with pytest.raises(ValueError, match="Append or Replace"):
        host._on_edit_build(json.dumps({"path": str(target), "mode": "new"}).encode())
    assert _post(host._on_edit_build_info)["last"] == str(target)
    assert host._on_edit_preview_patch(json.dumps({"path": str(target)}).encode()).body == b"ok"
    assert host.previewed.wait(5)


def test_the_patch_path_is_checked(tmp_path):
    host = _Builder(tmp_path)
    for bad in ("", "relative.esp", str(tmp_path / "x.txt"), str(tmp_path / "nowhere" / "x.esp")):
        with pytest.raises(ValueError):
            host._on_edit_build_check(json.dumps({"path": bad}).encode())
    with pytest.raises(ValueError, match="No such"):
        host._on_edit_build_status(b'{"job": 9}')


def test_lua_chart_lists_functions_and_opens_charts(tmp_path):
    """The Editor's Lua panel: a script's functions to pick from, then a chart opened."""
    host = _Host(tmp_path)
    src = "local function a() end\nfunction b() a() end\nreturn { engineHandlers = { onUpdate = b } }\n"
    got = json.loads(
        host._on_edit_lua_chart(
            json.dumps({"path": "scripts/x.lua", "text": src, "kind": "functions"}).encode()
        ).body
    )
    assert [(f["name"], f["line"]) for f in got["functions"]] == [("a", 1), ("b", 2)]
    shown: list[tuple[str, str, str]] = []
    host._lua_show_chart = lambda h, c, t: shown.append((h, c, t))  # type: ignore[attr-defined]
    for kind, line in (("flow", 2), ("calls", None)):
        body = {"path": "scripts/x.lua", "text": src, "kind": kind, "line": line}
        assert json.loads(host._on_edit_lua_chart(json.dumps(body).encode()).body) == {
            "shown": True
        }
    assert shown[0][0] == "scripts/x.lua - b" and "flowchart" in shown[0][1].lower()
    assert shown[1][0].endswith("call graph")
    with pytest.raises(ValueError):
        host._on_edit_lua_chart(b'{"path": "x", "text": "", "kind": "nope"}')


def test_search_endpoint(tmp_path):
    host = _Host(tmp_path)
    got = json.loads(host._on_edit_search(b'{"tag": "NPC_", "query": "data.level=2"}').body)
    assert got["hits"] == {"fargoth": "data.level: 2"}


def test_undo_and_redo_endpoints(tmp_path):
    host = _Host(tmp_path)
    _post(host._on_edit_set, tag="NPC_", id="fargoth", path="data.level", value="7")
    before = host.refreshed
    got = _post(host._on_edit_undo)
    assert got["done"] and got["undo"] == 0 and got["redo"] == 1
    assert not host._queue.fields
    assert host.refreshed == before + 1
    assert host.ui_threads == {"fake-tk"}
    got = _post(host._on_edit_redo)
    assert got["done"] and got["undo"] == 1 and got["redo"] == 0
    assert host._queue.fields
    assert not _post(host._on_edit_redo)["done"]


def test_revision_moves_with_the_pool(tmp_path):
    host = _Host(tmp_path)
    rev0 = _post(host._on_edit_revision)["rev"]
    _post(host._on_edit_set, tag="NPC_", id="fargoth", path="data.level", value="7")
    got = _post(host._on_edit_revision)
    assert got["rev"] > rev0 and got["undo"] == 1
    # A change from elsewhere (the Patch Builder) moves it too.
    rev1 = got["rev"]
    host._queue.remove_record("Npc", "fargoth")
    assert _post(host._on_edit_revision)["rev"] > rev1


def test_set_many_endpoint(tmp_path):
    host = _Host(tmp_path)
    got = _post(
        host._on_edit_set_many, tag="NPC_", ids=["fargoth", "nobody"], path="data.level", value="5"
    )
    assert got == {"changed": 1, "failed": [], "missing": ["nobody"]}
    assert _post(host._on_edit_revision)["undo"] == 1
