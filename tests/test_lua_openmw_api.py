"""wraithguard.lua.openmw_api: the setup's OpenMW Lua API, and Teal checks against it.

The documentation fixtures here are written for these tests in OpenMW's comment format;
none of OpenMW's own (GPLv3) files are copied.
"""

from __future__ import annotations

import subprocess
import sys
import sysconfig
import threading
from typing import TYPE_CHECKING

import pytest

from wraithguard.lua.api import API
from wraithguard.lua.openmw_api import (
    OpenmwDocs,
    TealSetup,
    _type_label,
    _typo_of,
    api_from_docs,
    check_cfg,
    find_resources,
    findings_for,
    read_docs,
)
from wraithguard.lua.parser import parse

if TYPE_CHECKING:
    from pathlib import Path

native = pytest.importorskip("wraithguard_native")
needs_teal = pytest.mark.skipif(
    not hasattr(native, "lua_check"), reason="Rust backend without Teal"
)

_CORE = """---
-- Core things.
-- @context global|menu|local|player|load
-- @module core
-- @usage local core = require('openmw.core')

---
-- An object in the world.
-- @type GameObject
-- @field #string recordId Its record.
-- @field #number count How many.

---
-- Whether it still exists.
-- @function [parent=#GameObject] isValid
-- @param self
-- @return #boolean

---
-- @type ObjectList
-- @list <#GameObject>

---
-- Sends a global event.
-- @function [parent=#core] sendGlobalEvent
-- @param #string name The event.
-- @param #table data (optional) Its data.
"""

_NEARBY = """---
-- What is near.
-- @context local
-- @module nearby
-- @usage local nearby = require('openmw.nearby')

---
-- The actors near.
-- @field [parent=#nearby] openmw.core#ObjectList actors
"""

_SELF = """---
-- The object the script is on.
-- @context local
-- @module Self
-- @usage local self = require('openmw.self')
-- @extends openmw.core#GameObject

---
-- @field [parent=#Self] #table controls
"""

_AI = """return {
    interfaceName = 'Helper',
    --- A helper interface.
    -- @module Helper
    -- @context local
    -- @usage require('openmw.interfaces').Helper
    interface = {
        --- @field [parent=#Helper] #number version
        version = 1,
    },
}
"""


def _install(root: Path) -> Path:
    res = root / "resources"
    (res / "lua_api" / "openmw").mkdir(parents=True)
    (res / "lua_api" / "openmw" / "core.lua").write_text(_CORE, encoding="utf-8")
    (res / "lua_api" / "openmw" / "nearby.lua").write_text(_NEARBY, encoding="utf-8")
    (res / "lua_api" / "openmw" / "self.lua").write_text(_SELF, encoding="utf-8")
    (res / "vfs" / "scripts" / "omw").mkdir(parents=True)
    (res / "vfs" / "scripts" / "omw" / "helper.lua").write_text(_AI, encoding="utf-8")
    (res / "version").write_text("0.52.0\nabc123\n", encoding="utf-8")
    return res


_SCRIPT = """local core = require('openmw.core')
local nearby = require('openmw.nearby')
local self = require('openmw.self')
local missing = require('scripts.mymod.nothere')
local function onUpdate(dt)
  for _, a in ipairs(nearby.actors) do
    if a:isValid() then print(a.recrdId, a.count) end
  end
  print(self.recordId, self.controls, self.secretThing, self._internal)
  core.sendGlobalEvent('Ping', {}, 'extra')
  notDefinedAnywhere()
  local unusedOne = 1
  print(missing)
end
return { engineHandlers = { onUpdate = onUpdate } }
"""


def _setup(tmp_path: Path) -> Path:
    data = tmp_path / "Data Files"
    (data / "scripts" / "mymod").mkdir(parents=True)
    (data / "scripts" / "mymod" / "main.lua").write_text(_SCRIPT, encoding="utf-8")
    (data / "mymod.omwscripts").write_text("PLAYER: scripts/mymod/main.lua\n", encoding="utf-8")
    cfg = tmp_path / "openmw.cfg"
    cfg.write_text(f'data="{data}"\ncontent=mymod.omwscripts\n', encoding="utf-8")
    return cfg


def test_find_resources(tmp_path, monkeypatch):
    monkeypatch.delenv("WG_OPENMW_RESOURCES", raising=False)
    res = _install(tmp_path / "OpenMW")
    assert find_resources(None, tmp_path / "OpenMW") == res  # the install folder
    assert find_resources(None, res) == res
    cfg = tmp_path / "openmw.cfg"
    cfg.write_text("resources=OpenMW/resources\n", encoding="utf-8")
    assert find_resources(cfg) == res
    monkeypatch.setenv("WG_OPENMW_RESOURCES", str(res))
    assert find_resources(None) == res
    assert find_resources(None, tmp_path / "nowhere") == res


def test_typo_rules():
    assert _typo_of("recrdId", ["recordId", "count"]) == "recordId"
    assert _typo_of("getConsoleMode", ["setConsoleMode"]) is None  # another function
    assert _typo_of("somethingElse", ["recordId"]) is None
    assert _type_label("nearby__nearby") == "openmw.nearby"
    assert _type_label("core__GameObject") == "openmw.core GameObject"
    assert _type_label("aux_time__time") == "openmw_aux.time"
    assert _type_label("I_AI__AI") == "I.AI"


def test_findings_for_keeps_what_plain_lua_can_trust():
    setup = TealSetup(folder=None, source="openmw", description="", members={"core__GameObject": ["recordId"]})  # type: ignore[arg-type]
    src = "local a = 1\nfor k, v in pairs(x) do print(k, v) end\nprint(a)\n"
    tree = parse(src)

    def d(kind, msg, line=1, sev="error"):
        return {"kind": kind, "message": msg, "line": line, "col": 1, "severity": sev, "rule": None}

    diags = [
        d("invalid_key", "invalid key 'recrdId' in record 'a' of type core__GameObject"),
        d("invalid_key", "invalid key 'day' in record 'a' of type record core__GameObject", 2),
        d("invalid_key", "invalid key '_x' in type core__GameObject", 3),
        d("invalid_key", "invalid key 'selected' in record 'env' of type record (I: any)", 4),
        d("arity", "wrong number of arguments (given 3, expects at least 1 and at most 2)", 5),
        d("arity", "wrong number of arguments (given 1, expects 2)", 6),
        d("unknown_variable", "unknown variable: v", 7),
        d("unknown_variable", "unknown variable: nowhere", 8),
        d("module_not_found", "module not found: 'openmw.gone'", 9),
        d("module_not_found", "module not found: 'scripts.x'", 10),
        d("unused_variable", "unused variable z: integer", 11, "warning"),
        d("unused_argument", "unused argument dt: any", 12, "warning"),
        d("type", "argument 1: got string, expected number", 13),
    ]
    got = [
        (f.code, f.line, f.severity)
        for f in findings_for(diags, teal=False, tree=tree, setup=setup)
    ]
    assert got == [
        ("API_TYPO", 1, "warn"),
        ("API_UNDOCUMENTED", 2, "info"),
        ("API_INTERNAL", 3, "info"),
        ("TOO_MANY_ARGS", 5, "warn"),
        ("UNKNOWN_GLOBAL", 8, "warn"),
        ("REQUIRE_NOT_FOUND", 9, "error"),
        ("REQUIRE_NOT_FOUND", 10, "warn"),
        ("UNUSED_LOCAL", 11, "info"),
    ]
    # A Teal file gets every type error.
    teal = [f.code for f in findings_for(diags, teal=True, tree=tree, setup=setup)]
    assert teal.count("TEAL") == 3


def test_api_from_docs():
    docs = OpenmwDocs(
        resources=None,  # type: ignore[arg-type]
        version="0.52.0",
        modules=[
            {"require": "openmw.nearby", "interface": None, "contexts": ["local"]},
            {
                "require": "openmw.util",
                "interface": None,
                "contexts": ["global", "menu", "local", "player", "load"],
            },
            {"require": "openmw.interfaces", "interface": "Helper", "contexts": ["local"]},
        ],
    )
    api = api_from_docs(docs)
    assert api.openmw == "0.52.0" and api.revision == 0
    assert api.packages == {"openmw.nearby": frozenset({"local", "player"})}
    assert "Helper" in api.builtin_interfaces
    assert api.handlers == API.handlers


@needs_teal
def test_read_docs_from_an_install(tmp_path):
    docs = read_docs(_install(tmp_path))
    assert docs is not None
    assert docs.version == "0.52.0"
    assert docs.packages == ["openmw.core", "openmw.nearby", "openmw.self"]
    assert docs.interfaces == ["Helper"]
    assert "isValid" in docs.members["self__Self"]  # inherited from GameObject


@needs_teal
def test_check_cfg_end_to_end(tmp_path, monkeypatch):
    monkeypatch.delenv("WG_OPENMW_RESOURCES", raising=False)
    res = _install(tmp_path / "OpenMW")
    cfg = _setup(tmp_path)
    scan = check_cfg(cfg, resources=res)
    assert scan.api.openmw == "0.52.0"
    assert "Teal checked 1 script(s)" in scan.api_source
    found = {(f.code, f.line) for f in scan.scripts[0].info.findings}
    assert ("API_TYPO", 7) in found  # a.recrdId
    assert ("API_UNDOCUMENTED", 9) in found  # self.secretThing
    assert ("API_INTERNAL", 9) in found  # self._internal
    assert ("TOO_MANY_ARGS", 10) in found
    assert ("UNKNOWN_GLOBAL", 11) in found
    assert ("REQUIRE_NOT_FOUND", 4) in found
    assert ("UNUSED_LOCAL", 12) in found
    # self is a GameObject: a.count, self.recordId and self.controls are fine.
    msgs = " ".join(f.message for f in scan.scripts[0].info.findings)
    assert (
        "count" not in msgs
        and "controls" not in msgs
        and "'recordId'" not in msgs.replace("did you mean 'recordId'", "")
    )


@needs_teal
def test_check_cfg_without_an_install_still_lints(tmp_path, monkeypatch):
    monkeypatch.delenv("WG_OPENMW_RESOURCES", raising=False)
    monkeypatch.setattr("wraithguard.lua.openmw_api._INSTALL_GLOBS", ())
    cfg = _setup(tmp_path)
    scan = check_cfg(cfg)
    assert "no OpenMW install found" in scan.api_source
    codes = {f.code for f in scan.scripts[0].info.findings}
    assert "UNKNOWN_GLOBAL" in codes and "UNUSED_LOCAL" in codes
    assert not codes & {"API_TYPO", "API_UNDOCUMENTED"}


@needs_teal
def test_written_declarations_are_kept(tmp_path, monkeypatch):
    monkeypatch.delenv("WG_OPENMW_RESOURCES", raising=False)
    res = _install(tmp_path / "OpenMW")
    out = tmp_path / "decl"
    check_cfg(_setup(tmp_path), resources=res, write_declarations=out)
    assert (out / "wg_openmw_api.d.tl").is_file()
    assert (out / "openmw" / "nearby.d.tl").is_file()


@pytest.mark.skipif(
    not sysconfig.get_config_var("Py_GIL_DISABLED"), reason="not a free-threaded build"
)
def test_the_backend_keeps_free_threaded_python_free():
    """On 3.14t, importing an extension that does not declare itself free-threading
    safe turns the GIL back on for the whole process. A fresh interpreter, so nothing
    else the test run imported can be the one that did it."""
    code = (
        "import sys, wraithguard_native, wraithguard.lua.openmw_api\n"
        "print(sys._is_gil_enabled())\n"
    )
    out = subprocess.run(  # noqa: S603 -- a fixed argv, no shell
        [sys.executable, "-c", code], capture_output=True, text=True, check=True
    )
    assert out.stdout.strip() == "False", out.stderr


@needs_teal
def test_checks_from_many_threads_agree(tmp_path, monkeypatch):
    """The Lua functions hold no shared state: threads calling them at once (each
    sharing its files out over threads of its own) get the single-threaded answers."""
    monkeypatch.delenv("WG_OPENMW_RESOURCES", raising=False)
    res = _install(tmp_path / "OpenMW")
    cfg = _setup(tmp_path)

    def found() -> list[tuple[str, int, str]]:
        scan = check_cfg(cfg, resources=res)
        return [(f.code, f.line, f.message) for f in scan.scripts[0].info.findings]

    want = found()
    got: list[list[tuple[str, int, str]] | BaseException] = [[] for _ in range(6)]

    def run(i: int) -> None:
        try:
            got[i] = found()
        except BaseException as exc:  # noqa: BLE001 -- handed to the main thread, failed there
            got[i] = exc

    threads = [threading.Thread(target=run, args=(i,)) for i in range(6)]
    for t in threads:
        t.start()
    for t in threads:
        t.join()
    assert all(g == want for g in got), got
