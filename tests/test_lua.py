"""wraithguard.lua: the OpenMW Lua reader, parser and checks."""

from __future__ import annotations

import struct
from typing import TYPE_CHECKING

import pytest

from wraithguard.lua.__main__ import main
from wraithguard.lua.analysis import analyze
from wraithguard.lua.api import contexts_for_flags
from wraithguard.lua.flowchart import expr_text, flowchart, flowchart_html
from wraithguard.lua.lexer import LuaSyntaxError, tokenize
from wraithguard.lua.lual import read_lual
from wraithguard.lua.omwscripts import parse_omwscripts
from wraithguard.lua.parser import Node, parse
from wraithguard.lua.report import all_findings, render
from wraithguard.lua.scan import read_cfg_lua, scan_load_order

if TYPE_CHECKING:
    from pathlib import Path

# -- lexer ---------------------------------------------------------------------------


def test_tokens_kinds_and_comments():
    toks = tokenize('local x = 0x1F + 1.5e3 .. "a" -- note\nreturn x', comments=True)
    kinds = [(t.kind, t.text) for t in toks]
    assert kinds == [
        ("keyword", "local"),
        ("name", "x"),
        ("op", "="),
        ("number", "0x1F"),
        ("op", "+"),
        ("number", "1.5e3"),
        ("op", ".."),
        ("string", '"a"'),
        ("comment", "-- note"),
        ("keyword", "return"),
        ("name", "x"),
        ("eof", ""),
    ]
    assert toks[-2].line == 2
    assert all(t.kind != "comment" for t in tokenize("x = 1 --[==[ long\ncomment ]==] y = 2"))


def test_string_values():
    assert tokenize("x = [==[\nhi]==]")[2].value == "hi"
    assert tokenize(r'x = "\65\x42\n\z    C"')[2].value == "AB\nC"
    assert tokenize("x = 'it''s'")[2].value == "it"


@pytest.mark.parametrize("src", ['x = "open', "x = [[never closed", "x = 3x", "x = @"])
def test_lexer_errors(src):
    with pytest.raises(LuaSyntaxError):
        tokenize(src)


# -- parser --------------------------------------------------------------------------


def _shape(node: Node) -> object:
    """A compact (kind, value, children) form for comparisons."""
    if not node.children:
        return node.value if node.value is not None else node.kind
    return (node.kind, node.value, [_shape(c) for c in node.children])


def test_precedence_and_associativity():
    ret = parse("return 1 + 2 * 3 ^ 2 ^ 1").children[0]
    expr = ret.children[0].children[0]
    assert _shape(expr) == (
        "Binop",
        "+",
        ["1", ("Binop", "*", ["2", ("Binop", "^", ["3", ("Binop", "^", ["2", "1"])])])],
    )
    neg = parse("return -2 ^ 2").children[0].children[0].children[0]
    assert neg.kind == "Unop"
    assert neg.children[0].kind == "Binop"
    cat = parse("return a .. b .. c").children[0].children[0].children[0]
    assert cat.children[1].kind == "Binop"  # right associative


def test_statements_parse():
    chunk = parse("""
        local function f(a, ...) return ... end
        function M.sub:go(x) return self, x end
        for i = 1, 10, 2 do if i > 3 then break elseif i then goto skip else end end
        for k, v in pairs(t) do t[k] = v end
        while false do end
        repeat local y = 1 until y
        ::skip::
        obj:method "s"
        f{1, 2; x = 3, [4] = 5}
        a, b.c = 1, 2
        """)
    kinds = [s.kind for s in chunk.children]
    assert kinds == [
        "LocalFunction",
        "FunctionStat",
        "ForNum",
        "ForIn",
        "While",
        "Repeat",
        "Label",
        "CallStat",
        "CallStat",
        "Assign",
    ]
    method = chunk.children[1]
    assert method.value == "M.sub:go"
    assert method.children[0].children[0].children[0].value == "self"
    table = chunk.children[8].children[0].children[1]
    assert [c.kind for c in table.children] == ["Item", "Item", "Field", "Field"]


@pytest.mark.parametrize(
    "src",
    ["x = = 1", "return 1 // 2", "x = a & b", "f(", "if x then", "1 + 1", "local function() end"],
)
def test_parse_errors(src):
    with pytest.raises(LuaSyntaxError):
        parse(src)


TEAL = """
local record Point
   x: number
   y: number
   record Inner
      z: {string: integer}
   end
   metamethod __add: function(Point, Point): Point
end
local enum Mode
   "fast"
   "slow"
end
local type Callback = function(string, ...: any): integer, string
local type Map<K> = {K: {Point}}
global VERSION: string = "1"
global function hello(name?: string, ...: number): string, integer
   local a, b: number, string = 1, "x"
   local c <const>: Point = { x = 1, y = 2 }
   local t = { label: string = "p", self:get(1) }
   local p = (a as number) // 2
   if c is Point then return name or "", p end
   return "", 0
end
local function first<T>(xs: {T}): T
   return xs[1]
end
"""


def test_teal_parses():
    chunk = parse(TEAL, teal=True)
    kinds = [s.kind for s in chunk.children]
    assert kinds == [
        "TypeDecl",
        "TypeDecl",
        "TypeDecl",
        "TypeDecl",
        "Local",
        "FunctionStat",
        "LocalFunction",
    ]
    assert chunk.children[0].value == "record Point"
    assert chunk.children[4].value == "global"
    assert any(n.kind == "Cast" for n in chunk.walk())
    with pytest.raises(LuaSyntaxError):
        parse(TEAL)  # not Lua
    assert '{"if c is ..."}' in flowchart(chunk.children[5])


def test_walk_and_label():
    chunk = parse("local s = 'hello'")
    assert [n.kind for n in chunk.walk()][:3] == ["Block", "Local", "Names"]
    assert any(n.label() == "String 'hello'" for n in chunk.walk())


# -- .omwscripts -------------------------------------------------------------------------


def test_omwscripts():
    text = "# c\nGLOBAL: scripts/a.lua\nNPC, creature: Scripts\\B.lua\nBOGUS: x.lua\nnot a line\n"
    out = parse_omwscripts(text, "Mod.omwscripts")
    assert [e.path for e in out.entries] == ["scripts/a.lua", "Scripts\\B.lua", "x.lua"]
    assert out.entries[1].flags == ("NPC", "CREATURE")
    assert out.entries[1].key == "scripts/b.lua"
    assert [line for line, _ in out.problems] == [4, 5]


def test_omwscripts_space_separated_flags():
    out = parse_omwscripts("PLAYER NPC CREATURE: scripts/x.lua\n", "M.omwscripts")
    assert out.entries[0].flags == ("PLAYER", "NPC", "CREATURE")
    assert not out.problems


def test_guarded_and_inventory_loops():
    src = (
        "local nearby = require('openmw.nearby')\n"
        "local types = require('openmw.types')\n"
        "local t = 0\n"
        "local function onUpdate(dt)\n"
        "  for _, i in ipairs(types.Actor.inventory(self):getAll()) do end\n"
        "  if t > 1 then for _, a in ipairs(nearby.actors) do print(a) end end\n"
        "  if dt == 0 then return end\n"
        "  for _, a in ipairs(nearby.items) do end\n"
        "end\n"
        "return { engineHandlers = { onUpdate = onUpdate } }\n"
    )
    found = {(f.code, f.line): f.severity for f in analyze(src).findings}
    assert ("PERF_LOOP", 5) not in found  # one actor's inventory
    assert found[("PERF_LOOP", 6)] == "info"  # under a condition
    assert ("PERF_PRINT", 6) not in found
    assert found[("PERF_LOOP", 8)] == "info"  # after an early return


def _rec(tag: bytes, body: bytes) -> bytes:
    return tag + struct.pack("<I", len(body)) + b"\0" * 8 + body


def _sub(tag: bytes, data: bytes) -> bytes:
    return tag + struct.pack("<I", len(data)) + data


def test_lual_records(tmp_path):
    lual = (
        _sub(b"LUAS", b"scripts/a.lua\0")
        + _sub(b"LUAF", struct.pack("<I", 1 << 2) + b"NPC_")
        + _sub(b"LUAD", b"init")
        + _sub(b"LUAS", b"scripts/b.lua\0")
        + _sub(b"LUAF", struct.pack("<I", 0))
        + _sub(b"LUAR", b"\x01some_id")
    )
    addon = tmp_path / "Mod.omwaddon"
    addon.write_bytes(_rec(b"TES3", _sub(b"HEDR", b"\0" * 300)) + _rec(b"LUAL", lual))
    out = read_lual(addon, "Mod.omwaddon")
    assert [(e.path, e.flags) for e in out.entries] == [
        ("scripts/a.lua", ("PLAYER", "NPC")),
        ("scripts/b.lua", ("CUSTOM",)),
    ]
    assert not out.problems
    _write(tmp_path / "scripts" / "a.lua", "return {}\n")
    result = scan_load_order([tmp_path], ["Mod.omwaddon"])
    assert result.lual_files == ["Mod.omwaddon"]
    missing = {p for p, f in all_findings(result) if f.code == "MISSING_SCRIPT"}
    assert missing == {"scripts/b.lua"}


def test_expr_text():
    expr = parse("return a.b:c(1, 'x') + -y, not z").children[0].children[0].children
    assert [expr_text(e) for e in expr] == ["a.b:c(1, 'x') + -y", "not z"]


def test_flowchart():
    src = (
        "local x = 1\n"
        "if x then return 1 elseif y then x = 2 else print(x) end\n"
        "while x do break end\n"
        "for i = 1, 3 do end\n"
        "repeat x = x + 1 until x > 3\n"
        "goto done\n"
        "::done::\n"
    )
    chart = flowchart(parse(src))
    assert chart.startswith("flowchart TD")
    for part in ('{"if x"}', '{"if y"}', '{"while x"}', '{"for i = 1, 3"}', "until x &gt; 3"):
        assert part in chart
    assert "-->|yes|" in chart
    assert '["::done::"]' in chart
    fn = parse("local function f(a) if a then return 1 end return 2 end").children[0]
    assert '{"if a"}' in flowchart(fn)
    assert "mermaid" in flowchart_html("t", [("t", chart)])


def test_contexts():
    assert contexts_for_flags(("GLOBAL",)) == {"global"}
    assert contexts_for_flags(("NPC", "PLAYER")) == {"local", "player"}
    assert contexts_for_flags(("CUSTOM",)) == {"local", "player"}


# -- one script ----------------------------------------------------------------------------

SCRIPT = """
local nearby = require('openmw.nearby')
local core = require('openmw.core')
local function count()
  local n = 0
  for _, a in ipairs(nearby.actors) do
    for _, b in ipairs(nearby.actors) do n = n + 1 end
  end
  return n
end
local function onUpdate(dt)
  count()
  core.sendGlobalEvent('MyEvent', {})
end
return {
  interfaceName = 'Mine',
  interface = { version = 1, get = function() return 1 end },
  engineHandlers = { onUpdate = onUpdate, onKeyPress = function(k) end },
  eventHandlers = { Other = function() end },
}
"""


def test_analyze_declarations_and_perf():
    info = analyze(SCRIPT, frozenset({"player"}))
    assert info.interface_name == "Mine"
    assert info.interface_members == ["get", "version"]
    assert set(info.engine_handlers) == {"onUpdate", "onKeyPress"}
    assert set(info.event_handlers) == {"Other"}
    assert info.sent_events == {"MyEvent": 13}
    assert set(info.requires) == {"openmw.nearby", "openmw.core"}
    codes = {(f.code, f.line) for f in info.findings}
    assert ("PERF_LOOP", 6) in codes
    assert ("PERF_NESTED", 7) in codes
    assert not any(f.code in ("PACKAGE_CONTEXT", "HANDLER_CONTEXT") for f in info.findings)


def test_analyze_context_errors():
    info = analyze(SCRIPT, frozenset({"global"}))
    codes = {f.code for f in info.findings}
    assert "PACKAGE_CONTEXT" in codes  # openmw.nearby in a global script
    assert "HANDLER_CONTEXT" in codes  # onKeyPress in a global script


def test_analyze_local_table_and_typos():
    src = (
        "local M = {}\n"
        "M.engineHandlers = { onUpdte = function() end }\n"
        "M.interfaceName = 'X'\n"
        "return M\n"
    )
    info = analyze(src, frozenset({"global"}))
    assert info.interface_name == "X"
    assert "onUpdte" in info.engine_handlers
    assert any(f.code == "UNKNOWN_HANDLER" for f in info.findings)


def test_analyze_syntax_error():
    info = analyze("return {")
    assert info.tree is None
    assert [f.code for f in info.findings] == ["SYNTAX"]


# -- the load order ---------------------------------------------------------------------


def _write(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")


def test_scan_load_order(tmp_path):
    d1, d2 = tmp_path / "mod1", tmp_path / "mod2"
    _write(
        d1 / "My.omwscripts",
        "GLOBAL: scripts/m/g.lua\nNPC: scripts/m/actor.lua\nPLAYER: scripts/m/missing.lua\n",
    )
    _write(d1 / "scripts" / "m" / "g.lua", "return {}\n")
    _write(
        d1 / "scripts" / "m" / "actor.lua",
        "return { engineHandlers = { onUpdate = function(dt) end } }\n",
    )
    _write(d2 / "Other.omwscripts", "GLOBAL: Scripts/M/G.lua\nGLOBAL: scripts/o/o.lua\n")
    _write(
        d2 / "Scripts" / "M" / "G.lua",
        "local core = require('openmw.core')\ncore.sendGlobalEvent('Nobody')\n"
        "return { interfaceName = 'Shared', interface = {} }\n",
    )
    _write(d2 / "scripts" / "o" / "o.lua", "return { interfaceName = 'Shared', interface = {} }\n")
    content = ["Morrowind.esm", "My.omwscripts", "Other.omwscripts", "Gone.omwscripts"]
    content.append("x.omwaddon")

    result = scan_load_order([d1, d2], content)

    assert result.omwaddons == ["x.omwaddon"]
    assert [r.path for r in result.scripts] == [
        "scripts/m/g.lua",
        "scripts/m/actor.lua",
        "scripts/m/missing.lua",
        "scripts/o/o.lua",
    ]
    g = result.scripts[0]
    assert len(g.providers) == 2
    assert g.file is not None
    assert g.file.parent.parent.parent == d2
    assert g.info is not None
    assert g.info.interface_name == "Shared"
    codes = {(path, f.code) for path, f in all_findings(result)}
    assert ("", "MISSING_OMWSCRIPTS") in codes
    assert ("scripts/m/missing.lua", "MISSING_SCRIPT") in codes
    assert ("scripts/m/g.lua", "DUPLICATE_REGISTRATION") in codes
    assert ("scripts/m/g.lua", "OVERRIDDEN_FILE") in codes
    assert ("scripts/m/g.lua", "UNHANDLED_EVENT") in codes
    assert ("scripts/o/o.lua", "INTERFACE_OVERRIDE") in codes
    assert ("scripts/m/actor.lua", "PER_ACTOR_FRAME") in codes
    text = render(result)
    assert "MISSING_SCRIPT" in text
    assert "scripts/m/actor.lua  (NPC)  [onUpdate]" in text


def test_read_cfg_and_main(tmp_path, capsys):
    mod = tmp_path / "mods" / "a"
    _write(mod / "A.omwscripts", "GLOBAL: scripts/a.lua\n")
    cfg = tmp_path / "openmw.cfg"
    cfg.write_text(
        'data="mods/a"\ndata=mods/b\ndata-local=local\n'
        "content=Morrowind.esm\ncontent=A.omwscripts\n#content=no.omwscripts\n",
        encoding="utf-8",
    )
    dirs, content = read_cfg_lua(cfg)
    assert dirs == [tmp_path / "mods/a", tmp_path / "mods/b", tmp_path / "local"]
    assert content == ["Morrowind.esm", "A.omwscripts"]
    assert main([str(cfg)]) == 1  # scripts/a.lua is missing
    assert "MISSING_SCRIPT" in capsys.readouterr().out
