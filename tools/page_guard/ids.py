"""Element ids the viewer page's scripts look up that nothing in the page creates.

The page is one script assembled from ``viewer-shell/ui/src`` in ``ui/ORDER``. After a merge
of Gardenfell's page, a script can look up an element that only markup we left out (the
grass tools) had; the lookup returns null and the first use throws. This lists every id the
scripts ask for (``$('#x')``, ``getElementById('x')``, any ``'#x'`` string) that no markup
attribute or script assignment defines.

Usage: ``python tools/page_guard/ids.py viewer-shell/ui`` - one ``id<TAB>part`` per line.
Compare with a run on the page before the merge; ids built inside template strings are
reported too, so the list is never empty.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

_DEFINED = (
    re.compile(r"""\bid\s*=\s*["']([A-Za-z][\w-]*)["']"""),
    re.compile(r"""\.id\s*=\s*["']([A-Za-z][\w-]*)["']"""),
    re.compile(r"""id:\s*["']([A-Za-z][\w-]*)["']"""),
)
_USED = re.compile(
    r"""getElementById\(\s*["']([A-Za-z][\w-]*)["']\s*\)"""
    r"""|\$\(\s*["']#?([A-Za-z][\w-]*)["']\s*\)"""
    r"""|["']#([A-Za-z][\w-]*)["']"""
)


def parts(ui: Path) -> list[Path]:
    """The page's parts in load order.

    Args:
        ui: The ``viewer-shell/ui`` folder.

    Returns:
        Each part named in ``ORDER``, in that order.
    """
    names = [
        line.strip()
        for line in (ui / "ORDER").read_text(encoding="utf-8").splitlines()
        if line.strip() and not line.startswith("#")
    ]
    return [ui / "src" / n for n in names]


def missing_ids(ui: Path) -> dict[str, str]:
    """The ids the scripts use that nothing defines.

    Args:
        ui: The ``viewer-shell/ui`` folder.

    Returns:
        Id -> the first part that uses it.
    """
    texts = {p.name: p.read_text(encoding="utf-8") for p in parts(ui)}
    everything = "".join(texts.values())
    defined: set[str] = set()
    for pattern in _DEFINED:
        defined.update(pattern.findall(everything))
    used: dict[str, str] = {}
    for name, text in texts.items():
        if not name.endswith(".js"):
            continue
        for m in _USED.finditer(text):
            used.setdefault(m.group(1) or m.group(2) or m.group(3), name)
    return {i: f for i, f in sorted(used.items()) if i not in defined}


def main(argv: list[str]) -> int:
    """Print the missing ids.

    Args:
        argv: The command line; ``argv[1]`` is the ui folder.

    Returns:
        2 on a bad command line, else 0 (a report, not a gate).
    """
    if len(argv) != 2:
        print(__doc__)
        return 2
    for i, f in missing_ids(Path(argv[1])).items():
        print(f"{i}\t{f}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
