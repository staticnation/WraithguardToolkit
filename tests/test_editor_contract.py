"""The Editor page / Wraithguard contract (``viewer-shell/ui/tests/editor/contract.json``).

Each link the page asks: the request keys it needs and may send, and what it answers.
The page's own tests check every request they make and every stand-in answer against
the file (``harness.js``); these check the file against Wraithguard - the handlers'
documented bodies, and real answers for the links with an example.
"""

from __future__ import annotations

import ast
import json
import re
import sys
import time
from pathlib import Path
from typing import Any

from tests.test_editorlink import _Builder, _post
from wraithguard.patch import testrun

ROOT = Path(__file__).resolve().parent.parent
CONTRACT = ROOT / "viewer-shell/ui/tests/editor/contract.json"
LINK_SOURCE = ROOT / "wraithguard/gui/editorlink.py"


def _contract() -> dict[str, dict[str, Any]]:
    """The contract's links."""
    links: dict[str, dict[str, Any]] = json.loads(CONTRACT.read_text(encoding="utf-8"))["links"]
    return links


def _registered() -> dict[str, str]:
    """Link name -> its handler's name, as ``editor_links`` registers them."""
    src = LINK_SOURCE.read_text(encoding="utf-8")
    # `reg`: register_post with the shared journal read again first (`_synced`).
    pattern = r'"(edit\w+)":\s*reg\(\s*"\w+",\s*self\.(\w+),?\s*\)'
    return dict(re.findall(pattern, src))


def documented(handler: str) -> tuple[list[str], list[str]]:
    """The request keys a handler's docstring gives: ``(needed, optional)``.

    Its ``Args`` give the body as ``{a, b, c?}`` (``?``: may be left out); where it
    gives alternatives (``{cell, points, edges}`` or ``{cell, revert}``), a key is
    needed when every alternative has it.
    """
    tree = ast.parse(LINK_SOURCE.read_text(encoding="utf-8"))
    fn = next(n for n in ast.walk(tree) if isinstance(n, ast.FunctionDef) and n.name == handler)
    doc = ast.get_docstring(fn) or ""
    args = doc.split("Args:", 1)[1].split("Returns:", 1)[0] if "Args:" in doc else ""
    shapes: list[tuple[list[str], list[str]]] = []
    for body in re.findall(r"``(\{.*?\})``", args, re.S):
        inner = re.sub(r"\[.*?\]", "", body[1:-1].replace("\n", " "))
        need: list[str] = []
        may: list[str] = []
        for part in (p.strip() for p in inner.split(",")):
            key = part.split(":")[0].strip()
            if key:
                (may if key.endswith("?") else need).append(key.rstrip("?"))
        shapes.append((need, may))
    assert shapes, f"{handler} does not document its request body"
    needed = [k for k in shapes[0][0] if all(k in s[0] for s in shapes)]
    every = [k for s in shapes for k in (*s[0], *s[1])]
    optional = [k for k in dict.fromkeys(every) if k not in needed]
    return needed, optional


def test_every_registered_link_is_in_the_contract():
    assert sorted(_registered()) == sorted(_contract())


def test_the_contract_asks_what_each_handler_documents():
    links = _contract()
    for link, handler in _registered().items():
        need, may = documented(handler)
        assert (links[link]["req"], links[link]["opt"]) == (need, may), link


#: When an example runs, for those that need another's effect: a reference placed first
#: (``$placed``), and the build and test run last, as a build empties the pool (editSet
#: before them, after undo and revert, so there is something to write).
PHASE: dict[str, int] = {
    "editNew": 1,
    "editNewSet": 1,
    "editNewRemove": 2,
    "editSet": 2,
    "editTestRun": 3,  # keeps the pool
    "editBuild": 4,
    "editBuildStatus": 5,
    "editPreviewPatch": 5,
}


def _fill(value: Any, slots: dict[str, Any]) -> Any:
    """An example's body with ``$tmp``, ``$placed`` and ``$job`` filled in."""
    if isinstance(value, dict):
        return {k: _fill(v, slots) for k, v in value.items()}
    if isinstance(value, list):
        return [_fill(v, slots) for v in value]
    if isinstance(value, str) and value.startswith("$"):
        name, _, rest = value[1:].partition("/")
        return f"{slots[name]}/{rest}" if rest else slots[name]
    return value


def test_examples_answer_as_the_contract_says(tmp_path, monkeypatch):
    monkeypatch.setattr(testrun, "scratch_dir", lambda: tmp_path)
    monkeypatch.setattr(testrun, "_remembered_file", lambda: tmp_path / "openmw_test.json")
    # The "game" is this Python: it refuses OpenMW's arguments and exits.
    monkeypatch.setattr(testrun, "find_openmw", lambda _given=None: Path(sys.executable))
    host = _Builder(tmp_path)
    handlers = _registered()
    slots: dict[str, Any] = {"tmp": tmp_path.as_posix()}
    links = sorted(_contract().items(), key=lambda kv: PHASE.get(kv[0], 0))
    for link, entry in links:
        if "example" not in entry:
            continue
        runs = [(entry["example"], entry["ans"])]
        if "alt" in entry:
            runs.append((entry["alt_example"], entry["alt"]))
        for example, want in runs:
            assert set(entry["req"]) <= set(example) <= set(entry["req"]) | set(entry["opt"]), link
            body = json.dumps(_fill(example, slots)).encode("utf-8")
            raw = getattr(host, handlers[link])(body).body
            if want == "ok":
                assert raw == b"ok", link
                continue
            got = json.loads(raw)
            if want == "list":
                assert isinstance(got, list), link
            elif isinstance(want, list):
                assert isinstance(got, dict) and set(want) <= set(got), (link, sorted(got))
            if link == "editPlace":
                slots["placed"] = got["uid"]
            if isinstance(got, dict) and "job" in got:
                slots["job"] = got["job"]
                _wait(host, got["job"])


def _wait(host: _Builder, job: int) -> None:
    """Wait for a build or test run to finish (it must, for the next example)."""
    for _ in range(300):
        state = _post(host._on_edit_build_status, job=job, **{"from": 0})
        if state["state"] != "running":
            assert state["state"] == "done", state
            return
        time.sleep(0.02)
    raise AssertionError(f"job {job} did not finish")


def test_every_links_answer_is_pinned_with_an_example():
    links = _contract().items()
    assert [link for link, entry in links if entry["ans"] is None or "example" not in entry] == []
