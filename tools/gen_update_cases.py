r"""Regenerate ``tests/fixtures/update_cases.json`` from ``wraithguard.net.updaters``.

The inputs are written here (and some made from a fixed seed); the answers are what the Python
gives. ``ordercore::updates`` (``viewer-shell/ordercore``) runs the same file, so the two cannot
drift. The network is never touched: ``urllib.request.urlopen`` is replaced by a script of
responses, the clock by a fixed one, and the environment by the case's own.

Families:

* ``urls``: what the URL guard of ``fetch_url_bytes`` says before anything is fetched;
* ``fetches``: a scripted response through ``fetch_url_bytes`` with a size cap;
* ``tails``: ``relative_tail``, ``_data_path_tails_from_api`` and ``parse_cache_paths``;
* ``flows``: ``update_plugin_order_yml``, ``fetch_list_data_paths`` and ``update_rule_files`` over
  a temp folder: the report lines, the files left behind (a ``.bak`` carries the fixed stamp) and
  the URLs requested, in order;
* ``ages``: ``rule_file_ages`` for files of given ages.

For ``plugin-order.yml`` the YAML reader is the caller's, so each case records what the real
``parse_plugin_order_yml`` made of every text the flow can parse (``parse``).

The detail a *parser* puts into a failure (a JSON syntax error's position) is that parser's own;
the cases mark those (``parser_detail``) and the Rust side is checked for the words around it.

Run: ``python tools/gen_update_cases.py``.
"""

from __future__ import annotations

import json
import os
import sys
import tempfile
from datetime import datetime
from pathlib import Path
from typing import Any
from unittest import mock

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "tests" / "fixtures" / "update_cases.json"

ENV_KEYS = ("MLOX_PLUGIN_ORDER_URL", "MLOX_RULES_URL_TEMPLATE", "MOMW_API_HOST")
STAMP = "20261022-120000"


#: The fixed clock's epoch seconds: the same wherever the generator runs (``datetime.timestamp``
#: of a naive time depends on the local time zone, and the file must not).
NOW_EPOCH = 1_792_670_400.0


class _FixedClock(datetime):
    """``datetime`` that always says it is 2026-10-22 12:00:00, local time, at ``NOW_EPOCH``."""

    @classmethod
    def now(cls, tz: Any = None) -> _FixedClock:
        return cls(2026, 10, 22, 12, 0, 0)

    def timestamp(self) -> float:
        return NOW_EPOCH


class Rng:
    """A small generator of its own, so the file does not move with Python's ``random``."""

    def __init__(self, seed: int) -> None:
        """Start from ``seed``."""
        self.state = (seed * 2654435761 + 12345) % (1 << 32)

    def below(self, n: int) -> int:
        """A number from 0 up to, not including, ``n``."""
        self.state = (self.state * 1664525 + 1013904223) % (1 << 32)
        return (self.state >> 8) % n

    def pick(self, items: list[str]) -> str:
        """One of the items."""
        return items[self.below(len(items))]


# ---- the scripted network


class _Reply:
    def __init__(self, data: bytes) -> None:
        self.data = data

    def __enter__(self) -> _Reply:
        return self

    def __exit__(self, *exc: object) -> None:
        return None

    def read(self, n: int) -> bytes:
        return self.data[:n]


class _Reached(Exception):  # noqa: N818 - a signal, not an error
    """The guard let the URL through to the network."""


def _scripted(script: dict[str, dict[str, Any]], calls: list[str]) -> Any:
    def urlopen(url: str, timeout: Any = None, context: Any = None) -> _Reply:
        calls.append(url)
        reply = script.get(url)
        if reply is None:
            msg = f"<urlopen error no route to {url}>"
            raise OSError(msg)
        if "error" in reply:
            raise OSError(reply["error"])
        return _Reply(reply["body"].encode("utf-8"))

    return urlopen


# ---- the URL guard

HAND_URLS: list[str] = [
    "http://example.com/x",
    "https://example.com/x?y=1#z",
    "HTTP://EXAMPLE.COM",
    "hTTpS://example.com",
    "http://user@example.com:8080/p",
    "http://example.com:80",
    "  https://example.com/lead",
    "\t\nhttps://example.com/",
    "ht\ttp://example.com/",
    "http://exa\nmple.com/",
    "https://example.com/a b",
    "file:///etc/passwd",
    "file://C:/Windows/win.ini",
    "FILE:///x",
    "ftp://example.com/x",
    "gopher://example.com/x",
    "data:text/plain,hello",
    "javascript:alert(1)",
    "HTTP://",
    "https://",
    "http:///path",
    "http://?q",
    "http://#f",
    "http:/example.com",
    "http:example.com",
    "https:",
    "https:/",
    "//example.com/x",
    "/path/only",
    "not-a-url",
    "",
    " ",
    ":",
    ":http://x",
    "1http://example.com",
    "h@ttp://example.com",
    "http+x://example.com",
    "a.b-c+d://example.com",
    "x_y://example.com",
    "c:/Users/x",
    "c:\\Users\\x",
    "mailto:a@b.c",
    "http://[::1]/x",
    "http://[::1",
    "http://::1]/",
    "http://exämple.com/",
    "http://example.com/ä",
    "\uff48ttp://example.com",
    "\x00http://example.com",
    "\x1fhttps://example.com/",
    "http://example.com\\@evil.com",
    "http://@/",
    "http://a/b://c",
]


def _fuzz_url(r: Rng) -> str:
    scheme = r.pick(
        ["http", "https", "HTTP", "HtTpS", "ftp", "file", "", "c", "a1", "x-y", "1a", "h t"]
    )
    sep = r.pick(["://", "://", ":/", ":", "//", ":///", "::"])
    host = r.pick(
        ["example.com", "", "u@h.com", "h:80", "h.com:", "a b", "ex\tample", "é.com", "[::1]", "."]
    )
    rest = r.pick(["", "/", "/p", "/p?q=1", "?q", "#f", "/a://b", " "])
    lead = r.pick(["", "", "", " ", "\t", "\n", "\x01"])
    url = f"{lead}{scheme}{sep}{host}{rest}"
    return url.replace("\u00e9", "\u00e9") if r.below(9) else url.upper()


def url_case(url: str) -> dict[str, Any]:
    """What the guard says of ``url`` before anything is fetched."""
    from wraithguard.net import updaters

    calls: list[str] = []
    with mock.patch("urllib.request.urlopen", _scripted({}, calls)):
        try:
            updaters.fetch_url_bytes(url, timeout=1)
        except ValueError as exc:
            return {"url": url, "refused": str(exc)}
        except OSError:
            return {"url": url, "refused": None, "reached": calls == [url]}
    return {"url": url, "refused": None, "reached": True}


# ---- fetches

FETCHES: list[tuple[str, dict[str, Any], int]] = [
    ("http://h/ok", {"body": "hello"}, 100),
    ("http://h/empty", {"body": ""}, 100),
    ("http://h/at-cap", {"body": "x" * 10}, 10),
    ("http://h/over-cap", {"body": "x" * 11}, 10),
    ("http://h/over-cap-big", {"body": "x" * 5000}, 1500),
    ("http://h/at-cap-big", {"body": "x" * 1234}, 1234),
    ("http://h/error", {"error": "<urlopen error [Errno 111] Connection refused>"}, 100),
    ("http://h/timeout", {"error": "timed out"}, 100),
    ("file:///etc/passwd", {"body": "secret"}, 100),
    ("https://", {"body": "x"}, 100),
    ("http://h/multibyte", {"body": "caf\u00e9"}, 100),
]

# ---- tails

TAIL_VALUES: list[tuple[str, str | None]] = [
    ("C:/games/OpenMWMods/Cat/Mod/dir", None),
    ('"C:\\games\\OpenMWMods\\Cat\\Mod\\dir"', None),
    ("'c:/GAMES/openmwmods/Cat/Mod'", None),
    ("  C:/games/OpenMWMods/Cat/Mod  ", None),
    ("Cat/Mod", None),
    ("", None),
    ("C:/games/OpenMWMods/", None),
    ("C:/games/OpenMWMods", None),
    ('""C:/games/OpenMWMods/A""', None),
    ("x/C:/games/OpenMWMods/A/C:/games/OpenMWMods/B", None),
    ("D:/mods/Cat/Mod", "D:/mods"),
    ("D:\\mods\\Cat\\Mod", "D:\\mods\\"),
    ("D:/mods//Cat", "D:/mods/"),
    ("C:/games/OpenMWMods/Caf\u00e9/Mod", None),
    ("\u00dfC:/games/OpenMWMods/x", None),
]

CACHE_TEXTS: list[str] = [
    "",
    "# header\n# more\nA/B\nC/D\n",
    "A\\B\r\n\r\n  C/D  \n# not\n",
    "\ufeffA/B\n",
    "A/B\x0bC/D\x0cE/F\u2028G/H\n",
    "  # indented comment\nX\n",
]


def _data_block(lines: list[str]) -> str:
    return "\n".join(lines)


def _payload(block: Any) -> str:
    return json.dumps({"openmw_cfg": {"data paths": block}})


API_PAYLOADS: list[str] = [
    _payload('data="C:/games/OpenMWMods/Cat/Mod1/dir"\ndata="C:/games/OpenMWMods/Cat/Mod2"\n'),
    _payload(
        "# comment\n  data=C:\\games\\OpenMWMods\\Cat\\Mod\\dir  \nnot data\ndata=relative/already\n"
    ),
    _payload(""),
    _payload("data=\nfoo=bar\n"),
    _payload(
        "data=C:/games/OpenMWMods/A\x0bdata=C:/games/OpenMWMods/B\u2028data=C:/games/OpenMWMods/C"
    ),
    _payload(5),
    _payload(["data=x"]),
    _payload(None),
    json.dumps({"openmw_cfg": {}}),
    json.dumps({"openmw_cfg": []}),
    json.dumps({"openmw_cfg": "text"}),
    json.dumps({"openmw_cfg": 3}),
    json.dumps({"openmw_cfg": None}),
    json.dumps({"openmw_cfg": True}),
    json.dumps({"openmw_cfg": 1.5}),
    json.dumps({}),
    json.dumps([]),
    json.dumps("x"),
    json.dumps(7),
    json.dumps(None),
    json.dumps(2.5),
    json.dumps(False),
    "",
    "not json",
    "{",
    '{"openmw_cfg": {"data paths": "data=x"',
    '\ufeff{"openmw_cfg": {"data paths": "data=C:/games/OpenMWMods/A"}}',
    '{"openmw_cfg": {"data paths": "data=C:/games/OpenMWMods/A"}, "openmw_cfg": {"data paths": "data=B"}}',
    _payload("data=C:/games/OpenMWMods/Caf\u00e9"),
    '{"openmw_cfg": {"data paths": "data=C:/games/OpenMWMods/\\ud83d\\ude00x"}}',
]


def tails_case(payload: str) -> dict[str, Any]:
    """What ``_data_path_tails_from_api`` makes of one response."""
    from wraithguard.net.updaters import _data_path_tails_from_api

    try:
        return {
            "payload": payload,
            "tails": _data_path_tails_from_api(payload.encode("utf-8", "surrogatepass")),
        }
    except ValueError as exc:
        own = isinstance(exc.__cause__, ValueError)  # a JSON or UTF-8 error: that parser's text
        return {"payload": payload, "value_error": str(exc), "parser_detail": own}


# ---- flows


def make_yml(n: int, tag: str = "total-overhaul") -> str:
    """A plugin-order.yml of ``n`` entries."""
    parts = [
        f'- for_mod: "Mod{i}"\n  file_name: "File{i}.esp"\n  on_lists:\n    - "{tag}"\n'
        for i in range(n)
    ]
    return "# Mod Plugin data\n\n" + "\n".join(parts)


def make_api(n: int) -> str:
    """A cfg-generator response with ``n`` data paths."""
    lines = [f'data="C:/games/OpenMWMods/Cat/Mod{i}/dir"' for i in range(n)]
    return _payload(_data_block(lines) + "\n")


def make_rules(extra: str = "") -> str:
    """A small mlox rule file."""
    return f"[Version 0.1]\n[Order]\nA.esp\nB.esp\n{extra}"


P1, P2 = (
    "https://gitlab.com/modding-openmw/modding-openmw.com/-/raw/master/momw/momw/data_seeds/data/"
    "plugin-order.yml?ref_type=heads&inline=false",
    "https://gitlab.com/api/v4/projects/modding-openmw%2Fmodding-openmw.com/repository/files/"
    "momw%2Fmomw%2Fdata_seeds%2Fdata%2Fplugin-order.yml/raw?ref=master",
)
YML_OK = make_yml(150)
YML_NEW = make_yml(180, "other-list")
YML_SMALL = make_yml(40)
RULES = "https://raw.githubusercontent.com/DanaePlays/mlox-rules/main/{}"
API = "https://modding-openmw.com/api/cfg-generator/{}"


def ok(body: str) -> dict[str, Any]:
    """A response with this body."""
    return {"body": body}


def err(text: str) -> dict[str, Any]:
    """A request that fails with this text."""
    return {"error": text}


Flow = dict[str, Any]


def _flow(kind: str, name: str, **kw: Any) -> Flow:
    return {"kind": kind, "name": name, "files": {}, "script": {}, "env": {}, **kw}


FLOWS: list[Flow] = [
    # --- plugin-order.yml
    _flow("yml", "a fresh install", file="plugin-order.yml", script={P1: ok(YML_OK)}),
    _flow(
        "yml",
        "already up to date",
        file="plugin-order.yml",
        files={"plugin-order.yml": YML_OK},
        script={P1: ok(YML_OK)},
    ),
    _flow(
        "yml",
        "a changed file replaces the old and keeps it",
        file="plugin-order.yml",
        files={"plugin-order.yml": YML_OK},
        script={P1: ok(YML_NEW)},
    ),
    _flow(
        "yml",
        "the first source fails and the second works",
        file="plugin-order.yml",
        script={P1: err("<urlopen error timed out>"), P2: ok(YML_OK)},
    ),
    _flow(
        "yml",
        "an error page is not the file",
        file="plugin-order.yml",
        files={"plugin-order.yml": YML_OK},
        script={P1: ok("<html>404 not found</html>"), P2: ok("<html>oops</html>")},
    ),
    _flow(
        "yml",
        "too few entries is refused",
        file="plugin-order.yml",
        files={"plugin-order.yml": YML_OK},
        script={P1: ok(YML_SMALL), P2: err("HTTP Error 500")},
    ),
    _flow("yml", "no source works", file="plugin-order.yml", script={}),
    _flow(
        "yml",
        "the setting beats the environment beats the built-in",
        file="plugin-order.yml",
        urls=["https://mirror.example/a.yml", "https://mirror.example/b.yml"],
        env={"MLOX_PLUGIN_ORDER_URL": "https://env.example/p.yml"},
        script={
            "https://mirror.example/b.yml": ok(YML_OK),
            "https://env.example/p.yml": ok(YML_NEW),
        },
    ),
    _flow(
        "yml",
        "the environment beats the built-in",
        file="plugin-order.yml",
        env={"MLOX_PLUGIN_ORDER_URL": "https://env.example/p.yml"},
        script={"https://env.example/p.yml": ok(YML_NEW), P1: ok(YML_OK)},
    ),
    _flow(
        "yml",
        "an empty environment variable is not set",
        file="plugin-order.yml",
        env={"MLOX_PLUGIN_ORDER_URL": ""},
        script={P1: ok(YML_OK)},
    ),
    _flow(
        "yml",
        "an empty list of sources is not set either",
        file="plugin-order.yml",
        urls=[],
        script={P1: ok(YML_OK)},
    ),
    _flow(
        "yml",
        "a hostile source is refused by name and never fetched",
        file="plugin-order.yml",
        urls=["file:///etc/passwd", "ftp://h/x", "https://", "https://mirror.example/ok.yml"],
        script={"https://mirror.example/ok.yml": ok(YML_OK)},
    ),
    _flow(
        "yml",
        "a file that will not parse",
        file="plugin-order.yml",
        script={P1: ok("file_name: [\non_lists: {\n- - -\n"), P2: ok(YML_OK)},
    ),
    _flow(
        "yml",
        "an old file that does not parse leaves no count",
        file="plugin-order.yml",
        files={"plugin-order.yml": "file_name: [\non_lists: {"},
        script={P1: ok(YML_OK)},
    ),
    # --- a list's data paths
    _flow(
        "paths",
        "a fresh cache",
        file="paths.txt",
        list="total-overhaul",
        script={API.format("total-overhaul"): ok(make_api(25))},
    ),
    _flow(
        "paths",
        "an unchanged cache",
        file="paths.txt",
        list="total-overhaul",
        files={
            "paths.txt": "# old header\n" + "\n".join(f"Cat/Mod{i}/dir" for i in range(25)) + "\n"
        },
        script={API.format("total-overhaul"): ok(make_api(25))},
    ),
    _flow(
        "paths",
        "a changed cache keeps the old one",
        file="paths.txt",
        list="total-overhaul",
        files={"paths.txt": "Cat/Old/dir\n"},
        script={API.format("total-overhaul"): ok(make_api(12))},
    ),
    _flow("paths", "no list name", file="paths.txt", list="   ", script={}),
    _flow(
        "paths",
        "a list name with spaces around it",
        file="paths.txt",
        list="  my-list  ",
        script={API.format("my-list"): ok(make_api(11))},
    ),
    _flow(
        "paths",
        "a host given, with slashes",
        file="paths.txt",
        list="x",
        host="https://mirror.example//",
        script={"https://mirror.example/api/cfg-generator/x": ok(make_api(10))},
    ),
    _flow(
        "paths",
        "a host from the environment",
        file="paths.txt",
        list="x",
        env={"MOMW_API_HOST": "https://env.example/"},
        script={"https://env.example/api/cfg-generator/x": ok(make_api(10))},
    ),
    _flow(
        "paths",
        "the request fails",
        file="paths.txt",
        list="x",
        script={API.format("x"): err("HTTP Error 404: Not Found")},
    ),
    _flow(
        "paths",
        "a list that does not exist",
        file="paths.txt",
        list="nope",
        script={API.format("nope"): ok("<html>no</html>")},
    ),
    _flow(
        "paths",
        "a payload with another shape",
        file="paths.txt",
        list="x",
        script={API.format("x"): ok(json.dumps({"openmw_cfg": []}))},
    ),
    _flow(
        "paths",
        "a payload without the field",
        file="paths.txt",
        list="x",
        script={API.format("x"): ok(json.dumps({"openmw_cfg": {}}))},
    ),
    _flow(
        "paths",
        "a payload whose field is not a string",
        file="paths.txt",
        list="x",
        script={API.format("x"): ok(_payload(5))},
    ),
    _flow(
        "paths",
        "too few paths",
        file="paths.txt",
        list="x",
        script={API.format("x"): ok(make_api(9))},
    ),
    _flow(
        "paths", "a hostile host is refused", file="paths.txt", list="x", host="file://", script={}
    ),
    _flow(
        "paths",
        "a list name with a slash",
        file="paths.txt",
        list="a/b",
        script={API.format("a/b"): ok(make_api(10))},
    ),
    # --- rule files
    _flow(
        "rules",
        "both managed files, fresh",
        paths=["mlox_base.txt", "mlox_user.txt"],
        script={
            RULES.format("mlox_base.txt"): ok(make_rules()),
            RULES.format("mlox_user.txt"): ok(make_rules("[Note]\nA.esp\n")),
        },
    ),
    _flow(
        "rules",
        "a file of the user's own is left alone",
        paths=["mlox_base.txt", "my_rules.txt", "MLOX_USER.TXT"],
        files={"my_rules.txt": "mine"},
        script={
            RULES.format("mlox_base.txt"): ok(make_rules()),
            RULES.format("MLOX_USER.TXT"): ok(make_rules()),
        },
    ),
    _flow(
        "rules",
        "up to date and updated",
        paths=["mlox_base.txt", "mlox_user.txt"],
        files={"mlox_base.txt": make_rules(), "mlox_user.txt": "old [Order]\n"},
        script={
            RULES.format("mlox_base.txt"): ok(make_rules()),
            RULES.format("mlox_user.txt"): ok(make_rules("x" * 2000)),
        },
    ),
    _flow(
        "rules",
        "a download that is not a rule file",
        paths=["mlox_base.txt", "mlox_user.txt"],
        files={"mlox_base.txt": make_rules()},
        script={
            RULES.format("mlox_base.txt"): ok("<html>moved</html>"),
            RULES.format("mlox_user.txt"): ok(""),
        },
    ),
    _flow(
        "rules",
        "a failed download goes on to the next",
        paths=["mlox_base.txt", "mlox_user.txt"],
        script={
            RULES.format("mlox_base.txt"): err("<urlopen error timed out>"),
            RULES.format("mlox_user.txt"): ok(make_rules()),
        },
    ),
    _flow(
        "rules",
        "a template of the caller's",
        paths=["mlox_base.txt"],
        template="https://mirror.example/rules/{name}.txt",
        env={"MLOX_RULES_URL_TEMPLATE": "https://env.example/{name}"},
        script={"https://mirror.example/rules/mlox_base.txt.txt": ok(make_rules())},
    ),
    _flow(
        "rules",
        "a template from the environment",
        paths=["mlox_base.txt"],
        env={"MLOX_RULES_URL_TEMPLATE": "https://env.example/{name}"},
        script={"https://env.example/mlox_base.txt": ok(make_rules())},
    ),
    _flow(
        "rules",
        "a template without the placeholder",
        paths=["mlox_base.txt"],
        template="https://mirror.example/rules",
        script={},
    ),
    _flow(
        "rules",
        "a template with another field",
        paths=["mlox_base.txt"],
        template="https://m.example/{branch}/{name}",
        script={},
    ),
    _flow(
        "rules",
        "a template with an unclosed brace",
        paths=["mlox_base.txt"],
        template="https://m.example/{name}/{",
        script={},
    ),
    _flow(
        "rules",
        "a template with an unclosed field",
        paths=["mlox_base.txt"],
        template="https://m.example/{name}/{x",
        script={},
    ),
    _flow(
        "rules",
        "a template with a stray closing brace",
        paths=["mlox_base.txt"],
        template="https://m.example/}{name}",
        script={},
    ),
    _flow(
        "rules",
        "a template with a positional field",
        paths=["mlox_base.txt"],
        template="https://m.example/{}/{name}",
        script={},
    ),
    _flow(
        "rules",
        "a template with a numbered field",
        paths=["mlox_base.txt"],
        template="https://m.example/{3}/{name}",
        script={},
    ),
    _flow(
        "rules",
        "a template with a quote in it",
        paths=["mlox_base.txt"],
        template="it's {name} {x}",
        script={},
    ),
    _flow(
        "rules",
        "a template with escaped braces",
        paths=["mlox_base.txt"],
        template="https://m.example/{{literal}}/{name}",
        script={"https://m.example/{literal}/mlox_base.txt": ok(make_rules())},
    ),
    _flow(
        "rules",
        "a template with a spec is not read",
        paths=["mlox_base.txt"],
        template="https://m.example/{name}/{name:>20}",
        script={},
    ),
    _flow(
        "rules",
        "a hostile template",
        paths=["mlox_base.txt"],
        template="file:///etc/{name}",
        script={},
    ),
    _flow("rules", "no files at all", paths=[], script={}),
]


def _fuzz_flows() -> list[Flow]:
    out: list[Flow] = []
    for seed in range(1, 41):
        r = Rng(seed)
        kind = r.pick(["yml", "paths", "rules"])
        name = f"made up, seed {seed}"
        words = [r.pick(["a", "b"]) for _ in range(3)]
        if kind == "rules":
            paths: list[str] = []
            for _ in range(1 + r.below(3)):
                pick = r.pick(["mlox_base.txt", "mlox_user.txt", "other.txt", "MLOX_BASE.TXT"])
                # two spellings of one name are one file on a case-insensitive disk
                if pick.lower() not in {q.lower() for q in paths}:
                    paths.append(pick)
            script: dict[str, dict[str, Any]] = {}
            files: dict[str, str] = {}
            for p in paths:
                url = RULES.format(p)
                roll = r.below(5)
                script[url] = [
                    ok(make_rules(words[0])),
                    ok("nope"),
                    err("<urlopen error x>"),
                    ok(make_rules()),
                    ok(""),
                ][roll]
                if r.below(2):
                    files[p] = make_rules(words[1]) if r.below(2) else make_rules()
            out.append(_flow("rules", name, paths=paths, files=files, script=script))
        elif kind == "yml":
            first = [ok(YML_OK), ok(YML_NEW), ok(YML_SMALL), err("boom"), ok("junk")][r.below(5)]
            second = [ok(YML_OK), err("boom"), ok("junk")][r.below(3)]
            files = (
                {"plugin-order.yml": [YML_OK, YML_NEW, "", "junk"][r.below(4)]}
                if r.below(2)
                else {}
            )
            files = {k: v for k, v in files.items() if v}
            out.append(
                _flow(
                    "yml",
                    name,
                    file="plugin-order.yml",
                    files=files,
                    script={P1: first, P2: second},
                )
            )
        else:
            n = [5, 10, 11, 30][r.below(4)]
            files = (
                {"paths.txt": "\n".join(f"Cat/Mod{i}/dir" for i in range(n)) + "\n"}
                if r.below(2)
                else {}
            )
            out.append(
                _flow(
                    "paths",
                    name,
                    file="paths.txt",
                    list=r.pick(["x", "total-overhaul", " y "]),
                    files=files,
                    script={},
                )
            )
            slug = out[-1]["list"].strip()
            out[-1]["script"] = {API.format(slug): ok(make_api(n + r.below(3)))}
    return out


AGES: list[tuple[str, dict[str, float | None]]] = [
    ("fresh and old and missing", {"a.txt": 0.0, "b.txt": 3.5, "c.txt": 400.2, "gone.txt": None}),
    ("a file from the future", {"a.txt": -2.5}),
    ("almost a day", {"a.txt": 0.999, "b.txt": 1.0, "c.txt": 1.001}),
    ("nothing", {}),
]


def _parse_table(texts: list[str]) -> list[dict[str, Any]]:
    from wraithguard.momw import parse_plugin_order_yml

    table = []
    for text in dict.fromkeys(texts):
        with tempfile.TemporaryDirectory() as tmp:
            p = Path(tmp) / "x.yml"
            p.write_bytes(text.encode("utf-8"))
            try:
                table.append({"text": text, "entries": len(parse_plugin_order_yml(p))})
            except Exception as exc:  # noqa: BLE001 - the observation is the message
                table.append({"text": text, "error": str(exc)})
    return table


def flow_case(flow: Flow) -> dict[str, Any]:
    """Run one flow over a temp folder and record what it did."""
    from wraithguard.net import updaters

    calls: list[str] = []
    with tempfile.TemporaryDirectory() as tmp, mock.patch.dict(os.environ):
        for key in ENV_KEYS:
            os.environ.pop(key, None)
        os.environ.update(flow["env"])
        root = Path(tmp)
        for name, text in flow["files"].items():
            (root / name).write_bytes(text.encode("utf-8"))
        with (
            mock.patch("urllib.request.urlopen", _scripted(flow["script"], calls)),
            mock.patch.object(updaters, "datetime", _FixedClock),
        ):
            if flow["kind"] == "yml":
                report = updaters.update_plugin_order_yml(
                    root / flow["file"], urls=flow.get("urls")
                )
            elif flow["kind"] == "paths":
                report = updaters.fetch_list_data_paths(
                    root / flow["file"], flow["list"], host=flow.get("host")
                )
            else:
                report = updaters.update_rule_files(
                    [root / p for p in flow["paths"]], url_template=flow.get("template")
                )
        after = {p.name: p.read_bytes().decode("utf-8") for p in sorted(root.iterdir())}
    out: dict[str, Any] = {"report": report, "after": after, "requested": calls}
    if flow["kind"] == "paths":
        bodies = [r["body"] for r in flow["script"].values() if "body" in r]
        out["parser_detail"] = any(tails_case(b).get("parser_detail") for b in bodies)
    if flow["kind"] == "yml":
        texts = [r["body"] for r in flow["script"].values() if "body" in r] + list(
            flow["files"].values()
        )
        out["parse"] = _parse_table(texts)
    return out


def ages_case(files: dict[str, float | None]) -> dict[str, Any]:
    """``rule_file_ages`` of files modified this many days (and fractions) ago."""
    from wraithguard.net import updaters

    now = _FixedClock.now().timestamp()
    with tempfile.TemporaryDirectory() as tmp, mock.patch.object(updaters, "datetime", _FixedClock):
        paths = []
        for name, days in files.items():
            path = Path(tmp) / name
            paths.append(path)
            if days is not None:
                path.write_bytes(b"x")
                os.utime(path, (now - days * 86400, now - days * 86400))
        got = updaters.rule_file_ages(paths)
    return {"files": files, "now": now, "ages": [[n, a] for n, a in got]}


def cases() -> dict[str, Any]:
    """The cases, answered by the Python code."""
    sys.path.insert(0, str(ROOT))
    from wraithguard.net import updaters
    from wraithguard.net.updaters import fetch_url_bytes

    urls = [*HAND_URLS]
    r = Rng(3)
    urls += [_fuzz_url(r) for _ in range(500)]
    fetches = []
    for url, reply, cap in FETCHES:
        calls: list[str] = []
        with mock.patch("urllib.request.urlopen", _scripted({url: reply}, calls)):
            try:
                body = fetch_url_bytes(url, timeout=1, max_bytes=cap)
                got: dict[str, Any] = {"ok": body.decode("utf-8")}
            except ValueError as exc:
                got = {"value_error": str(exc)}
            except OSError as exc:
                got = {"os_error": str(exc)}
        fetches.append({"url": url, "reply": reply, "max_bytes": cap, **got})
    flows = [*FLOWS, *_fuzz_flows()]
    return {
        "urls": [url_case(u) for u in urls],
        "fetches": fetches,
        "constants": {
            "max_download_bytes": updaters.MAX_DOWNLOAD_BYTES,
            "managed": list(updaters.MANAGED_RULE_FILES),
            "rules_template": updaters.RULES_URL_TEMPLATE,
            "plugin_order_urls": list(updaters.PLUGIN_ORDER_URLS),
            "cfg_generator_url": updaters.API_CFG_GENERATOR_URL,
            "default_host": updaters.DEFAULT_MOMW_HOST,
        },
        "tails": {
            "relative": [_relative_case(v, b) for v, b in TAIL_VALUES],
            "api": [tails_case(p) for p in API_PAYLOADS],
            "cache": [
                {"text": t, "paths": updaters.parse_cache_paths(t.encode("utf-8"))}
                for t in CACHE_TEXTS
            ],
        },
        "flows": [{**f, **flow_case(f), "stamp": STAMP} for f in flows],
        "ages": [{"name": n, **ages_case(f)} for n, f in AGES],
    }


def _relative_case(value: str, base: str | None) -> dict[str, Any]:
    from wraithguard.momw_datapaths import relative_tail

    return {
        "value": value,
        "base": base,
        "tail": relative_tail(value) if base is None else relative_tail(value, base),
    }


def main() -> int:
    """Write the file.

    Returns:
        0.
    """
    out = cases()
    doc = {
        "_about": (
            "The downloads: the URL guard, the size cap, and the update flows over a scripted "
            "network, answered by wraithguard.net.updaters; run by tests/test_update_cases.py and "
            "viewer-shell/ordercore/tests/update_cases.rs. Regenerate: python tools/gen_update_cases.py"
        ),
        **out,
    }
    OUT.write_bytes((json.dumps(doc, indent=1, ensure_ascii=False) + "\n").encode("utf-8"))
    print(
        f"{len(out['urls'])} urls, {len(out['fetches'])} fetches, {len(out['flows'])} flows -> "
        f"{OUT.relative_to(ROOT)}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
