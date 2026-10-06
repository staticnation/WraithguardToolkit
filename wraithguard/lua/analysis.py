"""What one OpenMW Lua script declares, and what it does every frame.

An OpenMW script returns a table: ``interfaceName`` and ``interface`` (what it offers
other scripts), ``engineHandlers`` (``onUpdate``, ``onActive``, ...) and
``eventHandlers``. :func:`analyze` finds those - in the returned table constructor, or
in a local table the script fills and returns - and checks them against the API
rules in :mod:`.api` for the contexts the script runs in. It also follows the
per-frame handlers (``onUpdate``, ``onFrame``) into the script's own functions and
reports loops over large collections (``nearby.actors``, ``world.activeActors``,
``types.*.records``, ``cell:getAll()``) and other costly calls made every frame.

The same walk reports the garbage a per-frame handler makes - a table constructor, a
closure, a string built or a vector made inside a loop (``GC_*``): OpenMW tracks every
script's memory and runs one collector for the Lua state, which pauses the scripts, so
garbage made every frame costs every frame. And anything OpenMW's sandbox leaves out of
Lua (``collectgarbage``, ``loadstring``, ``io``, ``os.clock``, writing into ``string``)
is an error: the script fails there when it runs (``SANDBOX``).

These are static checks: a script that throttles its own ``onUpdate`` (counts time
and returns early) is still reported, and the message says so.

Copyright (c) 2026 StaticNation.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any

from wraithguard.lua.api import API, ApiVersion
from wraithguard.lua.lexer import LuaSyntaxError
from wraithguard.lua.parser import Node, parse_py

#: Collections that hold every actor, item or record in reach, after aliases are
#: resolved to the package's short name (``local nb = require("openmw.nearby")``).
_BIG_COLLECTIONS = (
    "nearby.actors",
    "nearby.items",
    "nearby.activators",
    "nearby.containers",
    "nearby.doors",
    "world.activeActors",
    "world.cells",
)
#: Calls that are costly to make every frame (method or function name).
_COSTLY_CALLS = {
    "findPath": "asks the navigator for a path",
    "castRenderingRay": "casts a ray against rendered geometry",
    "updateAll": "relayouts every UI element",
}
_LOOPS = frozenset({"ForIn", "ForNum", "While", "Repeat"})
#: Globals Lua has and OpenMW's sandbox does not (``components/lua/luastate.cpp``).
_NOT_IN_SANDBOX = {
    "collectgarbage": (
        "collectgarbage is not in OpenMW's sandbox: the call fails. OpenMW runs the "
        "collector itself (memory limits: [Lua] in settings.cfg); make less garbage instead"
    ),
    "load": "load is not in OpenMW's sandbox: scripts cannot compile code at run time",
    "loadstring": "loadstring is not in OpenMW's sandbox: scripts cannot compile code at run time",
    "loadfile": "loadfile is not in OpenMW's sandbox: read files with openmw.vfs",
    "dofile": "dofile is not in OpenMW's sandbox: use require",
    "setfenv": "setfenv is not in OpenMW's sandbox",
    "getfenv": "getfenv is not in OpenMW's sandbox",
    "gcinfo": "gcinfo is not in OpenMW's sandbox",
    "newproxy": "newproxy is not in OpenMW's sandbox",
    "module": "module is not in OpenMW's sandbox: return a table from the script instead",
    "io": "io is not in OpenMW's sandbox: read files with openmw.vfs, keep data with openmw.storage",
    "debug": "debug is not in OpenMW's sandbox (openmw.debug is a different thing)",
    "package": "package is not in OpenMW's sandbox",
}
_OS_ALLOWED = frozenset({"date", "difftime", "time"})
_READ_ONLY_PACKAGES = frozenset({"coroutine", "math", "string", "table", "utf8", "os"})
#: Calls that make a new vector, colour or transform (userdata the collector frees).
_USERDATA_MAKERS = frozenset(
    {"vector2", "vector3", "vector4", "rgb", "rgba", "hex", "move", "rotate", "scale"}
)
_SEND_EVENT_FUNCS = frozenset({"sendGlobalEvent", "sendMenuEvent"})


@dataclass(frozen=True)
class Finding:
    """One thing a check found.

    Attributes:
        severity: ``error``, ``warn`` or ``info``.
        code: A short stable code, e.g. ``PERF_LOOP``.
        line: The 1-based line it is about (0 when it is about the whole file).
        message: What it means, in a sentence.
    """

    severity: str
    code: str
    line: int
    message: str


@dataclass
class ScriptInfo:
    """What :func:`analyze` learned about one script.

    Attributes:
        interface_name: Its ``interfaceName``, if it offers an interface.
        interface_line: Where that is declared.
        interface_members: The names in its ``interface`` table.
        engine_handlers: Engine handler name -> line.
        event_handlers: Event name -> line.
        requires: ``require``d module -> first line.
        sent_events: Event name -> first line it is sent (``sendEvent``,
            ``core.sendGlobalEvent``, ``core.sendMenuEvent``).
        findings: The checks' findings.
        tree: The syntax tree, or None when the source does not parse.
    """

    interface_name: str | None = None
    interface_line: int = 0
    interface_members: list[str] = field(default_factory=list)
    engine_handlers: dict[str, int] = field(default_factory=dict)
    event_handlers: dict[str, int] = field(default_factory=dict)
    requires: dict[str, int] = field(default_factory=dict)
    sent_events: dict[str, int] = field(default_factory=dict)
    findings: list[Finding] = field(default_factory=list)
    tree: Node | None = None


class _Symbols:
    """The chunk's top-level functions, tables and ``require`` aliases."""

    def __init__(self, chunk: Node) -> None:
        """Collect them from the chunk's top-level statements.

        Args:
            chunk: The parsed chunk.
        """
        self.funcs: dict[str, Node] = {}
        self.tables: dict[str, Node] = {}
        self.fields: dict[str, dict[str, Node]] = {}
        self.aliases: dict[str, str] = {}
        for stmt in chunk.children:
            if stmt.kind in ("LocalFunction", "FunctionStat") and stmt.value:
                self.funcs[stmt.value.replace(":", ".")] = stmt.children[0]
            elif stmt.kind == "Local":
                names, exprs = stmt.children
                for name, expr in zip(names.children, exprs.children):
                    self._bind(name.value or "", expr)
            elif stmt.kind == "Assign":
                targets, exprs = stmt.children
                for target, expr in zip(targets.children, exprs.children):
                    self._assign(target, expr)

    def _bind(self, name: str, expr: Node) -> None:
        """Record ``name = expr`` when it names a function, table or package.

        Args:
            name: The variable.
            expr: Its value.
        """
        if expr.kind == "Function":
            self.funcs[name] = expr
        elif expr.kind == "Table":
            self.tables[name] = expr
        else:
            module = _required_module(expr)
            if module:
                self.aliases[name] = module.rsplit(".", 1)[-1]

    def _assign(self, target: Node, expr: Node) -> None:
        """Record a top-level assignment (``M.onUpdate = f``, ``T = {}``).

        Args:
            target: The assigned target.
            expr: Its value.
        """
        if target.kind == "Name" and target.value:
            self._bind(target.value, expr)
            return
        if target.kind != "Index" or target.value != ".":
            return
        obj, key = target.children
        if obj.kind == "Name" and obj.value and key.value:
            self.fields.setdefault(obj.value, {})[key.value] = expr
            if expr.kind == "Function":
                self.funcs[f"{obj.value}.{key.value}"] = expr

    def table_fields(self, node: Node | None) -> dict[str, Node] | None:
        """The named fields of a table, followed through a local name.

        Args:
            node: A table constructor, or a name bound to one at the top level.

        Returns:
            Field name -> value node, or None when it is not a table this can see.
        """
        if node is None:
            return None
        if node.kind == "Table":
            found: dict[str, Node] = {}
            for c in node.children:
                key = c.children[0]
                if c.kind == "Field" and key.kind == "String" and key.value:
                    found[key.value] = c.children[1]
            return found
        if node.kind == "Name" and node.value:
            base = self.tables.get(node.value)
            extra = self.fields.get(node.value, {})
            if base is None and not extra:
                return None
            out = self.table_fields(base) or {}
            out.update(extra)
            return out
        return None

    def function_of(self, node: Node) -> Node | None:
        """The function a handler value refers to.

        Args:
            node: A function expression, a name or ``M.name``.

        Returns:
            The ``Function`` node, or None when it is defined elsewhere.
        """
        if node.kind == "Function":
            return node
        name = _dotted(node, {})
        return self.funcs.get(name) if name else None


def _required_module(expr: Node) -> str | None:
    """The module of ``require("x")`` (possibly in parentheses).

    Args:
        expr: An expression.

    Returns:
        ``x``, or None when the expression is not a require.
    """
    while expr.kind == "Paren":
        expr = expr.children[0]
    if expr.kind != "Call" or len(expr.children) < 2:
        return None
    fn, arg = expr.children[0], expr.children[1]
    if fn.kind == "Name" and fn.value == "require" and arg.kind == "String":
        return arg.value
    return None


def _dotted(node: Node, aliases: dict[str, str]) -> str | None:
    """A name chain as text: ``nearby.actors``, ``types.NPC.records``, ``cell:getAll``.

    Args:
        node: A ``Name``, an ``Index`` chain with constant keys, or a method call.
        aliases: Local name -> package short name, so ``nb.actors`` reads
            ``nearby.actors``.

    Returns:
        The text, or None when the chain has a computed part.
    """
    if node.kind == "Name" and node.value:
        return aliases.get(node.value, node.value)
    if node.kind == "Index" and node.value == ".":
        base = _dotted(node.children[0], aliases)
        key = node.children[1].value
        return f"{base}.{key}" if base and key else None
    if node.kind == "Method":
        base = _dotted(node.children[0], aliases)
        return f"{base}:{node.value}" if base else f"?:{node.value}"
    if node.kind == "Call":
        inner = _dotted(node.children[0], aliases)
        return f"{inner}()" if inner else None
    return None


def _loop_collection(loop: Node, aliases: dict[str, str]) -> str | None:
    """What a generic ``for`` iterates, when it is a big collection.

    Args:
        loop: A ``ForIn`` node.
        aliases: As for :func:`_dotted`.

    Returns:
        The collection's text (``nearby.actors``), or None when it is not one of the
        big ones.
    """
    exprs = loop.children[1].children
    if not exprs:
        return None
    it = exprs[0]
    if it.kind == "Call" and len(it.children) >= 2:
        fn = _dotted(it.children[0], aliases)
        if fn in ("ipairs", "pairs"):
            it = it.children[1]
    text = _dotted(it, aliases)
    if not text:
        return None
    if text in _BIG_COLLECTIONS or text.endswith(".records"):
        return text
    # cell:getAll(type) lists a whole cell; an inventory's getAll is one actor's items.
    if ":getAll" in text and "inventory" not in text.lower():
        return text
    return None


def _returns(node: Node) -> bool:
    """Does this subtree return (outside any function defined inside it)?

    Args:
        node: The subtree, typically an ``If``.

    Returns:
        True when a ``return`` in it leaves the enclosing function.
    """
    stack = [node]
    while stack:
        n = stack.pop()
        if n.kind == "Return":
            return True
        stack.extend(c for c in n.children if c.kind != "Function")
    return False


class _FrameWalker:
    """Follows one per-frame handler through the script's own functions."""

    def __init__(self, info: ScriptInfo, symbols: _Symbols, handler: str) -> None:
        """Set up for one handler.

        Args:
            info: Where findings go.
            symbols: The script's top-level names.
            handler: The handler's name (``onUpdate``), for messages.
        """
        self.info = info
        self.symbols = symbols
        self.handler = handler
        self.seen: set[int] = set()
        self.reported: set[tuple[str, int]] = set()

    def function(
        self, fn: Node, guarded: bool = False, loops: list[str] | None = None, in_loop: bool = False
    ) -> None:
        """Walk a function's body, once (from the first call that reaches it).

        Args:
            fn: The ``Function`` node.
            guarded: It is called from under a condition (see :meth:`walk`).
            loops: The big loops the call sits inside (for what the body allocates).
            in_loop: Whether any loop encloses the call.
        """
        if id(fn) in self.seen:
            return
        self.seen.add(id(fn))
        # Loops over big collections inside the body are reported as the body's own,
        # so the walk starts fresh there; garbage it makes is per iteration of the caller.
        self.walk(fn.children[1], [], guarded, in_loop or bool(loops))

    def report(self, severity: str, code: str, line: int, message: str) -> None:
        """Add a finding, once per code and line.

        Args:
            severity: ``warn`` or ``info``.
            code: The finding's code.
            line: Its line.
            message: Its message.
        """
        if (code, line) in self.reported:
            return
        self.reported.add((code, line))
        self.info.findings.append(Finding(severity, code, line, message))

    def walk(
        self, node: Node, loops: list[str], guarded: bool = False, in_loop: bool = False
    ) -> None:
        """Visit a subtree, tracking the big loops it sits inside.

        Code under an ``if``, or after an ``if`` that can return, is *guarded*: it
        may well not run every frame (a timer, a mode check, a debug switch), so what
        it does is reported as a note rather than a warning, and its prints and
        garbage not at all.

        Args:
            node: The subtree.
            loops: The big collections iterated by enclosing loops.
            guarded: Whether a condition stands between the handler and this code.
            in_loop: Whether any loop encloses it (garbage made there is made per
                iteration).
        """
        if node.kind == "Function":
            # The closure's body runs later, if at all - not this frame; making the
            # closure is this frame's garbage.
            self.garbage(
                node, "GC_CLOSURE", "makes a new function (closure)", loops, guarded, in_loop
            )
            return
        if node.kind == "Block":
            for child in node.children:
                self.walk(child, loops, guarded, in_loop)
                if child.kind == "If" and _returns(child):
                    guarded = True  # an early return: the rest may not run
            return
        if node.kind == "If":
            for child in node.children:
                self.walk(child, loops, True, in_loop)
            return
        inner = loops
        if node.kind == "ForIn":
            coll = _loop_collection(node, self.symbols.aliases)
            if coll:
                self.big_loop(node, coll, loops, guarded)
                inner = [*loops, coll]
        elif node.kind in ("Call", "Method"):
            self.call(node, loops, guarded, in_loop)
            if in_loop and self._makes_userdata(node):
                self.garbage(
                    node, "GC_USERDATA", "makes a new vector/colour/transform", loops, guarded, True
                )
        elif node.kind == "Table":
            self.garbage(node, "GC_TABLE", "makes a new table", loops, guarded, in_loop)
        elif node.kind == "Binop" and node.value == ".." and in_loop:
            self.garbage(node, "GC_STRING", "builds a new string", loops, guarded, True)
        looping = in_loop or node.kind in _LOOPS
        for child in node.children:
            # A table inside a table constructor is part of the same construction.
            if node.kind in ("Table", "Field", "Item") and child.kind == "Table":
                for grand in child.children:
                    self.walk(grand, inner, guarded, looping)
                continue
            self.walk(child, inner, guarded, looping)

    def _makes_userdata(self, node: Node) -> bool:
        """Whether a call makes a new vector, colour or transform.

        ``util.vector3(...)``, ``util.color.rgb(...)``, ``util.transform.move(...)``.

        Args:
            node: A ``Call`` or ``Method`` node.

        Returns:
            True when it does.
        """
        if node.kind != "Call":
            return False
        name = _dotted(node.children[0], self.symbols.aliases) or ""
        parts = name.split(".")
        return len(parts) >= 2 and parts[0] == "util" and parts[-1] in _USERDATA_MAKERS

    def garbage(
        self, node: Node, code: str, what: str, loops: list[str], guarded: bool, in_loop: bool
    ) -> None:
        """Report something the handler allocates for the collector to free.

        Garbage made every frame is what makes the collector run, and OpenMW's Lua
        collector pauses the scripts while it does; inside a loop it is made per
        iteration. Guarded code is left out: it is usually a timer or a mode change.

        Args:
            node: Where it is made.
            code: ``GC_TABLE``, ``GC_CLOSURE``, ``GC_STRING`` or ``GC_USERDATA``.
            what: What it makes, for the message.
            loops: The big loops it sits inside.
            guarded: Whether it sits under a condition.
            in_loop: Whether any loop encloses it.
        """
        if guarded:
            return
        if loops:
            msg = f"{self.handler} {what} for every one of {loops[-1]}, every frame"
        elif in_loop:
            msg = f"{self.handler} {what} on every pass of a loop, every frame"
        else:
            msg = f"{self.handler} {what} every frame: hoist it out, or reuse one"
        self.report(
            "warn" if in_loop else "info", code, node.line, msg + " (garbage for the collector)"
        )

    def big_loop(self, node: Node, coll: str, loops: list[str], guarded: bool) -> None:
        """Report a loop over a big collection.

        Args:
            node: The ``ForIn`` node.
            coll: What it iterates.
            loops: The big loops around it.
            guarded: Whether it sits under a condition.
        """
        severity = "info" if guarded else "warn"
        when = "when its condition holds" if guarded else "every frame"
        if loops:
            msg = (
                f"{self.handler} loops over {coll} inside a loop over {loops[-1]} {when}: "
                "the work grows with the square of what is nearby"
            )
            self.report(severity, "PERF_NESTED", node.line, msg)
        else:
            tip = "" if guarded else "; consider a timer or the engine's events"
            msg = f"{self.handler} loops over {coll} {when}{tip}"
            self.report(severity, "PERF_LOOP", node.line, msg)

    def call(self, node: Node, loops: list[str], guarded: bool, in_loop: bool = False) -> None:
        """Check one call: costly API calls, and calls into the script's functions.

        Args:
            node: A ``Call`` or ``Method`` node.
            loops: The big loops it sits inside.
            guarded: Whether it sits under a condition.
            in_loop: Whether any loop encloses it.
        """
        if node.kind == "Method":
            name = node.value or ""
        else:
            fn = node.children[0]
            name = (fn.children[1].value if fn.kind == "Index" else fn.value) or ""
            target = self.symbols.function_of(fn)
            if target is not None:
                self.function(target, guarded, loops, in_loop)
        why = _COSTLY_CALLS.get(name)
        if why:
            where = f" inside a loop over {loops[-1]}" if loops else ""
            when = "when its condition holds" if guarded else "every frame"
            self.report(
                "warn" if loops and not guarded else "info",
                "PERF_CALL",
                node.line,
                f"{self.handler} calls {name}(){where} {when}: it {why}",
            )
        if name == "print" and node.kind == "Call" and not guarded:
            self.report("info", "PERF_PRINT", node.line, f"{self.handler} prints every frame")


def bound_names(tree: Node | None) -> set[str]:
    """Every name a script binds anywhere.

    Locals, parameters, loop variables, and globals it assigns: a name in here is the
    script's own, whatever else the same name means outside it.

    Args:
        tree: The script's syntax tree.

    Returns:
        The names.
    """
    out: set[str] = set()
    if tree is None:
        return out
    for n in tree.walk():
        if n.kind in ("Local", "ForIn"):
            out.update(c.value for c in n.children[0].children if c.value)
        elif n.kind == "ForNum" and n.children and n.children[0].value:
            out.add(n.children[0].value)
        elif n.kind == "LocalFunction" and n.value:
            out.add(n.value)
        elif n.kind == "Params":
            out.update(c.value for c in n.children if c.kind == "Name" and c.value)
        elif n.kind == "Assign":
            out.update(t.value for t in n.children[0].children if t.kind == "Name" and t.value)
        elif n.kind == "FunctionStat" and n.value and "." not in n.value and ":" not in n.value:
            out.add(n.value)
    return out


def _sandbox_checks(info: ScriptInfo, chunk: Node) -> None:
    """What OpenMW's Lua sandbox does not have, used anyway.

    OpenMW gives scripts only part of Lua's standard library (``components/lua/
    luastate.cpp``): the safe base functions, ``coroutine``, ``math``, ``string``,
    ``table`` and ``utf8`` (read-only), and ``os.date``/``os.difftime``/``os.time``.
    Anything else is nil there, and a call to it fails when it runs. A name the script
    defines itself is its own and not reported.

    Args:
        info: Where findings go.
        chunk: The parsed chunk.
    """
    own = bound_names(chunk)
    seen: set[tuple[str, int]] = set()

    def flag(line: int, what: str, msg: str) -> None:
        """Report once per name and line.

        Args:
            line: Its line.
            what: The name.
            msg: The message.
        """
        if (what, line) not in seen:
            seen.add((what, line))
            info.findings.append(Finding("error", "SANDBOX", line, msg))

    for n in chunk.walk():
        if n.kind == "Name" and n.value in _NOT_IN_SANDBOX and n.value not in own:
            flag(n.line, n.value, _NOT_IN_SANDBOX[n.value])
        elif n.kind == "Index" and n.value == ".":
            obj, key = n.children
            unsandboxed = key.value and key.value not in _OS_ALLOWED
            if obj.kind == "Name" and obj.value == "os" and "os" not in own and unsandboxed:
                flag(
                    n.line,
                    f"os.{key.value}",
                    f"os.{key.value} is not in OpenMW's sandbox (only os.date, os.difftime and os.time are)",
                )
        elif n.kind == "Assign":
            for t in n.children[0].children:
                if t.kind == "Index" and t.children[0].kind == "Name":
                    pkg = t.children[0].value
                    if pkg in _READ_ONLY_PACKAGES and pkg not in own:
                        flag(
                            t.line,
                            f"{pkg}=",
                            f"{pkg} is read-only in OpenMW: this assignment fails",
                        )


def _collect_calls(info: ScriptInfo, chunk: Node) -> None:
    """Record the script's ``require``s and the events it sends.

    Args:
        info: Where to record them.
        chunk: The parsed chunk.
    """
    for node in chunk.walk():
        module = _required_module(node)
        if module:
            info.requires.setdefault(module, node.line)
            continue
        if node.kind == "Method" and node.value == "sendEvent" and len(node.children) >= 2:
            arg = node.children[1]
            if arg.kind == "String" and arg.value:
                info.sent_events.setdefault(arg.value, node.line)
        elif node.kind == "Call" and len(node.children) >= 2:
            fn, arg = node.children[0], node.children[1]
            name = fn.children[1].value if fn.kind == "Index" else fn.value
            if name in _SEND_EVENT_FUNCS and arg.kind == "String" and arg.value:
                info.sent_events.setdefault(arg.value, node.line)


def _check_contexts(info: ScriptInfo, contexts: frozenset[str], api: ApiVersion) -> None:
    """Handlers and packages used where the script does not run.

    Args:
        info: The script's declarations (findings are added to it).
        contexts: Where it runs (:func:`.api.contexts_for_flags`).
        api: The API version's rules.
    """
    if not contexts:
        return
    where = "/".join(sorted(contexts))
    for name, line in info.engine_handlers.items():
        allowed = api.handlers.get(name)
        if allowed is None:
            info.findings.append(
                Finding("warn", "UNKNOWN_HANDLER", line, f"{name} is not an engine handler")
            )
        elif not contexts & allowed:
            info.findings.append(
                Finding(
                    "warn",
                    "HANDLER_CONTEXT",
                    line,
                    f"{name} is never called in a {where} script "
                    f"(only {'/'.join(sorted(allowed))})",
                )
            )
    for module, line in info.requires.items():
        allowed = api.packages.get(module)
        if allowed is not None and not contexts & allowed:
            info.findings.append(
                Finding(
                    "error",
                    "PACKAGE_CONTEXT",
                    line,
                    f"{module} is not available to a {where} script "
                    f"(only {'/'.join(sorted(allowed))}); require fails there",
                )
            )


def _native_analyze() -> Any:  # noqa: ANN401 - the backend's function, or None
    """The Rust backend's analysis (``viewer-shell/luacore/src/analysis.rs``), or None."""
    try:
        import wraithguard_native
    except ImportError:
        return None
    return getattr(wraithguard_native, "lua_analyze", None)


def analyze(src: str, contexts: frozenset[str] | None = None, api: ApiVersion = API) -> ScriptInfo:
    """Read one script (in Rust when the backend is built).

    Args:
        src: Its source.
        contexts: Where it runs (None or empty: unknown - the context checks are
            skipped).
        api: The API version to check against.

    Returns:
        What it declares, and the findings.
    """
    native = _native_analyze()
    if native is None:
        return analyze_py(src, contexts, api)
    got = native(
        src,
        sorted(contexts or ()),
        {k: sorted(v) for k, v in api.handlers.items()},
        sorted(api.per_frame),
        {k: sorted(v) for k, v in api.packages.items()},
    )
    return info_from_native(got)


def info_from_native(got: dict[str, Any]) -> ScriptInfo:
    """A :class:`ScriptInfo` from the backend's dict (``lua_analyze``, ``lua_scan``).

    Args:
        got: The dict.

    Returns:
        The info, its tree rebuilt as :class:`.parser.Node` objects.
    """
    from wraithguard.lua.parser import _from_native

    return ScriptInfo(
        interface_name=got["interface_name"],
        interface_line=int(got["interface_line"]),
        interface_members=list(got["interface_members"]),
        engine_handlers=dict(got["engine_handlers"]),
        event_handlers=dict(got["event_handlers"]),
        requires=dict(got["requires"]),
        sent_events=dict(got["sent_events"]),
        findings=[Finding(*f) for f in got["findings"]],
        tree=_from_native(got["tree"]) if got["tree"] is not None else None,
    )


def analyze_py(
    src: str, contexts: frozenset[str] | None = None, api: ApiVersion = API
) -> ScriptInfo:
    """Read one script in Python (:func:`analyze`'s fallback, and its reference).

    Args:
        src: Its source.
        contexts: Where it runs (None or empty: unknown - the context checks are
            skipped).
        api: The API version to check against.

    Returns:
        What it declares, and the findings.
    """
    contexts = contexts or frozenset()
    info = ScriptInfo()
    try:
        chunk = parse_py(src)
    except LuaSyntaxError as exc:
        info.findings.append(Finding("error", "SYNTAX", exc.line, str(exc)))
        return info
    info.tree = chunk
    symbols = _Symbols(chunk)
    _collect_calls(info, chunk)
    _sandbox_checks(info, chunk)

    last = chunk.children[-1] if chunk.children else None
    returned = None
    if last is not None and last.kind == "Return" and last.children[0].children:
        returned = symbols.table_fields(last.children[0].children[0])
    if returned is None:
        return _finish(info, contexts, api)  # e.g. a settings script: nothing to declare

    name_node = returned.get("interfaceName")
    if name_node is not None and name_node.kind == "String":
        info.interface_name = name_node.value
        info.interface_line = name_node.line
    members = symbols.table_fields(returned.get("interface"))
    if members:
        info.interface_members = sorted(members)
    engine = symbols.table_fields(returned.get("engineHandlers")) or {}
    events = symbols.table_fields(returned.get("eventHandlers")) or {}
    info.engine_handlers = {name: value.line for name, value in engine.items()}
    info.event_handlers = {name: value.line for name, value in events.items()}
    for name in sorted(engine):
        if name in api.per_frame:
            fn = symbols.function_of(engine[name])
            if fn is not None:
                _FrameWalker(info, symbols, name).function(fn)
    return _finish(info, contexts, api)


def _finish(info: ScriptInfo, contexts: frozenset[str], api: ApiVersion) -> ScriptInfo:
    """Run the context checks and order the findings.

    Args:
        info: The script's declarations.
        contexts: Where it runs.
        api: The API version's rules.

    Returns:
        ``info``, findings sorted by line.
    """
    _check_contexts(info, contexts, api)
    info.findings.sort(key=lambda f: (f.line, f.code))
    return info
