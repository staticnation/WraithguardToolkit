"""Calling ``ordercore`` (``viewer-shell/ordercore``) through the native module.

Since port step 4.2 slice 2k the load order - rules, sort, checks, export, the rule maker,
the downloads - has no Python body: ``compute_plan`` / ``write_plan`` and the rest ask for
their native function with :func:`require`, which says plainly when the module is missing.
The few functions still answered both ways (the master check the Tk tools call, say) use
:func:`native` / :func:`call`, keeping their body for a decline (``OrderUnsupportedError``,
or a string with a lone surrogate from a cfg read with ``surrogateescape``) and for
:data:`FORCE_PYTHON` (``WG_ORDERCORE=python``, or :func:`python_only`).

:func:`translated` puts ordercore's English messages back into the user's language.
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


def require(name: str) -> Callable[..., Any]:
    """The native function ``name``: the load order has no Python body any more.

    Raises:
        RuntimeError: The native module is not built, or is too old to have it.
    """
    fn: Callable[..., Any] | None = getattr(_native, name, None) if _native is not None else None
    if fn is None:
        raise RuntimeError(
            "Wraithguard's Rust backend (wraithguard_native) is missing or too old for the load "
            f"order ({name}); build it with `pip install ./native`."
        )
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


def _matched(message: str, template: str) -> dict[str, object] | None:
    """What filled each ``%(name)s`` / ``%(name)d`` of ``template`` in ``message``, if it is one."""
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
    if m is None:
        return None
    return {k: int(v) if k in numbers else v for k, v in m.groupdict().items()}


def translated(message: str, templates: Iterable[str | tuple[str, str]]) -> str:
    """One of ordercore's English messages in the user's language.

    ordercore words a message exactly as the Python's English ``msgid`` does (the case files
    pin it), so the message is matched against those templates and rendered again through
    gettext with what filled each ``%(name)s``. A ``(singular, plural)`` pair is an ``ngettext``
    message, rendered for its ``count`` (or its first number). A message that matches nothing
    is tried again without its leading blank lines and spaces (printed outside the msgid), and
    otherwise kept as is. Where two templates fit, the one with more literal text wins.

    Args:
        message: What ordercore said.
        templates: The ``msgid`` strings it can be, with their ``%(name)s`` slots.

    Returns:
        The translated message.
    """
    # Under another name: the template is a msgid already extracted where the Python words it,
    # and tools/make_pot.py reads only literal gettext() calls.
    from wraithguard.i18n import gettext as render, ngettext as render_plural

    # The most literal text first: "  in-memory scan: %(plugins)d ..." before "  %(file)s: ...",
    # which would take it too.
    def literal(template: str | tuple[str, str]) -> int:
        text = template[0] if isinstance(template, tuple) else template
        return -len(re.sub(r"%\(\w+\)[sd]", "", text))

    templates = sorted(templates, key=literal)
    lead = message[: len(message) - len(message.lstrip("\n "))]
    for text in (message, message[len(lead) :]) if lead else (message,):
        prefix = lead if text is not message else ""
        for template in templates:
            if isinstance(template, tuple):
                one, many = template
                for form in (one, many):
                    values = _matched(text, form)
                    if values is not None:
                        numbers = [v for v in values.values() if isinstance(v, int)]
                        count = values.get("count", numbers[0] if numbers else 1)
                        return prefix + render_plural(one, many, int(str(count))) % values
            else:
                values = _matched(text, template)
                if values is not None:
                    return prefix + render(template) % values
    return message
