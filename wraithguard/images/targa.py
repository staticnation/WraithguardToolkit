"""Decoding Targa images, which Morrowind meshes name far more often than ship.

A ``NiSourceTexture`` written in 2003 usually says ``.tga``, because that is
what the exporter of the day wrote. The file beside it is usually ``.dds``,
because that is what the packager converted it to, and the engine falls back
from one to the other -- which is why
:mod:`wraithguard.nif.textures` substitutes extensions. But not always: plenty
of mods ship the Targa, and some ship *only* the Targa. Without this, those
resolve, load, and then fail to decode.

**The format is two ideas.** A header giving dimensions and depth, then pixels
either laid out plainly or run-length encoded a packet at a time. Both are
here. Color-mapped images are handled too, since 8-bit paletted Targas turn up
in older mods where disk space mattered.

**Two traps, both silent.** Channels are stored **blue first**, so a decoder
that assumes RGB produces an image that looks right until you notice the sky is
orange. And the origin is the **bottom-left** unless a descriptor bit says
otherwise, so a decoder that ignores it returns the image upside down -- which
on a tiling texture is not obvious at all, and on a comparison would report two
identical files as different.
"""

from __future__ import annotations

import struct
from typing import Final

from wraithguard.images.image import Image, ImageError
from wraithguard.logging_setup import get_logger

LOG = get_logger(__name__)

#: Bytes of fixed header before the optional identification field.
_HEADER_SIZE: Final[int] = 18

#: Image type codes. The high bit of the low nibble distinguishes run-length
#: encoded variants from plain ones, which is why they differ by eight.
_COLOUR_MAPPED: Final[int] = 1
_TRUE_COLOUR: Final[int] = 2
_GREYSCALE: Final[int] = 3
_RLE_COLOUR_MAPPED: Final[int] = 9
_RLE_TRUE_COLOUR: Final[int] = 10
_RLE_GREYSCALE: Final[int] = 11

#: Types that carry run-length encoded pixel data.
_RLE_TYPES: Final[frozenset[int]] = frozenset(
    {_RLE_COLOUR_MAPPED, _RLE_TRUE_COLOUR, _RLE_GREYSCALE}
)

#: Types that index into a color map rather than storing pixels directly.
_MAPPED_TYPES: Final[frozenset[int]] = frozenset({_COLOUR_MAPPED, _RLE_COLOUR_MAPPED})

#: Descriptor bit meaning the first row stored is the top one. Clear -- which
#: is the common case -- means the image is stored bottom-up.
_TOP_ORIGIN: Final[int] = 0x20

#: Descriptor bit meaning each row runs right to left.
_RIGHT_ORIGIN: Final[int] = 0x10

#: The same ceiling the DDS decoder applies, for the same reason: a corrupt
#: header must not be able to ask for an enormous allocation.
_MAX_PIXELS: Final[int] = 64 << 20


class TargaError(ImageError):
    """Raised when a Targa image cannot be decoded."""


def _unpack_pixel(raw: bytes, depth: int) -> bytes:
    """Turn one stored pixel into RGBA.

    Args:
        raw: The stored bytes, blue first.
        depth: Bits per pixel.

    Returns:
        Four bytes of RGBA.

    Raises:
        TargaError: If the depth is not one the format defines.
    """
    if depth == 32:
        blue, green, red, alpha = raw[0], raw[1], raw[2], raw[3]
        return bytes((red, green, blue, alpha))
    if depth == 24:
        return bytes((raw[2], raw[1], raw[0], 255))
    if depth == 16:
        # 1:5:5:5, with the single alpha bit almost always ignored in practice.
        # Replicating the low bits keeps full-scale input at full-scale output,
        # as elsewhere.
        packed = raw[0] | (raw[1] << 8)
        red = (packed >> 10) & 0x1F
        green = (packed >> 5) & 0x1F
        blue = packed & 0x1F
        return bytes(
            (
                (red << 3) | (red >> 2),
                (green << 3) | (green >> 2),
                (blue << 3) | (blue >> 2),
                255,
            )
        )
    if depth == 8:
        return bytes((raw[0], raw[0], raw[0], 255))
    raise TargaError(f"unsupported Targa depth: {depth} bits per pixel")


def _read_color_map(data: bytes, offset: int, length: int, depth: int) -> list[bytes]:
    """Read the palette of a color-mapped image.

    Args:
        data: The whole file.
        offset: Where the map starts.
        length: How many entries it holds.
        depth: Bits per entry.

    Returns:
        One RGBA entry per palette slot.

    Raises:
        TargaError: If the map runs off the end of the file.
    """
    step = (depth + 7) // 8
    if offset + length * step > len(data):
        raise TargaError("Targa color map runs past the end of the file")
    return [
        _unpack_pixel(data[offset + index * step : offset + index * step + step], depth)
        for index in range(length)
    ]


def _decode_plain(data: bytes, offset: int, count: int, step: int) -> list[bytes]:
    """Read uncompressed pixel data.

    Args:
        data: The whole file.
        offset: Where the pixels start.
        count: How many pixels to read.
        step: Bytes per pixel.

    Returns:
        The raw stored bytes of each pixel.

    Raises:
        TargaError: If the data ends early.
    """
    if offset + count * step > len(data):
        raise TargaError(
            f"Targa pixel data ends early: wanted {count * step} byte(s), "
            f"file holds {len(data) - offset}"
        )
    return [data[offset + index * step : offset + index * step + step] for index in range(count)]


def _decode_rle(data: bytes, offset: int, count: int, step: int) -> list[bytes]:
    """Read run-length encoded pixel data.

    Each packet begins with a control byte. Its high bit selects the kind and
    its low seven bits carry the count minus one, so a packet describes between
    one and 128 pixels: a *run* repeats a single stored pixel, and a *literal*
    is followed by that many distinct ones.

    Args:
        data: The whole file.
        offset: Where the packets start.
        count: How many pixels the image holds.
        step: Bytes per pixel.

    Returns:
        The raw stored bytes of each pixel.

    Raises:
        TargaError: If the packets end before the image is filled.
    """
    pixels: list[bytes] = []
    position = offset
    while len(pixels) < count:
        if position >= len(data):
            raise TargaError(
                f"Targa run-length data ends early: {len(pixels)} of {count} pixel(s) decoded"
            )
        control = data[position]
        position += 1
        run = (control & 0x7F) + 1
        if control & 0x80:
            if position + step > len(data):
                raise TargaError("Targa run packet is truncated")
            pixel = data[position : position + step]
            position += step
            # A run may legally overrun the final row; the format allows a
            # packet to cross the row boundary, so this is clamped rather than
            # rejected.
            pixels.extend([pixel] * min(run, count - len(pixels)))
            continue
        if position + run * step > len(data):
            raise TargaError("Targa literal packet is truncated")
        pixels.extend(
            data[position + index * step : position + index * step + step]
            for index in range(min(run, count - len(pixels)))
        )
        position += run * step
    return pixels


def read_tga(data: bytes) -> Image:
    """Decode a Targa image.

    Args:
        data: The whole file.

    Returns:
        The image, in top-down reading order whatever the file's own origin.

    Raises:
        TargaError: If the file is malformed, truncated, or uses a variant this
            decoder does not handle.
    """
    if len(data) < _HEADER_SIZE:
        raise TargaError("too short to be a Targa image")
    try:
        (
            id_length,
            has_map,
            image_type,
            _map_first,
            map_length,
            map_depth,
            _x_origin,
            _y_origin,
            width,
            height,
            depth,
            descriptor,
        ) = struct.unpack_from("<BBBHHBHHHHBB", data, 0)
    except struct.error as exc:
        raise TargaError(f"Targa header is truncated: {exc}") from exc

    if width <= 0 or height <= 0:
        raise TargaError(f"implausible dimensions {width}x{height}")
    if width * height > _MAX_PIXELS:
        raise TargaError(f"implausible size: {width}x{height} is {width * height} pixel(s)")
    if image_type not in {
        _COLOUR_MAPPED,
        _TRUE_COLOUR,
        _GREYSCALE,
        _RLE_COLOUR_MAPPED,
        _RLE_TRUE_COLOUR,
        _RLE_GREYSCALE,
    }:
        raise TargaError(f"unsupported Targa image type {image_type}")

    offset = _HEADER_SIZE + id_length
    palette: list[bytes] = []
    if has_map or image_type in _MAPPED_TYPES:
        palette = _read_color_map(data, offset, map_length, map_depth)
        offset += map_length * ((map_depth + 7) // 8)
    if image_type in _MAPPED_TYPES and not palette:
        raise TargaError("color-mapped Targa carries no color map")

    step = (depth + 7) // 8
    if step < 1:
        raise TargaError(f"unsupported Targa depth: {depth} bits per pixel")
    count = width * height
    stored = (
        _decode_rle(data, offset, count, step)
        if image_type in _RLE_TYPES
        else _decode_plain(data, offset, count, step)
    )

    if image_type in _MAPPED_TYPES:
        rgba = [_palette_lookup(palette, entry) for entry in stored]
    else:
        rgba = [_unpack_pixel(entry, depth) for entry in stored]

    return Image(width, height, bytes(_orient(rgba, width, height, descriptor)))


def _palette_lookup(palette: list[bytes], entry: bytes) -> bytes:
    """Resolve one color-map index.

    Args:
        palette: The color map.
        entry: The stored index, one or two bytes.

    Returns:
        Four bytes of RGBA.

    Raises:
        TargaError: If the index is outside the map. That means the file and
            its own header disagree, which is worth reporting rather than
            silently clamping to a color the image never contained.
    """
    index = int.from_bytes(entry, "little")
    if not 0 <= index < len(palette):
        raise TargaError(f"color-map index {index} is outside a map of {len(palette)}")
    return palette[index]


def _orient(rgba: list[bytes], width: int, height: int, descriptor: int) -> bytearray:
    """Put the rows and columns into reading order.

    A Targa stores its first row at the **bottom** unless the descriptor says
    otherwise, which is the opposite of every other format here. Getting this
    wrong returns a vertically mirrored image -- something that looks entirely
    plausible on a tiling texture, and that would make a comparison call two
    identical files different.

    Args:
        rgba: One four-byte pixel per position, in stored order.
        width: Image width.
        height: Image height.
        descriptor: The header's descriptor byte.

    Returns:
        Pixels left to right, top to bottom.
    """
    out = bytearray(width * height * 4)
    top_down = bool(descriptor & _TOP_ORIGIN)
    right_to_left = bool(descriptor & _RIGHT_ORIGIN)
    for row in range(height):
        target_row = row if top_down else height - 1 - row
        source = row * width
        start = target_row * width * 4
        if right_to_left:
            for column in range(width):
                at = start + (width - 1 - column) * 4
                out[at : at + 4] = rgba[source + column]
            continue
        out[start : start + width * 4] = b"".join(rgba[source : source + width])
    return out
