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
:mod:`mlox_subset.images.dds`.

**Why bother.** Morrowind itself never shipped BC7 -- it predates the format by
a decade. OpenMW supports it, so current high-resolution replacers use it, and
those are exactly the mods most likely to be in conflict with one another. A
BC7 texture this tool could not read would show as "cannot decode" on the pair
the user most wants compared.

**Provenance.** The mode table, partition tables, anchor tables and
interpolation weights below are the format's definition, published by Khronos
in the OpenGL BPTC specification and by Microsoft in the Direct3D 11 BC7
documentation. They are transcribed from that public description, not from any
implementation -- the same basis as :mod:`mlox_subset.nif`, and for the same
licensing reasons. See ``NIF_PROVENANCE.md``. Every entry is exercised against
an independent decoder by ``tools/check_images.py``, which is what makes a
transcription slip visible rather than a rare wrong block.

**Speed.** This is arithmetic per pixel in Python, and it is the slowest
decoder here by a wide margin -- roughly a second per megapixel. That is
acceptable because a viewer decodes one or two textures on demand, not a
collection. It would not be acceptable in a batch scan, and nothing batches it.
"""

from __future__ import annotations

from typing import Final

from mlox_subset.images.image import ImageError
from mlox_subset.logging_setup import get_logger

LOG = get_logger(__name__)

#: Bytes in one BC7 block, which always covers 4x4 pixels.
BLOCK_BYTES: Final[int] = 16

#: How many color subsets each mode cuts the block into.
_SUBSETS: Final[tuple[int, ...]] = (3, 2, 3, 2, 1, 1, 1, 2)

#: Bits of partition number, selecting which shape the subsets take.
_PARTITION_BITS: Final[tuple[int, ...]] = (4, 6, 6, 6, 0, 0, 0, 6)

#: Bits of rotation, naming a channel that was swapped into alpha before
#: encoding. Only the single-subset modes carry one.
_ROTATION_BITS: Final[tuple[int, ...]] = (0, 0, 0, 0, 2, 2, 0, 0)

#: Bits of index selector, saying which of the two index sets drives color.
_SELECTOR_BITS: Final[tuple[int, ...]] = (0, 0, 0, 0, 1, 0, 0, 0)

#: Bits per color channel in each endpoint, before any P-bit.
_COLOUR_BITS: Final[tuple[int, ...]] = (4, 6, 5, 7, 5, 7, 7, 5)

#: Bits of alpha per endpoint. Zero means the mode stores no alpha at all and
#: the surface is opaque.
_ALPHA_BITS: Final[tuple[int, ...]] = (0, 0, 0, 0, 6, 8, 7, 5)

#: Whether each endpoint carries its own low-order P-bit.
_ENDPOINT_P: Final[tuple[int, ...]] = (1, 0, 0, 1, 0, 0, 1, 1)

#: Whether each *subset's* two endpoints share one P-bit between them.
_SHARED_P: Final[tuple[int, ...]] = (0, 1, 0, 0, 0, 0, 0, 0)

#: Bits per pixel in the primary index set.
_INDEX_BITS: Final[tuple[int, ...]] = (3, 3, 2, 2, 2, 2, 4, 2)

#: Bits per pixel in the secondary index set; zero when the mode has only one.
_INDEX_BITS_2: Final[tuple[int, ...]] = (0, 0, 0, 0, 3, 2, 0, 0)

#: Interpolation weights by index width. An index does not scale a channel
#: linearly across 0-255; it selects one of these weights out of 64, which is
#: what makes the endpoints reachable exactly.
_WEIGHTS: Final[dict[int, tuple[int, ...]]] = {
    2: (0, 21, 43, 64),
    3: (0, 9, 18, 27, 37, 46, 55, 64),
    4: (0, 4, 9, 13, 17, 21, 26, 30, 34, 38, 43, 47, 51, 55, 60, 64),
}

# The partition tables say, for each of the 64 partition numbers, which subset
# each of the 16 pixels belongs to. They are the whole reason BC7 beats BC1 on
# blocks containing an edge: the encoder picks the shape that separates the two
# sides, and each side gets its own pair of endpoints.

#: Subset membership for the two-subset modes.
_PARTITIONS_2: Final[tuple[tuple[int, ...], ...]] = (
    (0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1),
    (0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1),
    (0, 1, 1, 1, 0, 1, 1, 1, 0, 1, 1, 1, 0, 1, 1, 1),
    (0, 0, 0, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0, 1, 1, 1),
    (0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 1, 1),
    (0, 0, 1, 1, 0, 1, 1, 1, 0, 1, 1, 1, 1, 1, 1, 1),
    (0, 0, 0, 1, 0, 0, 1, 1, 0, 1, 1, 1, 1, 1, 1, 1),
    (0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 1, 1, 0, 1, 1, 1),
    (0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 1, 1),
    (0, 0, 1, 1, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1),
    (0, 0, 0, 0, 0, 0, 0, 1, 0, 1, 1, 1, 1, 1, 1, 1),
    (0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 1, 1, 1),
    (0, 0, 0, 1, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1),
    (0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1),
    (0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1),
    (0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1),
    (0, 0, 0, 0, 1, 0, 0, 0, 1, 1, 1, 0, 1, 1, 1, 1),
    (0, 1, 1, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0),
    (0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 1, 1, 1, 0),
    (0, 1, 1, 1, 0, 0, 1, 1, 0, 0, 0, 1, 0, 0, 0, 0),
    (0, 0, 1, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0),
    (0, 0, 0, 0, 1, 0, 0, 0, 1, 1, 0, 0, 1, 1, 1, 0),
    (0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 1, 1, 0, 0),
    (0, 1, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 0, 1),
    (0, 0, 1, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 0),
    (0, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 1, 0, 0),
    (0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0),
    (0, 0, 1, 1, 0, 1, 1, 0, 0, 1, 1, 0, 1, 1, 0, 0),
    (0, 0, 0, 1, 0, 1, 1, 1, 1, 1, 1, 0, 1, 0, 0, 0),
    (0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0),
    (0, 1, 1, 1, 0, 0, 0, 1, 1, 0, 0, 0, 1, 1, 1, 0),
    (0, 0, 1, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 1, 0, 0),
    (0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1),
    (0, 0, 0, 0, 1, 1, 1, 1, 0, 0, 0, 0, 1, 1, 1, 1),
    (0, 1, 0, 1, 1, 0, 1, 0, 0, 1, 0, 1, 1, 0, 1, 0),
    (0, 0, 1, 1, 0, 0, 1, 1, 1, 1, 0, 0, 1, 1, 0, 0),
    (0, 0, 1, 1, 1, 1, 0, 0, 0, 0, 1, 1, 1, 1, 0, 0),
    (0, 1, 0, 1, 0, 1, 0, 1, 1, 0, 1, 0, 1, 0, 1, 0),
    (0, 1, 1, 0, 1, 0, 0, 1, 0, 1, 1, 0, 1, 0, 0, 1),
    (0, 1, 0, 1, 1, 0, 1, 0, 1, 0, 1, 0, 0, 1, 0, 1),
    (0, 1, 1, 1, 0, 0, 1, 1, 1, 1, 0, 0, 1, 1, 1, 0),
    (0, 0, 0, 1, 0, 0, 1, 1, 1, 1, 0, 0, 1, 0, 0, 0),
    (0, 0, 1, 1, 0, 0, 1, 0, 0, 1, 0, 0, 1, 1, 0, 0),
    (0, 0, 1, 1, 1, 0, 1, 1, 1, 1, 0, 1, 1, 1, 0, 0),
    (0, 1, 1, 0, 1, 0, 0, 1, 1, 0, 0, 1, 0, 1, 1, 0),
    (0, 0, 1, 1, 1, 1, 0, 0, 1, 1, 0, 0, 0, 0, 1, 1),
    (0, 1, 1, 0, 0, 1, 1, 0, 1, 0, 0, 1, 1, 0, 0, 1),
    (0, 0, 0, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 0, 0, 0),
    (0, 1, 0, 0, 1, 1, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0),
    (0, 0, 1, 0, 0, 1, 1, 1, 0, 0, 1, 0, 0, 0, 0, 0),
    (0, 0, 0, 0, 0, 0, 1, 0, 0, 1, 1, 1, 0, 0, 1, 0),
    (0, 0, 0, 0, 0, 1, 0, 0, 1, 1, 1, 0, 0, 1, 0, 0),
    (0, 1, 1, 0, 1, 1, 0, 0, 1, 0, 0, 1, 0, 0, 1, 1),
    (0, 0, 1, 1, 0, 1, 1, 0, 1, 1, 0, 0, 1, 0, 0, 1),
    (0, 1, 1, 0, 0, 0, 1, 1, 1, 0, 0, 1, 1, 1, 0, 0),
    (0, 0, 1, 1, 1, 0, 0, 1, 1, 1, 0, 0, 0, 1, 1, 0),
    (0, 1, 1, 0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 0, 0, 1),
    (0, 1, 1, 0, 0, 0, 1, 1, 0, 0, 1, 1, 1, 0, 0, 1),
    (0, 1, 1, 1, 1, 1, 1, 0, 1, 0, 0, 0, 0, 0, 0, 1),
    (0, 0, 0, 1, 1, 0, 0, 0, 1, 1, 1, 0, 0, 1, 1, 1),
    (0, 0, 0, 0, 1, 1, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1),
    (0, 0, 1, 1, 0, 0, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0),
    (0, 0, 1, 0, 0, 0, 1, 0, 1, 1, 1, 0, 1, 1, 1, 0),
    (0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 1, 1, 0, 1, 1, 1),
)

#: Subset membership for the three-subset modes.
_PARTITIONS_3: Final[tuple[tuple[int, ...], ...]] = (
    (0, 0, 1, 1, 0, 0, 1, 1, 0, 2, 2, 1, 2, 2, 2, 2),
    (0, 0, 0, 1, 0, 0, 1, 1, 2, 2, 1, 1, 2, 2, 2, 1),
    (0, 0, 0, 0, 2, 0, 0, 1, 2, 2, 1, 1, 2, 2, 1, 1),
    (0, 2, 2, 2, 0, 0, 2, 2, 0, 0, 1, 1, 0, 1, 1, 1),
    (0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 2, 2, 1, 1, 2, 2),
    (0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 2, 2, 0, 0, 2, 2),
    (0, 0, 2, 2, 0, 0, 2, 2, 1, 1, 1, 1, 1, 1, 1, 1),
    (0, 0, 1, 1, 0, 0, 1, 1, 2, 2, 1, 1, 2, 2, 1, 1),
    (0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2),
    (0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2),
    (0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 2, 2, 2, 2),
    (0, 0, 1, 2, 0, 0, 1, 2, 0, 0, 1, 2, 0, 0, 1, 2),
    (0, 1, 1, 2, 0, 1, 1, 2, 0, 1, 1, 2, 0, 1, 1, 2),
    (0, 1, 2, 2, 0, 1, 2, 2, 0, 1, 2, 2, 0, 1, 2, 2),
    (0, 0, 1, 1, 0, 1, 1, 2, 1, 1, 2, 2, 1, 2, 2, 2),
    (0, 0, 1, 1, 2, 0, 0, 1, 2, 2, 0, 0, 2, 2, 2, 0),
    (0, 0, 0, 1, 0, 0, 1, 1, 0, 1, 1, 2, 1, 1, 2, 2),
    (0, 1, 1, 1, 0, 0, 1, 1, 2, 0, 0, 1, 2, 2, 0, 0),
    (0, 0, 0, 0, 1, 1, 2, 2, 1, 1, 2, 2, 1, 1, 2, 2),
    (0, 0, 2, 2, 0, 0, 2, 2, 0, 0, 2, 2, 1, 1, 1, 1),
    (0, 1, 1, 1, 0, 1, 1, 1, 0, 2, 2, 2, 0, 2, 2, 2),
    (0, 0, 0, 1, 0, 0, 0, 1, 2, 2, 2, 1, 2, 2, 2, 1),
    (0, 0, 0, 0, 0, 0, 1, 1, 0, 1, 2, 2, 0, 1, 2, 2),
    (0, 0, 0, 0, 1, 1, 0, 0, 2, 2, 1, 0, 2, 2, 1, 0),
    (0, 1, 2, 2, 0, 1, 2, 2, 0, 0, 1, 1, 0, 0, 0, 0),
    (0, 0, 1, 2, 0, 0, 1, 2, 1, 1, 2, 2, 2, 2, 2, 2),
    (0, 1, 1, 0, 1, 2, 2, 1, 1, 2, 2, 1, 0, 1, 1, 0),
    (0, 0, 0, 0, 0, 1, 1, 0, 1, 2, 2, 1, 1, 2, 2, 1),
    (0, 0, 2, 2, 1, 1, 0, 2, 1, 1, 0, 2, 0, 0, 2, 2),
    (0, 1, 1, 0, 0, 1, 1, 0, 2, 0, 0, 2, 2, 2, 2, 2),
    (0, 0, 1, 1, 0, 1, 2, 2, 0, 1, 2, 2, 0, 0, 1, 1),
    (0, 0, 0, 0, 2, 0, 0, 0, 2, 2, 1, 1, 2, 2, 2, 1),
    (0, 0, 0, 0, 0, 0, 0, 2, 1, 1, 2, 2, 1, 2, 2, 2),
    (0, 2, 2, 2, 0, 0, 2, 2, 0, 0, 1, 2, 0, 0, 1, 1),
    (0, 0, 1, 1, 0, 0, 1, 2, 0, 0, 2, 2, 0, 2, 2, 2),
    (0, 1, 2, 0, 0, 1, 2, 0, 0, 1, 2, 0, 0, 1, 2, 0),
    (0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 0, 0, 0, 0),
    (0, 1, 2, 0, 1, 2, 0, 1, 2, 0, 1, 2, 0, 1, 2, 0),
    (0, 1, 2, 0, 2, 0, 1, 2, 1, 2, 0, 1, 0, 1, 2, 0),
    (0, 0, 1, 1, 2, 2, 0, 0, 1, 1, 2, 2, 0, 0, 1, 1),
    (0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 0, 0, 0, 0, 1, 1),
    (0, 1, 0, 1, 0, 1, 0, 1, 2, 2, 2, 2, 2, 2, 2, 2),
    (0, 0, 0, 0, 0, 0, 0, 0, 2, 1, 2, 1, 2, 1, 2, 1),
    (0, 0, 2, 2, 1, 1, 2, 2, 0, 0, 2, 2, 1, 1, 2, 2),
    (0, 0, 2, 2, 0, 0, 1, 1, 0, 0, 2, 2, 0, 0, 1, 1),
    (0, 2, 2, 0, 1, 2, 2, 1, 0, 2, 2, 0, 1, 2, 2, 1),
    (0, 1, 0, 1, 2, 2, 2, 2, 2, 2, 2, 2, 0, 1, 0, 1),
    (0, 0, 0, 0, 2, 1, 2, 1, 2, 1, 2, 1, 2, 1, 2, 1),
    (0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 2, 2, 2, 2),
    (0, 2, 2, 2, 0, 1, 1, 1, 0, 2, 2, 2, 0, 1, 1, 1),
    (0, 0, 0, 2, 1, 1, 1, 2, 0, 0, 0, 2, 1, 1, 1, 2),
    (0, 0, 0, 0, 2, 1, 1, 2, 2, 1, 1, 2, 2, 1, 1, 2),
    (0, 2, 2, 2, 0, 1, 1, 1, 0, 1, 1, 1, 0, 2, 2, 2),
    (0, 0, 0, 2, 1, 1, 1, 2, 1, 1, 1, 2, 0, 0, 0, 2),
    (0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0, 2, 2, 2, 2),
    (0, 0, 0, 0, 0, 0, 0, 0, 2, 1, 1, 2, 2, 1, 1, 2),
    (0, 1, 1, 0, 0, 1, 1, 0, 2, 2, 2, 2, 2, 2, 2, 2),
    (0, 0, 2, 2, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 2, 2),
    (0, 0, 2, 2, 1, 1, 2, 2, 1, 1, 2, 2, 0, 0, 2, 2),
    (0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 1, 1, 2),
    (0, 0, 0, 2, 0, 0, 0, 1, 0, 0, 0, 2, 0, 0, 0, 1),
    (0, 2, 2, 2, 1, 2, 2, 2, 0, 2, 2, 2, 1, 2, 2, 2),
    (0, 1, 0, 1, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2),
    (0, 1, 1, 1, 2, 0, 1, 1, 2, 2, 0, 1, 2, 2, 2, 0),
)

# One index per subset is stored a bit short, because its high bit is implied:
# the encoder orders each subset's endpoints so that the anchor pixel's index
# has a zero top bit. Reading those pixels at full width desynchronises every
# index after them, so these tables are load-bearing rather than an
# optimisation.

#: Which pixel anchors subset 1 in the two-subset modes.
_ANCHOR_2: Final[tuple[int, ...]] = (
    15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15,
    15, 2, 8, 2, 2, 8, 8, 15, 2, 8, 2, 2, 8, 8, 2, 2,
    15, 15, 6, 8, 2, 8, 15, 15, 2, 8, 2, 2, 2, 15, 15, 6,
    6, 2, 6, 8, 15, 15, 2, 2, 15, 15, 15, 15, 15, 2, 2, 15,
)  # fmt: skip

#: Which pixel anchors subset 1 in the three-subset modes.
_ANCHOR_3_1: Final[tuple[int, ...]] = (
    3, 3, 15, 15, 8, 3, 15, 15, 8, 8, 6, 6, 6, 5, 3, 3,
    3, 3, 8, 15, 3, 3, 6, 10, 5, 8, 8, 6, 8, 5, 15, 15,
    8, 15, 3, 5, 6, 10, 8, 15, 15, 3, 15, 5, 15, 15, 15, 15,
    3, 15, 5, 5, 5, 8, 5, 10, 5, 10, 8, 13, 15, 12, 3, 3,
)  # fmt: skip

#: Which pixel anchors subset 2 in the three-subset modes.
_ANCHOR_3_2: Final[tuple[int, ...]] = (
    15, 8, 8, 3, 15, 15, 3, 8, 15, 15, 15, 15, 15, 15, 15, 8,
    15, 8, 15, 3, 15, 8, 15, 8, 3, 15, 6, 10, 15, 15, 10, 8,
    15, 3, 15, 10, 10, 8, 9, 10, 6, 15, 8, 15, 3, 6, 6, 8,
    15, 3, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 3, 15, 15, 8,
)  # fmt: skip

#: A fully transparent block, returned for the reserved bit pattern.
_VOID: Final[bytes] = bytes(64)


class _Bits:
    """A little-endian bit cursor over one 128-bit block.

    BC7 has no field at a fixed offset -- the mode decides the width of
    everything after it -- so fields are taken in order rather than unpacked.
    """

    __slots__ = ("_pos", "_value")

    def __init__(self, value: int, start: int) -> None:
        """Start reading just past the mode bits.

        Args:
            value: The block as a 128-bit integer.
            start: The first bit position to read from.
        """
        self._value = value
        self._pos = start

    def take(self, count: int) -> int:
        """Read the next field.

        Args:
            count: Its width in bits. Zero is allowed and reads nothing, which
                keeps the callers free of ``if width:`` around every field a
                mode may omit.

        Returns:
            The value.
        """
        if count <= 0:
            return 0
        out = (self._value >> self._pos) & ((1 << count) - 1)
        self._pos += count
        return out


def _unquantise(value: int, bits: int) -> int:
    """Widen an endpoint channel to a full byte.

    The low bits are replicated from the high ones rather than zero-filled, so
    that an all-ones endpoint reaches 255. Shifting alone would cap white at
    248 and give every decoded texture a faint dark cast -- the same trap as
    the 5:6:5 expansion in the DXT decoder.

    Args:
        value: The stored channel.
        bits: How many bits it occupies, including any P-bit.

    Returns:
        The channel scaled to 0-255.
    """
    if bits >= 8:
        return value
    return ((value << (8 - bits)) | (value >> (2 * bits - 8))) & 0xFF


def _interpolate(first: int, second: int, weight: int) -> int:
    """Blend two endpoint channels.

    Args:
        first: The channel at one end.
        second: The channel at the other.
        weight: The weight out of 64.

    Returns:
        The blended channel, 0-255.
    """
    return ((64 - weight) * first + weight * second + 32) >> 6


def _anchors(subsets: int, partition: int) -> tuple[int, ...]:
    """Which pixel anchors each subset.

    Args:
        subsets: How many subsets the mode uses.
        partition: The partition number.

    Returns:
        One pixel index per subset. Subset 0 always anchors at pixel 0.
    """
    if subsets == 1:
        return (0,)
    if subsets == 2:
        return (0, _ANCHOR_2[partition])
    return (0, _ANCHOR_3_1[partition], _ANCHOR_3_2[partition])


def _read_endpoints(bits: _Bits, mode: int) -> list[list[int]]:
    """Read and widen every endpoint the block carries.

    Channels are stored plane by plane -- all the reds, then all the greens --
    rather than endpoint by endpoint, and the P-bits come after all of them.
    An endpoint is therefore not assembled until the whole field group is read.

    Args:
        bits: The cursor, positioned after the mode's leading fields.
        mode: The block mode.

    Returns:
        One ``[r, g, b, a]`` per endpoint, each channel 0-255, in subset order:
        subset 0's pair first.
    """
    count = _SUBSETS[mode] * 2
    color_bits, alpha_bits = _COLOUR_BITS[mode], _ALPHA_BITS[mode]
    planes = [[bits.take(color_bits) for _ in range(count)] for _ in range(3)]
    planes.append([bits.take(alpha_bits) for _ in range(count)] if alpha_bits else [0] * count)

    # A P-bit is a shared low-order bit appended to every channel of an
    # endpoint at once. It buys the encoder half a step of precision on all
    # four channels for one bit, which is why it exists.
    parity: list[int] | None = None
    if _ENDPOINT_P[mode]:
        parity = [bits.take(1) for _ in range(count)]
    elif _SHARED_P[mode]:
        shared = [bits.take(1) for _ in range(_SUBSETS[mode])]
        parity = [shared[index // 2] for index in range(count)]

    color_width = color_bits + (1 if parity else 0)
    alpha_width = alpha_bits + (1 if parity else 0)
    endpoints: list[list[int]] = []
    for index in range(count):
        channels: list[int] = []
        for plane in range(3):
            raw = planes[plane][index]
            if parity is not None:
                raw = (raw << 1) | parity[index]
            channels.append(_unquantise(raw, color_width))
        if not alpha_bits:
            # The mode stores no alpha, which means opaque -- not absent.
            channels.append(255)
        else:
            raw = planes[3][index]
            if parity is not None:
                raw = (raw << 1) | parity[index]
            channels.append(_unquantise(raw, alpha_width))
        endpoints.append(channels)
    return endpoints


def _read_indices(bits: _Bits, width: int, anchors: tuple[int, ...], subset_of: list[int] | None
                  ) -> list[int]:
    """Read one index set, allowing for the short anchor indices.

    Args:
        bits: The cursor.
        width: Bits per index at full width.
        anchors: Pixels whose index is stored one bit short.
        subset_of: Subset membership per pixel, or ``None`` for a single
            subset. Unused beyond identifying anchors, which ``anchors``
            already gives, so it is accepted only to keep the call sites
            symmetrical.

    Returns:
        Sixteen indices.
    """
    del subset_of
    short = set(anchors)
    return [bits.take(width - 1 if pixel in short else width) for pixel in range(16)]


def decode_block(block: bytes) -> bytes:
    """Decode one 4x4 BC7 block.

    Args:
        block: Exactly :data:`BLOCK_BYTES` bytes.

    Returns:
        64 bytes of RGBA: sixteen pixels in reading order.

    Raises:
        ImageError: If the block is not the right length.
    """
    if len(block) != BLOCK_BYTES:
        raise ImageError(f"a BC7 block is {BLOCK_BYTES} bytes, got {len(block)}")
    value = int.from_bytes(block, "little")

    # The mode is a run of zeros terminated by a one. All eight low bits zero
    # is reserved, and the format says such a block decodes to transparent
    # black rather than being an error -- so a single bad block in a texture
    # shows as a hole instead of failing the whole surface.
    mode = -1
    for candidate in range(8):
        if (value >> candidate) & 1:
            mode = candidate
            break
    if mode < 0:
        return _VOID

    bits = _Bits(value, mode + 1)
    partition = bits.take(_PARTITION_BITS[mode])
    rotation = bits.take(_ROTATION_BITS[mode])
    selector = bits.take(_SELECTOR_BITS[mode])

    subsets = _SUBSETS[mode]
    endpoints = _read_endpoints(bits, mode)

    if subsets == 1:
        membership = [0] * 16
    elif subsets == 2:
        membership = list(_PARTITIONS_2[partition])
    else:
        membership = list(_PARTITIONS_3[partition])

    anchors = _anchors(subsets, partition)
    width_1, width_2 = _INDEX_BITS[mode], _INDEX_BITS_2[mode]
    indices_1 = _read_indices(bits, width_1, anchors, None)
    # Only single-subset modes carry a second index set, so pixel 0 is its
    # only anchor.
    indices_2 = _read_indices(bits, width_2, (0,), None) if width_2 else None

    if indices_2 is None:
        color_idx, color_w = indices_1, width_1
        alpha_idx, alpha_w = indices_1, width_1
    elif selector:
        # The index selector swaps which set drives color and which drives
        # alpha. Ignoring it decodes mode 4 blocks with the two confused,
        # which looks like plausible color with wrong transparency.
        color_idx, color_w = indices_2, width_2
        alpha_idx, alpha_w = indices_1, width_1
    else:
        color_idx, color_w = indices_1, width_1
        alpha_idx, alpha_w = indices_2, width_2

    color_weights = _WEIGHTS[color_w]
    alpha_weights = _WEIGHTS[alpha_w]
    out = bytearray(64)
    for pixel in range(16):
        subset = membership[pixel]
        low, high = endpoints[subset * 2], endpoints[subset * 2 + 1]
        weight = color_weights[color_idx[pixel]]
        red = _interpolate(low[0], high[0], weight)
        green = _interpolate(low[1], high[1], weight)
        blue = _interpolate(low[2], high[2], weight)
        alpha = _interpolate(low[3], high[3], alpha_weights[alpha_idx[pixel]])
        # A rotation means the encoder moved one color channel into the alpha
        # slot before compressing, because it had more detail there. Undoing it
        # is a swap, and skipping it produces an image that is almost right and
        # obviously wrong in one channel.
        if rotation == 1:
            red, alpha = alpha, red
        elif rotation == 2:
            green, alpha = alpha, green
        elif rotation == 3:
            blue, alpha = alpha, blue
        at = pixel * 4
        out[at : at + 4] = bytes((red, green, blue, alpha))
    return bytes(out)


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
    out = bytearray(width * height * 4)
    offset = 0
    for block_y in range(0, height, 4):
        for block_x in range(0, width, 4):
            if offset + BLOCK_BYTES > len(data):
                raise ImageError(
                    f"BC7 data ends early: wanted {BLOCK_BYTES} byte(s) at {offset}, "
                    f"file holds {len(data) - offset}"
                )
            pixels = decode_block(data[offset : offset + BLOCK_BYTES])
            offset += BLOCK_BYTES
            # A surface's dimensions need not be multiples of four, so the
            # blocks along the right and bottom edges are partly outside it.
            columns = min(4, width - block_x)
            for row in range(min(4, height - block_y)):
                start = ((block_y + row) * width + block_x) * 4
                source = row * 16
                out[start : start + columns * 4] = pixels[source : source + columns * 4]
    LOG.debug("decoded %dx%d BC7 surface", width, height)
    return out
