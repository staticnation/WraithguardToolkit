"""Decoding BC7 blocks, the one Morrowind-adjacent format that is not simple.

BC1, BC3 and BC5 are one interpolation each and fit in a paragraph. BC7 is a
different kind of thing: a 16-byte block carries one of **eight modes**, and
the mode decides how many color subsets the block is cut into, how wide the
endpoints are, whether alpha is stored at all, whether the block carries a
second index set for alpha, and whether a channel has been rotated into alpha
before encoding. Nothing in the block is at a fixed bit offset. The mode is
found by counting low zero bits, and every field after it is read relative to
what the mode said.

That is why this is its own module rather than three more branches in
:mod:`wraithguard.images.dds`.

**Why bother.** Morrowind itself never shipped BC7 -- it predates the format by
a decade. OpenMW supports it, so current high-resolution replacers use it, and
those are exactly the mods most likely to be in conflict with one another. A
BC7 texture this tool could not read would show as "cannot decode" on the pair
the user most wants compared.

**Provenance.** The mode table, partition tables, anchor tables and
interpolation weights below are the format's definition, published by Khronos
in the OpenGL BPTC specification and by Microsoft in the Direct3D 11 BC7
documentation. They are transcribed from that public description, not from any
implementation -- the same basis as :mod:`wraithguard.nif`, and for the same
licensing reasons. See ``NIF_PROVENANCE.md``. Every entry is exercised against
an independent decoder by ``tools/check_images.py``, which is what makes a
transcription slip visible rather than a rare wrong block.

**Where the work happens.** The per-block decode is in Rust
(``native/src/img.rs``, ``dds_decode``), a step-for-step port of the Python
that used to live here; ``tests/test_images_native_parity.py`` keeps the two in
agreement. In Python it was roughly a second per megapixel.
"""

from __future__ import annotations

from typing import Final

import wraithguard_native as _native

from wraithguard.images.image import ImageError
from wraithguard.logging_setup import get_logger

LOG = get_logger(__name__)

#: Bytes in one BC7 block, which always covers 4x4 pixels.
BLOCK_BYTES: Final[int] = 16


def decode_block(block: bytes) -> bytes:
    """Decode one 4x4 BC7 block.

    A reserved mode (all eight low bits zero) decodes to transparent black
    rather than failing, as the format specifies -- one bad block shows as a
    hole instead of losing the texture.

    Args:
        block: Exactly :data:`BLOCK_BYTES` bytes.

    Returns:
        64 bytes of RGBA: sixteen pixels in reading order.

    Raises:
        ImageError: If the block is not the right length.
    """
    if len(block) != BLOCK_BYTES:
        raise ImageError(f"a BC7 block is {BLOCK_BYTES} bytes, got {len(block)}")
    return bytes(decode_surface(block, 4, 4))


def decode_surface(data: bytes, width: int, height: int) -> bytearray:
    """Decode a whole BC7 surface.

    Args:
        data: The surface bytes.
        width: Surface width in pixels.
        height: Surface height in pixels.

    Returns:
        ``width * height * 4`` bytes of RGBA.

    Raises:
        ImageError: If the data runs out before the surface is covered.
    """
    try:
        pixels = _native.dds_decode(bytes(data), width, height, b"DX10")
    except ValueError as exc:
        raise ImageError(str(exc)) from exc
    LOG.debug("decoded %dx%d BC7 surface", width, height)
    return bytearray(pixels)
