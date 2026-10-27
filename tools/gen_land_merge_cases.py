#!/usr/bin/env python3
"""Regenerate ``tests/fixtures/land_merge_cases.json``: Merge Lands, pinned for the Rust port.

Each case is a small load order described as *recipes* rather than as grids -- integer
formulas for heights, colours and textures -- so the fixture stays a few kilobytes while
the plugins it stands for are tens of kilobytes each. This script builds the plugins from
the recipes, runs the Python merge on them (``build_merged_lands``, the native path: no
tes3conv), and records what came out: every progress line, the merged plugin's size and
SHA-256, and the SHA-256 of each input it built.

``viewer-shell/landcore/tests/merge_cases.rs`` builds the same plugins from the same
recipes (checked against the input hashes), runs ``landcore::service`` on them and must
produce the same lines and the same bytes.

Usage::

    python tools/gen_land_merge_cases.py            # rewrite the fixture
    python tools/gen_land_merge_cases.py --check    # fail if it is out of date

The recipes (one per LAND record; every key optional except ``grid``):

- ``flags``: the ``DATA`` word.
- ``heights``: ``[base, a, b, m, cx, cy, r, bump]`` - in stored units (world / 8),
  ``base + (x*a + y*b) % m``, plus ``bump`` within ``r`` of ``(cx, cy)``.
- ``normals``: ``"auto"`` (from the heights, as the merge computes them) or ``"zero"``.
- ``world_map``: ``"auto"`` (from the heights) or ``"zero"`` or 81 values.
- ``colors``: ``[r, g, b, k]`` - ``(r + x*k % 50, g + y*k % 50, b)``.
- ``textures``: ``[fill, every, other]`` - ``other`` where ``(x + y) % every == 0``, else
  ``fill``; both are the plugin's own ``LTEX`` index + 1.
- ``deleted``: the record's DELETED flag.

Merge Lands is MIT (David Von Derau, 2022); ``wraithguard/land`` is our port of it.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import shutil
import struct
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))

from wraithguard.land import service  # noqa: E402
from wraithguard.land.emit import pack_texture_indices, world_map_from_heights  # noqa: E402
from wraithguard.land.heights import vertex_normals_from_heights  # noqa: E402

OUT = ROOT / "tests" / "fixtures" / "land_merge_cases.json"
SIZE = 65


def heights_of(recipe: list[int]) -> list[int]:
    """Stored heights (world / 8), row-major."""
    base, a, b, m, cx, cy, r, bump = recipe
    out = []
    for y in range(SIZE):
        for x in range(SIZE):
            v = base + (x * a + y * b) % m
            if (x - cx) ** 2 + (y - cy) ** 2 <= r * r:
                v += bump
            out.append(v)
    return out


def vhgt(stored: list[int]) -> bytes:
    """A VHGT payload whose decode is ``stored`` * 8."""
    offset = stored[0]
    deltas = []
    for y in range(SIZE):
        for x in range(SIZE):
            if x == 0:
                deltas.append(0 if y == 0 else stored[y * SIZE] - stored[(y - 1) * SIZE])
            else:
                deltas.append(stored[y * SIZE + x] - stored[y * SIZE + x - 1])
    if any(d < -128 or d > 127 for d in deltas):
        raise SystemExit("a recipe makes a step VHGT cannot store")
    return struct.pack("<f", float(offset)) + struct.pack(f"<{SIZE * SIZE}b", *deltas) + bytes(3)


def sub(tag: bytes, payload: bytes) -> bytes:
    """One subrecord."""
    return tag + struct.pack("<I", len(payload)) + payload


def record(tag: bytes, body: bytes, flags: int = 0) -> bytes:
    """One record, its header and body."""
    return tag + struct.pack("<III", len(body), 0, flags) + body


def land(r: dict) -> bytes:
    """A LAND record from its recipe."""
    gx, gy = r["grid"]
    body = sub(b"INTV", struct.pack("<ii", gx, gy))
    if "flags" in r:
        body += sub(b"DATA", struct.pack("<I", r["flags"]))
    stored = heights_of(r["heights"]) if "heights" in r else None
    world = [v * 8 for v in stored] if stored else None
    if stored is not None:
        if r.get("normals", "auto") == "auto":
            rows = [[float(v) for v in world[y * SIZE : (y + 1) * SIZE]] for y in range(SIZE)]
            normals = [c for row in vertex_normals_from_heights(rows) for t in row for c in t]
        else:
            normals = [0] * (3 * SIZE * SIZE)
        body += sub(b"VNML", struct.pack(f"<{3 * SIZE * SIZE}b", *normals))
        body += sub(b"VHGT", vhgt(stored))
    wm = r.get("world_map")
    if wm is not None:
        if wm == "auto":
            rows = [[float(v) for v in world[y * SIZE : (y + 1) * SIZE]] for y in range(SIZE)]
            values = [v for row in world_map_from_heights(rows) for v in row]
        elif wm == "zero":
            values = [0] * 81
        else:
            values = wm
        body += sub(b"WNAM", struct.pack("<81b", *values))
    if "colors" in r:
        cr, cg, cb, k = r["colors"]
        data = bytearray()
        for y in range(SIZE):
            for x in range(SIZE):
                data += bytes(((cr + x * k % 50) % 256, (cg + y * k % 50) % 256, cb % 256))
        body += sub(b"VCLR", bytes(data))
    if "textures" in r:
        fill, every, other = r["textures"]
        rows = [[other if (x + y) % every == 0 else fill for x in range(16)] for y in range(16)]
        body += sub(b"VTEX", pack_texture_indices(rows))
    return record(b"LAND", body, 0x20 if r.get("deleted") else 0)


def ltex(t: list) -> bytes:
    """An LTEX record: id, index, file name, deleted."""
    ident, index, file_name = t[:3]
    deleted = len(t) > 3 and t[3]
    body = sub(b"NAME", ident.encode("latin-1") + b"\0") + sub(b"INTV", struct.pack("<I", index))
    if file_name is not None:
        body += sub(b"DATA", file_name.encode("latin-1") + b"\0")
    return record(b"LTEX", body, 0x20 if deleted else 0)


def plugin(p: dict) -> bytes:
    """A whole plugin from its recipe."""
    records = [ltex(t) for t in p.get("ltex", [])] + [land(r) for r in p.get("land", [])]
    is_master = p["name"].lower().endswith(".esm")
    hedr = (
        struct.pack("<fI", 1.3, 1 if is_master else 0)
        + bytes(32)
        + bytes(256)
        + struct.pack("<I", len(records))
    )
    body = sub(b"HEDR", hedr)
    for m in p.get("masters", []):
        body += sub(b"MAST", m.encode("latin-1") + b"\0") + sub(b"DATA", struct.pack("<Q", 0))
    return record(b"TES3", body) + b"".join(records)


def flat(base: int = 400) -> list[int]:
    """A gentle slope."""
    return [base, 1, 1, 9, 0, 0, -1, 0]


#: The cases. Masters: a 3x3 world with textures; mods that overlap, add land, carry
#: settings, delete, repeat the master, and a previous merge to be skipped.
CASES: list[dict] = [
    {
        "name": "a load order with overlapping, new, resolved and redundant terrain",
        "plugins": [
            {
                "name": "Base.esm",
                "ltex": [
                    ["Grass", 0, "grass.dds"],
                    ["Rock", 1, "rock.dds"],
                    ["Sand", 2, "sand.dds"],
                ],
                "land": [
                    {
                        "grid": [x, y],
                        "flags": 7,
                        "heights": flat(400 + 3 * x + 5 * y),
                        "world_map": "auto",
                        "colors": [120, 110, 90, 3],
                        "textures": [1, 5, 2],
                    }
                    for y in (-1, 0, 1)
                    for x in (-1, 0, 1)
                ]
                + [
                    {
                        "grid": [3, 0],
                        "flags": 1,
                        "heights": [-300, 0, 1, 1, 0, 0, -1, 0],
                        "world_map": "zero",
                    },
                ],
            },
            {
                "name": "Second.esm",
                "masters": ["Base.esm"],
                "land": [{"grid": [1, 1], "flags": 2, "colors": [10, 20, 30, 1]}],
            },
            {
                "name": "Hills.esp",
                "masters": ["Base.esm"],
                "ltex": [["Mud", 0, "mud.dds"], ["Rock", 1, "rock2.dds"]],
                "land": [
                    {
                        "grid": [0, 0],
                        "flags": 7,
                        "heights": [400, 1, 1, 9, 20, 20, 8, 60],
                        "world_map": "auto",
                        "colors": [120, 110, 90, 3],
                        "textures": [1, 3, 2],
                    },
                    {
                        "grid": [2, 0],
                        "flags": 5,
                        "heights": [380, 2, 0, 7, 32, 32, 10, 30],
                        "textures": [2, 4, 1],
                    },
                ],
            },
            {
                "name": "Valleys.esp",
                "masters": ["Base.esm"],
                "land": [
                    {
                        "grid": [0, 0],
                        "flags": 1,
                        "heights": [400, 1, 1, 9, 24, 24, 6, -50],
                        "normals": "zero",
                    },
                    {"grid": [0, 1], "flags": 1, "heights": [405, 1, 1, 9, 32, 0, 5, 25]},
                ],
            },
            {
                "name": "Blend.esp",
                "masters": ["Base.esm"],
                "toml": 'version = "0"\nmeta_type = "Patch"\n\n[height_map]\nconflict_strategy = "Resolve"\n',
                "land": [{"grid": [0, 0], "flags": 1, "heights": [400, 1, 1, 9, 22, 22, 7, 90]}],
            },
            {
                "name": "Painter.esp",
                "masters": ["Base.esm"],
                "toml": 'version = "0"\nmeta_type = "Patch"\n\n[height_map]\nincluded = false\n',
                "ltex": [["Lava", 0, "lava.dds"]],
                "land": [
                    {
                        "grid": [-1, -1],
                        "flags": 5,
                        "heights": [999, 3, 3, 5, 0, 0, -1, 0],
                        "textures": [1, 2, 0],
                    },
                ],
            },
            {
                "name": "Same.esp",
                "masters": ["Base.esm"],
                "land": [
                    {"grid": [-1, 0], "flags": 2, "colors": [120, 110, 90, 3]},
                    {"grid": [1, -1], "flags": 2, "colors": [1, 2, 3, 7]},
                ],
            },
            {
                "name": "Gone.esp",
                "masters": ["Base.esm"],
                "ltex": [["Ghost", 0, "ghost.dds", True]],
                "land": [
                    {
                        "grid": [-1, 1],
                        "flags": 1,
                        "heights": [100, 0, 0, 1, 0, 0, -1, 0],
                        "deleted": True,
                    }
                ],
            },
            {
                "name": "Raise.esp",
                "masters": ["Base.esm"],
                "land": [{"grid": [1, 0], "flags": 1, "heights": [403, 1, 1, 9, 24, 32, 6, 120]}],
            },
            {
                "name": "Lower.esp",
                "masters": ["Base.esm"],
                "land": [
                    {"grid": [1, 0], "flags": 1, "heights": [403, 1, 1, 9, 37, 32, 6, -120]},
                    {"grid": [3, 0], "flags": 1, "heights": [-300, 0, 1, 1, 64, 30, 3, -20]},
                ],
            },
            {
                "name": "Old Merged Lands.esp",
                "masters": ["Base.esm"],
                "toml": 'version = "0"\nmeta_type = "MergedLands"\n',
                "land": [{"grid": [0, 0], "flags": 1, "heights": [0, 0, 0, 1, 0, 0, -1, 0]}],
            },
            {"name": "NoTerrain.esp", "masters": ["Base.esm"]},
            {"name": "Scripts.omwscripts", "raw": "PLAYER: scripts/x.lua\n"},
        ],
        "order": [
            "Base.esm",
            "Second.esm",
            "Hills.esp",
            "Valleys.esp",
            "Blend.esp",
            "Painter.esp",
            "Same.esp",
            "Gone.esp",
            "Raise.esp",
            "Lower.esp",
            "Old Merged Lands.esp",
            "NoTerrain.esp",
            "Missing.esp",
            "Scripts.omwscripts",
        ],
        "verbose": True,
    },
    {
        "name": "a mod painting with a texture no LTEX defines, passed through",
        "plugins": [
            {
                "name": "Base.esm",
                "ltex": [["Grass", 0, "grass.dds"]],
                "land": [{"grid": [0, 0], "flags": 5, "heights": flat(), "textures": [1, 2, 1]}],
            },
            {
                "name": "Odd.esp",
                "masters": ["Base.esm"],
                "land": [{"grid": [0, 0], "flags": 4, "textures": [1, 3, 9]}],
            },
            {
                "name": "Bump.esp",
                "masters": ["Base.esm"],
                "land": [{"grid": [0, 0], "flags": 1, "heights": [400, 1, 1, 9, 30, 30, 4, 40]}],
            },
        ],
        "order": ["Base.esm", "Odd.esp", "Bump.esp"],
        "substitute": False,
    },
    {
        "name": "per-layer strategies: curvature, ignore, colours blended, world map overwritten",
        "plugins": [
            {
                "name": "Base.esm",
                "land": [
                    {
                        "grid": [x, 0],
                        "flags": 3,
                        "heights": flat(200 + 7 * x),
                        "world_map": "auto",
                        "colors": [50, 60, 70, 2],
                    }
                    for x in (0, 1)
                ],
            },
            {
                "name": "A.esp",
                "masters": ["Base.esm"],
                "land": [
                    {
                        "grid": [0, 0],
                        "flags": 3,
                        "heights": [200, 1, 1, 9, 30, 30, 9, 70],
                        "world_map": [5] * 81,
                        "colors": [90, 60, 70, 2],
                    },
                    {
                        "grid": [1, 0],
                        "flags": 3,
                        "heights": [207, 1, 1, 9, 10, 10, 5, 40],
                        "colors": [50, 80, 70, 4],
                    },
                ],
            },
            {
                "name": "B.esp",
                "masters": ["Base.esm"],
                "toml": 'version = "0"\nmeta_type = "Patch"\n\n[height_map]\nconflict_strategy = "Curvature"\n\n'
                '[vertex_colors]\nconflict_strategy = "Resolve"\n\n[world_map_data]\nconflict_strategy = "Overwrite"\n',
                "land": [
                    {
                        "grid": [0, 0],
                        "flags": 3,
                        "heights": [200, 1, 1, 9, 34, 34, 8, -60],
                        "world_map": [-3] * 81,
                        "colors": [10, 60, 200, 2],
                    }
                ],
            },
            {
                "name": "C.esp",
                "masters": ["Base.esm"],
                "toml": 'version = "0"\nmeta_type = "Patch"\n\n[height_map]\nconflict_strategy = "Ignore"\n',
                "land": [{"grid": [1, 0], "flags": 1, "heights": [207, 1, 1, 9, 12, 12, 6, -45]}],
            },
        ],
        "order": ["Base.esm", "A.esp", "B.esp", "C.esp"],
    },
    {
        "name": "nothing to merge",
        "plugins": [
            {"name": "Base.esm", "land": [{"grid": [0, 0], "flags": 1, "heights": flat()}]},
            {
                "name": "Copy.esp",
                "masters": ["Base.esm"],
                "land": [{"grid": [0, 0], "flags": 1, "heights": flat()}],
            },
        ],
        "order": ["Base.esm", "Copy.esp"],
    },
    {
        "name": "no masters",
        "plugins": [{"name": "Lonely.esp"}],
        "order": ["Lonely.esp"],
    },
]


def run(case: dict) -> dict:
    """One case built, merged by the Python, and recorded."""
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        data = root / "data"
        data.mkdir()
        hashes = {}
        for p in case["plugins"]:
            path = data / p["name"]
            body = p["raw"].encode() if "raw" in p else plugin(p)
            path.write_bytes(body)
            hashes[p["name"]] = hashlib.sha256(body).hexdigest()
            if "toml" in p:
                (data / (path.stem + ".mergedlands.toml")).write_text(
                    p["toml"], encoding="utf-8", newline="\n"
                )
        out = root / "out" / service.DEFAULT_NAME
        lines: list[str] = []
        expect: dict = {"inputs": hashes}
        try:
            result = service.build_merged_lands(
                data_files=[data],
                load_order=case["order"],
                converter=None,
                output=out,
                report=lines.append,
                substitute_unknown_textures=case.get("substitute", True),
                verbose=case.get("verbose", False),
            )
            expect["error"] = None
            if result.output is not None:
                body = out.read_bytes()
                expect["size"] = len(body)
                expect["sha256"] = hashlib.sha256(body).hexdigest()
                marker = out.with_name(out.stem + ".mergedlands.toml")
                expect["marker"] = marker.read_text(encoding="utf-8").replace("\r\n", "\n")
            expect["result"] = {
                "cells": result.cells_written,
                "textures": result.textures_written,
                "contested": result.contested,
                "major": result.major,
                "seams": result.seam_vertices,
                "slopes": result.slope_vertices,
                "clamped": result.clamped,
                "dropped": result.dropped,
                "borrowed": result.borrowed,
            }
        except service.MergeServiceError as exc:
            expect["error"] = str(exc)
        expect["lines"] = [line.replace(str(root), "<root>") for line in lines]
        shutil.rmtree(root, ignore_errors=True)
    return {**case, "expect": expect}


def main() -> int:
    """Write the fixture, or check it."""
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--check", action="store_true", help="fail if the fixture is out of date")
    args = parser.parse_args()
    doc = {
        "note": "Generated by tools/gen_land_merge_cases.py -- do not edit.",
        "cases": [run(c) for c in CASES],
    }
    text = json.dumps(doc, indent=1, ensure_ascii=False) + "\n"
    if args.check:
        if not OUT.is_file() or OUT.read_text(encoding="utf-8") != text:
            print(f"{OUT} is out of date: run tools/gen_land_merge_cases.py")
            return 1
        return 0
    OUT.write_text(text, encoding="utf-8", newline="\n")
    print(f"wrote {OUT}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
