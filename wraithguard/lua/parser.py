"""A Lua 5.1 (LuaJIT) parser: source to a syntax tree.

A recursive-descent parser over :mod:`.lexer`'s tokens, following the grammar of the
Lua 5.1 reference manual (section 8) plus LuaJIT's ``goto``/labels. Every node is a
:class:`Node`: a kind, its position, an optional value and its children, whose
layout per kind is listed below. One shape for all of them keeps the tree view
generic and the checks in :mod:`.analysis` simple.

Expressions:
    ``Nil`` ``True`` ``False`` ``Vararg`` - no children.
    ``Number`` / ``String`` - ``value`` is the number's text / the decoded string.
    ``Name`` - ``value`` is the name.
    ``Function`` - [``Params``, ``Block``]; ``Params`` holds ``Name`` nodes and a
    trailing ``Vararg`` when it takes ``...``.
    ``Table`` - ``Field`` [key, value] (``a = 1`` gives a ``String`` key with
    ``value`` "a"; ``[k] = v`` the key expression) and ``Item`` [value].
    ``Binop`` / ``Unop`` - ``value`` is the operator; [left, right] / [operand].
    ``Index`` - [object, key]; ``value`` is ``.`` (key a ``String``) or ``[]``.
    ``Call`` - [function, args...]. ``Method`` - [object, args...], ``value`` the name.
    ``Paren`` - [expression].

Statements:
    ``Block`` - [statements...].
    ``Local`` - [``Names``, ``Exprs``]. ``Assign`` - [``Targets``, ``Exprs``].
    ``CallStat`` - [call]. ``Do`` - [``Block``]. ``While`` - [condition, ``Block``].
    ``Repeat`` - [``Block``, condition]. ``If`` - [``Clause`` [condition, ``Block``]...,
    then an ``Else`` [``Block``] if there is one].
    ``ForNum`` - [``Name``, start, stop, (step,) ``Block``].
    ``ForIn`` - [``Names``, ``Exprs``, ``Block``].
    ``LocalFunction`` - [``Function``], ``value`` the name.
    ``FunctionStat`` - [``Function``], ``value`` the dotted name (``a.b:c``).
    ``Return`` - [``Exprs``]. ``Break``. ``Goto`` / ``Label`` - ``value`` the label.

Teal (``parse(src, teal=True)``): the same tree, with the types read and set aside -
annotations (``local x: T``, ``f(a?: T): R``), generics (``<T>``), ``x as T`` and
``x is T`` (a ``Cast`` [expression], ``value`` ``as``/``is``), ``global`` declarations
(``Local`` with ``value`` ``global``, or ``FunctionStat``), and ``record``/``enum``/
``interface``/``type`` declarations (``TypeDecl``, ``value`` e.g. ``record Point``).
Teal's Lua 5.3 operators (``//``, ``&``, ``|``, ``~``, ``<<``, ``>>``) are accepted too.
Reference: https://teal-language.org/book/latest/index.html

Copyright (c) 2026 StaticNation.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import TYPE_CHECKING

from wraithguard.lua.lexer import LuaSyntaxError, tokenize

if TYPE_CHECKING:
    from collections.abc import Iterator

    from wraithguard.lua.lexer import Token

# (left, right) binding power, from the Lua 5.1 sources (lparser.c priority[]).
_BINARY: dict[str, tuple[int, int]] = {
    "or": (1, 1),
    "and": (2, 2),
    "<": (3, 3),
    ">": (3, 3),
    "<=": (3, 3),
    ">=": (3, 3),
    "~=": (3, 3),
    "==": (3, 3),
    "..": (5, 4),
    "+": (6, 6),
    "-": (6, 6),
    "*": (7, 7),
    "/": (7, 7),
    "%": (7, 7),
    "^": (10, 9),
}
_UNARY_PRIORITY = 8
_NOT_51 = frozenset({"//", "&", "|", "<<", ">>"})
# Teal (Lua 5.3) operators, on the 5.1 scale above: bitwise between comparison and
# concatenation, ``//`` with ``*``.
_BINARY_TEAL: dict[str, tuple[int, int]] = {
    "|": (4, 4),
    "~": (4, 4),
    "&": (4, 4),
    "<<": (4, 4),
    ">>": (4, 4),
    "//": (7, 7),
}
_TYPE_DECLS = frozenset({"record", "enum", "interface", "type"})
_BLOCK_END = frozenset({"end", "else", "elseif", "until"})


@dataclass
class Node:
    """One node of the syntax tree (layouts in the module docstring).

    Attributes:
        kind: What it is (``Call``, ``Local``, ...).
        line: Its 1-based line.
        col: Its 1-based column.
        value: Its name, operator or literal, when it has one.
        children: Its children.
    """

    kind: str
    line: int
    col: int
    value: str | None = None
    children: list[Node] = field(default_factory=list)

    def walk(self) -> Iterator[Node]:
        """This node and every node under it, depth first.

        Yields:
            The nodes.
        """
        stack = [self]
        while stack:
            node = stack.pop()
            yield node
            stack.extend(reversed(node.children))

    def label(self) -> str:
        """A one-line description, for a tree view.

        Returns:
            ``Kind value`` (``value`` only when there is one).
        """
        if self.value is None:
            return self.kind
        shown = self.value if len(self.value) <= 60 else self.value[:57] + "..."
        return f"{self.kind} {shown!r}" if self.kind == "String" else f"{self.kind} {shown}"


class _Parser:
    """The parser's state: the tokens and where it is in them."""

    def __init__(self, tokens: list[Token], teal: bool = False) -> None:
        """Start at the first token.

        Args:
            tokens: The tokens, comments left out, ending in ``eof``.
            teal: Read Teal (see the module docstring).
        """
        self.toks = tokens
        self.i = 0
        self.teal = teal

    # -- token helpers ----------------------------------------------------------
    @property
    def tok(self) -> Token:
        """The current token."""
        return self.toks[self.i]

    def peek(self, offset: int = 1) -> Token:
        """A token ahead of the current one.

        Args:
            offset: How far ahead.

        Returns:
            That token, or the final ``eof``.
        """
        return self.toks[min(self.i + offset, len(self.toks) - 1)]

    def check(self, text: str) -> bool:
        """Is the current token the keyword or operator ``text``?

        Args:
            text: The keyword or operator.

        Returns:
            True when it is.
        """
        t = self.tok
        return t.kind in ("keyword", "op") and t.text == text

    def accept(self, text: str) -> bool:
        """Step over ``text`` if it is next.

        Args:
            text: The keyword or operator.

        Returns:
            True when it was there (and is now consumed).
        """
        if self.check(text):
            self.i += 1
            return True
        return False

    def expect(self, text: str, opener: Token | None = None) -> Token:
        """Consume ``text`` or fail.

        Args:
            text: The keyword or operator required.
            opener: The token that opened the construct, named in the error.

        Returns:
            The consumed token.

        Raises:
            LuaSyntaxError: Something else is there.
        """
        t = self.tok
        if not self.check(text):
            where = f" (to close {opener.text!r} at line {opener.line})" if opener else ""
            raise self.error(f"{text!r} expected{where} near {t.text or '<eof>'!r}")
        self.i += 1
        return t

    def name(self) -> Token:
        """Consume a name or fail.

        Returns:
            The name token.

        Raises:
            LuaSyntaxError: The next token is not a name.
        """
        t = self.tok
        if t.kind != "name":
            raise self.error(f"name expected near {t.text or '<eof>'!r}")
        self.i += 1
        return t

    def error(self, message: str) -> LuaSyntaxError:
        """An error at the current token.

        Args:
            message: What is wrong.

        Returns:
            The error, for the caller to raise.
        """
        return LuaSyntaxError(message, self.tok.line, self.tok.col)

    @staticmethod
    def node(kind: str, at: Token, value: str | None = None, *kids: Node) -> Node:
        """A node positioned at a token.

        Args:
            kind: Its kind.
            at: The token it starts at.
            value: Its value.
            *kids: Its children.

        Returns:
            The node.
        """
        return Node(kind, at.line, at.col, value, list(kids))

    # -- blocks and statements ----------------------------------------------------
    def block(self) -> Node:
        """A block: statements up to ``end``/``else``/``elseif``/``until``/eof.

        Returns:
            The ``Block`` node.
        """
        start = self.tok
        out = self.node("Block", start)
        while True:
            t = self.tok
            if t.kind == "eof" or (t.kind == "keyword" and t.text in _BLOCK_END):
                return out
            if t.kind == "keyword" and t.text == "return":
                out.children.append(self.return_stat())
                return out
            if t.kind == "keyword" and t.text == "break":
                self.i += 1
                out.children.append(self.node("Break", t))
                self.accept(";")
                continue
            stmt = self.statement()
            if stmt is not None:
                out.children.append(stmt)

    def return_stat(self) -> Node:
        """``return [explist] [;]``, which must end its block.

        Returns:
            The ``Return`` node.
        """
        t = self.tok
        self.i += 1
        exprs = self.node("Exprs", self.tok)
        nxt = self.tok
        ends = nxt.kind == "eof" or (nxt.kind == "keyword" and nxt.text in _BLOCK_END)
        if not (ends or self.check(";")):
            exprs.children = self.exprlist()
        self.accept(";")
        return self.node("Return", t, None, exprs)

    def statement(self) -> Node | None:
        """One statement (None for a lone ``;``).

        Returns:
            The statement's node, or None.

        Raises:
            LuaSyntaxError: Not a statement.
        """
        t = self.tok
        if self.accept(";"):
            return None
        if t.kind == "keyword":
            handler = {
                "if": self.if_stat,
                "while": self.while_stat,
                "do": self.do_stat,
                "for": self.for_stat,
                "repeat": self.repeat_stat,
                "function": self.function_stat,
                "local": self.local_stat,
                "goto": self.goto_stat,
            }.get(t.text)
            if handler is not None:
                return handler()
        if self.check("::"):
            self.i += 1
            name = self.name()
            self.expect("::")
            return self.node("Label", t, name.text)
        if self.teal and t.kind == "name" and t.text == "global" and self.peek().kind != "op":
            return self.local_stat(scope="global")
        return self.expr_stat()

    def if_stat(self) -> Node:
        """``if c then b {elseif c then b} [else b] end``.

        Returns:
            The ``If`` node.
        """
        start = self.tok
        out = self.node("If", start)
        while True:
            clause_tok = self.tok
            self.i += 1  # 'if' or 'elseif'
            cond = self.expr()
            self.expect("then")
            out.children.append(self.node("Clause", clause_tok, None, cond, self.block()))
            if not self.check("elseif"):
                break
        if self.check("else"):
            else_tok = self.tok
            self.i += 1
            out.children.append(self.node("Else", else_tok, None, self.block()))
        self.expect("end", start)
        return out

    def while_stat(self) -> Node:
        """``while c do b end``.

        Returns:
            The ``While`` node.
        """
        start = self.tok
        self.i += 1
        cond = self.expr()
        self.expect("do")
        body = self.block()
        self.expect("end", start)
        return self.node("While", start, None, cond, body)

    def do_stat(self) -> Node:
        """``do b end``.

        Returns:
            The ``Do`` node.
        """
        start = self.tok
        self.i += 1
        body = self.block()
        self.expect("end", start)
        return self.node("Do", start, None, body)

    def repeat_stat(self) -> Node:
        """``repeat b until c``.

        Returns:
            The ``Repeat`` node.
        """
        start = self.tok
        self.i += 1
        body = self.block()
        self.expect("until", start)
        return self.node("Repeat", start, None, body, self.expr())

    def for_stat(self) -> Node:
        """A numeric or generic ``for``.

        Returns:
            The ``ForNum`` or ``ForIn`` node.
        """
        start = self.tok
        self.i += 1
        first = self.name()
        if self.accept("="):
            parts = [self.node("Name", first, first.text), self.expr()]
            self.expect(",")
            parts.append(self.expr())
            if self.accept(","):
                parts.append(self.expr())
            self.expect("do")
            parts.append(self.block())
            self.expect("end", start)
            return self.node("ForNum", start, None, *parts)
        names = self.node("Names", first, None, self.node("Name", first, first.text))
        while self.accept(","):
            n = self.name()
            names.children.append(self.node("Name", n, n.text))
        self.expect("in")
        exprs = self.node("Exprs", self.tok, None, *self.exprlist())
        self.expect("do")
        body = self.block()
        self.expect("end", start)
        return self.node("ForIn", start, None, names, exprs, body)

    def function_stat(self) -> Node:
        """``function a.b:c(params) body end``.

        Returns:
            The ``FunctionStat`` node.
        """
        start = self.tok
        self.i += 1
        parts = [self.name().text]
        method = False
        while self.check(".") or self.check(":"):
            sep = self.tok.text
            self.i += 1
            parts.append(sep + self.name().text)
            if sep == ":":
                method = True
                break
        fn = self.function_body(start, method=method)
        return self.node("FunctionStat", start, "".join(parts), fn)

    def local_stat(self, scope: str = "local") -> Node:
        """``local function f ...`` or ``local a, b = ...`` (Teal: also ``global``).

        Args:
            scope: ``local``, or ``global`` for Teal's global declarations.

        Returns:
            The ``LocalFunction``, ``FunctionStat`` (a global function), ``Local`` or
            (Teal) ``TypeDecl`` node.
        """
        start = self.tok
        self.i += 1
        if self.accept("function"):
            name = self.name()
            kind = "LocalFunction" if scope == "local" else "FunctionStat"
            return self.node(kind, start, name.text, self.function_body(start))
        t = self.tok
        if self.teal and t.kind == "name" and self.peek().kind == "name":
            if t.text in _TYPE_DECLS:
                return self.type_decl()
            if t.text == "macroexp":  # a compile-time macro; read as a function
                self.i += 1
                name = self.name()
                return self.node("LocalFunction", start, name.text, self.function_body(start))
        first = self.name()
        names = self.node("Names", first, None, self.node("Name", first, first.text))
        self.annotation()
        while self.accept(","):
            if self.teal and self.tok.kind != "name":
                self.skip_type(multi=True)  # ``local a, b: A, B``: the types after the names
                continue
            n = self.name()
            names.children.append(self.node("Name", n, n.text))
            self.annotation()
        exprs = self.node("Exprs", self.tok)
        if self.accept("="):
            exprs.children = self.exprlist()
        return self.node("Local", start, None if scope == "local" else scope, names, exprs)

    # -- Teal's types, read and set aside -----------------------------------------------
    def annotation(self) -> None:
        """After a declared name: a Lua 5.4 ``<attrib>`` and a Teal ``: type``."""
        if not self.teal:
            return
        if self.check("<"):
            self.i += 1
            self.name()
            self.expect(">")
        if self.accept(":"):
            self.skip_type(multi=True)

    def is_method_call(self) -> bool:
        """At ``name :``: is it ``obj:method(...)`` rather than a typed field ``x: T = v``?

        Returns:
            True for a method call.
        """
        after = self.peek(3)
        return self.peek(2).kind == "name" and (after.text in ("(", "{") or after.kind == "string")

    def type_decl(self) -> Node:
        """``record``/``enum``/``interface`` ``Name ... end``, or ``type Name = T``.

        Returns:
            The ``TypeDecl`` node.
        """
        kw = self.tok
        self.i += 1
        name = self.name()
        if kw.text == "type":
            if self.check("<"):
                self.skip_brackets("<", ">")  # a generic alias: ``type Map<K> = ...``
            if self.accept("="):
                t = self.tok
                if t.kind == "name" and t.text == "require":
                    self.expr()
                else:
                    self.skip_type(multi=True)
        else:
            if self.check("<"):
                self.skip_brackets("<", ">")
            self.skip_decl_body(kw)
        return self.node("TypeDecl", kw, f"{kw.text} {name.text}")

    def skip_decl_body(self, opener: Token) -> None:
        """Skip a record/enum/interface body to its ``end`` (nested ones included).

        Args:
            opener: The ``record``/``enum``/``interface`` token, for errors.

        Raises:
            LuaSyntaxError: The body never ends.
        """
        depth = 1
        while depth:
            t = self.tok
            if t.kind == "eof":
                raise LuaSyntaxError(
                    f"{opener.text} at line {opener.line} is never closed", t.line, t.col
                )
            if t.kind == "keyword" and t.text == "end":
                depth -= 1
            elif t.kind == "name" and t.text in ("record", "enum", "interface"):
                nxt = self.peek()
                if nxt.kind == "name" or nxt.text == "<":
                    depth += 1
            self.i += 1

    def skip_brackets(self, open_: str, close: str) -> None:
        """Skip from an opening bracket to its match (``>>`` closes two ``<``).

        Args:
            open_: The opening bracket, at the current token.
            close: Its closing bracket.

        Raises:
            LuaSyntaxError: It is never closed.
        """
        start = self.tok
        depth = 0
        while True:
            t = self.tok
            if t.kind == "eof":
                raise LuaSyntaxError(f"{open_!r} is never closed", start.line, start.col)
            if t.kind == "op":
                if t.text == open_:
                    depth += 1
                elif t.text == close:
                    depth -= 1
                elif open_ == "<" and t.text == ">>":
                    depth -= 2
            self.i += 1
            if depth <= 0:
                return

    def skip_type(self, multi: bool = False) -> None:
        """Skip one Teal type, unions (``A | B``) included.

        Args:
            multi: A function type here may return a list (``function(): A, B``) - in a
                declaration's annotation, where no comma can follow; not in a parameter
                list or a table constructor, where a comma ends the type.
        """
        self.skip_type_atom(multi)
        while self.accept("|"):
            self.skip_type_atom(multi)

    def skip_type_atom(self, multi: bool = False) -> None:
        """Skip one type without unions.

        A name (``a.b<T>``), ``{...}``, ``(...)``, a function type, a string (an enum
        value), ``nil``, ``...`` or an inline record.

        Args:
            multi: As for :meth:`skip_type`.
        """
        t = self.tok
        if t.kind == "keyword" and t.text == "function":
            self.i += 1
            if self.check("<"):
                self.skip_brackets("<", ">")
            if self.check("("):
                self.skip_brackets("(", ")")
            if self.accept(":"):
                if multi:
                    self.skip_returns()
                else:
                    self.skip_type()
                    self.accept("...")
            return
        if t.kind == "string" or t.text in ("nil", "..."):
            self.i += 1
            return
        if self.check("{"):
            self.skip_brackets("{", "}")
            return
        if self.check("("):
            self.skip_brackets("(", ")")
            return
        if t.kind == "name" and t.text in ("record", "enum", "interface"):
            self.i += 1
            self.skip_decl_body(t)
            return
        self.name()
        while self.accept("."):
            self.name()
        if self.check("<"):
            self.skip_brackets("<", ">")

    def skip_returns(self) -> None:
        """Skip a function's return types: ``T``, ``T, U``, ``(T, U)``, ``T...``."""
        self.skip_type()
        if self.accept("..."):
            return
        while self.accept(","):
            self.skip_type()
        self.accept("...")

    def goto_stat(self) -> Node:
        """``goto label`` (LuaJIT). ``goto`` is also a valid *name* in plain 5.1.

        Returns:
            The ``Goto`` node, or an expression statement when ``goto`` is a name.
        """
        start = self.tok
        if self.peek().kind != "name":
            return self.expr_stat()
        self.i += 1
        return self.node("Goto", start, self.name().text)

    def expr_stat(self) -> Node:
        """A call statement or an assignment.

        Returns:
            The ``CallStat`` or ``Assign`` node.

        Raises:
            LuaSyntaxError: An expression that is neither.
        """
        start = self.tok
        target = self.suffixed_expr()
        if self.check("=") or self.check(","):
            targets = self.node("Targets", start, None, target)
            while self.accept(","):
                targets.children.append(self.suffixed_expr())
            for tnode in targets.children:
                if tnode.kind not in ("Name", "Index"):
                    raise LuaSyntaxError("cannot assign to this", tnode.line, tnode.col)
            self.expect("=")
            exprs = self.node("Exprs", self.tok, None, *self.exprlist())
            return self.node("Assign", start, None, targets, exprs)
        if target.kind not in ("Call", "Method"):
            raise LuaSyntaxError("a statement cannot start here", start.line, start.col)
        return self.node("CallStat", start, None, target)

    # -- expressions ----------------------------------------------------------------
    def exprlist(self) -> list[Node]:
        """``expr {, expr}``.

        Returns:
            The expressions.
        """
        out = [self.expr()]
        while self.accept(","):
            out.append(self.expr())
        return out

    def expr(self, limit: int = 0) -> Node:
        """An expression whose binary operators bind tighter than ``limit``.

        Args:
            limit: The binding power to stop at (0 for a whole expression).

        Returns:
            The expression's node.

        Raises:
            LuaSyntaxError: A 5.3 operator, or no expression.
        """
        t = self.tok
        unary = (t.kind == "keyword" and t.text == "not") or (
            t.kind == "op" and t.text in ("-", "#", "~")
        )
        if unary:
            if t.text == "~" and not self.teal:
                raise self.error("'~' (bitwise not) is not Lua 5.1 / LuaJIT")
            self.i += 1
            left = self.node("Unop", t, t.text, self.expr(_UNARY_PRIORITY))
        else:
            left = self.simple_expr()
        while True:
            op = self.tok
            if self.teal and op.kind == "name" and op.text in ("as", "is"):
                self.i += 1
                self.skip_type()
                left = Node("Cast", left.line, left.col, op.text, [left])
                continue
            if op.kind not in ("op", "keyword"):
                return left
            teal_op = self.teal and op.kind == "op" and op.text in _BINARY_TEAL
            if not teal_op and (op.text in _NOT_51 or (op.kind == "op" and op.text == "~")):
                raise self.error(f"{op.text!r} is not Lua 5.1 / LuaJIT (it is Lua 5.3)")
            prio = _BINARY_TEAL[op.text] if teal_op else _BINARY.get(op.text)
            if prio is None or prio[0] <= limit:
                return left
            self.i += 1
            right = self.expr(prio[1])
            left = Node("Binop", left.line, left.col, op.text, [left, right])

    def simple_expr(self) -> Node:
        """A literal, a function, a table constructor or a suffixed expression.

        Returns:
            The node.
        """
        t = self.tok
        simple = {"nil": "Nil", "true": "True", "false": "False", "...": "Vararg"}
        if t.kind in ("keyword", "op") and t.text in simple:
            self.i += 1
            return self.node(simple[t.text], t)
        if t.kind == "number":
            self.i += 1
            return self.node("Number", t, t.value)
        if t.kind == "string":
            self.i += 1
            return self.node("String", t, t.value)
        if self.check("{"):
            return self.table()
        if self.check("function"):
            self.i += 1
            return self.function_body(t)
        return self.suffixed_expr()

    def primary_expr(self) -> Node:
        """A name or a parenthesized expression.

        Returns:
            The node.

        Raises:
            LuaSyntaxError: Neither is there.
        """
        t = self.tok
        if t.kind == "name":
            self.i += 1
            return self.node("Name", t, t.text)
        if self.accept("("):
            inner = self.expr()
            self.expect(")", t)
            return self.node("Paren", t, None, inner)
        raise self.error(f"unexpected symbol near {t.text or '<eof>'!r}")

    def suffixed_expr(self) -> Node:
        """A primary expression followed by ``.x``, ``[e]``, ``:m(args)``, ``(args)``.

        Returns:
            The node.
        """
        e = self.primary_expr()
        while True:
            t = self.tok
            if self.accept("."):
                key = self.name()
                e = Node("Index", e.line, e.col, ".", [e, self.node("String", key, key.text)])
            elif self.accept("["):
                key_expr = self.expr()
                self.expect("]", t)
                e = Node("Index", e.line, e.col, "[]", [e, key_expr])
            elif self.accept(":"):
                m = self.name()
                e = Node("Method", e.line, e.col, m.text, [e, *self.call_args()])
            elif self.check("(") or self.check("{") or t.kind == "string":
                if self.check("(") and t.line != self.toks[self.i - 1].line:
                    # Lua 5.1: "ambiguous syntax (function call x new statement)". Teal
                    # reads it as a new statement.
                    if self.teal:
                        return e
                    raise self.error("ambiguous syntax: '(' on a new line after an expression")
                e = Node("Call", e.line, e.col, None, [e, *self.call_args()])
            else:
                return e

    def call_args(self) -> list[Node]:
        """``(explist)``, a table constructor, or a string.

        Returns:
            The argument expressions.

        Raises:
            LuaSyntaxError: No arguments are there.
        """
        t = self.tok
        if t.kind == "string":
            self.i += 1
            return [self.node("String", t, t.value)]
        if self.check("{"):
            return [self.table()]
        self.expect("(")
        if self.accept(")"):
            return []
        args = self.exprlist()
        self.expect(")", t)
        return args

    def table(self) -> Node:
        """``{ fields }``.

        Returns:
            The ``Table`` node.
        """
        start = self.expect("{")
        out = self.node("Table", start)
        while not self.check("}"):
            t = self.tok
            if self.accept("["):
                key = self.expr()
                self.expect("]", t)
                self.expect("=")
                out.children.append(self.node("Field", t, None, key, self.expr()))
            elif (
                self.teal
                and t.kind == "name"
                and self.peek().text == ":"
                and not self.is_method_call()
            ):
                self.i += 2  # Teal: ``name: T = value``
                self.skip_type()
                self.expect("=")
                key = self.node("String", t, t.text)
                out.children.append(self.node("Field", t, t.text, key, self.expr()))
            elif t.kind == "name" and self.peek().kind == "op" and self.peek().text == "=":
                self.i += 2
                key = self.node("String", t, t.text)
                out.children.append(self.node("Field", t, t.text, key, self.expr()))
            else:
                out.children.append(self.node("Item", t, None, self.expr()))
            if not (self.accept(",") or self.accept(";")):
                break
        self.expect("}", start)
        return out

    def function_body(self, start: Token, *, method: bool = False) -> Node:
        """``(params) block end``.

        Args:
            start: The token that began the function (for the node and errors).
            method: A ``function a:b()``: ``self`` is the first parameter.

        Returns:
            The ``Function`` node.
        """
        if self.teal and self.check("<"):
            self.skip_brackets("<", ">")
        open_tok = self.expect("(")
        params = self.node("Params", open_tok)
        if method:
            params.children.append(self.node("Name", open_tok, "self"))
        if not self.check(")"):
            while True:
                if self.check("..."):
                    params.children.append(self.node("Vararg", self.tok))
                    self.i += 1
                    if self.teal and self.accept(":"):
                        self.skip_type()
                    break
                n = self.name()
                params.children.append(self.node("Name", n, n.text))
                if self.teal:
                    self.accept("?")
                    if self.accept(":"):
                        self.skip_type()
                if not self.accept(","):
                    break
        self.expect(")", open_tok)
        if self.teal and self.accept(":"):
            self.skip_returns()
        body = self.block()
        self.expect("end", start)
        return self.node("Function", start, None, params, body)


def parse(src: str, *, teal: bool = False) -> Node:
    """Parse a Lua chunk (or, with ``teal``, a Teal one).

    Args:
        src: The source text.
        teal: It is Teal (a ``.tl`` file): read its types and set them aside.

    Returns:
        The chunk's ``Block``.

    Raises:
        LuaSyntaxError: The source is not valid Lua 5.1 / LuaJIT (or Teal).
    """
    p = _Parser(tokenize(src, teal=teal), teal=teal)
    chunk = p.block()
    if p.tok.kind != "eof":
        raise p.error(f"{p.tok.text!r} where the chunk should have ended")
    return chunk
