"""Writing PNG files with nothing but the standard library.

An image is only useful once something can display it, and the viewers in this
project are HTML pages and Tk. Both read PNG, neither reads DDS, and PNG is a
container around zlib -- which is in the standard library. So the encoder is
about sixty lines and adds no dependency to a onefile build.

Deliberately not a general PNG writer. It emits one thing: 8-bit RGBA,
non-interlaced, filter type 0 on every scanline. Filtering would shrink the
output, but these images are viewed once and discarded, and a filter chosen
badly costs more time than the bytes are worth.
"""

from __future__ import annotations

import struct
import zlib
from typing import TYPE_CHECKING, Final

if TYPE_CHECKING:
    from wraithguard.images.image import Image

#: The eight bytes every PNG starts with.
_SIGNATURE: Final[bytes] = b"\x89PNG\r\n\x1a\n"

#: Color type 6 is truecolor with alpha; bit depth 8 per channel.
_BIT_DEPTH: Final[int] = 8
_COLOUR_TYPE_RGBA: Final[int] = 6

#: zlib level. Six is the default trade; these images are transient, so paying
#: nine for a few percent is time spent to no purpose.
_COMPRESSION_LEVEL: Final[int] = 6


def _chunk(tag: bytes, payload: bytes) -> bytes:
    """Frame one PNG chunk.

    Args:
        tag: The four-byte chunk type.
        payload: The chunk body.

    Returns:
        Length, type, body and CRC.
    """
    return (
        struct.pack(">I", len(payload))
        + tag
        + payload
        + struct.pack(">I", zlib.crc32(tag + payload) & 0xFFFFFFFF)
    )


def encode_png(image: Image) -> bytes:
    """Encode a decoded surface as an RGBA PNG.

    The buffer length is guaranteed to match the dimensions by :class:`Image`
    itself -- its ``__post_init__`` raises ``ImageError`` on a mismatch, so a
    length re-check here would be dead code that no caller could ever trigger.

    Args:
        image: The surface to write.

    Returns:
        The whole PNG file.
    """
    header = struct.pack(
        ">IIBBBBB", image.width, image.height, _BIT_DEPTH, _COLOUR_TYPE_RGBA, 0, 0, 0
    )
    stride = image.width * 4
    # Each scanline is prefixed with its filter type. Zero means "none", which
    # is what keeps this loop simple and fast.
    raw = bytearray()
    for row in range(image.height):
        raw.append(0)
        raw += image.pixels[row * stride : (row + 1) * stride]
    return b"".join(
        (
            _SIGNATURE,
            _chunk(b"IHDR", header),
            _chunk(b"IDAT", zlib.compress(bytes(raw), _COMPRESSION_LEVEL)),
            _chunk(b"IEND", b""),
        )
    )
