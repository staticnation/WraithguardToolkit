"""wraithguard.lua: the OpenMW Lua reader, parser and checks."""

from __future__ import annotations

import struct
from typing import TYPE_CHECKING

import pytest

from wraithguard.lua.__main__ import main
from wraithguard.lua.analysis import analyze
from wraithguard.lua.api import contexts_for_flags
from wraithguard.lua.callgraph import MAIN, call_graph, call_graph_chart
from wraithguard.lua.flowchart import expr_text, flowchart, flowchart_html
from wraithguard.lua.lexer import LuaSyntaxError, tokenize
from wraithguard.lua.lual import read_lual
from wraithguard.lua.omwscripts import parse_omwscripts
from wraithguard.lua.parser import Node, parse
from wraithguard.lua.report import all_findings, render
from wraithguard.lua.scan import read_cfg_lua, scan_load_order
from wraithguard.viz.library import mermaid_source

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


def test_flowchart_html_loads_the_bundled_library():
    served = flowchart_html("t", [("t", "flowchart TD")], library_url="http://h/m.js?t=a&b")
    assert "<script src='http://h/m.js?t=a&amp;b'></script>" in served
    assert "cdn" not in served.lower()
    # Inlined (a page opened from disk): nothing in it may end the script element.
    library = mermaid_source()
    assert 'globalThis["mermaid"]' in library
    page = flowchart_html("t", [("t", "flowchart TD")], library=library)
    body = page.split("<script>", 1)[1]
    assert "</script" not in body.split("</script>", 1)[0]
    assert "<!--" not in page
    inlined = flowchart_html("t", [], library="var s = '</script><!--';")
    assert "var s = '<\\/script><\\x21--';" in inlined


_CALLS_SRC = """
local M = {}
local function helper(x) return x * 2 end
function M.step(dt) return helper(dt) end
function M:tick() self:other() end
function M:other() end
local function unused() end
local update = function(dt)
  M.step(dt)
  time.runRepeatedly(function() helper(1) end, 1)
  nearby.castRay(1, 2)
end
local util = { scale = function(v) return helper(v) end }
helper(3)
return {
  engineHandlers = {
    onUpdate = update,
    onSave = function() return util.scale(1) end,
  },
  eventHandlers = { Foo = M.tick },
}
"""


def test_call_graph():
    graph = call_graph(parse(_CALLS_SRC))
    assert list(graph.functions) == [
        "helper",
        "M.step",
        "M.tick",
        "M.other",
        "unused",
        "update",
        "util.scale",
        "engineHandlers.onSave",
    ]
    assert graph.entries == [
        "engineHandlers.onSave",
        "engineHandlers.onUpdate",
        "eventHandlers.Foo",
    ]
    assert set(graph.calls) == {
        (MAIN, "helper"),
        ("M.step", "helper"),
        ("M.tick", "M.other"),  # self:other()
        ("update", "M.step"),
        ("update", "helper"),  # from the anonymous callback written inside it
        ("util.scale", "helper"),
        ("engineHandlers.onSave", "util.scale"),
        ("engineHandlers.onUpdate", "update"),
        ("eventHandlers.Foo", "M.tick"),
    }


def test_call_graph_chart():
    chart = call_graph_chart(parse(_CALLS_SRC))
    assert chart.startswith("flowchart LR")
    assert '(["engineHandlers.onUpdate"])' in chart  # an entry point
    assert '["helper<br/>line 3"]' in chart
    assert '["unused<br/>line 7"]' in chart  # called by nothing, still shown
    assert "castRay" not in chart
    # Nothing calls from the top level: no main-chunk node.
    assert MAIN not in call_graph_chart(parse("local function f() end return {}"))


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


# -- garbage and the sandbox ------------------------------------------------------------

_GC_SRC = """local nearby = require('openmw.nearby')
local util = require('openmw.util')
local cache = {}
local function helper(a)
  return { a = a }
end
local function onUpdate(dt)
  local opts = { ignore = nil }
  for _, actor in ipairs(nearby.actors) do
    local pos = util.vector3(1, 2, 3)
    local label = 'actor ' .. tostring(actor)
    local cb = function() return actor end
    helper(actor)
  end
  for i = 1, 10 do
    cache[i] = { i, nested = { i } }
  end
  if dt > 1 then
    local guarded = {}
  end
  local msg = 'once ' .. dt
  return opts, pos, label, cb, msg
end
return { engineHandlers = { onUpdate = onUpdate } }
"""


def test_garbage_made_every_frame():
    found = {(f.line, f.code, f.severity) for f in analyze(_GC_SRC, frozenset({"local"})).findings}
    assert (5, "GC_TABLE", "warn") in found  # helper's table, called once per actor
    assert (8, "GC_TABLE", "info") in found  # once a frame
    assert (10, "GC_USERDATA", "warn") in found
    assert (11, "GC_STRING", "warn") in found
    assert (12, "GC_CLOSURE", "warn") in found
    assert (16, "GC_TABLE", "warn") in found  # the nested table is the same construction
    assert sum(1 for f in found if f[0] == 16) == 1
    assert not any(f[0] in (19, 21) for f in found)  # guarded; a string outside loops
    # Not a per-frame handler: nothing reported.
    quiet = _GC_SRC.replace("onUpdate = onUpdate", "onActive = onUpdate")
    assert not [
        f for f in analyze(quiet, frozenset({"local"})).findings if f.code.startswith("GC_")
    ]


def test_what_the_sandbox_does_not_have():
    src = (
        "collectgarbage('collect')\n"
        "local f = loadstring('return 1')\n"
        "print(os.time(), os.clock())\n"
        "string.myext = 1\n"
        "local debug = require('openmw.debug')\n"
        "print(debug.toggleRenderMode, io)\n"
    )
    found = {(f.line, f.code) for f in analyze(src).findings if f.severity == "error"}
    assert found == {(1, "SANDBOX"), (2, "SANDBOX"), (3, "SANDBOX"), (4, "SANDBOX"), (6, "SANDBOX")}
    msgs = [f.message for f in analyze(src).findings]
    assert any("collectgarbage" in m and "make less garbage" in m for m in msgs)
    assert any(m.startswith("os.clock") for m in msgs) and not any(
        m.startswith("os.time") for m in msgs
    )
    assert not any(m.startswith("debug ") for m in msgs)  # the script's own `debug`


def test_scripts_held_in_archives_are_scanned(tmp_path, monkeypatch):
    """OpenMW reads scripts out of fallback archives, under every loose file."""
    native = pytest.importorskip("wraithguard_native")
    if not hasattr(native, "bsa_bytes"):
        pytest.skip("the backend cannot write an archive")
    from wraithguard.lua import scan as scan_mod

    monkeypatch.setattr(scan_mod, "_archive_cache", lambda: tmp_path / "cache")
    data = tmp_path / "Data Files"
    data.mkdir()
    (data / "Arch.bsa").write_bytes(
        native.bsa_bytes(
            [
                ("scripts\\arch\\main.lua", b"local x = 1\nreturn {}\n"),
                ("scripts\\arch\\both.lua", b"return { from = 'archive' }\n"),
                ("textures\\a.dds", b"DDS"),
            ]
        )
    )
    (data / "scripts" / "arch").mkdir(parents=True)
    (data / "scripts" / "arch" / "both.lua").write_text(
        "return { from = 'loose' }\n", encoding="utf-8"
    )
    (data / "arch.omwscripts").write_text(
        "GLOBAL: scripts/arch/main.lua\nGLOBAL: scripts/arch/both.lua\n", encoding="utf-8"
    )
    cfg = tmp_path / "openmw.cfg"
    cfg.write_text(
        f'data="{data}"\nfallback-archive=Arch.bsa\ncontent=arch.omwscripts\n', encoding="utf-8"
    )
    out = scan_mod.scan_cfg(cfg)
    recs = {r.path: r for r in out.scripts}
    main = recs["scripts/arch/main.lua"]
    assert main.file is not None and "Arch.bsa" in main.file.parts and main.info is not None
    both = recs["scripts/arch/both.lua"]
    assert both.file == data / "scripts" / "arch" / "both.lua"  # the loose file wins
    assert len(both.providers) == 2
    assert not any(f.code == "MISSING_SCRIPT" for _p, f in out.findings)
    assert not list((tmp_path / "cache").glob("**/a.dds"))  # only scripts come out
    assert scan_mod.extract_archive_scripts([data / "Arch.bsa"]) == [main.file.parents[2]]  # reused


def test_lual_without_the_backend_reads_the_same(tmp_path, monkeypatch):
    from wraithguard.lua import lual

    body = (
        _sub(b"LUAS", b"scripts/a.lua\0")
        + _sub(b"LUAF", struct.pack("<I", 1 << 2) + b"NPC_")
        + _sub(b"LUAS", b"scripts/b.lua\0")
        + _sub(b"LUAF", struct.pack("<I", 0))
        + _sub(b"LUAR", b"\x01some_id")
    )
    addon = tmp_path / "Mod.omwaddon"
    addon.write_bytes(_rec(b"TES3", _sub(b"HEDR", b"\0" * 300)) + _rec(b"LUAL", body))
    fast = read_lual(addon, "Mod.omwaddon")
    monkeypatch.setattr(lual, "_native_reader", lambda: None)
    slow = read_lual(addon, "Mod.omwaddon")
    assert [(e.path, e.flags) for e in fast.entries] == [(e.path, e.flags) for e in slow.entries]
    assert fast.problems == slow.problems == []


def test_published_teal_declarations_are_found_near_the_install(tmp_path):
    """OpenMW's ``teal_declarations`` (a CI download) beside or in the install."""
    from wraithguard.lua.openmw_api import find_teal_declarations

    resources = tmp_path / "OpenMW" / "resources"
    resources.mkdir(parents=True)
    assert find_teal_declarations(resources) is None
    decl = tmp_path / "OpenMW" / "teal_declarations" / "openmw"
    decl.mkdir(parents=True)
    (decl / "core.d.tl").write_text("local record core end return core\n", encoding="utf-8")
    assert find_teal_declarations(None, resources) == decl.parent


def test_cyan_global_env_def_is_read(tmp_path):
    """``global_env_def`` in a mod's tlconfig.lua names its globals' module."""
    from wraithguard.lua.openmw_api import tlconfig_globals

    mod = tmp_path / "mod"
    mod.mkdir()
    (mod / "tlconfig.lua").write_text(
        'return { include_dir = { "types" }, global_env_def = "my_globals" }\n', encoding="utf-8"
    )
    assert tlconfig_globals([mod, tmp_path / "missing"]) == ["my_globals"]


def test_rust_tokenizer_matches_the_python_one():
    """The Rust backend's tokens (when built) are the Python module's, errors included."""
    import pytest

    from wraithguard.lua.lexer import LuaSyntaxError, _native_tokenize, tokenize, tokenize_py

    native = _native_tokenize()
    if native is None:
        pytest.skip("the native module is not built")
    from pathlib import Path

    samples = [
        p.read_text(encoding="utf-8", errors="replace")
        for p in Path(__file__).parent.rglob("*.lua")
    ]
    samples += [
        '#!/bin/lua\nlocal x = 0x1p4 .. "a\\n\\65\\x41\\u{48}\\z  b" --[[ c ]] [==[\nlong]==] 3LL 2i',
        "local t = { a = 1.5e3, [ [[k]] ] = .5 } -- é ü\nreturn t?.a",
        "x = 'unfinished",
        "x = 1a",
        "x = '\\q'",
        "goto continue ::continue::",
    ]
    for src in samples:
        for comments in (False, True):
            for teal in (False, True):
                try:
                    want = tokenize_py(src, comments=comments, teal=teal)
                except LuaSyntaxError as exc:
                    with pytest.raises(LuaSyntaxError) as got:
                        tokenize(src, comments=comments, teal=teal)
                    assert (got.value.line, got.value.col, str(got.value)) == (
                        exc.line,
                        exc.col,
                        str(exc),
                    )
                    continue
                got_tokens = [tuple(t) for t in native(src, comments, teal)]
                assert got_tokens == [
                    (t.kind, t.text, t.start, t.line, t.col, t.value) for t in want
                ], src[:60]


def test_engine_handlers_follow_the_release():
    """Each release's table: 0.51 has load scripts, 0.50 not, 0.49 fewer built-in events."""
    from wraithguard.lua.api import API, API_049, API_050, for_version

    assert for_version("0.51.0") is API and for_version("0.52.0") is API
    assert for_version("0.50.1") is API_050 and for_version("0.49.0") is API_049
    assert for_version("0.48.0") is API_049 and for_version(None) is API
    assert "onContentFilesLoaded" in API.handlers
    assert "onContentFilesLoaded" not in API_050.handlers
    assert "load" not in API_050.handlers["onInterfaceOverride"]
    assert "ModifyStat" in API_050.builtin_events and "ModifyStat" not in API_049.builtin_events
    assert "Combat" not in API_049.builtin_interfaces


def test_rust_parser_matches_the_python_one():
    """The Rust backend's tree (when built) is the Python module's, node for node."""
    import pytest

    from wraithguard.lua.lexer import LuaSyntaxError
    from wraithguard.lua.parser import _native_parse, parse, parse_py

    if _native_parse() is None:
        pytest.skip("the native module is not built")
    from pathlib import Path

    samples = [
        (p.read_text(encoding="utf-8", errors="replace"), p.suffix == ".tl")
        for p in Path(__file__).parent.rglob("*.*l*")
        if p.suffix in (".lua", ".tl")
    ]
    samples += [
        ("local a, b = 1, 'x'\nfunction t.f:g(x, ...) return x .. 1 + 2 ^ -3 end", False),
        ("for i = 1, 10, 2 do goto continue ::continue:: end\nrepeat x = x + 1 until x > 3", False),
        ("local t = { a = 1, [2] = 3, 4; f = function() end }\nprint(#t, not t.a)", False),
        ("local record P x: number end\nlocal function f<T>(a?: T): string, integer end", True),
        ("global g: {string:number} = {}\nlocal x = y as integer\nlocal n = 1 // 2 | 3", True),
        ("x = = 1", False),
        ("a = 1 // 2", False),
        ("f\n(1)", False),
        ("if x then", False),
    ]
    for src, teal in samples:
        try:
            want = parse_py(src, teal=teal)
        except LuaSyntaxError as exc:
            with pytest.raises(LuaSyntaxError) as got:
                parse(src, teal=teal)
            assert (got.value.line, got.value.col, str(got.value)) == (exc.line, exc.col, str(exc))
            continue
        assert parse(src, teal=teal) == want, src[:60]


def test_rust_analysis_matches_the_python_one():
    """The Rust backend's analysis (when built) is the Python module's, finding for finding."""
    import pytest

    from wraithguard.lua.analysis import _native_analyze, analyze, analyze_py
    from wraithguard.lua.api import API, API_049

    if _native_analyze() is None:
        pytest.skip("the native module is not built")
    from pathlib import Path

    samples = [
        p.read_text(encoding="utf-8", errors="replace")
        for p in Path(__file__).parent.rglob("*.lua")
    ]
    samples += [
        "local nearby = require('openmw.nearby')\nlocal M = {}\nfunction M.tick(dt)\n"
        "  for _, a in ipairs(nearby.actors) do\n    local t = {a = a}\n    print('x' .. a.id)\n"
        "    for _, b in pairs(nearby.items) do end\n  end\n  if dt > 1 then return end\n  M.slow()\nend\n"
        "function M.slow() local v = util.vector3(1, 2, 3) end\n"
        "return { interfaceName = 'Mine', interface = { a = 1, b = 2 },\n"
        "  engineHandlers = { onUpdate = M.tick, onKeyPress = function() end, onWhatever = 1 },\n"
        "  eventHandlers = { Ping = function() end } }\n",
        "collectgarbage()\nlocal x = os.clock()\nstring.foo = 1\nself:sendEvent('Hit', {})\n"
        "core.sendGlobalEvent('Ping')\nlocal w = require('openmw.world')\n",
        "x = = 1",
    ]
    for src in samples:
        for contexts, api in (
            (frozenset(), API),
            (frozenset({"global"}), API),
            (frozenset({"player"}), API_049),
        ):
            want, got = analyze_py(src, contexts, api), analyze(src, contexts, api)
            assert got == want, src[:60]


def _native_or_skip(name: str):
    """The backend's ``name``, or skip when it is not built (or built before it)."""
    native = pytest.importorskip("wraithguard_native")
    fn = getattr(native, name, None)
    if fn is None:
        pytest.skip(f"the native module has no {name}")
    return fn


_OMWSCRIPTS_SAMPLES = [
    "﻿# c\r\nGLOBAL: scripts/a.lua\r\n\r\nplayer npc: Scripts\\B.lua\n",
    "nope\nWEIRD, NPC: c.lua\n: d.lua\nGLOBAL:\n  MENU ,LOAD :  x/y.lua  \n",
    "",
    "CUSTOM: a.lua\x0bGLOBAL: b.lua\x85PLAYER: c.lua",
]


def test_rust_omwscripts_matches_the_python_one():
    from wraithguard.lua.omwscripts import parse_omwscripts_py

    _native_or_skip("lua_omwscripts")
    for text in _OMWSCRIPTS_SAMPLES:
        assert parse_omwscripts(text, "m.omwscripts") == parse_omwscripts_py(text, "m.omwscripts")


def _scan_fixture(tmp_path):
    d1, d2 = tmp_path / "mod1", tmp_path / "mod2"
    _write(d1 / "My.omwscripts", "GLOBAL: scripts/m/g.lua\nNPC, PLAYER: scripts/m/actor.lua\n")
    _write(d1 / "My.omwscripts.bak", "")
    _write(d1 / "scripts" / "m" / "g.lua", "return {}\n")
    _write(
        d1 / "scripts" / "m" / "actor.lua",
        "return { interfaceName = 'AI', interface = {},\n"
        "  engineHandlers = { onUpdate = function(dt) end, onInterfaceOverride = function() end } }\n",
    )
    _write(
        d2 / "Other.omwscripts",
        "GLOBAL: Scripts/M/G.lua\nGLOBAL: scripts/o/o.lua\nPLAYER: scripts/gone.lua\nbad line\n",
    )
    _write(
        d2 / "Scripts" / "M" / "G.lua",
        "local core = require('openmw.core')\ncore.sendGlobalEvent('Nobody')\n"
        "return { interfaceName = 'Shared', interface = {} }\n",
    )
    _write(d2 / "scripts" / "o" / "o.lua", "return { interfaceName = 'Shared', interface = {} }\n")
    content = [
        "Morrowind.esm",
        "My.omwscripts",
        "Other.omwscripts",
        "Gone.omwscripts",
        "x.omwaddon",
    ]
    return [d1, d2], content


def test_rust_scan_matches_the_python_one(tmp_path):
    from wraithguard.lua.scan import scan_load_order_py

    _native_or_skip("lua_scan")
    dirs, content = _scan_fixture(tmp_path)
    got, want = scan_load_order(dirs, content), scan_load_order_py(dirs, content)
    assert got.findings == want.findings
    assert got.omwscripts == want.omwscripts
    assert got.omwaddons == want.omwaddons
    assert got.lual_files == want.lual_files
    assert len(got.scripts) == len(want.scripts)
    for g, w in zip(got.scripts, want.scripts, strict=True):
        assert (g.path, g.registrations, g.flags, g.providers) == (
            w.path,
            w.registrations,
            w.flags,
            w.providers,
        )
        assert g.info == w.info
        assert g.read_error == w.read_error
    assert render(got) == render(want)


def test_rust_report_matches_the_python_one(tmp_path):
    from wraithguard.lua.report import render_py

    _native_or_skip("lua_report")
    dirs, content = _scan_fixture(tmp_path)
    result = scan_load_order(dirs, content)
    result.api_source = "checked"
    for info in (True, False):
        assert render(result, info=info) == render_py(result, info=info)


def test_rust_teal_mapping_matches_the_python_one(tmp_path):
    from wraithguard.lua import openmw_api as oa

    _native_or_skip("lua_findings_for")
    tree = parse("local mine = 1\nfunction glob() end\nreturn {}\n")
    diags = [
        {"kind": "invalid_key", "severity": "error", "line": 9, "message": k}
        for k in (
            "invalid key 'recrdId' in type self__self",
            "invalid key 'getConsoleMod' in record of type ui__ui",
            "invalid key '_hidden' in type core__GameObject",
            "invalid key 'day' in type aux_time__time",
            "invalid key 'x' in record {}",
        )
    ]
    diags += [
        {"kind": "arity", "severity": "error", "line": 2, "message": m}
        for m in (
            "wrong number of arguments (given 3, expects 2)",
            "wrong number of arguments (given 1, expects at least 2 and at most 3)",
            "wrong number of arguments (given 4, expects 1 or 2)",
        )
    ]
    diags += [
        {"kind": "unknown_variable", "severity": "error", "line": 1, "message": m}
        for m in ("unknown variable: foo", "unknown variable: mine", "unknown variable: glob")
    ]
    diags += [
        {"kind": "module_not_found", "severity": "error", "line": 4, "message": m}
        for m in ("module not found: 'openmw.nope'", "module not found: 'my.mod'", "odd")
    ]
    diags += [
        {
            "kind": "unused_variable",
            "severity": "warning",
            "line": 5,
            "message": "unused variable x: integer",
        },
        {"kind": "type", "severity": "error", "line": 6, "message": "mismatch"},
        {"kind": "syntax", "severity": "warning", "line": 7, "message": "odd syntax"},
        {"kind": "type", "severity": "error", "line": 6, "message": "mismatch"},
    ]
    members = {
        "self__self": ["recordId", "position"],
        "ui__ui": ["setConsoleMode", "getConsoleMode"],
        "core__GameObject": ["id"],
    }
    for source in ("openmw", "stubs"):
        setup = oa.TealSetup(tmp_path, source, "x", members)
        for teal in (False, True):
            want = oa.findings_for_py(diags, teal=teal, tree=tree, setup=setup)
            assert oa.findings_for(diags, teal=teal, tree=tree, setup=setup) == want

    mod = tmp_path / "mod"
    (mod / "sub").mkdir(parents=True)
    (mod / "types").mkdir()
    (mod / "src").mkdir()
    (mod / "tlconfig.lua").write_text(
        'return { include_dir = { "types", "nope" }, source_dir = "src",\n'
        "  global_env_def = ' g1 ' }\n",
        encoding="utf-8",
    )
    (mod / "sub" / "tlconfig.lua").write_text(
        'return { include_dir = {\'.\', "../types"}, global_env_def = "g2" }\n', encoding="utf-8"
    )
    dirs = [mod, tmp_path / "missing"]
    assert oa.tlconfig_dirs(dirs) == oa.tlconfig_dirs_py(dirs)
    assert oa.tlconfig_globals(dirs) == oa.tlconfig_globals_py(dirs) == ["g1", "g2"]
    resources = tmp_path / "OpenMW" / "resources"
    resources.mkdir(parents=True)
    assert oa.find_teal_declarations(resources) is None
    decl = tmp_path / "OpenMW" / "teal_declarations"
    decl.mkdir()
    (decl / "core.d.tl").write_text("", encoding="utf-8")
    assert oa.find_teal_declarations(None, resources) == oa.find_teal_declarations_py(resources)
    (resources / "lua_api" / "openmw").mkdir(parents=True)
    assert oa.find_resources(None, resources.parent) == oa.find_resources_py(None, resources.parent)
    cfg = tmp_path / "openmw.cfg"
    cfg.write_text('data="x"\nResources = "OpenMW/resources"\n', encoding="utf-8")
    assert oa.find_resources(cfg) == oa.find_resources_py(cfg) == resources


def test_rust_charts_match_the_python_ones():
    from wraithguard.lua.callgraph import call_graph_chart_py
    from wraithguard.lua.flowchart import flowchart_py, render_py

    _native_or_skip("lua_flowchart")
    from pathlib import Path

    samples = [
        _CALLS_SRC,
        "local x = 1\nif a then return 1 elseif b then x = 2 else x = 3 end\n"
        "while x do if y then break end end\nrepeat x = x - 1 until x < 0\n"
        "for i = 1, 10, 2 do goto skip end\n::skip::\nfor k, v in pairs(t) do print(k, 'a \"q\" <b>') end\n"
        "do local function f(...) return ... end end\nreturn",
        "return { engineHandlers = { onUpdate = M.tick }, x = { y = function() end } }",
    ]
    samples += [
        p.read_text(encoding="utf-8", errors="replace")
        for p in Path(__file__).parent.rglob("*.lua")
    ]
    for src in samples:
        try:
            tree = parse(src)
        except LuaSyntaxError:
            continue
        assert flowchart(tree) == flowchart_py(tree)
        for node in tree.walk():
            if node.kind in ("Function", "LocalFunction", "FunctionStat"):
                assert flowchart(node) == flowchart_py(node)
        assert call_graph_chart(tree) == call_graph_chart_py(tree)
    nodes = {"n1": ("stadium", "Start"), "n2": ("diamond", 'a & "b"\n<c>')}
    edges = [("n1", "n2", None), ("n2", "n1", "yes"), ("n2", "n1", "")]
    from wraithguard.lua.flowchart import render as render_chart

    assert render_chart(nodes, edges) == render_py(nodes, edges)
