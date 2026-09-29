"""Decoding DDS textures to plain RGBA, with no third-party dependency.

A texture conflict raises the same question a mesh conflict does -- *does the
winner actually look different?* -- and answering it needs pixels. Every Python
library that decodes DDS is either a large dependency, a wrapper around a C
library that complicates a onefile build, or licensed incompatibly with this
project. ``pydds`` is the closest fit and is GPLv3, which would relicense
everything here. The formats are arithmetic, so they are decoded here.

**What is handled.** BC1 through BC5 and BC7, plus uncompressed surfaces
described by channel masks. That covers Morrowind's own textures (BC1, BC3 and
the occasional uncompressed surface), the normal maps that current mods ship
(BC5), the single-channel height and gloss masks that come with them (BC4),
and the BC7 that OpenMW-era replacers use. BC6H is HDR, has no use in this
game, and is refused by name rather than guessed at.

**Two eras of header.** The original pixel format identifies compression with
a four-character code. Anything defined after Direct3D 10 -- BC7 included --
sets that code to ``DX10`` and puts the real format in a twenty-byte extension
header after it. Both are read; a file that says ``DX10`` and then stops is a
truncation, not a format this decoder lacks, and it says so.

**Normal maps are not color.** BC5 stores two channels because a tangent-space
normal's third can be recomputed, and this reconstructs it (in
``native/src/img.rs``, ``decode_bc5``). Morrowind and OpenMW both use the **DirectX**
convention, so the green channel is *not* flipped on load. Flipping it, which
tooling written against the OpenGL convention does by default, would make every
normal map in the collection compare as different from an identical copy of
itself.

**Where the pixels are decoded.** The block decoders run in the Rust module
(``native/src/img.rs``), ported step for step from the Python that was here;
this file keeps the header parsing, mip selection and error types.
``tests/test_images_native_parity.py`` holds the two byte-identical.

**The block formats are arithmetic, not expression.** A DXT1 block is two
16-bit colors and sixteen 2-bit indices; the decode is the interpolation the
format defines. That is a fact about the format, derived here from the public
description and checked against real files, in the same way and for the same
licensing reasons as ``wraithguard.nif`` -- see ``NIF_PROVENANCE.md``.
"""

from __future__ import annotations

import struct
from dataclasses import dataclass
from typing import Final

import wraithguard_native as _native

from wraithguard.images.image import Image, ImageError
from wraithguard.logging_setup import get_logger

LOG = get_logger(__name__)

#: The four bytes every DDS file starts with.
MAGIC: Final[bytes] = b"DDS "

#: Header length in bytes, excluding the magic. The field is in the file too
#: and is checked rather than trusted.
_HEADER_SIZE: Final[int] = 124

#: Length of the Direct3D 10 extension header that follows a ``DX10`` code.
_DX10_SIZE: Final[int] = 20

#: Flag in ``pixel_format.flags`` meaning "the FourCC field is meaningful".
_DDPF_FOURCC: Final[int] = 0x4

#: Flag meaning the surface stores uncompressed RGB.
_DDPF_RGB: Final[int] = 0x40

#: Flag meaning an uncompressed surface carries alpha.
_DDPF_ALPHAPIXELS: Final[int] = 0x1

#: Flag meaning the surface is a single luminance channel.
_DDPF_LUMINANCE: Final[int] = 0x20000

#: The code that says "the real format is in the extension header".
_DX10: Final[bytes] = b"DX10"

#: Bytes per compressed block, by FourCC. DXT1 and the single-channel BC4 pack
#: eight; everything else carries a second eight-byte half.
_BLOCK_BYTES: Final[dict[bytes, int]] = {
    b"DXT1": 8,
    b"DXT2": 16,
    b"DXT3": 16,
    b"DXT4": 16,
    b"DXT5": 16,
    b"ATI1": 8,
    b"BC4U": 8,
    b"BC4S": 8,
    b"ATI2": 16,
    b"BC5U": 16,
    b"BC5S": 16,
}

#: FourCCs that mean "one channel, stored as a DXT5-style interpolated block".
_ONE_CHANNEL: Final[frozenset[bytes]] = frozenset({b"ATI1", b"BC4U", b"BC4S"})

#: FourCCs that mean "two channels, stored as a pair of those blocks".
_TWO_CHANNEL: Final[frozenset[bytes]] = frozenset({b"ATI2", b"BC5U", b"BC5S"})

#: DXGI format numbers mapped onto the older code that means the same layout.
#: Each block format has typeless, unorm and sRGB spellings that differ only in
#: how a GPU samples them, not in how the bits decode.
_DXGI_TO_FOURCC: Final[dict[int, bytes]] = {
    70: b"DXT1", 71: b"DXT1", 72: b"DXT1",
    73: b"DXT3", 74: b"DXT3", 75: b"DXT3",
    76: b"DXT5", 77: b"DXT5", 78: b"DXT5",
    79: b"ATI1", 80: b"ATI1", 81: b"ATI1",
    82: b"ATI2", 83: b"ATI2", 84: b"ATI2",
    97: _DX10, 98: _DX10, 99: _DX10,
}  # fmt: skip

#: DXGI numbers this decoder deliberately refuses, with why. Saying "BC6H is
#: HDR and this game has no use for it" is a better answer than "unsupported
#: format 95", because it tells the user not to wait for it.
_DXGI_REFUSED: Final[dict[int, str]] = {
    95: "BC6H is an HDR format with no use in this game",
    96: "BC6H is an HDR format with no use in this game",
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


class DdsError(ImageError):
    """Raised when a DDS texture cannot be decoded.

    Kept as its own type, rather than folded into :class:`ImageError`, so that
    a caller walking a folder can tell "this DDS is broken" from "this is not
    a DDS at all".
    """


def _resolve_dx10(data: bytes) -> tuple[bytes, int]:
    """Read the Direct3D 10 extension header and say what it means.

    Args:
        data: The whole file.

    Returns:
        The equivalent FourCC -- or :data:`_DX10` itself for BC7, which has no
        older spelling -- and the offset at which the surface begins.

    Raises:
        DdsError: If the header is missing or names a format not handled.
    """
    start = 4 + _HEADER_SIZE
    if len(data) < start + _DX10_SIZE:
        raise DdsError("file claims a DX10 extension header and then ends")
    dxgi = struct.unpack_from("<I", data, start)[0]
    if dxgi in _DXGI_REFUSED:
        raise DdsError(f"unsupported DXGI format {dxgi}: {_DXGI_REFUSED[dxgi]}")
    fourcc = _DXGI_TO_FOURCC.get(dxgi)
    if fourcc is None:
        raise DdsError(f"unsupported DXGI format {dxgi}")
    return fourcc, start + _DX10_SIZE


def _unit_bytes(fourcc: bytes, pf_flags: int, bit_count: int) -> tuple[bool, int]:
    """Whether a surface is block-compressed, and its block or pixel size.

    Args:
        fourcc: The (already DX10-resolved) compression tag.
        pf_flags: The pixel-format flags.
        bit_count: Bits per pixel for an uncompressed surface.

    Returns:
        ``(compressed, unit_bytes)`` -- ``unit_bytes`` is the size of one 4x4
        block when compressed, else one pixel.
    """
    if pf_flags & _DDPF_FOURCC:
        if fourcc == _DX10:
            return True, 16  # BC7: one 16-byte block per 4x4
        block = _BLOCK_BYTES.get(fourcc)
        if block is not None:
            return True, block
    return False, max(1, bit_count // 8)


def _level_bytes(width: int, height: int, *, compressed: bool, unit: int) -> int:
    """The byte size of one mip level at ``width`` x ``height``."""
    if compressed:
        return max(1, (width + 3) // 4) * max(1, (height + 3) // 4) * unit
    return width * height * unit


def _choose_level(width: int, height: int, mipcount: int, max_dim: int) -> int:
    """The largest mip level whose longest side is within ``max_dim``.

    Args:
        width: Top-level width.
        height: Top-level height.
        mipcount: How many levels the file stores.
        max_dim: The longest side to allow.

    Returns:
        A level index in ``[0, mipcount)``; the smallest available level if even
        that is larger than ``max_dim``.
    """
    for level in range(mipcount):
        if max(1, width >> level, height >> level) <= max_dim:
            return level
    return mipcount - 1


def read_dds(data: bytes, max_dimension: int | None = None) -> Image:
    """Decode a DDS texture.

    Args:
        data: The whole file.
        max_dimension: When set and the file stores mipmaps, decode the largest
            mip whose longest side is within this many pixels instead of the full
            surface -- a 2048px diffuse becomes a 1024px decode, a quarter of the
            work and a quarter of the memory, which is what makes a whole cell's
            textures affordable. Omitted (the default), the top surface is used,
            so a conflict comparison still judges the image at full resolution.

    Returns:
        The chosen surface as RGBA. Mip chains below the chosen level are ignored.

    Raises:
        DdsError: If the file is not a DDS, is truncated, or uses a format this
            decoder does not handle. Never a :class:`struct.error`: these files
            arrive from mod archives and must fail as a finding.
    """
    if len(data) < 4 + _HEADER_SIZE or not data.startswith(MAGIC):
        raise DdsError("not a DDS file: missing the 'DDS ' magic")
    try:
        size, _flags, height, width = struct.unpack_from("<IIII", data, 4)
        (mipcount,) = struct.unpack_from("<I", data, 4 + 24)
        if size != _HEADER_SIZE:
            raise DdsError(f"DDS header claims {size} bytes, expected {_HEADER_SIZE}")
        if not 0 < width <= _MAX_DIMENSION or not 0 < height <= _MAX_DIMENSION:
            raise DdsError(f"implausible dimensions {width}x{height}")
        if width * height > _MAX_PIXELS:
            raise DdsError(f"implausible size: {width}x{height} is {width * height} pixel(s)")
        pf_flags, fourcc, bit_count, r_mask, g_mask, b_mask, a_mask = struct.unpack_from(
            "<I4sIIIII", data, 4 + 76
        )
        start = 4 + _HEADER_SIZE
        if pf_flags & _DDPF_FOURCC and fourcc == _DX10:
            fourcc, start = _resolve_dx10(data)
        # Skip to a smaller mip when asked and there is a chain to skip into.
        if max_dimension and mipcount > 1 and max(width, height) > max_dimension:
            compressed, unit = _unit_bytes(fourcc, pf_flags, bit_count)
            level = _choose_level(width, height, mipcount, max_dimension)
            offset = sum(
                _level_bytes(
                    max(1, width >> k), max(1, height >> k), compressed=compressed, unit=unit
                )
                for k in range(level)
            )
            if start + offset < len(data):  # a corrupt chain falls back to level 0
                start += offset
                width = max(1, width >> level)
                height = max(1, height >> level)
        surface = data[start:]

        if pf_flags & _DDPF_FOURCC:
            pixels, label = _decode_compressed(surface, width, height, fourcc)
        elif pf_flags & (_DDPF_RGB | _DDPF_ALPHAPIXELS | _DDPF_LUMINANCE):
            if pf_flags & _DDPF_LUMINANCE and not r_mask:
                # A luminance surface with no explicit mask is grey at full
                # depth. Treating it as an empty red channel would decode the
                # whole image to black without failing.
                r_mask = g_mask = b_mask = (1 << bit_count) - 1
            try:
                pixels = _native.dds_uncompressed(
                    surface, width, height, bit_count, (r_mask, g_mask, b_mask, a_mask)
                )
            except ValueError as exc:
                raise DdsError(str(exc)) from exc
            label = f"{bit_count}-bit uncompressed"
        else:
            raise DdsError(f"unsupported DDS pixel format flags {pf_flags:#010x}")
    except struct.error as exc:  # pragma: no cover - the length guard above means every
        # unpack_from here has the bytes it needs; kept as a defensive net (see the
        # docstring: these mod-archive files must fail as a finding, never a struct.error).
        raise DdsError(f"DDS header is truncated: {exc}") from exc
    LOG.debug("decoded %dx%d %s", width, height, label)
    return Image(width, height, pixels)


def _decode_compressed(surface: bytes, width: int, height: int, fourcc: bytes) -> tuple[bytes, str]:
    """Dispatch a block-compressed surface to the decoder for its format.

    Args:
        surface: The surface bytes.
        width: Surface width.
        height: Surface height.
        fourcc: The compression tag, already resolved through any DX10 header.

    Returns:
        The RGBA pixels and a name for the log line.

    Raises:
        DdsError: If the format is not one this decoder handles.
    """
    if fourcc == _DX10:
        label = "BC7"
    elif fourcc in _TWO_CHANNEL:
        label = "BC5"
    elif fourcc in _ONE_CHANNEL:
        label = "BC4"
    elif fourcc in _BLOCK_BYTES:
        label = fourcc.decode("ascii", "replace")
    else:
        readable = fourcc.decode("ascii", "replace").strip("\x00")
        raise DdsError(f"unsupported texture compression {readable!r}")
    # The block decoders -- DXT1-5, BC4, BC5 (Z rebuilt, DirectX green) and BC7 --
    # are in native/src/img.rs, ported step for step from the Python that was
    # here; tests/test_images_native_parity.py holds the two byte-identical.
    try:
        pixels = _native.dds_decode(surface, width, height, fourcc)
    except ValueError as exc:
        # BC7 always reported through the base ImageError; keep that.
        raise (ImageError if label == "BC7" else DdsError)(str(exc)) from exc
    return pixels, label


#: Block-compressed formats a browser can upload straight to the GPU, mapped to
#: the short label the viewer's JS turns into a WebGL internal format. S3TC
#: (DXT1/3/5) and BPTC (BC7) go behind the common
#: ``WEBGL_compressed_texture_s3tc`` / ``EXT_texture_compression_bptc``
#: extensions. BC4/BC5 (the single- and two-channel height, gloss and normal
#: masks) go behind ``EXT_texture_compression_rgtc`` -- three.js already
#: reconstructs a two-channel normal map's Z on the GPU when a texture's
#: format says "packed RG" (``isPackedRGFormat``, checked against
#: ``material.normalMap.format``), and already samples a bump map's single
#: channel as ``.x``, so BC5 and BC4 need no shader change here, only the
#: format label below and the client-side extension lookup to match it.
#: Uncompressed surfaces have nothing to pass through.
_GPU_FORMATS: Final[dict[bytes, str]] = {
    b"DXT1": "dxt1",
    b"DXT3": "dxt3",
    b"DXT5": "dxt5",
    b"ATI1": "bc4",
    b"BC4U": "bc4",
    b"BC4S": "bc4",
    b"ATI2": "bc5",
    b"BC5U": "bc5",
    b"BC5S": "bc5",
    _DX10: "bc7",  # BC7, resolved from the DX10 extension header
}

#: The format labels :func:`dds_passthrough` can return, for a consumer (the
#: viewer) to check a format against without hand-duplicating this list --
#: that duplication is exactly how the viewer ended up rejecting "bc7" long
#: after this function already supported it.
GPU_PASSTHROUGH_FORMATS: Final[frozenset[str]] = frozenset(_GPU_FORMATS.values())


@dataclass(frozen=True)
class CompressedTexture:
    """A block-compressed DDS surface ready to hand straight to the GPU.

    No decode has happened: the block bytes are exactly as the file stored them,
    for a WebGL page to upload through a ``THREE.CompressedTexture``.

    Attributes:
        format: The GPU format label -- ``"dxt1"``, ``"dxt3"``, ``"dxt5"`` or
            ``"bc7"`` -- naming the WebGL internal format to upload as.
        width: The top mip level's width in pixels.
        height: The top mip level's height in pixels.
        levels: ``(width, height, byte_length)`` per mip level, largest first, for
            the whole chain the file actually stores (a truncated chain stops at
            the last whole level).
        data: The mip levels' block bytes, concatenated in ``levels`` order.
    """

    format: str
    width: int
    height: int
    levels: list[tuple[int, int, int]]
    data: bytes


def dds_passthrough(data: bytes) -> CompressedTexture | None:
    """Describe a DDS for direct GPU upload, or ``None`` to fall back to a decode.

    The block-compressed formats a browser can upload without decoding (S3TC and
    BPTC -- see :data:`_GPU_FORMATS`) are the largest, most numerous textures in a
    collection, so handing their blocks straight to the GPU skips the CPU decode
    entirely. This reads only the header and slices the mip chain; it never
    touches a block.

    Args:
        data: The whole DDS file.

    Returns:
        A :class:`CompressedTexture` when the surface is a GPU-uploadable
        block format, else ``None`` -- for an uncompressed surface, BC4/BC5, a
        format this does not pass through, or a file too malformed to trust. The
        caller then decodes with :func:`read_dds` exactly as before, so a ``None``
        is a fallback, never a failure.
    """
    if len(data) < 4 + _HEADER_SIZE or not data.startswith(MAGIC):
        return None
    try:
        size, _flags, height, width = struct.unpack_from("<IIII", data, 4)
        (mipcount,) = struct.unpack_from("<I", data, 4 + 24)
        pf_flags, fourcc = struct.unpack_from("<I4s", data, 4 + 76)
        if size != _HEADER_SIZE:
            return None
        if not 0 < width <= _MAX_DIMENSION or not 0 < height <= _MAX_DIMENSION:
            return None
        if width * height > _MAX_PIXELS:
            return None
        if not pf_flags & _DDPF_FOURCC:
            return None  # uncompressed: nothing to pass through
        start = 4 + _HEADER_SIZE
        if fourcc == _DX10:
            fourcc, start = _resolve_dx10(data)  # raises DdsError on a refused/odd format
    except (struct.error, DdsError):
        return None
    label = _GPU_FORMATS.get(fourcc)
    if label is None:
        return None
    unit = 16 if fourcc == _DX10 else _BLOCK_BYTES[fourcc]
    levels: list[tuple[int, int, int]] = []
    payload = bytearray()
    offset = start
    for level in range(max(1, mipcount)):
        lw = max(1, width >> level)
        lh = max(1, height >> level)
        length = _level_bytes(lw, lh, compressed=True, unit=unit)
        if offset + length > len(data):
            break  # a truncated chain: keep the whole levels we have
        payload += data[offset : offset + length]
        levels.append((lw, lh, length))
        offset += length
    if not levels:
        return None  # not even the top level is whole -- let the decoder report it
    return CompressedTexture(label, width, height, levels, bytes(payload))
