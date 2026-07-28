"""Decoding DDS textures to plain RGBA, with no third-party dependency.

A texture conflict raises the same question a mesh conflict does -- *does the
winner actually look different?* -- and answering it needs pixels. Every Python
library that decodes DDS is either a large dependency, a wrapper around a C
library that complicates a onefile build, or licensed incompatibly with this
project. The formats Morrowind ships are small and arithmetic, so they are
decoded here.

**What is supported, and why that is enough.** Morrowind-era textures are
DXT1, DXT3 and DXT5, plus the occasional uncompressed surface. In the local
corpus that is 18, 14 and 17 files respectively with one uncompressed, and
nothing else at all. BC7 exists in newer replacers now that OpenMW supports it
and is deliberately not handled yet; an unsupported format produces a
:class:`DdsError`, never a wrong image.

**The block formats are arithmetic, not expression.** A DXT1 block is two
16-bit colours and sixteen 2-bit indices; the decode is the interpolation the
format defines. That is a fact about the format, derived here from the public
description and checked against real files, in the same way and for the same
licensing reasons as ``mlox_subset.nif`` -- see ``NIF_PROVENANCE.md``.
"""

from __future__ import annotations

import struct
from dataclasses import dataclass
from typing import Final

from mlox_subset.logging_setup import get_logger

LOG = get_logger(__name__)

#: The four bytes every DDS file starts with.
_MAGIC: Final[bytes] = b"DDS "

#: Header length in bytes, excluding the magic. The field is in the file too
#: and is checked rather than trusted.
_HEADER_SIZE: Final[int] = 124

#: Flag in ``pixel_format.flags`` meaning "the FourCC field is meaningful".
_DDPF_FOURCC: Final[int] = 0x4

#: Flag meaning the surface stores uncompressed RGB.
_DDPF_RGB: Final[int] = 0x40

#: Flag meaning an uncompressed surface carries alpha.
_DDPF_ALPHAPIXELS: Final[int] = 0x1

#: Bytes per compressed block, by FourCC. DXT1 packs colour only; the rest
#: carry eight bytes of alpha in front of the same colour block.
_BLOCK_BYTES: Final[dict[bytes, int]] = {
    b"DXT1": 8,
    b"DXT2": 16,
    b"DXT3": 16,
    b"DXT4": 16,
    b"DXT5": 16,
}

#: A guard on dimensions read from the file, so a corrupt header cannot ask for
#: a multi-gigabyte allocation.
_MAX_DIMENSION: Final[int] = 1 << 15

#: A guard on the *total* pixel count, which is the one that matters. Bounding
#: each dimension alone is not enough: 32768 x 32768 passes that check and then
#: asks for 4.3 GB of RGBA before a single byte of texture data is read. The
#: largest texture in any Morrowind setup is a few thousand pixels square, so
#: 64 megapixels is far above anything real and far below anything dangerous.
_MAX_PIXELS: Final[int] = 64 << 20


class DdsError(Exception):
    """Raised when a texture cannot be decoded."""


@dataclass(frozen=True, slots=True)
class Image:
    """A decoded surface.

    Attributes:
        width: Width in pixels.
        height: Height in pixels.
        pixels: ``width * height * 4`` bytes of non-premultiplied RGBA.
    """

    width: int
    height: int
    pixels: bytes

    def pixel(self, x: int, y: int) -> tuple[int, int, int, int]:
        """Read one pixel.

        Args:
            x: Column.
            y: Row.

        Returns:
            Red, green, blue and alpha, each 0-255.

        Raises:
            IndexError: If the coordinates are outside the image.
        """
        if not (0 <= x < self.width and 0 <= y < self.height):
            raise IndexError(f"({x}, {y}) is outside {self.width}x{self.height}")
        start = (y * self.width + x) * 4
        r, g, b, a = self.pixels[start : start + 4]
        return (r, g, b, a)


def _expand_565(value: int) -> tuple[int, int, int]:
    """Expand a 16-bit 5:6:5 colour to 8 bits per channel.

    The low bits are replicated from the high ones rather than zero-filled, so
    that full-scale input maps to full-scale output. Shifting alone would make
    white decode as 248, 252, 248 and give every decoded texture a faint cast.

    Args:
        value: The packed colour.

    Returns:
        Red, green and blue, each 0-255.
    """
    r = (value >> 11) & 0x1F
    g = (value >> 5) & 0x3F
    b = value & 0x1F
    return ((r << 3) | (r >> 2), (g << 2) | (g >> 4), (b << 3) | (b >> 2))


def _colour_table(c0: int, c1: int, *, punchthrough: bool) -> list[bytes]:
    """Build the four-entry colour table for one block.

    Args:
        c0: First endpoint, packed 5:6:5.
        c1: Second endpoint, packed 5:6:5.
        punchthrough: Whether the three-colour mode is available. DXT1 selects
            it by endpoint order; DXT3 and DXT5 never use it, because their
            alpha lives in its own block.

    Returns:
        Four RGBA entries, each already packed as four bytes so the decode loop
        can assign them into the output without rebuilding them per pixel.
    """
    r0, g0, b0 = _expand_565(c0)
    r1, g1, b1 = _expand_565(c1)
    table = [bytes((r0, g0, b0, 255)), bytes((r1, g1, b1, 255))]
    if punchthrough and c0 <= c1:
        # Three colours and a transparent slot: the midpoint, then a hole.
        table.append(bytes(((r0 + r1) // 2, (g0 + g1) // 2, (b0 + b1) // 2, 255)))
        table.append(b"\x00\x00\x00\x00")
    else:
        table.append(bytes(((2 * r0 + r1) // 3, (2 * g0 + g1) // 3, (2 * b0 + b1) // 3, 255)))
        table.append(bytes(((r0 + 2 * r1) // 3, (g0 + 2 * g1) // 3, (b0 + 2 * b1) // 3, 255)))
    return table


def _alpha_table(a0: int, a1: int) -> list[int]:
    """Build the eight-entry alpha table of a DXT5 block.

    Args:
        a0: First alpha endpoint.
        a1: Second alpha endpoint.

    Returns:
        Eight alpha values, 0-255.
    """
    if a0 > a1:
        return [a0, a1, *[((7 - i) * a0 + i * a1) // 7 for i in range(1, 7)]]
    # Six interpolated values plus explicit transparent and opaque slots.
    return [a0, a1, *[((5 - i) * a0 + i * a1) // 5 for i in range(1, 5)], 0, 255]


def _decode_blocks(data: bytes, width: int, height: int, fourcc: bytes) -> bytearray:
    """Decode a whole compressed surface.

    Args:
        data: The surface bytes.
        width: Surface width.
        height: Surface height.
        fourcc: The compression tag.

    Returns:
        RGBA pixels.

    Raises:
        DdsError: If the data runs out mid-surface.
    """
    stride = _BLOCK_BYTES[fourcc]
    has_alpha_block = stride == 16
    explicit_alpha = fourcc in (b"DXT2", b"DXT3")
    out = bytearray(width * height * 4)
    offset = 0
    for block_y in range(0, height, 4):
        for block_x in range(0, width, 4):
            if offset + stride > len(data):
                raise DdsError(
                    f"texture data ends early: wanted {stride} byte(s) at {offset}, "
                    f"file holds {len(data) - offset}"
                )
            block = data[offset : offset + stride]
            offset += stride
            alphas: list[int] | None = None
            if has_alpha_block:
                alphas = (
                    _explicit_alphas(block[:8])
                    if explicit_alpha
                    else _interpolated_alphas(block[:8])
                )
                block = block[8:]
            c0, c1, bits = struct.unpack_from("<HHI", block, 0)
            table = _colour_table(c0, c1, punchthrough=not has_alpha_block)
            # Whole rows of four pixels are written in one slice assignment.
            # Per-pixel assignment decoded the corpus in 23 seconds; a 2048px
            # texture is a quarter of a million blocks, and the loop overhead
            # rather than the arithmetic was the cost.
            wide = block_x + 4 <= width
            for row in range(4):
                y = block_y + row
                if y >= height:
                    break
                shift = 8 * row
                quad = (bits >> shift) & 0xFF
                indices = (quad & 0x3, (quad >> 2) & 0x3, (quad >> 4) & 0x3, (quad >> 6) & 0x3)
                start = (y * width + block_x) * 4
                if alphas is None:
                    if wide:
                        out[start : start + 16] = b"".join(table[i] for i in indices)
                        continue
                    for col in range(min(4, width - block_x)):
                        out[start + col * 4 : start + col * 4 + 4] = table[indices[col]]
                    continue
                base = 4 * row
                for col in range(4 if wide else max(0, width - block_x)):
                    at = start + col * 4
                    out[at : at + 4] = table[indices[col]]
                    out[at + 3] = alphas[base + col]
    return out


def _explicit_alphas(block: bytes) -> list[int]:
    """Read a DXT3 alpha block: sixteen 4-bit values.

    Args:
        block: The eight alpha bytes.

    Returns:
        Sixteen alpha values, 0-255.
    """
    values: list[int] = []
    for byte in block:
        low, high = byte & 0xF, byte >> 4
        # Replicated, not shifted, for the same reason as the colour channels:
        # 0xF must reach 255 or every decoded texture is slightly transparent.
        values.append(low * 17)
        values.append(high * 17)
    return values


def _interpolated_alphas(block: bytes) -> list[int]:
    """Read a DXT5 alpha block: two endpoints and sixteen 3-bit indices.

    Args:
        block: The eight alpha bytes.

    Returns:
        Sixteen alpha values, 0-255.
    """
    table = _alpha_table(block[0], block[1])
    packed = int.from_bytes(block[2:8], "little")
    return [table[(packed >> (3 * i)) & 0x7] for i in range(16)]


def _decode_uncompressed(
    data: bytes, width: int, height: int, bit_count: int, masks: tuple[int, int, int, int]
) -> bytearray:
    """Decode an uncompressed surface described by channel masks.

    Args:
        data: The surface bytes.
        width: Surface width.
        height: Surface height.
        bit_count: Bits per pixel; 24 and 32 are supported.
        masks: Red, green, blue and alpha bit masks.

    Returns:
        RGBA pixels.

    Raises:
        DdsError: If the depth is unsupported or the data is short.
    """
    if bit_count not in (24, 32):
        raise DdsError(f"unsupported uncompressed depth: {bit_count} bits per pixel")
    step = bit_count // 8
    needed = width * height * step
    if len(data) < needed:
        raise DdsError(f"texture data ends early: wanted {needed} byte(s), got {len(data)}")
    shifts = [(_lowest_bit(mask), mask) for mask in masks]
    out = bytearray(width * height * 4)
    for index in range(width * height):
        raw = int.from_bytes(data[index * step : index * step + step], "little")
        channels = []
        for position, (shift, mask) in enumerate(shifts):
            if not mask:
                # No alpha mask means an opaque surface, not a transparent one.
                channels.append(255 if position == 3 else 0)
                continue
            channels.append(_scale_to_byte((raw & mask) >> shift, mask >> shift))
        out[index * 4 : index * 4 + 4] = bytes(channels)
    return out


def _lowest_bit(mask: int) -> int:
    """Position of a mask's lowest set bit.

    Args:
        mask: The channel mask.

    Returns:
        The shift, or ``0`` for an empty mask.
    """
    return (mask & -mask).bit_length() - 1 if mask else 0


def _scale_to_byte(value: int, maximum: int) -> int:
    """Scale a channel of arbitrary width to 0-255.

    Args:
        value: The channel value.
        maximum: The largest it can be.

    Returns:
        The scaled value.
    """
    return 255 if maximum <= 0 else (value * 255) // maximum


def read_dds(data: bytes) -> Image:
    """Decode a DDS texture.

    Args:
        data: The whole file.

    Returns:
        The top-level surface. Mipmaps are ignored: a conflict is judged on the
        image itself, and decoding chains nobody looks at wastes the time this
        module exists to save.

    Raises:
        DdsError: If the file is not a DDS, is truncated, or uses a format this
            decoder does not handle. Never a :class:`struct.error`: these files
            arrive from mod archives and must fail as a finding.
    """
    if len(data) < 4 + _HEADER_SIZE or not data.startswith(_MAGIC):
        raise DdsError("not a DDS file: missing the 'DDS ' magic")
    size, _flags, height, width = struct.unpack_from("<IIII", data, 4)
    if size != _HEADER_SIZE:
        raise DdsError(f"DDS header claims {size} bytes, expected {_HEADER_SIZE}")
    if not 0 < width <= _MAX_DIMENSION or not 0 < height <= _MAX_DIMENSION:
        raise DdsError(f"implausible dimensions {width}x{height}")
    if width * height > _MAX_PIXELS:
        raise DdsError(f"implausible size: {width}x{height} is {width * height} pixel(s)")
    pf_flags, fourcc, bit_count, r_mask, g_mask, b_mask, a_mask = struct.unpack_from(
        "<I4sIIIII", data, 4 + 76
    )
    surface = data[4 + _HEADER_SIZE :]
    if pf_flags & _DDPF_FOURCC:
        if fourcc not in _BLOCK_BYTES:
            raise DdsError(f"unsupported texture compression {fourcc.decode('ascii', 'replace')!r}")
        pixels = _decode_blocks(surface, width, height, fourcc)
    elif pf_flags & (_DDPF_RGB | _DDPF_ALPHAPIXELS):
        pixels = _decode_uncompressed(
            surface, width, height, bit_count, (r_mask, g_mask, b_mask, a_mask)
        )
    else:
        raise DdsError(f"unsupported DDS pixel format flags {pf_flags:#010x}")
    LOG.debug("decoded %dx%d %s", width, height, fourcc.decode("ascii", "replace").strip("\x00"))
    return Image(width, height, bytes(pixels))
