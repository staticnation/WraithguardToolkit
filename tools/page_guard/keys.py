"""String keys the viewer page uses that its catalogue (``05_text.js``) does not hold.

A ``T('key')`` call with no entry shows the raw key on screen; a ``data-tx``/``data-txt``
key with none falls back to the markup's own English, which is harmless. After a merge of
Gardenfell's page, script keys are the ones to look at.

Usage: ``python tools/page_guard/keys.py viewer-shell/ui [script]`` - one ``key<TAB>file``
per line; with ``script``, only ``T('key')`` calls. Compare with a run before the merge.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

_ENTRY = re.compile(r'^\s*"([A-Za-z0-9_.\-]+)":\s*(?:\[|$)', re.M)
_SCRIPT = re.compile(r"\bT\(\s*'([A-Za-z0-9_.\-]+)'")
_MARKUP = re.compile(r'data-txt?="([A-Za-z0-9_.\-]+)"')


def missing_keys(ui: Path, script_only: bool) -> dict[str, str]:
    """The keys used and not in the catalogue.

    Args:
        ui: The ``viewer-shell/ui`` folder.
        script_only: Only ``T('key')`` calls, not markup keys.

    Returns:
        Key -> the first file that uses it.
    """
    src = ui / "src"
    have = set(_ENTRY.findall((src / "05_text.js").read_text(encoding="utf-8")))
    used: dict[str, str] = {}
    for f in sorted(src.iterdir()):
        text = f.read_text(encoding="utf-8")
        for m in _SCRIPT.finditer(text):
            used.setdefault(m.group(1), f.name)
        if not script_only:
            for m in _MARKUP.finditer(text):
                used.setdefault(m.group(1), f.name)

    def known(k: str) -> bool:
        """Whether the catalogue has the key, or its plural forms.

        Args:
            k: A key.

        Returns:
            True when ``k``, ``k.one`` or ``k.other`` is an entry.
        """
        return k in have or f"{k}.one" in have or f"{k}.other" in have

    return {k: f for k, f in sorted(used.items()) if not known(k)}


def main(argv: list[str]) -> int:
    """Print the missing keys.

    Args:
        argv: The command line; ``argv[1]`` is the ui folder, ``argv[2]`` may be ``script``.

    Returns:
        2 on a bad command line, else 0 (a report, not a gate).
    """
    if len(argv) not in (2, 3) or (len(argv) == 3 and argv[2] != "script"):
        print(__doc__)
        return 2
    for k, f in missing_keys(Path(argv[1]), len(argv) == 3).items():
        print(f"{k}\t{f}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
