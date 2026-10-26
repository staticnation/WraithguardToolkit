"""Calling ``ordercore`` (``viewer-shell/ordercore``) through the native module.

Port step 4.2 slice 2j. The load order's rules, sort, checks, export, rule maker and updates
are answered by the Rust crate the viewer uses, so Python and the viewer cannot read a load
order two ways. Each Python function keeps its own body as the fallback, used when:

* the native module is not built (a source checkout without ``pip install ./native``);
* ``ordercore`` declines the input (``OrderUnsupportedError``: something it does not read, which
  each crate module's text names);
* :data:`FORCE_PYTHON` is set, as the case generators in ``tools/`` set it, so the expected
  answers they write still come from the Python side and the Rust is checked against them.

The bodies go with Python when the Load order page takes over (slice 2k).
"""

from __future__ import annotations

import os
import re
from contextlib import contextmanager
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from collections.abc import Callable, Iterable, Iterator

try:
    import wraithguard_native as _native
except ImportError:  # pragma: no cover - the native module is built in CI and the app
    _native = None  # type: ignore[assignment]

#: Answer from the Python bodies only (``WG_ORDERCORE=python``, or set by a generator).
FORCE_PYTHON: bool = os.environ.get("WG_ORDERCORE", "").lower() == "python"


@contextmanager
def python_only() -> Iterator[None]:
    """Answer from the Python bodies inside the block (the case generators use it)."""
    global FORCE_PYTHON
    was = FORCE_PYTHON
    FORCE_PYTHON = True
    try:
        yield
    finally:
        FORCE_PYTHON = was


def native(name: str) -> Callable[..., Any] | None:
    """The native function ``name``, or None when Python is to answer.

    Args:
        name: The function's name in ``wraithguard_native``.

    Returns:
        The function, or None when the module is missing, does not have it, or
        :data:`FORCE_PYTHON` is set.
    """
    if FORCE_PYTHON or _native is None:
        return None
    fn: Callable[..., Any] | None = getattr(_native, name, None)
    return fn


def declined(exc: BaseException) -> bool:
    """Whether ``exc`` is ordercore declining an input (the caller then answers itself).

    That is ``OrderUnsupportedError``, and a string Rust cannot take: one holding a lone surrogate,
    which is how Python carries the undecodable bytes of a cfg read with ``surrogateescape``.
    """
    if isinstance(exc, UnicodeEncodeError):
        return True
    kind = getattr(_native, "OrderUnsupportedError", None)
    return kind is not None and isinstance(exc, kind)


def call(
    name: str, fallback: Callable[[], Any], *args: object
) -> Any:  # noqa: ANN401 - the answer's type is the caller's
    """``ordercore``'s answer through ``name``, or ``fallback()``'s.

    Args:
        name: The native function.
        fallback: The Python body, called with no arguments when ordercore does not answer.
        *args: What the native function takes.

    Returns:
        The answer.
    """
    fn = native(name)
    if fn is None:
        return fallback()
    try:
        return fn(*args)
    except Exception as exc:
        if declined(exc):
            return fallback()
        raise


def translated(message: str, templates: Iterable[str]) -> str:
    """One of ordercore's English messages in the user's language.

    ordercore words a message exactly as the Python's English ``msgid`` does (the case files
    pin it), so the message is matched against those templates and rendered again through
    gettext with what filled each ``%(name)s``. A message no template matches is kept as is.

    Args:
        message: What ordercore said.
        templates: The ``msgid`` strings it can be, with their ``%(name)s`` slots.

    Returns:
        The translated message.
    """
    # Under another name: the template is a msgid already extracted where the Python words it,
    # and tools/make_pot.py reads only literal gettext() calls.
    from wraithguard.i18n import gettext as render

    for template in templates:
        parts = re.split(r"%\((\w+)\)([sd])", template)
        pattern, seen, numbers = "", set(), set()
        for i in range(0, len(parts), 3):
            pattern += re.escape(parts[i])
            if i + 2 < len(parts):
                name, kind = parts[i + 1], parts[i + 2]
                if name in seen:
                    pattern += f"(?P={name})"
                else:
                    seen.add(name)
                    pattern += f"(?P<{name}>-?\\d+)" if kind == "d" else f"(?P<{name}>.*?)"
                    if kind == "d":
                        numbers.add(name)
        m = re.fullmatch(pattern, message, re.DOTALL)
        if m:
            values: dict[str, object] = {
                k: int(v) if k in numbers else v for k, v in m.groupdict().items()
            }
            return render(template) % values
    return message
