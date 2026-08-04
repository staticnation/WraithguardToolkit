r"""Check that a plugin survives ``.esp`` -> ``.json`` -> ``.esp`` unchanged.

**Why this runs before any merging code is written.** Wraithguard has been
read-only for its whole life: it parses, compares, reports and renders, and it
cannot damage a load order because it never writes bytes back. Merged Lands and
the diff patcher both change that, and a plugin writer that is subtly wrong
produces files that load, look plausible, and corrupt terrain or records in
ways nobody notices until they walk to the cell.

So the writer is not ours. Records are emitted as JSON in ``tes3conv``'s schema
and ``tes3conv`` performs the binary encoding -- a tool the community already
uses and trusts, rather than a TES3 serializer invented here. That is a much
better position for something with this failure mode, but it is only worth
anything if the conversion is genuinely lossless.

**This measures that, on real files.** It converts a plugin to JSON, converts
the JSON back, and compares. Nothing built on top of the writer can be trusted
further than this check passes.

**What a difference means, and what it does not.** A byte difference is not
automatically data loss: a converter may legitimately normalise field order,
padding, or a header timestamp while preserving every record faithfully. So a
mismatch is reported with its size and location rather than treated as a
verdict, and the record-count comparison is reported separately -- that one
*is* unambiguous.

**Expect very few to be byte-identical, and do not read that as failure.**
``tes3conv`` calls ``plugin.sort_objects()`` on every parse -- deliberately, so
that JSON diffs between two plugins line up. A plugin whose records were not
already in that order therefore *cannot* come back byte-identical, however
perfect the conversion.

**Why the test is convergence rather than equality.** An earlier version of
this tool called any JSON difference lossy, and reported the three vanilla ESMs
as failures. Investigating with ``diff_roundtrip_json.py`` found the cause:
``tes3`` omits a reference's ``NAM9`` (object count) subrecord when the count
is 1, because an absent ``NAM9`` *means* 1. Bethesda's editor wrote the
redundant field; modern tools do not. That is a normalisation to a canonical
form, not data loss -- and a check that calls it loss is measuring the wrong
thing.

So the plugin is converted twice. What matters is whether it settles: a
converter that rewrites something once and then leaves it alone has a canonical
form and has reached it, while one that changes the file on every pass is
losing a little each time and will keep going. Only the second is dangerous,
and only the second is reported as a failure.

**Convergence is necessary, not sufficient.** A converter that dropped every
script would converge too. This tool proves the conversion is a fixed point;
``diff_roundtrip_json.py`` says what the fixed point cost, and a person decides
whether that field mattered. Neither question answers the other.

Usage:
    python tools/check_plugin_roundtrip.py "E:/OpenMW/Morrowind/Data Files"
    python tools/check_plugin_roundtrip.py "E:/.../Data Files" --tes3conv path/to/tes3conv.exe
    python tools/check_plugin_roundtrip.py "E:/.../Data Files" --limit 20
"""

from __future__ import annotations

import argparse
import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Final

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

#: Extensions that hold TES3 records. ``.omwaddon`` and ``.omwgame`` are
#: OpenMW's own names for the same format.
PLUGIN_SUFFIXES = frozenset({".esp", ".esm", ".omwaddon", ".omwgame"})

#: How many plugins to check unless told otherwise. A full Data Files folder is
#: hundreds of plugins and several gigabytes; a spread finds a systematic
#: conversion fault just as well.
SAMPLE = 25

#: Places the converter tends to be, relative to the repository root. It ships
#: inside other people's tools as often as it is installed on its own, so
#: looking only on ``PATH`` finds nothing on a machine that plainly has it.
_NEARBY: Final[tuple[str, ...]] = (
    "../TES3 Conflictsolver/Root/tes3conv.exe",
    "../tes3conv/tes3conv.exe",
    "../tes3conv-master/target/release/tes3conv.exe",
    "tools/tes3conv.exe",
)

#: Outcomes that are not a failure. ``normalised`` is here on the strength of a
#: measurement rather than an assumption -- see the module docstring.
_PASSES: Final[frozenset[str]] = frozenset({"identical", "stable", "normalised"})


def find_tes3conv(explicit: str | None) -> str | None:
    """Locate the converter.

    Checks, in order: what was asked for, ``PATH``, then the places it is
    commonly found near a modding setup.

    Args:
        explicit: A path given on the command line, if any.

    Returns:
        A runnable path, or ``None`` when it cannot be found.
    """
    if explicit:
        return explicit if Path(explicit).is_file() else None
    found = shutil.which("tes3conv") or shutil.which("tes3conv.exe")
    if found:
        return found
    root = Path(__file__).resolve().parent.parent
    for relative in _NEARBY:
        candidate = (root / relative).resolve()
        if candidate.is_file():
            return str(candidate)
    return None


def convert(tool: str, source: Path, target: Path) -> str:
    """Run one conversion.

    Args:
        tool: The converter's path.
        source: Input file.
        target: Output file.

    Returns:
        An error message, or ``""`` on success.
    """
    try:
        result = subprocess.run(  # noqa: S603 -- a fixed argv, no shell
            [tool, str(source), str(target), "--overwrite"],
            capture_output=True,
            text=True,
            timeout=300,
            check=False,
        )
    except (OSError, subprocess.SubprocessError) as exc:
        return f"could not run the converter: {exc}"
    if result.returncode != 0:
        return (result.stderr or result.stdout or "converter failed").strip()[:200]
    return "" if target.is_file() else "converter reported success but wrote nothing"


def record_count(path: Path) -> int | None:
    """Count the records in a converted JSON file.

    The count is the check that actually means something. Byte equality can
    fail for reasons that are not data loss -- normalised padding, a rewritten
    header -- but a record going missing is unambiguous.

    Args:
        path: The JSON file.

    Returns:
        How many records it holds, or ``None`` when it cannot be read.
    """
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return None
    return len(data) if isinstance(data, list) else None


def check_one(tool: str, plugin: Path, folder: Path) -> tuple[str, str]:
    """Round-trip one plugin, twice, and report whether it converges.

    Args:
        tool: The converter's path.
        plugin: The plugin to check.
        folder: A scratch directory.

    Returns:
        An outcome word and a detail string.
    """
    # Every rewritten file keeps the input's extension: tes3 derives the
    # header's file type from it, and writing an .esm back as .esp reports a
    # header difference the harness caused rather than the converter.
    suffix = plugin.suffix
    json_one, json_two = folder / "one.json", folder / "two.json"
    esp_one, esp_two = folder / f"one{suffix}", folder / f"two{suffix}"

    #: Each step, as (input, output, what a failure there would mean).
    steps = (
        (plugin, json_one, "unreadable"),
        (json_one, esp_one, "unwritable"),
        (esp_one, json_two, "unreadable"),
        (json_two, esp_two, "unwritable"),
    )
    for source, target, blame in steps:
        failure = convert(tool, source, target)
        if failure:
            return (blame, failure)

    original = plugin.read_bytes()
    once, twice = esp_one.read_bytes(), esp_two.read_bytes()
    if original == once:
        return ("identical", f"{len(original)} bytes")

    before, after = record_count(json_one), record_count(json_two)
    if before != after:
        return ("lossy", f"records {before} -> {after}")

    detail = f"{len(original)} -> {len(once)} bytes, {before} records held"
    if json_one.read_bytes() == json_two.read_bytes():
        return ("stable", detail)
    if once == twice:
        # A one-time normalisation: the converter changed something on the
        # first pass and then changed nothing further. A fixed point is what
        # separates "normalises to a canonical form" from "loses a little each
        # time", and only the second is a threat to a file written repeatedly.
        return ("normalised", detail)
    return ("drifting", f"{detail}, and the second pass changed it again")


def main(argv: list[str] | None = None) -> int:
    """Round-trip a sample of plugins.

    Args:
        argv: Command-line arguments.

    Returns:
        A process exit code; non-zero when anything lost records.
    """
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("folder", type=Path, help="a Data Files directory")
    parser.add_argument("--tes3conv", default=None, help="path to the converter")
    parser.add_argument("--limit", type=int, default=SAMPLE, help=f"plugins (default {SAMPLE})")
    args = parser.parse_args(argv)

    tool = find_tes3conv(args.tes3conv)
    if tool is None:
        print("tes3conv was not found. Pass --tes3conv with its path.", file=sys.stderr)
        return 2
    if not args.folder.is_dir():
        print(f"not a directory: {args.folder}", file=sys.stderr)
        return 2

    plugins = sorted(
        p for p in args.folder.iterdir() if p.suffix.lower() in PLUGIN_SUFFIXES and p.is_file()
    )
    if not plugins:
        print(f"no plugins under {args.folder}")
        return 0
    step = max(1, len(plugins) // max(1, args.limit))
    sample = plugins[::step][: args.limit]
    print(f"{len(plugins)} plugin(s) present; round-tripping {len(sample)}\n")

    tally: dict[str, int] = {}
    failed: list[str] = []
    normalised: list[str] = []
    for plugin in sample:
        with tempfile.TemporaryDirectory() as scratch:
            outcome, detail = check_one(tool, plugin, Path(scratch))
        tally[outcome] = tally.get(outcome, 0) + 1
        if outcome in _PASSES:
            print(f"  {outcome:11} {plugin.name}")
            if outcome == "normalised":
                normalised.append(plugin.name)
        else:
            print(f"  {outcome.upper():11} {plugin.name}: {detail}")
            failed.append(plugin.name)

    print("\n" + ", ".join(f"{count} {name}" for name, count in sorted(tally.items())))
    print(
        "\n  identical  -- the bytes came back exactly.\n"
        "  stable     -- bytes differ, JSON does not. Every record survived and\n"
        "                only record order changed. tes3conv sorts on every\n"
        "                parse by design, so this is the expected result for a\n"
        "                plugin whose records were not already in that order.\n"
        "  normalised -- the first pass changed something and the second pass\n"
        "                changed nothing further. The converter has a canonical\n"
        "                form and reached it. Run diff_roundtrip_json.py to see\n"
        "                which field, then judge that field on its merits.\n"
        "  drifting   -- it changed again on the second pass. Something is lost\n"
        "                per conversion rather than settled once, which is the\n"
        "                shape that gets worse every time a file is written.\n"
        "  lossy      -- the record count changed. Records were dropped."
    )
    if failed:
        print(f"\n{len(failed)} plugin(s) failed. Emitting JSON is NOT safe for these.")
        return 1
    if normalised:
        print(
            f"\nEvery sampled plugin converged. {len(normalised)} reached a canonical\n"
            "form on the first pass and held it -- which is a pass, but says\n"
            "nothing about whether the normalised field mattered. That question\n"
            "belongs to diff_roundtrip_json.py, one plugin at a time."
        )
        return 0
    print("\nEvery sampled plugin survived the round trip unchanged.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
