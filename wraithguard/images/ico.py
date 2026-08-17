"""Getting a usable image out of a Windows .ico, for platforms Windows never had to consider.

``.ico`` is a container, not an image format: an ``ICONDIR`` header followed
by one directory entry per size it carries, each pointing at either a classic
BMP/DIB frame or -- for anything built to look right past 256x256, which has
meant essentially every icon made since Vista -- a frame that is simply a PNG
file, byte for byte, dropped in whole.

That matters here because of what a Windows Tk build can do that a Linux one
can't: ``Tk.iconbitmap()`` has special-cased ``.ico`` loading on Windows for
decades, but on X11/Wayland it only understands XBM, a different and far
older format, so the same call there is a silent no-op. The portable
mechanism -- ``iconphoto()``, understood by every real window manager
including KWin (Steam Deck's Desktop Mode) -- needs a ``PhotoImage``, and
Tk's own built-in image formats are GIF/PGM/PPM and PNG, never ICO.

Rather than asking every consumer to ship a second icon asset to keep in
sync with the first, this reads the PNG frame already sitting inside the
``.ico`` the project already has. Parsing only, never pixels: what comes out
is exactly the compressed PNG bytes, ready to hand to something else's PNG
decoder -- Tk's built-in one, in practice.
"""

from __future__ import annotations

import struct
from typing import Final

#: PNG's own magic number, unrelated to anything ICO-specific -- this is
#: simply how a PNG file always begins, wherever it's embedded.
_PNG_MAGIC: Final[bytes] = b"\x89PNG\r\n\x1a\n"

#: Bytes an ICONDIR header takes: reserved (0), type (1 for icon), count.
_ICONDIR_SIZE: Final[int] = 6

#: Bytes one ICONDIRENTRY takes.
_ICONDIRENTRY_SIZE: Final[int] = 16


def largest_png_frame(data: bytes) -> bytes | None:
    """Find the largest PNG-encoded frame inside a ``.ico`` file's bytes.

    Args:
        data: The whole ``.ico`` file.

    Returns:
        The bytes of the largest PNG-encoded frame, exactly as they sit in
        the file -- a complete, independently valid PNG. ``None`` when
        ``data`` is not an ICONDIR at all, or every frame it lists is a
        classic BMP/DIB entry rather than a PNG one (an icon authored a very
        long time ago, before Vista made the larger sizes necessary).
    """
    if len(data) < _ICONDIR_SIZE:
        return None
    reserved, kind, count = struct.unpack_from("<HHH", data, 0)
    if reserved != 0 or kind != 1 or count == 0:
        return None  # not an ICONDIR at all

    best: tuple[int, int, int] | None = None  # (area, size, offset)
    for index in range(count):
        entry_offset = _ICONDIR_SIZE + index * _ICONDIRENTRY_SIZE
        if entry_offset + _ICONDIRENTRY_SIZE > len(data):
            break  # the header claimed more entries than the file holds
        width, height, _colors, _reserved, _planes, _bit_count, size, offset = struct.unpack_from(
            "<BBBBHHII", data, entry_offset
        )
        # 0 in either byte means 256, not zero -- ICONDIRENTRY's one quirk.
        area = (width or 256) * (height or 256)
        if size < len(_PNG_MAGIC) or offset + size > len(data):
            continue  # this entry's own bookkeeping doesn't fit the file
        if data[offset : offset + len(_PNG_MAGIC)] != _PNG_MAGIC:
            continue  # a classic BMP/DIB frame, not a PNG one
        if best is None or area > best[0]:
            best = (area, size, offset)

    if best is None:
        return None
    _area, size, offset = best
    return data[offset : offset + size]
