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
            "NPC_": [
                {
                    "type": "Npc",
                    "id": "fargoth",
                    "name": "Fargoth",
                    "data": {"level": 2},
                    "script": "FargothScript",
                },
                {"type": "Npc", "id": "hrisskar", "name": "Hrisskar", "data": {"level": 5}},
            ],
            "TES3": [{"type": "Header", "masters": []}],
            "CELL": [
                {
                    "type": "Cell",
                    "flags": "",
                    "id": "Vault",
                    "name": "Vault",
                    "data": {"flags": "IS_INTERIOR", "grid": [0, 0]},
                    "references": [
                        {
                            "mast_index": 0,
                            "refr_index": 1,
                            "id": "fargoth",
                            "temporary": False,
                            "translation": [0.0, 0.0, 0.0],
                            "rotation": [0.0, 0.0, 0.0],
                        }
                    ],
                }
            ],
            "LEVI": [
                {
                    "type": "LeveledItem",
                    "id": "random_dagger",
                    "leveled_item_flags": "CALCULATE_FROM_ALL_LEVELS",
                    "chance_none": 0,
                    "items": [["iron dagger", 1], ["steel dagger", 5]],
                }
            ],
            "SCPT": [
                {
                    "type": "Script",
                    "id": "FargothScript",
                    "text": "begin FargothScript\r\nshort done\r\nend\r\n",
                }
            ],
            "DIAL": [{"type": "Dialogue", "id": "Background", "dialogue_type": "Topic"}],
            "INFO": [
                {
                    "type": "DialogueInfo",
                    "id": "101",
                    "prev_id": "",
                    "next_id": "102",
                    "text": "I'm Fargoth.",
                    "flags": "",
                    "speaker_id": "fargoth",
                    "script_text": "Journal MS_Q 10\r\n",
                    "data": {"disposition": 0},
                },
                {
                    "type": "DialogueInfo",
                    "id": "102",
                    "prev_id": "101",
                    "next_id": "",
                    "text": "Who knows.",
                    "flags": "",
                    "data": {"disposition": 0},
                },
            ],
        }
        self._editor_session = EditorSession(
            [("Morrowind.esm", tmp_path / "Morrowind.esm")],
            self._queue,
            journal=tmp_path / "journal.json",
            read=lambda _p, tag: records.get(tag, []),
            read_all=lambda _p: [r for recs in records.values() for r in recs],
            read_dialogue=lambda _p: records["DIAL"] + records["INFO"],
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

    def finish_patch(self, result: Any, job: Any = None) -> Any:
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


def _level(table: dict, rid: str) -> object:
    labels = [c["label"] for c in table["columns"]]
    return table["rows"][rid][labels.index("Level")]


def test_category_columns_show_what_the_patch_would_leave(tmp_path):
    """A queued edit and a record the patch makes show in the type's own columns.

    The columns used to be read from the plugins alone, so an inline edit looked lost
    (the Modified marker said it was queued, the Level column still said 2).
    """
    host = _Host(tmp_path)
    before = _post(host._on_edit_columns, tag="NPC_")
    assert _level(before, "fargoth") == 2 and _level(before, "hrisskar") == 5

    _post(host._on_edit_set, tag="NPC_", id="fargoth", path="data.level", value="7")
    after = _post(host._on_edit_columns, tag="NPC_")
    assert _level(after, "fargoth") == 7  # the queued value, not the plugin's 2
    assert _level(after, "hrisskar") == 5  # untouched records are as they were
    # The plugin's own record is not changed by the overlay (it is the cache's).
    session = host._editor_session
    assert session._records("Morrowind.esm", "NPC_")["fargoth"]["data"]["level"] == 2

    made = _post(host._on_edit_insert, tag="NPC_", newId="Newcomer")
    assert made["id"] == "Newcomer"
    again = _post(host._on_edit_columns, tag="NPC_")
    assert "newcomer" in again["rows"]  # the patch's own record has a row

    _post(host._on_edit_revert, tag="NPC_", id="fargoth", path="data.level")
    assert _level(_post(host._on_edit_columns, tag="NPC_"), "fargoth") == 2


def test_new_land_makes_the_square_and_its_flat_land(tmp_path):
    """Land for a square nothing has: its cell and a flat ``LAND`` record, one change."""
    host = _Host(tmp_path)
    out = _post(host._on_edit_new_land, cell="(40, -12)", height=100)
    assert out == {"cell": "(40, -12)", "land": True, "madeCell": True}
    session = host._editor_session
    made = {(m.record_type, m.key) for m in session.queue.new_records}
    assert made == {("Cell", "(40, -12)"), ("Landscape", "(40, -12)")}
    pend = _post(host._on_edit_pending)
    assert {(p["tag"], p["id"]) for p in pend if p.get("made")} >= {("LAND", "(40, -12)")}
    # Land is not made twice, and an interior has none.
    with pytest.raises(Exception, match="has land already"):
        _post(host._on_edit_new_land, cell="(40, -12)")
    with pytest.raises(Exception, match="exterior square"):
        _post(host._on_edit_new_land, cell="Some room")
    # One undo takes the cell and its land away together.
    _post(host._on_edit_undo)
    assert not session.queue.new_records


def test_new_land_in_a_square_the_patch_has_the_cell_of_makes_only_land(tmp_path):
    host = _Host(tmp_path)
    _post(host._on_edit_new_cell, cell="(41, -12)")
    out = _post(host._on_edit_new_land, cell="(41, -12)")
    assert out["madeCell"] is False
    assert len(host._editor_session.queue.new_records) == 2


def test_new_land_refuses_a_bad_height_and_an_unknown_texture(tmp_path):
    host = _Host(tmp_path)
    with pytest.raises(Exception, match="height"):
        _post(host._on_edit_new_land, cell="(5, 5)", height=99999999)
    with pytest.raises(Exception, match="land texture"):
        _post(host._on_edit_new_land, cell="(5, 5)", texture="nothing")
    assert not host._editor_session.queue.new_records  # nothing half made


def test_new_land_passes_verify_and_is_queued_as_the_patchs_record(tmp_path):
    """The patch's land is checked like any record it makes, and queued with its height."""
    host = _Host(tmp_path)
    _post(host._on_edit_new_land, cell="(40, -12)", height=-50, texture="")
    report = _post(host._on_edit_verify)
    assert not [p for p in report.get("problems", []) if "40, -12" in str(p)]
    session = host._editor_session
    saved = {(m.record_type, m.key) for m in session.queue.new_records}
    assert ("Landscape", "(40, -12)") in saved
    land = next(m for m in session.queue.new_records if m.record_type == "Landscape")
    assert land.record["vertex_heights"]["offset"] == -50.0


def test_new_land_and_cell_skip_the_load_order_scan_when_the_viewer_has_looked(tmp_path):
    """``noLand``/``hasCell``/``absent`` say the viewer's world was looked in: no plugin is read."""
    host = _Host(tmp_path)
    session = host._editor_session
    reads: list[str] = []
    real = session._records

    def counting(name, tag):
        reads.append(tag)
        return real(name, tag)

    session._records = counting  # type: ignore[method-assign]
    out = _post(host._on_edit_new_land, cell="(60, 60)", noLand=True, hasCell=False)
    assert out == {"cell": "(60, 60)", "land": True, "madeCell": True}
    assert "LAND" not in reads and "CELL" not in reads
    # A square the viewer says has its cell: only land is made.
    out = _post(host._on_edit_new_land, cell="(61, 60)", noLand=True, hasCell=True)
    assert out["madeCell"] is False and "CELL" not in reads
    _post(host._on_edit_new_cell, cell="Nowhere Room", absent=True)
    assert "CELL" not in reads
    # The patch's own land and cells are still refused, scan or not.
    with pytest.raises(Exception, match="has land already"):
        _post(host._on_edit_new_land, cell="(60, 60)", noLand=True, hasCell=False)
    with pytest.raises(Exception, match="already"):
        _post(host._on_edit_new_cell, cell="Nowhere Room", absent=True)
    with pytest.raises(Exception, match="hasCell"):
        _post(host._on_edit_new_land, cell="(5, 5)", hasCell="yes")
