"""Control-flow flowcharts of Lua code, as Mermaid.

The Lua sibling of ``tools/ast_mermaid.py``'s ``cfg`` diagram: the same builder idea
(``cur`` holds the dangling ``(node, edge label)`` pairs still to be wired to whatever
comes next; an empty ``cur`` means every path has left, so what follows is unreachable)
walked over :mod:`.parser`'s tree instead of Python's. Branches are diamonds, loops
feed back to their test, ``return`` goes to End, ``break`` leaves its loop, ``goto``
jumps to its label. Plain statements in a row share one box.

:func:`expr_text` is a compact unparser for the labels, and :func:`flowchart_html` wraps
diagrams in a page the bundled mermaid.js (``wraithguard/viz/assets``) renders.

Copyright (c) 2026 StaticNation.
"""

from __future__ import annotations

import html
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from wraithguard.lua.parser import Node

#: The dangling ``(node id, edge label)`` pairs still to be wired.
_Edges = list[tuple[str, str | None]]

_LABEL_MAX = 60
_CONTROL = frozenset(
    {"If", "While", "Repeat", "ForNum", "ForIn", "Do", "Return", "Break", "Goto", "Label"}
)
_SIMPLE = {"Nil": "nil", "True": "true", "False": "false", "Vararg": "..."}


def _cut(text: str, n: int = _LABEL_MAX) -> str:
    """Trim text to ``n`` characters, marking the cut.

    Args:
        text: The text.
        n: The most to keep.

    Returns:
        The text, or its start and "...".
    """
    text = " ".join(text.split())
    return text if len(text) <= n else text[: n - 3] + "..."


def _args(nodes: list[Node]) -> str:
    """Call arguments as text.

    Args:
        nodes: The argument expressions.

    Returns:
        ``a, b, c``.
    """
    return ", ".join(expr_text(a) for a in nodes)


def expr_text(node: Node) -> str:
    """An expression (or a simple statement) as compact Lua-ish text.

    Args:
        node: The node.

    Returns:
        Its text; function bodies and table contents are abbreviated.
    """
    k, v, kids = node.kind, node.value, node.children
    if k in _SIMPLE:
        return _SIMPLE[k]
    if k in ("Name", "Number"):
        return v or ""
    if k == "String":
        return repr(_cut(v or "", 24))
    if k == "Index":
        if v == ".":
            return f"{expr_text(kids[0])}.{kids[1].value}"
        return f"{expr_text(kids[0])}[{expr_text(kids[1])}]"
    if k == "Call":
        return f"{expr_text(kids[0])}({_args(kids[1:])})"
    if k == "Method":
        return f"{expr_text(kids[0])}:{v}({_args(kids[1:])})"
    if k == "Binop":
        return f"{expr_text(kids[0])} {v} {expr_text(kids[1])}"
    if k == "Unop":
        sep = " " if v == "not" else ""
        return f"{v}{sep}{expr_text(kids[0])}"
    if k == "Cast":
        return f"{expr_text(kids[0])} {v} ..."
    if k == "TypeDecl":
        return v or k
    if k == "Paren":
        return f"({expr_text(kids[0])})"
    if k == "Function":
        return "function(" + _args(kids[0].children) + ") ... end"
    if k == "Table":
        return "{...}" if kids else "{}"
    if k == "Local":
        names = ", ".join(n.value or "" for n in kids[0].children)
        vals = _args(kids[1].children)
        return f"local {names} = {vals}" if vals else f"local {names}"
    if k == "Assign":
        return f"{_args(kids[0].children)} = {_args(kids[1].children)}"
    if k == "CallStat":
        return expr_text(kids[0])
    if k == "LocalFunction":
        return f"local function {v}(" + _args(kids[0].children[0].children) + ")"
    if k == "FunctionStat":
        return f"function {v}(" + _args(kids[0].children[0].children) + ")"
    return k


class _Builder:
    """Builds one control-flow graph (see the module docstring)."""

    def __init__(self) -> None:
        """Start with Start and End."""
        self.nodes: dict[str, tuple[str, str]] = {}
        self.edges: list[tuple[str, str, str | None]] = []
        self.loops: list[dict[str, _Edges]] = []
        self.labels: dict[str, str] = {}
        self.gotos: list[tuple[str, str]] = []
        self.start = self.add("stadium", "Start")
        self.end = self.add("stadium", "End")

    def add(self, shape: str, label: str) -> str:
        """Add a node.

        Args:
            shape: ``rect``, ``diamond`` or ``stadium``.
            label: Its text.

        Returns:
            Its id.
        """
        nid = f"n{len(self.nodes) + 1}"
        self.nodes[nid] = (shape, label)
        return nid

    def link(self, cur: _Edges, to: str) -> None:
        """Wire every dangling predecessor to a node.

        Args:
            cur: The predecessors.
            to: The node.
        """
        self.edges.extend((src, to, label) for src, label in cur)

    def loop(self, test: str, body: Node, cur: _Edges, yes: str, no: str) -> _Edges:
        """A loop whose test comes first (``while``, both ``for``s).

        Args:
            test: The test's label.
            body: The loop's block.
            cur: The predecessors.
            yes: The edge label into the body.
            no: The edge label out of the loop.

        Returns:
            The loop's exits.
        """
        cond = self.add("diamond", test)
        self.link(cur, cond)
        self.loops.append({"breaks": []})
        self.link(self.block(body, [(cond, yes)]), cond)
        breaks = self.loops.pop()["breaks"]
        return [(cond, no), *breaks]

    def block(self, block: Node, cur: _Edges) -> _Edges:
        """Wire a block's statements.

        Args:
            block: A ``Block`` node.
            cur: Its predecessors.

        Returns:
            Its exits (empty when every path left).
        """
        buf: list[str] = []

        def flush() -> None:
            """Put the buffered plain statements in one box."""
            nonlocal cur
            if buf:
                nid = self.add("rect", "\n".join(buf))
                self.link(cur, nid)
                cur = [(nid, None)]
                buf.clear()

        for st in block.children:
            if not cur and st.kind != "Label":
                break  # unreachable
            if st.kind not in _CONTROL:
                buf.append(_cut(expr_text(st)))
                continue
            flush()
            cur = self.statement(st, cur)
        flush()
        return cur

    def statement(self, st: Node, cur: _Edges) -> _Edges:
        """Wire one control statement.

        Args:
            st: The statement.
            cur: Its predecessors.

        Returns:
            Its exits.
        """
        k, kids = st.kind, st.children
        if k == "If":
            out: _Edges = []
            for part in kids:
                if part.kind == "Else":
                    out += self.block(part.children[0], cur)
                    cur = []
                    break
                cond = self.add("diamond", "if " + _cut(expr_text(part.children[0])))
                self.link(cur, cond)
                out += self.block(part.children[1], [(cond, "yes")])
                cur = [(cond, "no")]
            return out + cur
        if k == "While":
            return self.loop("while " + _cut(expr_text(kids[0])), kids[1], cur, "true", "false")
        if k == "ForNum":
            rng = ", ".join(expr_text(e) for e in kids[1:-1])
            return self.loop(f"for {kids[0].value} = {_cut(rng)}", kids[-1], cur, "next", "done")
        if k == "ForIn":
            names = ", ".join(n.value or "" for n in kids[0].children)
            its = _cut(_args(kids[1].children))
            return self.loop(f"for {names} in {its}", kids[2], cur, "next", "done")
        if k == "Repeat":
            top = self.add("rect", "repeat")
            self.link(cur, top)
            self.loops.append({"breaks": []})
            body = self.block(kids[0], [(top, None)])
            cond = self.add("diamond", "until " + _cut(expr_text(kids[1])))
            self.link(body, cond)
            self.edges.append((cond, top, "false"))
            return [(cond, "true"), *self.loops.pop()["breaks"]]
        if k == "Do":
            return self.block(kids[0], cur)
        if k == "Return":
            vals = _args(kids[0].children)
            nid = self.add("rect", _cut("return " + vals if vals else "return"))
            self.link(cur, nid)
            self.edges.append((nid, self.end, None))
            return []
        if k == "Break":
            nid = self.add("rect", "break")
            self.link(cur, nid)
            if self.loops:
                self.loops[-1]["breaks"].append((nid, None))
            return []
        if k == "Goto":
            nid = self.add("rect", f"goto {st.value}")
            self.link(cur, nid)
            self.gotos.append((nid, st.value or ""))
            return []
        # Label
        nid = self.add("rect", f"::{st.value}::")
        self.link(cur, nid)
        self.labels[st.value or ""] = nid
        return [(nid, None)]

    def build(self, body: Node) -> str:
        """Chart a block and render it.

        Args:
            body: The block (a function's body, or a whole chunk).

        Returns:
            The Mermaid flowchart.
        """
        self.link(self.block(body, [(self.start, None)]), self.end)
        for src, label in self.gotos:
            if label in self.labels:
                self.edges.append((src, self.labels[label], None))
        return render(self.nodes, self.edges)


def _esc(text: str) -> str:
    """Text for a quoted Mermaid label.

    Args:
        text: The label.

    Returns:
        It, with quotes and line breaks made safe.
    """
    return html.escape(text, quote=False).replace('"', "#quot;").replace("\n", "<br/>")


def render(nodes: dict[str, tuple[str, str]], edges: list[tuple[str, str, str | None]]) -> str:
    """A graph as a Mermaid flowchart.

    Args:
        nodes: Id -> (shape, label).
        edges: (from, to, label or None).

    Returns:
        The flowchart text.
    """
    lines = ["flowchart TD"]
    for nid, (shape, label) in nodes.items():
        t = _esc(label)
        if shape == "diamond":
            lines.append(f'  {nid}{{"{t}"}}')
        elif shape == "stadium":
            lines.append(f'  {nid}(["{t}"])')
        else:
            lines.append(f'  {nid}["{t}"]')
    for src, dst, edge in edges:
        lines.append(f"  {src} -->|{_esc(edge)}| {dst}" if edge else f"  {src} --> {dst}")
    return "\n".join(lines)


def flowchart(node: Node) -> str:
    """The control flow of a function, or of a whole chunk.

    Args:
        node: A ``Function``, ``LocalFunction`` or ``FunctionStat`` node, or a ``Block``.

    Returns:
        The Mermaid flowchart.
    """
    if node.kind in ("LocalFunction", "FunctionStat"):
        node = node.children[0]
    body = node.children[1] if node.kind == "Function" else node
    return _Builder().build(body)


#: The name mermaid.js is published under beside a :func:`flowchart_html` page.
MERMAID_JS = "mermaid.js"


def _inline_script(source: str) -> str:
    r"""A script's source made safe to sit inside a ``<script>`` element.

    ``</script`` would end the element and ``<!--`` can switch the HTML parser into
    its escaped script state; both only occur in mermaid.js's strings and regular
    expressions, where ``\/`` and ``\x21`` mean the same characters.

    Args:
        source: The JavaScript.

    Returns:
        It, safe to inline.
    """
    return source.replace("</script", "<\\/script").replace("<!--", "<\\x21--")


def flowchart_html(
    title: str,
    charts: list[tuple[str, str]],
    *,
    library: str | None = None,
    library_url: str = MERMAID_JS,
) -> str:
    """A page that renders Mermaid charts with the bundled mermaid.js.

    Args:
        title: The page title.
        charts: ``(heading, mermaid text)`` pairs.
        library: mermaid.js's source, to inline it (a page opened from disk).
        library_url: Where to load mermaid.js from when it is not inlined (its URL
            on the loopback server).

    Returns:
        The HTML.
    """
    cards = "\n".join(
        f"<section><h2>{html.escape(h)}</h2><pre class='mermaid'>{html.escape(body)}</pre>"
        "</section>"
        for h, body in charts
    )
    loader = (
        f"<script>{_inline_script(library)}</script>"
        if library is not None
        else f"<script src='{html.escape(library_url)}'></script>"
    )
    return (
        "<!DOCTYPE html><html lang='en'><head><meta charset='utf-8'>"
        f"<title>{html.escape(title)}</title><style>"
        "body{font-family:system-ui,sans-serif;margin:0;background:#141416;color:#e6e6e6}"
        "h1{font-size:17px;padding:14px 24px;margin:0;background:#1e1e1e}"
        "section{margin:20px;padding:12px;background:#1e1e1e;border-radius:8px}"
        "h2{font-size:14px;margin:0 0 10px}.mermaid svg{background:transparent!important}"
        f"</style></head><body><h1>{html.escape(title)}</h1>{cards}{loader}"
        # Without the library (a broken build) the charts stay as their text.
        "<script>if(window.mermaid)mermaid.initialize("
        "{startOnLoad:true,theme:'dark',securityLevel:'strict',maxTextSize:500000});"
        "</script></body></html>"
    )
