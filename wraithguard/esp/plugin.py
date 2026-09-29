"""Read a whole plugin into records, and write records back to a plugin.

The reading and writing are greatness7's ``tes3::esp`` (``native/src/esp.rs``): it
parses the file, validates every record, and writes them back. The records cross to
Python as dicts in the tes3conv schema, with no JSON in between, and
:mod:`wraithguard.esp.json` builds the record objects the toolkit works with (and
turns them back). Records the crate does not model (OpenMW's ``LUAL`` and the like)
cross as their raw bytes and come back as :class:`~wraithguard.esp.record.UnknownRecord`,
so a plugin that mixes them still round-trips exactly. A NaN or infinite float is
just a float here - JSON had no number for it, which is why a detour through JSON
could not carry one. The first record is always the ``TES3`` header.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, Any

from wraithguard.esp.flags import ObjectFlags
from wraithguard.esp.io import EspError
from wraithguard.esp.json import EspJsonError, record_from_json, record_to_json
from wraithguard.esp.record import UnknownRecord

if TYPE_CHECKING:
    from collections.abc import Iterable

    from wraithguard.esp.record import Record
    from wraithguard.esp.records import Header

#: The bridge's tag for a record the crate does not model (native/src/esp.rs RAW).
_RAW = "__Raw"


def _native():  # noqa: ANN202 - the extension module
    """The Rust backend, imported where it is needed (see wraithguard/nif/bsa.py)."""
    from wraithguard.nif.bsa import _native as native

    return native


def _from_items(items: list[dict[str, Any]]) -> list[Record]:
    """Record objects from the bridge's dicts."""
    records: list[Record] = []
    for item in items:
        if item.get("type") == _RAW:
            records.append(
                UnknownRecord(
                    str(item["tag"]).encode("latin-1"),
                    ObjectFlags(int(item["flags"])),
                    bytes(item["body"]),
                )
            )
            continue
        try:
            records.append(record_from_json(item))
        except (EspJsonError, KeyError, TypeError, ValueError) as exc:
            raise EspError(f"a {item.get('type')} record: {exc}") from exc
    return records


def _read(data: bytes, keep: frozenset[bytes] | None = None) -> list[Record]:
    """Parse ``data`` in the crate, optionally only the records tagged in ``keep``."""
    try:
        items = _native().plugin_records(
            bytes(data), list(keep) if keep is not None else None, False, True
        )
    except ValueError as exc:
        raise EspError(str(exc)) from exc
    return _from_items(items)


def read_header(data: bytes) -> Header:
    """Read just the plugin header -- the first, ``TES3`` record -- from ``data``.

    Only the first record is parsed, so ``data`` need only hold that record's
    bytes, and a plugin whose later records ``tes3conv`` would refuse still yields
    its header here.

    Args:
        data: The plugin's bytes, or at least a prefix holding the whole first
            record (its 16-byte header plus the body its size declares).

    Returns:
        The parsed :class:`~wraithguard.esp.records.Header`.

    Raises:
        EspError: If the bytes are too short, or the first record is not a
            ``TES3`` header.
    """
    from wraithguard.esp.records import Header  # deferred: keeps import order simple

    if len(data) < 16:
        raise EspError("too short to hold a record header")
    if bytes(data[:4]) != b"TES3":
        raise EspError(f"expected a TES3 header record, got {bytes(data[:4])!r}")
    size = int.from_bytes(data[4:8], "little")
    records = _read(bytes(data[: 16 + size]))
    if not records or not isinstance(records[0], Header):
        raise EspError("expected a TES3 header record")
    return records[0]


def read_plugin(data: bytes) -> list[Record]:
    """Read every record in ``data``, in file order.

    Args:
        data: The bytes of a plugin file.

    Returns:
        The records, ready to inspect or edit and hand back to
        :func:`write_plugin`.

    Raises:
        EspError: If the plugin is malformed.
    """
    return _read(data)


def read_plugin_filtered(data: bytes, keep: frozenset[bytes]) -> list[Record]:
    """Read only the records whose tag is in ``keep``; the rest are never parsed.

    Args:
        data: The bytes of a plugin file.
        keep: The record tags to read.

    Returns:
        The kept records, in file order.

    Raises:
        EspError: If the plugin is malformed.
    """
    return _read(data, frozenset(keep))


def _to_item(record: Record) -> dict[str, Any]:
    """One record as the bridge's dict."""
    if isinstance(record, UnknownRecord):
        return {
            "type": _RAW,
            "tag": record.wire_tag.decode("latin-1"),
            "flags": int(record.flags),
            "body": bytes(record.body),
        }
    return record_to_json(record)


def write_plugin(records: Iterable[Record]) -> bytes:
    """Write ``records`` back to plugin bytes, in the order given.

    Each record is written as it stands; the header's record count is not
    recomputed (the caller keeps it, as before).

    Args:
        records: The records to write.

    Returns:
        The plugin file's bytes.

    Raises:
        EspError: If a record cannot be written.
    """
    items = [_to_item(r) for r in records]
    try:
        return bytes(_native().plugin_write(items))
    except ValueError as exc:
        raise EspError(str(exc)) from exc
