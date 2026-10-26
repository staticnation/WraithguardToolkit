"""Translate mlox filename patterns into regular expressions.

Every ordering and predicate rule in the mlox databases names its plugins by
pattern rather than by exact filename, so this translation sits underneath all
rule matching. It follows mlox's own ``_filename_to_regex`` in every wildcard,
because diverging by one would silently change which rules apply to which
plugins -- the kind of bug that produces a subtly wrong load order rather than
an error -- and departs from it in one place, said first:

* Everything but the wildcards is plain text. (mlox escapes only ``(``, ``)``,
  ``+`` and ``.`` and leaves the rest to its expression, so a bracket in a name
  became a character class there; here it is the bracket the author wrote.)
* ``*`` becomes ``.*`` and ``?`` becomes ``.?`` -- note ``.?``, not ``.``, so a
  ``?`` also matches the empty string.
* ``<VER>`` (in any casing) expands to a version-number sub-pattern.
* Matching is anchored at both ends and case-insensitive, since OpenMW's
  virtual file system treats plugin names case-insensitively.
"""

from __future__ import annotations

import re
from functools import cache
from typing import Final

from wraithguard.versions import MLOX_VERSION_PATTERN

#: The parts of a pattern that are not plain text: ``<VER>`` in any casing, ``*`` and ``?``.
_RE_META_TOKEN: Final = re.compile(r"(<VER>)|([*?])", re.IGNORECASE)


def pattern_has_meta(pattern: str) -> bool:
    """Whether a pattern needs regex expansion at all.

    Plain filenames are the overwhelming majority, and comparing them directly
    is both faster and exact, so callers use this to skip the regex path.

    Args:
        pattern: An mlox filename pattern.

    Returns:
        ``True`` if the pattern contains ``*``, ``?``, or a ``<VER>`` token.
    """
    return ("*" in pattern) or ("?" in pattern) or ("<ver>" in pattern.lower())


@cache
def mlox_pattern_to_regex(pattern: str) -> re.Pattern[str]:
    """Compile one mlox filename pattern to an anchored, case-insensitive regex.

    Cached: the same handful of patterns recur across thousands of graph edges
    during a sort, so recompiling them would dominate the run.

    Everything but the three wildcards is plain text, as the rule's author meant it.
    mlox escapes only ``()+.`` and passes the rest to its expression, so its
    ``Julan Ashlander Companion <VER> [Rebirth].esp`` reads ``[Rebirth]`` as a
    character class and never matches the file it names; here it does.

    Args:
        pattern: An mlox filename pattern, e.g. ``"Better Bodies*.esp"``.

    Returns:
        A compiled, anchored, case-insensitive pattern.
    """
    parts = ["^"]
    pos = 0
    for m in _RE_META_TOKEN.finditer(pattern):
        parts.append(re.escape(pattern[pos : m.start()]))
        parts.append(MLOX_VERSION_PATTERN if m.group(1) else "." + m.group(2))
        pos = m.end()
    parts.append(re.escape(pattern[pos:]))
    parts.append("$")
    return re.compile("".join(parts), re.IGNORECASE)
