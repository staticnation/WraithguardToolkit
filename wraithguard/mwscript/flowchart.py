"""A Morrowind script's control flow as a Mermaid flowchart.

The Lua tools draw a function's flow (:mod:`wraithguard.lua.flowchart`); this draws a
Morrowind script's the same way, in the same style: runs of plain statements as boxes,
``if``/``elseif``/``else`` and ``while`` as decisions, ``return`` ending the frame. A
script runs from ``begin`` to ``end`` every frame, so the chart's end is "the next frame".

Read a line at a time, as the compiler reads blocks: what a line starts with decides
what it is, and a block that is never closed is closed at the end of the script.

Copyright (c) 2026 StaticNation.
"""

from __future__ import annotations

import re
from dataclasses import dataclass, field
from typing import Any

from wraithguard.lua.flowchart import render

#: Statements a box holds before the next one starts.
_RUN_MAX = 6
#: Characters of a line kept in a label.
_LABEL_MAX = 60

_HEAD = re.compile(r"^\s*([A-Za-z_]+)\b(.*)$")


def _code(line: str) -> str:
    """A line without its comment (``;`` outside quotes), stripped."""
    quoted = False
    for i, ch in enumerate(line):
        if ch == '"':
            quoted = not quoted
        elif ch == ";" and not quoted:
            return line[:i].strip()
    return line.strip()


def _cond(rest: str) -> str:
    """An ``if``/``while`` line's condition, without its outer brackets."""
    text = rest.strip()
    if text.startswith("(") and text.endswith(")"):
        text = text[1:-1].strip()
    return text


def _cut(text: str) -> str:
    """A label cut to the chart's width, with an ellipsis."""
    return text if len(text) <= _LABEL_MAX else text[: _LABEL_MAX - 1] + "…"


@dataclass
class _Block:
    """A run of items.

    ``("stmt", text)``, ``["if", arms, else]``, ``("while", cond, body)``, ``("return",)``.
    """

    items: list[Any] = field(default_factory=list)


def parse(text: str) -> tuple[str, _Block]:
    """The script's name and its body as nested blocks."""
    name = ""
    root = _Block()
    # The stack of open blocks: (kind, block, the if's arms or the while's item).
    stack: list[tuple[str, _Block, Any]] = [("root", root, None)]
    for raw in text.splitlines():
        line = _code(raw)
        if not line:
            continue
        m = _HEAD.match(line)
        word = m.group(1).lower() if m else ""
        rest = m.group(2) if m else ""
        cur = stack[-1][1]
        if word == "begin" and not name:
            name = rest.strip()
        elif word == "end":
            break
        elif word == "if":
            body = _Block()
            arms: list[tuple[str, _Block]] = [(_cond(rest), body)]
            item: list[Any] = ["if", arms, None]
            cur.items.append(item)
            stack.append(("if", body, item))
        elif word in ("elseif", "else") and stack[-1][0] == "if":
            _kind, _body, item = stack.pop()
            body = _Block()
            if word == "elseif":
                item[1].append((_cond(rest), body))
            else:
                item[2] = body
            stack.append(("if", body, item))
        elif word == "endif" and stack[-1][0] == "if":
            stack.pop()
        elif word == "while":
            body = _Block()
            cur.items.append(("while", _cond(rest), body))
            stack.append(("while", body, None))
        elif word == "endwhile" and stack[-1][0] == "while":
            stack.pop()
        elif word == "return":
            cur.items.append(("return",))
        elif word in ("short", "long", "float"):
            continue
        else:
            cur.items.append(("stmt", line))
    return name, root


class _Graph:
    """Nodes and edges, as :func:`wraithguard.lua.flowchart.render` takes them."""

    def __init__(self) -> None:
        """Start an empty chart."""
        self.nodes: dict[str, tuple[str, str]] = {}
        self.edges: list[tuple[str, str, str | None]] = []
        self._n = 0

    def node(self, shape: str, label: str) -> str:
        """Add a node; its id."""
        self._n += 1
        nid = f"n{self._n}"
        self.nodes[nid] = (shape, label)
        return nid

    def link(self, exits: list[tuple[str, str | None]], dst: str) -> None:
        """Join every open exit to ``dst``."""
        for src, label in exits:
            self.edges.append((src, dst, label))

    def block(
        self, blk: _Block, exits: list[tuple[str, str | None]], end: str
    ) -> list[tuple[str, str | None]]:
        """Draw a block after ``exits``.

        Returns:
            The block's own exits (empty when it always returns).
        """
        run: list[str] = []

        def flush() -> None:
            """Write the straight run of statements gathered so far as nodes."""
            nonlocal exits
            while run:
                chunk = run[:_RUN_MAX]
                del run[:_RUN_MAX]
                nid = self.node("box", "\n".join(_cut(s) for s in chunk))
                self.link(exits, nid)
                exits = [(nid, None)]

        for item in blk.items:
            kind = item[0]
            if kind == "stmt":
                run.append(item[1])
                continue
            flush()
            if kind == "return":
                nid = self.node("stadium", "return")
                self.link(exits, nid)
                self.edges.append((nid, end, None))
                return []
            if kind == "while":
                test = self.node("diamond", "while " + _cut(item[1]))
                self.link(exits, test)
                back = self.block(item[2], [(test, "yes")], end)
                self.link(back, test)
                exits = [(test, "no")]
                continue
            # if / elseif / else
            arms, otherwise = item[1], item[2]
            out: list[tuple[str, str | None]] = []
            come: list[tuple[str, str | None]] = exits
            for i, (cond, body) in enumerate(arms):
                test = self.node("diamond", ("if " if i == 0 else "elseif ") + _cut(cond))
                self.link(come, test)
                out += self.block(body, [(test, "yes")], end)
                come = [(test, "no")]
            if otherwise is not None:
                out += self.block(otherwise, come, end)
            else:
                out += come
            exits = out
        flush()
        return exits


def flowchart(text: str) -> str:
    """The Mermaid flowchart of a Morrowind script's source."""
    name, root = parse(text)
    g = _Graph()
    start = g.node("stadium", f"begin {name}".strip())
    end = g.node("stadium", "end (next frame)")
    exits = g.block(root, [(start, None)], end)
    g.link(exits, end)
    return render(g.nodes, g.edges)
