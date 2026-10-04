"""Call graphs of Lua scripts, as Mermaid: which of a script's functions calls which.

The Lua sibling of ``tools/ast_mermaid.py``'s ``calls`` diagram, one script at a time.
The functions are the ones the script defines and names: ``local function f``,
``function M.f`` / ``M:f``, ``local f = function``, ``M.f = function``, and functions in
table constructors, named by where the table sits (``engineHandlers.onUpdate`` in the
table the script returns). Anonymous functions - callbacks passed to ``time.runRepeatedly``,
``async:callback`` and the like - are part of the function they are written in.

Only calls this can resolve are drawn: to a name, or a constant ``a.b`` chain, that the
script defines (``M:f()`` as ``M.f``); ``self:f()`` when exactly one defined
``*.f``/``*:f`` matches. Calls into OpenMW's packages and other modules are not drawn - the chart is
about the script's own structure. The handlers the script hands to OpenMW (the fields
of the table it returns) are the entry points, drawn as stadiums; a handler that names
a function defined elsewhere in the script links to it.

Copyright (c) 2026 StaticNation.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import TYPE_CHECKING

from wraithguard.lua.analysis import _dotted
from wraithguard.lua.flowchart import render

if TYPE_CHECKING:
    from wraithguard.lua.parser import Node

#: The node for code at the top level of the chunk.
MAIN = "(main chunk)"


@dataclass
class CallGraph:
    """A script's functions and the calls between them.

    Attributes:
        functions: Name -> its ``Function`` node, in the order they are defined.
        entries: The names handed to OpenMW (fields of the returned table).
        calls: ``(caller, callee)`` pairs, in first-seen order; the caller may be
            :data:`MAIN`.
    """

    functions: dict[str, Node] = field(default_factory=dict)
    entries: list[str] = field(default_factory=list)
    calls: list[tuple[str, str]] = field(default_factory=list)


def _key(name: str) -> str:
    """A name as lookups compare it: ``M:f`` is ``M.f``.

    Args:
        name: The name as written.

    Returns:
        The lookup key.
    """
    return name.replace(":", ".")


class _Collector:
    """Finds the named functions and the entry points."""

    def __init__(self) -> None:
        """Start empty."""
        self.functions: dict[str, Node] = {}
        self.names: dict[int, str] = {}  # id(Function node) -> its name
        self.entries: list[str] = []
        self.aliases: list[tuple[str, Node]] = []  # (entry name, value naming a function)

    def name(self, fn: Node, name: str) -> None:
        """Record a named function (the first definition of a name keeps it).

        Args:
            fn: The ``Function`` node.
            name: Its name.
        """
        self.names[id(fn)] = name
        self.functions.setdefault(_key(name), fn)

    def chunk(self, chunk: Node) -> None:
        """Collect from a whole chunk.

        Args:
            chunk: The parsed chunk.
        """
        for stmt in chunk.children:
            if stmt.kind == "Return" and stmt.children and stmt.children[0].children:
                exported = stmt.children[0].children[0]
                if exported.kind == "Table":
                    self.table(exported, "", exported_table=True)
                    continue
            self.visit(stmt)

    def visit(self, node: Node) -> None:
        """Collect from a node and everything under it.

        Args:
            node: Any node.
        """
        kind = node.kind
        if kind in ("LocalFunction", "FunctionStat") and node.value:
            self.name(node.children[0], node.value)
        elif kind in ("Local", "Assign"):
            targets, exprs = node.children
            tables: set[int] = set()
            for target, expr in zip(targets.children, exprs.children):
                name = target.value if kind == "Local" else _dotted(target, {})
                if not name:
                    continue
                if expr.kind == "Function":
                    self.name(expr, name)
                elif expr.kind == "Table":
                    self.table(expr, name)
                    tables.add(id(expr))
            for child in (*targets.children, *exprs.children):
                if id(child) not in tables:  # table() has been through those
                    self.visit(child)
            return
        if kind == "Table":
            self.table(node, None)
            return
        for child in node.children:
            self.visit(child)

    def table(self, table: Node, path: str | None, exported_table: bool = False) -> None:
        """Name the functions in a table constructor by the table's place.

        Args:
            table: The ``Table`` node.
            path: Where it is bound (``M``, ``engineHandlers``), "" for the returned
                table itself, or None for a table with no name.
            exported_table: Whether it is (inside) the table the script returns.
        """
        for item in table.children:
            value = item.children[-1]
            key = item.children[0] if item.kind == "Field" else None
            if key is None or key.kind != "String" or not key.value:
                self.visit(value)
                continue
            name = f"{path}.{key.value}" if path else key.value
            if value.kind == "Function":
                if id(value) not in self.names:
                    self.name(value, name)
                if exported_table:
                    self.entries.append(name)
                self.visit(value)
            elif value.kind == "Table":
                self.table(value, name if path is not None else None, exported_table)
            else:
                if exported_table and _dotted(value, {}):
                    self.aliases.append((name, value))
                self.visit(value)


class _Calls:
    """Finds the calls each function makes."""

    def __init__(self, collected: _Collector) -> None:
        """Start from the collected functions.

        Args:
            collected: The named functions.
        """
        self.c = collected
        self.calls: dict[tuple[str, str], None] = {}

    def resolve(self, node: Node) -> str | None:
        """The defined function a call goes to.

        Args:
            node: A ``Call`` or ``Method`` node.

        Returns:
            Its lookup key, or None when it is not one of the script's functions.
        """
        funcs = self.c.functions
        if node.kind == "Call":
            name = _dotted(node.children[0], {})
            return _key(name) if name and _key(name) in funcs else None
        base = _dotted(node.children[0], {})
        method = node.value or ""
        if base and base != "self":
            return f"{base}.{method}" if f"{base}.{method}" in funcs else None
        matches = [k for k in funcs if k.rsplit(".", 1)[-1] == method and "." in k]
        return matches[0] if len(matches) == 1 else None

    def walk(self, caller: str, node: Node) -> None:
        """Record the calls under a node, made by ``caller``.

        Named functions inside are not the caller's: they are walked as their own.

        Args:
            caller: The calling function's key, or :data:`MAIN`.
            node: Where to look.
        """
        stack = list(reversed(node.children))
        while stack:
            n = stack.pop()
            if n.kind == "Function" and id(n) in self.c.names:
                continue
            if n.kind in ("Call", "Method"):
                callee = self.resolve(n)
                if callee is not None:
                    self.calls.setdefault((caller, callee), None)
            stack.extend(reversed(n.children))


def call_graph(chunk: Node) -> CallGraph:
    """The functions a script defines and the calls between them.

    Args:
        chunk: The parsed chunk.

    Returns:
        Its call graph.
    """
    collected = _Collector()
    collected.chunk(chunk)
    calls = _Calls(collected)
    calls.walk(MAIN, chunk)
    for key, fn in collected.functions.items():
        calls.walk(key, fn)
    graph = CallGraph(dict(collected.functions), list(dict.fromkeys(collected.entries)))
    for name, value in collected.aliases:
        target = _dotted(value, {})
        if target and _key(target) in collected.functions:
            graph.entries.append(name)
            calls.calls.setdefault((name, _key(target)), None)
    graph.calls = list(calls.calls)
    return graph


def call_graph_chart(chunk: Node) -> str:
    """A script's call graph as a Mermaid flowchart.

    Args:
        chunk: The parsed chunk.

    Returns:
        The flowchart text (left to right).
    """
    graph = call_graph(chunk)
    ids: dict[str, str] = {}
    nodes: dict[str, tuple[str, str]] = {}

    def node_id(name: str) -> str:
        """A function's chart id, adding its node the first time.

        Args:
            name: The function's key, or :data:`MAIN`.

        Returns:
            Its id.
        """
        if name not in ids:
            ids[name] = f"n{len(ids)}"
            fn = graph.functions.get(name)
            label = f"{name}\nline {fn.line}" if fn is not None else name
            shape = "stadium" if name in graph.entries or name == MAIN else "rect"
            nodes[ids[name]] = (shape, label)
        return ids[name]

    if any(caller == MAIN for caller, _ in graph.calls):
        node_id(MAIN)
    for name in graph.entries:
        node_id(name)
    for name in graph.functions:
        node_id(name)
    edges: list[tuple[str, str, str | None]] = [
        (node_id(a), node_id(b), None) for a, b in graph.calls
    ]
    return render(nodes, edges).replace("flowchart TD", "flowchart LR", 1)


__all__ = ["MAIN", "CallGraph", "call_graph", "call_graph_chart"]
