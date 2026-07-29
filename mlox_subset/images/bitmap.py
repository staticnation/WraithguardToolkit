"""Decoding Windows bitmaps, which Morrowind uses more than anyone expects.

BMP is the format the 2002 toolchain wrote when nothing else was called for,
and it survives in the base game's interface art and in a long tail of older
mods. Meshes reference it by name -- ``tx_something.bmp`` -- and the engine's
extension fallback means the file on disk is often a DDS instead, which
:mod:`mlox_subset.nif.textures` already handles. When the BMP really is there,
this reads it.

**Three headers, one layout.** A bitmap starts with a 14-byte file header, then
an information header whose *own first field is its length*. The 12-byte
original, the 40-byte one everything actually writes, and the 108- and 124-byte
extensions all begin with the same fields, so the length is read and the rest
taken from a common shape rather than branching per version.

**Two traps.** Rows are stored **bottom-up** unless the height is negative, and
each row is padded out to a multiple of four bytes -- so a decoder that assumes
``width * 3`` bytes per row drifts one row at a time and produces the diagonal
smear that is the classic symptom. Both are handled here, and both are what the
tests check.
"""

from __future__ import annotations

import struct
from typing import Final

from mlox_subset.images.image import Image, ImageError
from mlox_subset.logging_setup import get_logger

LOG = get_logger(__name__)

#: The two bytes every bitmap starts with.
MAGIC: Final[bytes] = b"BM"

#: Bytes in the file header, before the information header.
_FILE_HEADER: Final[int] = 14

#: Length of the original OS/2 information header, which stores its dimensions
#: as 16-bit values and its palette as three-byte entries.
_CORE_HEADER: Final[int] = 12

#: Compression codes. Anything else is refused rather than guessed at.
_BI_RGB: Final[int] = 0
_BI_RLE8: Final[int] = 1
_BI_RLE4: Final[int] = 2
_BI_BITFIELDS: Final[int] = 3

#: Names for the compression codes this decoder declines, so the message says
#: what the file is rather than only that it failed.
_REFUSED: Final[dict[int, str]] = {
    _BI_RLE8: "run-length encoded 8-bit",
    _BI_RLE4: "run-length encoded 4-bit",
    4: "JPEG-embedded",
    5: "PNG-embedded",
}

#: Depths that index into a palette rather than storing color directly.
_PALETTED: Final[frozenset[int]] = frozenset({1, 4, 8})

#: The same ceiling the other decoders apply.
_MAX_PIXELS: Final[int] = 64 << 20


class BitmapError(ImageError):
    """Raised when a bitmap cannot be decoded."""


def _read_palette(data: bytes, offset: int, count: int, *, wide: bool) -> list[bytes]:
    """Read the color table.

    Args:
        data: The whole file.
        offset: Where the table starts.
        count: How many entries it holds.
        wide: Whether entries are four bytes rather than three. The original
            OS/2 header uses three; everything since uses four.

    Returns:
        One RGBA entry per slot.

    Raises:
        BitmapError: If the table runs past the end of the file.
    """
    step = 4 if wide else 3
    if offset + count * step > len(data):
        raise BitmapError("bitmap color table runs past the end of the file")
    palette: list[bytes] = []
    for index in range(count):
        at = offset + index * step
        # Stored blue first, and the fourth byte is padding rather than alpha
        # in every bitmap that carries a palette.
        palette.append(bytes((data[at + 2], data[at + 1], data[at], 255)))
    return palette


def _row_bytes(width: int, depth: int) -> int:
    """How many bytes one stored row occupies, padding included.

    Args:
        width: Image width in pixels.
        depth: Bits per pixel.

    Returns:
        The stride, rounded up to a multiple of four.
    """
    return ((width * depth + 31) // 32) * 4


def _expand_row(row: bytes, width: int, depth: int, palette: list[bytes],
                masks: tuple[int, int, int, int] | None) -> bytes:
    """Turn one stored row into RGBA.

    Args:
        row: The stored bytes of the row.
        width: How many pixels it holds.
        depth: Bits per pixel.
        palette: The color table, for paletted depths.
        masks: Channel masks, when the header supplied them.

    Returns:
        ``width * 4`` bytes of RGBA.

    Raises:
        BitmapError: If a palette index is outside the table, or the depth is
            not one the format defines.
    """
    out = bytearray(width * 4)
    if depth in _PALETTED:
        per_byte = 8 // depth
        mask = (1 << depth) - 1
        for column in range(width):
            byte = row[column // per_byte]
            # Pixels are packed high bits first within each byte.
            shift = 8 - depth * (column % per_byte + 1)
            index = (byte >> shift) & mask
            if index >= len(palette):
                raise BitmapError(
                    f"palette index {index} is outside a table of {len(palette)}"
                )
            out[column * 4 : column * 4 + 4] = palette[index]
        return bytes(out)
    if depth == 24:
        for column in range(width):
            at = column * 3
            out[column * 4 : column * 4 + 4] = bytes((row[at + 2], row[at + 1], row[at], 255))
        return bytes(out)
    if depth == 32:
        for column in range(width):
            at = column * 4
            # The fourth byte is alpha only when the header declared a mask for
            # it. Plain 32-bit bitmaps leave it zero, and honouring that would
            # decode every such file to a fully transparent image -- which
            # looks exactly like a missing texture.
            alpha = row[at + 3] if masks and masks[3] else 255
            out[column * 4 : column * 4 + 4] = bytes((row[at + 2], row[at + 1], row[at], alpha))
        return bytes(out)
    if depth == 16:
        red_mask, green_mask, blue_mask, alpha_mask = masks or (0x7C00, 0x03E0, 0x001F, 0)
        for column in range(width):
            packed = row[column * 2] | (row[column * 2 + 1] << 8)
            out[column * 4 : column * 4 + 4] = bytes(
                (
                    _channel(packed, red_mask),
                    _channel(packed, green_mask),
                    _channel(packed, blue_mask),
                    _channel(packed, alpha_mask) if alpha_mask else 255,
                )
            )
        return bytes(out)
    raise BitmapError(f"unsupported bitmap depth: {depth} bits per pixel")


def _channel(packed: int, mask: int) -> int:
    """Pull one masked channel out of a packed pixel and scale it to a byte.

    Args:
        packed: The stored pixel.
        mask: The channel's bit mask.

    Returns:
        The channel, 0-255.
    """
    if not mask:
        return 0
    shift = (mask & -mask).bit_length() - 1
    span = mask >> shift
    return (((packed & mask) >> shift) * 255) // span if span else 0


def read_bmp(data: bytes) -> Image:
    """Decode a Windows bitmap.

    Args:
        data: The whole file.

    Returns:
        The image, in top-down reading order whatever the file's own row order.

    Raises:
        BitmapError: If the file is malformed, truncated, or uses a compression
            or depth this decoder does not handle.
    """
    if len(data) < _FILE_HEADER + 4 or not data.startswith(MAGIC):
        raise BitmapError("not a bitmap: missing the 'BM' magic")
    try:
        pixel_offset = struct.unpack_from("<I", data, 10)[0]
        header_size = struct.unpack_from("<I", data, _FILE_HEADER)[0]
        core = header_size == _CORE_HEADER
        if core:
            width, height, _planes, depth = struct.unpack_from("<HHHH", data, _FILE_HEADER + 4)
            compression, palette_count = _BI_RGB, 0
            signed_height = height
        elif header_size >= 40:
            (
                width,
                signed_height,
                _planes,
                depth,
                compression,
                _size,
                _xppm,
                _yppm,
                palette_count,
                _important,
            ) = struct.unpack_from("<iiHHIIiiII", data, _FILE_HEADER + 4)
            height = abs(signed_height)
        else:
            raise BitmapError(f"unrecognised bitmap header of {header_size} byte(s)")
    except struct.error as exc:
        raise BitmapError(f"bitmap header is truncated: {exc}") from exc

    if compression in _REFUSED:
        raise BitmapError(f"unsupported {_REFUSED[compression]} bitmap")
    if compression not in (_BI_RGB, _BI_BITFIELDS):
        raise BitmapError(f"unsupported bitmap compression {compression}")
    if width <= 0 or height <= 0:
        raise BitmapError(f"implausible dimensions {width}x{height}")
    if width * height > _MAX_PIXELS:
        raise BitmapError(f"implausible size: {width}x{height} is {width * height} pixel(s)")

    masks = _read_masks(data, header_size, compression)
    palette: list[bytes] = []
    if depth in _PALETTED:
        count = palette_count or (1 << depth)
        table_at = _FILE_HEADER + header_size + (16 if compression == _BI_BITFIELDS
                                                 and header_size == 40 else 0)
        palette = _read_palette(data, table_at, count, wide=not core)

    stride = _row_bytes(width, depth)
    needed = stride * height
    if pixel_offset + needed > len(data):
        raise BitmapError(
            f"bitmap pixel data ends early: wanted {needed} byte(s) at {pixel_offset}, "
            f"file holds {max(0, len(data) - pixel_offset)}"
        )

    # A negative height means the rows are already top-down; the usual positive
    # height means the first stored row is the bottom of the image.
    top_down = signed_height < 0
    out = bytearray(width * height * 4)
    for row in range(height):
        stored = data[pixel_offset + row * stride : pixel_offset + row * stride + stride]
        target = row if top_down else height - 1 - row
        at = target * width * 4
        out[at : at + width * 4] = _expand_row(stored, width, depth, palette, masks)
    LOG.debug("decoded %dx%d %d-bit bitmap", width, height, depth)
    return Image(width, height, bytes(out))


def _read_masks(
    data: bytes, header_size: int, compression: int
) -> tuple[int, int, int, int] | None:
    """Read the channel masks, wherever this header version keeps them.

    The 40-byte header has no room for them, so a bitfields image puts them
    immediately after it; the 108-byte header and later carry them inline.

    Args:
        data: The whole file.
        header_size: The information header's declared length.
        compression: The compression code.

    Returns:
        Red, green, blue and alpha masks, or ``None`` when the header declares
        none and the depth's default applies.
    """
    if header_size >= 108:
        try:
            return struct.unpack_from("<IIII", data, _FILE_HEADER + 40)
        except struct.error:
            return None
    if compression == _BI_BITFIELDS and header_size == 40:
        try:
            red, green, blue = struct.unpack_from("<III", data, _FILE_HEADER + 40)
        except struct.error:
            return None
        return (red, green, blue, 0)
    return None
