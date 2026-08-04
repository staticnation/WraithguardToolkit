"""Read ``LAND`` and ``LTEX`` records straight out of a plugin file.

**Why this exists.** ``tes3conv`` decodes a plugin by *understanding* every
record in it, and refuses the whole file when it meets one it does not know::

    Error: Custom { kind: InvalidData, error: "Unexpected Tag: LUAL" }

``LUAL`` is OpenMW's Lua-script configuration record. It is perfectly valid in
an ``.omwaddon`` or an OpenMW-CS ``.esm``, tes3conv targets vanilla TES3, and a
single one of them anywhere in a master stops a nine-hundred-mod merge before it
starts -- over a record with no bearing on terrain whatsoever.

A TES3 file does not require that understanding to walk. Every record and
subrecord is length-prefixed, so a reader that only wants ``LAND`` and ``LTEX``
can *skip* everything else by its declared size without knowing what it is.
That is what this does, and it is why it survives record types that did not
exist when it was written -- including whatever OpenMW adds next.

**This is a fallback, not a replacement.** tes3conv remains the primary reader
because it is the verified one, and it is still what writes the merged plugin.
This runs when tes3conv refuses a file, and its output is checked against
tes3conv's across every plugin available in
``tests/test_land_native.py``.

**Shape.** Records come back in exactly the form
:meth:`~wraithguard.land.diff.LandscapeLayers.from_record` already expects from
tes3conv -- same keys, same nesting -- so nothing downstream needs to know which
reader produced them. Grid payloads are raw ``bytes`` rather than
base64-over-zstd text, which the decoders already accept.

Record layout is from the UESP TES3 file format reference and cross-checked
against ``tes3`` (MIT, Greatness7): ``libs/esp/src/types/landscape.rs`` and
``landscapetexture.rs``.
"""

from __future__ import annotations

import json
import logging
import struct
from typing import TYPE_CHECKING, Any, Final

if TYPE_CHECKING:
    from collections.abc import Iterator
    from pathlib import Path

_log: Final = logging.getLogger(__name__)

#: A TES3 record header: tag, body size, unused, object flags.
_RECORD_HEADER: Final = 16

#: A subrecord header: tag, payload size.
_SUBRECORD_HEADER: Final = 8

#: Offset of the object flags word within a record header.
_FLAGS_OFFSET: Final = 12

#: The object flag meaning the plugin deleted this record.
_DELETED: Final = 0x20

#: Size of the ``VHGT`` payload: an ``f32`` offset, 65x65 ``i8`` deltas, and
#: three bytes of padding Bethesda never used.
_VHGT_SIZE: Final = 4232

#: Where the height deltas start and end inside ``VHGT``.
_VHGT_DATA: Final = slice(4, 4 + 65 * 65)

#: The four bytes every TES3 plugin begins with. Oblivion and Skyrim plugins
#: begin with ``TES4``, and a Morrowind load order can quite easily contain
#: one -- an armour pack ported from Skyrim that shipped its source ``.esp``,
#: say. Their record header is 24 bytes where this expects 16, so walking one
#: produces garbage that *looks* like landscape: a record tagged ``LAND`` at
#: nonsense coordinates carrying nonsense heights.
TES3_MAGIC: Final = b"TES3"

#: Record tags worth stopping for. Everything else is skipped by its length.
_WANTED: Final[frozenset[bytes]] = frozenset({b"LAND", b"LTEX"})

#: ``LAND`` subrecords, mapped to the key tes3conv gives them.
_GRIDS: Final[dict[bytes, str]] = {
    b"VNML": "vertex_normals",
    b"WNAM": "world_map_data",
    b"VCLR": "vertex_colors",
    b"VTEX": "texture_indices",
}

#: Suffix of the record-key sidecar the conflict scanner writes.
KEYS_SUFFIX: Final = ".keys.json"

#: Schema version of the sidecar this understands. A sidecar written by a
#: different version is ignored rather than guessed at.
KEYS_VERSION: Final = 3

#: Record types in a sidecar that mean the plugin has terrain.
_LANDSCAPE_TYPES: Final[frozenset[str]] = frozenset({"Landscape", "LandscapeTexture"})

#: Every landscape flag bit ``tes3`` names, in the spelling it writes.
_FLAG_NAMES: Final[tuple[tuple[int, str], ...]] = (
    (0x1, "USES_VERTEX_HEIGHTS_AND_NORMALS"),
    (0x2, "USES_VERTEX_COLORS"),
    (0x4, "USES_TEXTURES"),
)


class NativeReadError(Exception):
    """Raised when a file cannot be walked as a TES3 plugin at all."""


def _iter_records(data: bytes) -> Iterator[tuple[bytes, int, bytes]]:
    """Walk a plugin's top-level records.

    Unknown record types are skipped by their declared size rather than
    inspected, which is the entire point: this must not care what OpenMW or a
    future tool put in the file.

    Args:
        data: The whole file.

    Yields:
        ``(tag, object flags, body)`` for each record.
    """
    position = 0
    total = len(data)
    while position + _RECORD_HEADER <= total:
        tag = data[position : position + 4]
        (size,) = struct.unpack_from("<I", data, position + 4)
        (flags,) = struct.unpack_from("<I", data, position + _FLAGS_OFFSET)
        start = position + _RECORD_HEADER
        end = start + size
        if end > total:
            # A truncated final record. Plugins come from the internet; yield
            # what is there and stop rather than abandoning the whole file.
            _log.warning(
                "plugin ends mid-record (%s); ignoring the remainder",
                tag.decode("latin-1", "replace"),
            )
            return
        yield tag, flags, data[start:end]
        # Always advance past the header, so a zero-size record cannot loop.
        position = end


def _iter_subrecords(body: bytes) -> Iterator[tuple[bytes, bytes]]:
    """Walk one record's subrecords.

    Args:
        body: The record body, without its header.

    Yields:
        ``(tag, payload)`` for each subrecord.
    """
    position = 0
    total = len(body)
    while position + _SUBRECORD_HEADER <= total:
        tag = body[position : position + 4]
        (size,) = struct.unpack_from("<I", body, position + 4)
        start = position + _SUBRECORD_HEADER
        end = start + size
        if end > total:
            return
        yield tag, body[start:end]
        position = end


def _text(payload: bytes) -> str:
    """Decode a NUL-terminated TES3 string.

    Args:
        payload: The subrecord payload.

    Returns:
        The string, without its terminator. ``latin-1`` because that is what
        the Construction Set wrote and it cannot fail.
    """
    return payload.split(b"\x00", 1)[0].decode("latin-1", "replace")


def format_landscape_flags(value: int) -> str:
    """Render a ``DATA`` word the way tes3conv writes it.

    Only the three named bits are emitted. Real files carry an unnamed ``0x8``
    that ``tes3`` has no name for, and inventing one produces a string tes3conv
    rejects -- so it is dropped here exactly as tes3conv drops it.

    Args:
        value: The raw ``DATA`` word.

    Returns:
        A ``" | "``-joined flag string, empty when no named bit is set.
    """
    return " | ".join(name for bit, name in _FLAG_NAMES if value & bit)


def _landscape(flags: int, body: bytes) -> dict[str, Any] | None:
    """Decode one ``LAND`` record.

    Args:
        flags: The record's object flags.
        body: The record body.

    Returns:
        The record in tes3conv's shape, or ``None`` when it carries no grid
        coordinates and so cannot be placed.
    """
    record: dict[str, Any] = {
        "type": "Landscape",
        "flags": "DELETED" if flags & _DELETED else "",
    }
    grid: tuple[int, int] | None = None

    for tag, payload in _iter_subrecords(body):
        if tag == b"INTV" and len(payload) >= 8:
            grid = struct.unpack_from("<ii", payload, 0)
        elif tag == b"DATA" and len(payload) >= 4:
            (raw,) = struct.unpack_from("<I", payload, 0)
            record["landscape_flags"] = format_landscape_flags(raw)
        elif tag == b"VHGT" and len(payload) >= _VHGT_SIZE:
            (offset,) = struct.unpack_from("<f", payload, 0)
            record["vertex_heights"] = {
                "offset": offset,
                "data": payload[_VHGT_DATA],
            }
        elif tag in _GRIDS:
            record[_GRIDS[tag]] = {"data": payload}
        elif tag == b"DELE":
            record["flags"] = "DELETED"

    if grid is None:
        _log.warning("skipping a LAND record with no INTV coordinates")
        return None
    record["grid"] = [grid[0], grid[1]]
    return record


def _texture(flags: int, body: bytes) -> dict[str, Any] | None:
    """Decode one ``LTEX`` record.

    Args:
        flags: The record's object flags.
        body: The record body.

    Returns:
        The record in tes3conv's shape, or ``None`` when it has no identifier
        -- without one it cannot be matched against another plugin's texture,
        which is the only thing the shared table is for.
    """
    record: dict[str, Any] = {
        "type": "LandscapeTexture",
        "flags": "DELETED" if flags & _DELETED else "",
        "index": 0,
    }
    identifier: str | None = None

    for tag, payload in _iter_subrecords(body):
        if tag == b"NAME":
            identifier = _text(payload)
        elif tag == b"INTV" and len(payload) >= 4:
            (record["index"],) = struct.unpack_from("<I", payload, 0)
        elif tag == b"DATA":
            record["file_name"] = _text(payload)
        elif tag == b"DELE":
            record["flags"] = "DELETED"

    if not identifier:
        _log.warning("skipping an LTEX record with no NAME")
        return None
    record["id"] = identifier
    return record


def landscape_in_sidecar(plugin: Path, sidecar_dir: Path) -> bool | None:
    """Ask the scan cache whether a plugin has terrain, without opening it.

    The conflict scanner already writes a ``<stem>.keys.json`` beside every
    plugin it converts: one ``[type, id, deleted]`` row per record. That is an
    exact answer to *does this plugin have LAND or LTEX records*, where
    :func:`has_landscape` can only offer a byte-pattern guess.

    Worth the lookup: of 972 plugins with a sidecar here, **869 -- 89.4% --
    touch no landscape at all.** Each of those is a subprocess and a JSON
    document that would be converted only to discover there was nothing in it.

    **A stale sidecar is treated as no answer.** The scanner reuses a cache
    whose plugin is newer, because for its purposes a slightly old record list
    is better than re-running the converter. It is not better here: this
    decides whether a plugin is *read at all*, and skipping a plugin that has
    since gained terrain would drop that terrain from the merge silently. So
    anything newer than its sidecar falls through to reading the file.

    Args:
        plugin: The plugin file.
        sidecar_dir: The folder the scanner writes sidecars to.

    Returns:
        ``True`` or ``False`` when the sidecar answers, or ``None`` when there
        is no usable sidecar and the caller must find out for itself.
    """
    side = sidecar_dir / (plugin.stem + KEYS_SUFFIX)
    try:
        if side.stat().st_mtime < plugin.stat().st_mtime:
            _log.debug("%s is newer than its sidecar; reading the plugin", plugin.name)
            return None
        document = json.loads(side.read_text(encoding="utf-8", errors="replace"))
    except (OSError, ValueError):
        return None

    if not isinstance(document, dict) or document.get("v") != KEYS_VERSION:
        return None
    rows = document.get("d")
    if not isinstance(rows, list):
        return None
    return any(
        isinstance(row, (list, tuple)) and row and row[0] in _LANDSCAPE_TYPES for row in rows
    )


def has_landscape(path: Path, sidecar_dir: Path | None = None) -> bool:
    """Whether a plugin mentions landscape at all, without parsing it.

    Most of a load order does not touch terrain -- of 940 mods in a real order,
    the great majority have no ``LAND`` or ``LTEX`` record anywhere. Converting
    each one costs a subprocess and a JSON document that can run to hundreds of
    megabytes, to learn there was nothing to merge.

    This reads the file in chunks and looks for the two tags, overlapping each
    read by three bytes so a tag straddling a chunk boundary is still found.
    False positives are possible and harmless: the tag bytes could occur inside
    a script or a texture path, and the only cost is converting a file that
    turns out to have nothing.

    When ``sidecar_dir`` holds a current ``<stem>.keys.json`` its exact answer
    is used instead, which costs one small read and cannot false-positive.

    Args:
        path: The plugin.
        sidecar_dir: Where the conflict scanner keeps its record-key sidecars.
            Omit it to always scan the bytes.

    Returns:
        ``True`` when either tag appears. ``False`` for a file that is not a
        TES3 plugin at all -- Oblivion and Skyrim plugins have ``LAND`` records
        too, and their bytes would match. ``True`` when the file cannot be read,
        so that an I/O problem surfaces from the real reader with a real message
        rather than being silently reported as "no terrain here".
    """
    if sidecar_dir is not None:
        answer = landscape_in_sidecar(path, sidecar_dir)
        if answer is not None:
            return answer

    chunk = 1 << 20
    overlap = 3
    try:
        with path.open("rb") as handle:
            if handle.read(4) != TES3_MAGIC:
                return False
            handle.seek(0)
            tail = b""
            while True:
                block = handle.read(chunk)
                if not block:
                    return False
                window = tail + block
                if b"LAND" in window or b"LTEX" in window:
                    return True
                tail = window[-overlap:]
    except OSError:
        return True


def read_landscape_records(path: Path) -> list[dict[str, Any]]:
    """Read every ``LAND`` and ``LTEX`` record from a plugin.

    Args:
        path: The plugin file.

    Returns:
        The records, in file order, shaped as tes3conv writes them.

    Raises:
        NativeReadError: If the file cannot be read, or is not a TES3 plugin.
    """
    try:
        data = path.read_bytes()
    except (OSError, MemoryError) as exc:
        raise NativeReadError(f"could not read {path.name}: {exc}") from exc

    if not data.startswith(TES3_MAGIC):
        # Refusing is the whole point. This reader's tolerance -- skip anything
        # you do not recognise, by its declared length -- is exactly what makes
        # it dangerous on a format whose lengths are laid out differently.
        found = data[:4].decode("latin-1", "replace") if len(data) >= 4 else "nothing"
        raise NativeReadError(
            f"{path.name} is not a Morrowind plugin: it begins {found!r}, not "
            f"'TES3'. Reading it as one would invent landscape out of whatever "
            f"its bytes happen to spell."
        )

    records: list[dict[str, Any]] = []
    for tag, flags, body in _iter_records(data):
        if tag not in _WANTED:
            continue
        record = _landscape(flags, body) if tag == b"LAND" else _texture(flags, body)
        if record is not None:
            records.append(record)
    return records
