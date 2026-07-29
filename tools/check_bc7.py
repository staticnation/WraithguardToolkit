"""Check the BC7 decoder against an independent one, block pattern by block pattern.

BC7 is defined by tables -- eight mode rows, two 64-entry partition tables and
three 64-entry anchor tables. Roughly six hundred numbers, transcribed by hand
from the specification. A single wrong entry does not fail loudly: it produces
a correct-looking image with a handful of wrong 4x4 blocks, in whichever
partition shapes the encoder happened to choose. That is invisible in a unit
test written by the same person who transcribed the tables, because both encode
the same mistake.

So this does not test against expectations. It generates blocks that *force*
every table entry to be used -- all eight modes crossed with all 64 partitions
-- and compares against Pillow, which decodes BC7 through its own unrelated
implementation.

**Why random bits are valid input.** Every 128-bit pattern is a legal BC7 block
except the reserved one whose low eight bits are all zero. There is no checksum
and no structure to satisfy, so filling a block with noise below the mode bits
exercises endpoints, P-bits, index packing and anchor widths far more harshly
than any real texture would.

Pillow is a check, not a dependency: nothing in the shipped tool imports it.
See ``NIF_PROVENANCE.md`` for why that distinction is kept sharp.

Usage:
    python tools/check_bc7.py
    python tools/check_bc7.py --trials 200
"""

from __future__ import annotations

import argparse
import random
import struct
import sys
from collections import Counter
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from mlox_subset.images import bc7

#: DXGI format number for BC7 with unsigned normalised channels, which is the
#: only BC7 flavour a texture replacer produces.
DXGI_BC7_UNORM = 98

#: How many random blocks to try per mode and partition combination.
TRIALS = 24


def dx10_dds(blocks: bytes, width: int, height: int) -> bytes:
    """Wrap BC7 blocks in the DDS container Pillow expects.

    Args:
        blocks: The compressed surface.
        width: Surface width.
        height: Surface height.

    Returns:
        A complete DDS file with a DX10 extension header.
    """
    header = bytearray(124)
    struct.pack_into("<IIIIII", header, 0, 124, 0x1 | 0x2 | 0x4 | 0x1000 | 0x80000,
                     height, width, len(blocks), 0)
    # Pixel format: 32 bytes at offset 72, flagged as carrying a FourCC.
    struct.pack_into("<II4s", header, 72, 32, 0x4, b"DX10")
    struct.pack_into("<I", header, 108, 0x1000)
    extension = struct.pack("<IIIII", DXGI_BC7_UNORM, 3, 0, 1, 0)
    return b"DDS " + bytes(header) + extension + blocks


def make_block(rng: random.Random, mode: int, partition: int) -> bytes:
    """Build a BC7 block that uses a chosen mode and partition.

    Everything above the partition field is noise, which is what makes this a
    harsh test: the endpoints, P-bits and indices take values no encoder would
    ever choose and no hand-written test case would think of.

    Args:
        rng: The source of randomness.
        mode: Which of the eight modes to select.
        partition: The partition number, ignored by modes that have none.

    Returns:
        Sixteen bytes.
    """
    value = 1 << mode  # mode bits: a run of zeros then a one
    position = mode + 1
    partition_bits = (4, 6, 6, 6, 0, 0, 0, 6)[mode]
    if partition_bits:
        value |= (partition % (1 << partition_bits)) << position
        position += partition_bits
    for bit in range(position, 128):
        if rng.getrandbits(1):
            value |= 1 << bit
    return value.to_bytes(16, "little")


def main(argv: list[str] | None = None) -> int:
    """Compare the two decoders across every mode and partition.

    Args:
        argv: Command-line arguments.

    Returns:
        A process exit code; non-zero when any block differed.
    """
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--trials", type=int, default=TRIALS,
                        help=f"random blocks per mode/partition (default {TRIALS})")
    parser.add_argument("--seed", type=int, default=20260727, help="for a reproducible run")
    args = parser.parse_args(argv)

    try:
        from PIL import Image as PilImage
    except ImportError:
        print("Pillow is not installed; this check needs an independent decoder.",
              file=sys.stderr)
        return 2

    rng = random.Random(args.seed)  # noqa: S311 -- test data, not a secret
    failures: Counter[str] = Counter()
    checked = 0

    for mode in range(8):
        partitions = 64 if (4, 6, 6, 6, 0, 0, 0, 6)[mode] else 1
        for partition in range(partitions):
            blocks = b"".join(
                make_block(rng, mode, partition) for _ in range(args.trials)
            )
            # One block tall, N wide: each block stays a separate 4x4 tile.
            data = dx10_dds(blocks, 4 * args.trials, 4)
            try:
                reference = PilImage.open(__import__("io").BytesIO(data)).convert("RGBA").tobytes()
            except Exception as exc:  # noqa: BLE001 -- the oracle's failures are data too
                print(f"  mode {mode} partition {partition}: oracle refused: {exc}")
                failures[f"mode {mode}"] += 1
                continue
            ours = bytes(bc7.decode_surface(blocks, 4 * args.trials, 4))
            checked += args.trials
            if ours != reference:
                differing = sum(1 for a, b in zip(ours, reference) if a != b)
                if failures[f"mode {mode}"] == 0:
                    print(f"  MISMATCH mode {mode} partition {partition}: "
                          f"{differing} of {len(ours)} byte(s) differ")
                failures[f"mode {mode}"] += 1

    print(f"\nchecked {checked} block(s) across 8 mode(s) and every partition")
    if failures:
        print("mismatches by mode:")
        for name, count in sorted(failures.items()):
            print(f"  {name}: {count} partition(s)")
        return 1
    print("Every block matches an independent decoder byte for byte.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
