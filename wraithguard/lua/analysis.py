"""What one OpenMW Lua script declares, and what it does every frame.

An OpenMW script returns a table: ``interfaceName`` and ``interface`` (what it offers
other scripts), ``engineHandlers`` (``onUpdate``, ``onActive``, ...) and
``eventHandlers``. :func:`analyze` finds those - in the returned table constructor, or
in a local table the script fills and returns - and checks them against the API
rules in :mod:`.api` for the contexts the script runs in. It also follows the
per-frame handlers (``onUpdate``, ``onFrame``) into the script's own functions and
reports loops over large collections (``nearby.actors``, ``world.activeActors``,
``types.*.records``, ``cell:getAll()``) and other costly calls made every frame.

These are static checks: a script that throttles its own ``onUpdate`` (counts time
and returns early) is still reported, and the message says so.

Copyright (c) 2026 StaticNation.
"""

from __future__ import annotations

from dataclasses import dataclass, field

from wraithguard.lua.api import API, ApiVersion
from wraithguard.lua.lexer import LuaSyntaxError
from wraithguard.lua.parser import Node, parse

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

    def function(self, fn: Node, guarded: bool = False) -> None:
        """Walk a function's body, once.

        Args:
            fn: The ``Function`` node.
            guarded: It is called from under a condition (see :meth:`walk`).
        """
        if id(fn) in self.seen:
            return
        self.seen.add(id(fn))
        self.walk(fn.children[1], [], guarded)

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

    def walk(self, node: Node, loops: list[str], guarded: bool = False) -> None:
        """Visit a subtree, tracking the big loops it sits inside.

        Code under an ``if``, or after an ``if`` that can return, is *guarded*: it
        may well not run every frame (a timer, a mode check, a debug switch), so what
        it does is reported as a note rather than a warning, and its prints not at all.

        Args:
            node: The subtree.
            loops: The big collections iterated by enclosing loops.
            guarded: Whether a condition stands between the handler and this code.
        """
        if node.kind == "Function":
            return  # a closure made here runs later, if at all - not this frame
        if node.kind == "Block":
            for child in node.children:
                self.walk(child, loops, guarded)
                if child.kind == "If" and _returns(child):
                    guarded = True  # an early return: the rest may not run
            return
        if node.kind == "If":
            for child in node.children:
                self.walk(child, loops, True)
            return
        inner = loops
        if node.kind == "ForIn":
            coll = _loop_collection(node, self.symbols.aliases)
            if coll:
                self.big_loop(node, coll, loops, guarded)
                inner = [*loops, coll]
        elif node.kind in ("Call", "Method"):
            self.call(node, loops, guarded)
        for child in node.children:
            self.walk(child, inner, guarded)

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

    def call(self, node: Node, loops: list[str], guarded: bool) -> None:
        """Check one call: costly API calls, and calls into the script's functions.

        Args:
            node: A ``Call`` or ``Method`` node.
            loops: The big loops it sits inside.
            guarded: Whether it sits under a condition.
        """
        if node.kind == "Method":
            name = node.value or ""
        else:
            fn = node.children[0]
            name = (fn.children[1].value if fn.kind == "Index" else fn.value) or ""
            target = self.symbols.function_of(fn)
            if target is not None:
                self.function(target, guarded)
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


def analyze(src: str, contexts: frozenset[str] | None = None, api: ApiVersion = API) -> ScriptInfo:
    """Read one script.

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
        chunk = parse(src)
    except LuaSyntaxError as exc:
        info.findings.append(Finding("error", "SYNTAX", exc.line, str(exc)))
        return info
    info.tree = chunk
    symbols = _Symbols(chunk)
    _collect_calls(info, chunk)

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
