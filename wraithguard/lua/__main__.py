"""``python -m wraithguard.lua openmw.cfg``: the Lua report for a load order.

Copyright (c) 2026 StaticNation.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

from wraithguard.lua.report import all_findings, render
from wraithguard.lua.scan import scan_cfg


def main(argv: list[str] | None = None) -> int:
    """Scan an openmw.cfg's Lua scripts and print the report.

    Args:
        argv: The arguments (default: the command line's).

    Returns:
        1 when there are errors, else 0.
    """
    ap = argparse.ArgumentParser(prog="python -m wraithguard.lua", description=__doc__)
    ap.add_argument("cfg", type=Path, help="the openmw.cfg to read")
    ap.add_argument("--no-info", action="store_true", help="leave out the notes")
    args = ap.parse_args(argv)
    result = scan_cfg(args.cfg)
    sys.stdout.write(render(result, info=not args.no_info))
    return 1 if any(f.severity == "error" for _, f in all_findings(result)) else 0


if __name__ == "__main__":
    raise SystemExit(main())
