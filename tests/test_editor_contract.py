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
from pathlib import Path
from typing import Any

from tests.test_editorlink import _Host, _post

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
    pattern = r'"(edit\w+)":\s*server\.register_post\(\s*"\w+",\s*self\.(\w+),?\s*\)'
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


def test_examples_answer_as_the_contract_says(tmp_path):
    host = _Host(tmp_path)
    handlers = _registered()
    for link, entry in _contract().items():
        if "example" not in entry:
            continue
        body = entry["example"]
        assert set(entry["req"]) <= set(body) <= set(entry["req"]) | set(entry["opt"]), link
        got = _post(getattr(host, handlers[link]), **body)
        want = entry["ans"]
        if want == "list":
            assert isinstance(got, list), link
        elif isinstance(want, list):
            assert isinstance(got, dict) and set(want) <= set(got), (link, sorted(got))
