r"""Find *what* a plugin loses on a ``.esp`` -> ``.json`` -> ``.esp`` round trip.

``check_plugin_roundtrip.py`` answers whether anything was lost.
This answers what.

**Why a second tool.** The first check reported the three vanilla ESMs as lossy
while their record counts were unchanged -- 10,776, 48,296 and 10,000 records in
and the same number out. Nothing was dropped, so whatever changed is *inside*
records, and it is systematic rather than one bad record: three files written
years apart by the same studio all show it, and the plugins made by ordinary
mod tools do not.

That shape of result is worth naming precisely before anyone decides what it
means. A field that changes in 48,296 records is a different problem from one
that changes in three, and the fix -- or the decision to accept it -- depends on
which.

**How it works.** The plugin is converted to JSON, back to a plugin, and to JSON
again. The two JSON documents are compared structurally and every difference is
reduced to a *path* such as ``Cell.references[].scale``, with the list index
flattened so that the same field in ten thousand records tallies as one finding
rather than ten thousand. The result is a ranked list of what changed and how
often, with a few real before-and-after values for each.

**On reading the output.** A high count is not automatically worse than a low
one. A float differing in its last decimal place across every record is a
serialisation artefact and probably harmless; a single missing script or a
changed cell reference is not, however rare. The tool ranks by frequency because
that is what it can measure, and shows values because the values are what let a
person judge severity.

Usage:
    python tools/diff_roundtrip_json.py "E:/.../Data Files/Tribunal.esm"
    python tools/diff_roundtrip_json.py "E:/.../Morrowind.esm" --examples 5
    python tools/diff_roundtrip_json.py "E:/.../Tribunal.esm" --keep out/

Start with the smallest affected file. Tribunal.esm is 4.5 MB against
Morrowind.esm's 80 MB and shows the same systematic fault for a twentieth of
the memory -- this holds two decoded JSON documents in memory at once, and for
Morrowind.esm that is several gigabytes.
"""

from __future__ import annotations

import argparse
import json
import sys
import tempfile
from pathlib import Path
from typing import Any, Final

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from tools.check_plugin_roundtrip import convert, find_tes3conv

#: How many example values to keep per differing path. Enough to see whether a
#: difference is uniform or varies; not so many that the report is unreadable.
EXAMPLES: Final[int] = 3

#: Paths are truncated past this depth. Deeply nested differences are still
#: reported, just grouped at their ancestor, which keeps the tally meaningful.
_MAX_DEPTH: Final[int] = 12

#: Values are abbreviated to this width in the report. Some fields hold whole
#: compiled scripts.
_WIDTH: Final[int] = 90


def _abbreviate(value: Any) -> str:
    """Render a value short enough to print.

    Args:
        value: Any decoded JSON value.

    Returns:
        A single-line representation.
    """
    text = repr(value)
    if len(text) > _WIDTH:
        return f"{text[: _WIDTH - 3]}..."
    return text


def _walk(
    before: Any,
    after: Any,
    path: str,
    found: dict[str, list[tuple[str, str]]],
    depth: int = 0,
) -> None:
    """Record every difference between two decoded JSON values.

    List indices are flattened to ``[]`` on purpose: the same field differing in
    every record of a plugin is one finding, not ten thousand, and the whole
    value of this tool is that it says so.

    Args:
        before: The value from the first conversion.
        after: The value from the second.
        path: The dotted path reached so far.
        found: Accumulates path -> example differences, modified in place.
        depth: Current recursion depth.
    """
    if before == after:
        return
    if depth >= _MAX_DEPTH:
        found.setdefault(path, []).append((_abbreviate(before), _abbreviate(after)))
        return

    if isinstance(before, dict) and isinstance(after, dict):
        for key in sorted(set(before) | set(after)):
            step = f"{path}.{key}" if path else str(key)
            if key not in before:
                found.setdefault(f"{step} (added)", []).append(("-", _abbreviate(after[key])))
            elif key not in after:
                found.setdefault(f"{step} (LOST)", []).append((_abbreviate(before[key]), "-"))
            else:
                _walk(before[key], after[key], step, found, depth + 1)
        return

    if isinstance(before, list) and isinstance(after, list):
        if len(before) != len(after):
            found.setdefault(f"{path}[] (length)", []).append((str(len(before)), str(len(after))))
        for left, right in zip(before, after):
            _walk(left, right, f"{path}[]", found, depth + 1)
        return

    found.setdefault(path, []).append((_abbreviate(before), _abbreviate(after)))


def _kind(record: Any) -> str:
    """Name a record's type for grouping.

    Args:
        record: One decoded record.

    Returns:
        The record's type name, or ``"?"`` when it has none.
    """
    if isinstance(record, dict):
        for key in ("type", "Type", "kind"):
            if key in record and isinstance(record[key], str):
                return record[key]
    return "?"


def _load(path: Path) -> list[Any]:
    """Decode a converted plugin.

    Args:
        path: The JSON file.

    Returns:
        The record list.

    Raises:
        SystemExit: If the file cannot be decoded or is not a list.
    """
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except MemoryError:
        raise SystemExit(
            f"ran out of memory decoding {path.name}. This holds two JSON "
            "documents at once; try a smaller plugin first -- the fault is "
            "systematic and a small file shows it just as well."
        ) from None
    except (OSError, ValueError) as exc:
        raise SystemExit(f"could not read {path}: {exc}") from exc
    if not isinstance(data, list):
        raise SystemExit(f"{path} is not a record list")
    return data


def compare(first: Path, second: Path, examples: int) -> int:
    """Report every difference between two converted plugins.

    Args:
        first: JSON from the first conversion.
        second: JSON from the second.
        examples: How many example values to show per path.

    Returns:
        A process exit code; non-zero when anything differed.
    """
    before, after = _load(first), _load(second)
    print(f"{len(before)} record(s) in the first pass, {len(after)} in the second")
    if len(before) != len(after):
        print("  RECORD COUNT CHANGED -- records were added or dropped outright.")

    found: dict[str, list[tuple[str, str]]] = {}
    by_kind: dict[str, int] = {}
    changed = 0
    for left, right in zip(before, after):
        if left == right:
            continue
        changed += 1
        kind = _kind(left)
        by_kind[kind] = by_kind.get(kind, 0) + 1
        _walk(left, right, kind, found)

    if not found and len(before) == len(after):
        print("\nThe two conversions are identical. Nothing was lost.")
        return 0

    share = 100.0 * changed / len(before) if before else 0.0
    print(f"{changed} record(s) changed ({share:.1f}%)\n")
    print("by record type:")
    for kind, count in sorted(by_kind.items(), key=lambda item: -item[1]):
        print(f"  {count:>8}  {kind}")

    print(f"\n{len(found)} distinct field path(s) differ, most frequent first:\n")
    for path, samples in sorted(found.items(), key=lambda item: -len(item[1])):
        print(f"  {len(samples):>8}x  {path}")
        # The distinct values matter more than the examples, and this is the
        # whole argument. A field dropped 2,055 times is harmless if every
        # dropped value was the format's default, and is real data loss if even
        # one was not. Three sampled examples cannot tell those apart; a
        # complete tally can, so print it.
        tally: dict[str, int] = {}
        for was, _ in samples:
            tally[was] = tally.get(was, 0) + 1
        if len(tally) == 1:
            only, count = next(iter(tally.items()))
            print(f"              every one of the {count} was {only}")
        else:
            print(f"              {len(tally)} distinct value(s) before:")
            ranked = sorted(tally.items(), key=lambda item: -item[1])
            for value, count in ranked[:examples]:
                print(f"                {count:>8}x {value}")
            if len(ranked) > examples:
                print(f"                ... and {len(ranked) - examples} more")
        for was, now in samples[:examples]:
            print(f"              before: {was}  ->  after: {now}")
        print()

    print(
        "Reading this: a path ending (LOST) is a field the writer did not\n"
        "emit. Whether that is data loss depends on the values -- a field\n"
        "dropped only where it held the format's default is a normalisation,\n"
        "since a reader restores the same meaning either way. The distinct\n"
        "value tally above is what settles it. A numeric path differing in\n"
        "its last digits is a serialisation artefact and usually harmless."
    )
    return 1


def main(argv: list[str] | None = None) -> int:
    """Diff one plugin against its own round trip.

    Args:
        argv: Command-line arguments.

    Returns:
        A process exit code.
    """
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("plugin", type=Path, help="the plugin to examine")
    parser.add_argument("--tes3conv", default=None, help="path to the converter")
    parser.add_argument("--examples", type=int, default=EXAMPLES, help="values per path")
    parser.add_argument("--keep", type=Path, default=None, help="keep the JSON here")
    args = parser.parse_args(argv)

    tool = find_tes3conv(args.tes3conv)
    if tool is None:
        print("tes3conv was not found. Pass --tes3conv with its path.", file=sys.stderr)
        return 2
    if not args.plugin.is_file():
        print(f"no such plugin: {args.plugin}", file=sys.stderr)
        return 2

    with tempfile.TemporaryDirectory() as scratch:
        folder = args.keep if args.keep else Path(scratch)
        folder.mkdir(parents=True, exist_ok=True)
        # Write back under the *input's* extension. tes3 derives the header's
        # file type from it, so round-tripping an .esm through a file named
        # .esp reports a header difference that the harness caused rather than
        # the converter -- an artefact of the instrument, which is exactly the
        # kind of thing an instrument must not invent.
        first = folder / "first.json"
        back = folder / f"back{args.plugin.suffix}"
        second = folder / "second.json"

        print(f"converting {args.plugin.name} ...")
        for source, target, stage in (
            (args.plugin, first, "to JSON"),
            (first, back, "back to a plugin"),
            (back, second, "to JSON again"),
        ):
            failure = convert(tool, source, target)
            if failure:
                print(f"conversion failed {stage}: {failure}", file=sys.stderr)
                return 2

        original, rewritten = args.plugin.stat().st_size, back.stat().st_size
        print(f"{original} bytes -> {rewritten} ({rewritten - original:+d})\n")
        return compare(first, second, args.examples)


if __name__ == "__main__":
    raise SystemExit(main())
