"""The Editor's links as the page asks them: the shared link cases
(``tests/fixtures/link_cases.json``, which ``viewer-shell/editcore/tests/cases.rs`` runs
through ``editcore::links`` too) through :class:`~wraithguard.gui.editorlink.EditorLinkMixin`'s
handlers - the same JSON in and out, whichever side answers.

A case is a load order (each plugin's masters and records) and operations:
``["link", name, body]`` (the answer, or ``{error}`` with the refusal) and ``["journal",
document]`` (the shared journal written as the other side writes it: ``null`` removes
it). A new reference's editor name and a new response's id are random, so the cases name
each by a label (``U1``..., ``R1``...), in requests and answers alike. ``rev`` (how far the queue has changed) is
each side's own count and is left out.
"""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any

import pytest

pytest.importorskip("wraithguard_native")

from wraithguard.gui.editorlink import EditorLinkMixin
from wraithguard.patch.editor import TAG_TO_TYPE, EditorSession, queue_journal
from wraithguard.patch.queue import PatchQueue

CASES: list[dict[str, Any]] = json.loads(
    (Path(__file__).parent / "fixtures" / "link_cases.json").read_text(encoding="utf-8")
)["cases"]


class _Host(EditorLinkMixin):
    """Wraithguard's half with no window: the Tk thread is the caller's own."""

    def __init__(self, session: EditorSession) -> None:
        self._editor_session = session

    def _schedule_ui(self, _delay: int, fn: Any, *args: Any) -> None:
        fn(*args)

    def patch_queue(self) -> PatchQueue:
        return self._editor_session.queue

    def refresh_patch_views(self) -> None:
        pass  # the session journals every change itself

    def show_patch_builder(self) -> None:
        pass


def _swap(value: Any, names: dict[str, str]) -> Any:
    """``value`` with each string ``names`` has swapped."""
    if isinstance(value, str):
        return names.get(value, value)
    if isinstance(value, list):
        return [_swap(v, names) for v in value]
    if isinstance(value, dict):
        return {k: _swap(v, names) for k, v in value.items()}
    return value


def _handler(host: _Host, link: str) -> Any:
    """The handler a link is registered with, as ``editor_links`` wraps it."""
    import re

    src = Path(__file__).resolve().parent.parent / "wraithguard/gui/editorlink.py"
    found = re.search(rf'"{link}":\s*reg\(\s*"\w+",\s*self\.(\w+)', src.read_text(encoding="utf-8"))
    assert found, link
    return host._synced(getattr(host, found.group(1)))


def run(case: dict[str, Any], tmp: Path) -> dict[str, Any]:
    """A case's operations: what each gave, and the journal after them."""
    by_plugin = {p["name"]: p for p in case["plugins"]}

    def read(path: Path, tag: str) -> list[dict[str, Any]]:
        """One plugin's records of one tag."""
        p = by_plugin[path.name]
        if tag == "TES3":
            return [{"type": "Header", "masters": [[m, 0] for m in p.get("masters", [])]}]
        want = TAG_TO_TYPE.get(tag)
        return [json.loads(json.dumps(r)) for r in p["records"] if r["type"] == want]

    def read_all(path: Path) -> list[dict[str, Any]]:
        """All of one plugin's records, in file order."""
        p = by_plugin[path.name]
        head = {"type": "Header", "masters": [[m, 0] for m in p.get("masters", [])]}
        return [head, *(json.loads(json.dumps(r)) for r in p["records"])]

    def read_dialogue(path: Path) -> list[dict[str, Any]]:
        """One plugin's topics and responses, in file order."""
        return [r for r in read_all(path) if r["type"] in ("Dialogue", "DialogueInfo")]

    journal = tmp / "journal.json"
    session = EditorSession(
        [(p["name"], tmp / p["name"]) for p in case["plugins"]],
        PatchQueue(),
        journal,
        read=read,
        read_all=read_all,
        read_dialogue=read_dialogue,
    )
    host = _Host(session)
    labels: dict[str, str] = {}  # uid -> label
    results: list[Any] = []
    for op in case["ops"]:
        if op[0] == "journal":
            if op[1] is None:
                journal.unlink(missing_ok=True)
            else:
                journal.write_text(
                    json.dumps(_swap(op[1], {v: k for k, v in labels.items()})), encoding="utf-8"
                )
            results.append("written")
            continue
        _, link, body = op
        body = _swap(body, {v: k for k, v in labels.items()})
        try:
            payload = _handler(host, link)(json.dumps(body).encode("utf-8"))
            text = payload.body.decode("utf-8")
            try:
                out: Any = json.loads(text)
            except ValueError:
                out = text
            if link == "editRevision":
                out.pop("rev", None)
        except ValueError as exc:
            out = {"error": str(exc)}
        for n in session.queue.new_refs:
            if n.uid not in labels:
                labels[n.uid] = f"U{sum(v.startswith('U') for v in labels.values()) + 1}"
        for m in session.queue.new_records:
            if m.record_type == "DialogueInfo" and m.key not in labels:
                labels[m.key] = f"R{sum(v.startswith('R') for v in labels.values()) + 1}"
        results.append(out)
    return _swap({"results": results, "journal": queue_journal(session.queue)}, labels)


@pytest.mark.parametrize("case", CASES, ids=[c["name"] for c in CASES])
def test_case(tmp_path: Path, case: dict[str, Any]) -> None:
    assert run(case, tmp_path) == case["expect"]


def test_wraithguard_hands_the_viewer_the_links_it_answers() -> None:
    """``VIEWER_LINKS`` is ``editcore::links::ANSWERS``: a link neither side serves would
    fail only in the app, and one both serve would split the queue's history."""
    import re

    from wraithguard.gui.editorlink import VIEWER_LINKS

    src = Path(__file__).resolve().parent.parent / "viewer-shell/editcore/src/links.rs"
    text = src.read_text(encoding="utf-8")
    block = text.split("pub const ANSWERS: &[&str] = &[", 1)[1].split("];", 1)[0]
    assert set(re.findall(r'"(edit\w+)"', block)) == VIEWER_LINKS
    kept = text.split("pub const WRAITHGUARDS: &[&str] = &[", 1)[1].split("];", 1)[0]
    assert not set(re.findall(r'"(edit\w+)"', kept)) & VIEWER_LINKS
