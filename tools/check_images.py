"""Check every texture decoder against an independent one.

The unit tests in ``tests/test_images.py`` prove the decoders behave the way
this project expects. They cannot prove the expectation is right: a test and
the code it tests were written by the same person from the same reading of the
same specification, so a misread format passes both. Every real error in this
project so far -- the NIF bounding box, the BSA data offset, the texture
extension comparison -- was found by checking against something that did not
share those assumptions.

So this compares against Pillow, which implements all of these formats
separately, and against real files rather than only generated ones.

**What is compared, and what is deliberately not.**

*Block formats* are checked on random block data. Every bit pattern is a valid
BC block, so noise exercises endpoint ordering, the punch-through mode, index
packing and the interpolation tables far harder than a photograph would.

*BC5 is compared on red and green only.* That is not a weakened test, it is the
correct one: the format stores two channels, and blue is **reconstructed** from
them by whoever decodes it. Our reconstruction is checked separately, by
geometry, in :func:`check_normal_reconstruction` -- comparing it against
Pillow's choice would be comparing two conventions, not testing a decoder.

*Alpha is ignored for BC4*, which has none, and for formats Pillow hands back
in a mode without one.

Pillow is a check, not a dependency: nothing in the shipped tool imports it.
See ``NIF_PROVENANCE.md`` for why that line is kept sharp.

Usage:
    python tools/check_images.py
    python tools/check_images.py --corpus NifCorpus
"""

from __future__ import annotations

import argparse
import io
import math
import os
import struct
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from mlox_subset.images import ImageError, read_image
from mlox_subset.images.dds import read_dds

#: Block formats to exercise, with the bytes each block occupies and which
#: channels the format actually defines.
BLOCK_FORMATS: tuple[tuple[bytes, int, str], ...] = (
    (b"DXT1", 8, "RGBA"),
    (b"DXT3", 16, "RGBA"),
    (b"DXT5", 16, "RGBA"),
    (b"ATI1", 8, "RGB"),
    (b"BC4U", 8, "RGB"),
    (b"ATI2", 16, "RG"),
    (b"BC5U", 16, "RG"),
)

#: How many 4x4 blocks to generate per format.
BLOCKS = 512


def fourcc_dds(fourcc: bytes, blocks: bytes, width: int, height: int) -> bytes:
    """Wrap compressed blocks in a classic DDS container.

    Args:
        fourcc: The compression tag.
        blocks: The compressed surface.
        width: Surface width.
        height: Surface height.

    Returns:
        A complete DDS file.
    """
    header = bytearray(124)
    struct.pack_into("<IIIIII", header, 0, 124, 0x1 | 0x2 | 0x4 | 0x1000 | 0x80000,
                     height, width, len(blocks), 0)
    struct.pack_into("<II4s", header, 72, 32, 0x4, fourcc)
    struct.pack_into("<I", header, 108, 0x1000)
    return b"DDS " + bytes(header) + blocks


def compare(ours: bytes, theirs: bytes, channels: str, label: str) -> int:
    """Compare two RGBA buffers on the channels a format defines.

    Args:
        ours: Our decode.
        theirs: The oracle's, already converted to RGBA.
        channels: Which of ``RGBA`` carry format data.
        label: What is being compared, for the message.

    Returns:
        How many pixels differed.
    """
    wanted = [i for i, name in enumerate("RGBA") if name in channels]
    differing = 0
    worst = 0
    for pixel in range(0, min(len(ours), len(theirs)), 4):
        for offset in wanted:
            gap = abs(ours[pixel + offset] - theirs[pixel + offset])
            if gap:
                differing += 1
                worst = max(worst, gap)
                break
    if differing:
        print(f"    {label}: {differing} pixel(s) differ, worst channel gap {worst}")
    return differing


def check_block_formats(rng: object, pil: object) -> int:
    """Compare every block-compressed format against the oracle.

    Args:
        rng: Unused; randomness comes from :func:`os.urandom`, which needs no
            seeding to be a fair test of arbitrary bit patterns.
        pil: The Pillow ``Image`` module.

    Returns:
        How many formats failed.
    """
    del rng
    failures = 0
    print("block-compressed formats, on random block data:")
    for fourcc, stride, channels in BLOCK_FORMATS:
        blocks = os.urandom(stride * BLOCKS)
        data = fourcc_dds(fourcc, blocks, 4 * BLOCKS, 4)
        try:
            reference = pil.open(io.BytesIO(data)).convert("RGBA").tobytes()  # type: ignore[attr-defined]
        except Exception as exc:  # noqa: BLE001 -- the oracle declining is a result
            print(f"  {fourcc.decode()}: oracle refused: {exc}")
            failures += 1
            continue
        ours = read_dds(data).pixels
        name = fourcc.decode()
        if compare(ours, reference, channels, name):
            failures += 1
        else:
            print(f"  {name}: {BLOCKS} block(s) match on {channels}")
    return failures


def check_normal_reconstruction() -> int:
    """Check that BC5's reconstructed blue really is the missing normal axis.

    Pillow cannot answer this: the format does not store blue, so there is no
    right answer to compare against, only a convention. What *is* checkable is
    the geometry -- every decoded pixel should be a unit vector once mapped
    back out of 0-255, because that is the identity the reconstruction uses.

    A flat normal, pointing straight out of the surface, is the one value worth
    naming: it must come back as (128, 128, 255) give or take rounding, and
    getting it wrong tilts every flat surface in the game.

    Returns:
        How many checks failed.
    """
    print("BC5 blue reconstruction, checked by geometry:")
    # A block whose two endpoints are both mid-scale: every pixel is a normal
    # pointing straight up.
    flat = bytes([128, 128, 0, 0, 0, 0, 0, 0]) * 2
    image = read_dds(fourcc_dds(b"ATI2", flat, 4, 4))
    red, green, blue, _ = image.pixel(0, 0)
    failures = 0
    if not (126 <= red <= 130 and 126 <= green <= 130 and blue >= 250):
        print(f"    a flat normal decoded as ({red}, {green}, {blue}), expected ~(128, 128, 255)")
        failures += 1
    else:
        print(f"  a flat normal decodes to ({red}, {green}, {blue})")

    # Random bytes are not normals. Where x^2 + y^2 exceeds one there is no
    # real z, and the reconstruction clamps it to zero -- so those pixels
    # cannot be unit length, and demanding it of them tests nothing but the
    # random number generator. The first version of this check did exactly
    # that, passed on one seed and failed on the next. Both branches are
    # checked separately instead.
    image = read_dds(fourcc_dds(b"ATI2", os.urandom(16) * 4, 8, 8))
    worst = 0.0
    representable = clamped = 0
    for y in range(8):
        for x in range(8):
            r, g, b, _ = image.pixel(x, y)
            nx, ny = r / 127.5 - 1.0, g / 127.5 - 1.0
            if nx * nx + ny * ny > 1.0:
                clamped += 1
                # z clamps to zero, which encodes as the middle of the range.
                if not 126 <= b <= 129:
                    print(f"    an unrepresentable normal gave blue {b}, expected ~127")
                    failures += 1
                continue
            representable += 1
            nz = b / 127.5 - 1.0
            length = math.sqrt(nx * nx + ny * ny + nz * nz)
            worst = max(worst, abs(length - 1.0))
    # Rounding three channels to bytes costs about a hundredth of a unit; an
    # error in the reconstruction itself would be far larger than that.
    if representable and worst > 0.05:
        print(f"    decoded normals are not unit vectors: worst length error {worst:.3f}")
        failures += 1
    else:
        print(f"  {representable} representable normal(s) stay unit length to within "
              f"{worst:.3f}; {clamped} out-of-range one(s) clamped")
    return failures


def check_uncompressed(pil: object) -> int:
    """Compare the Targa and bitmap decoders against the oracle.

    Both formats are generated by Pillow rather than by hand, so the test data
    is not shaped by this project's reading of the specification. Each variant
    listed here is one that behaves differently in the decoder: row order,
    channel count, palette, and run-length encoding.

    Args:
        pil: The Pillow ``Image`` module.

    Returns:
        How many variants failed.
    """
    print("Targa and bitmap, against files the oracle wrote:")
    source = pil.frombytes("RGBA", (23, 17), os.urandom(23 * 17 * 4))  # type: ignore[attr-defined]
    variants: list[tuple[str, str, dict[str, object]]] = [
        ("TGA 32-bit", "TGA", {}),
        ("TGA 32-bit RLE", "TGA", {"rle": True}),
        ("TGA 24-bit", "TGA", {"_mode": "RGB"}),
        ("TGA 24-bit RLE", "TGA", {"_mode": "RGB", "rle": True}),
        ("TGA 8-bit grey", "TGA", {"_mode": "L"}),
        ("BMP 32-bit", "BMP", {}),
        ("BMP 24-bit", "BMP", {"_mode": "RGB"}),
        ("BMP 8-bit palette", "BMP", {"_mode": "P"}),
        ("BMP 1-bit", "BMP", {"_mode": "1"}),
    ]
    failures = 0
    for label, fmt, options in variants:
        mode = options.pop("_mode", None)
        image = source.convert(mode) if mode else source
        buffer = io.BytesIO()
        try:
            image.save(buffer, fmt, **options)
        except Exception as exc:  # noqa: BLE001 -- an oracle that cannot write it is a skip
            print(f"  {label}: oracle cannot write it ({exc}); skipped")
            continue
        data = buffer.getvalue()
        reference = pil.open(io.BytesIO(data)).convert("RGBA").tobytes()  # type: ignore[attr-defined]
        try:
            ours = read_image(data).pixels
        except ImageError as exc:
            print(f"    {label}: we could not decode it: {exc}")
            failures += 1
            continue
        # Alpha is only meaningful where the source format carried it.
        channels = "RGBA" if mode in (None, "RGBA") else "RGB"
        if compare(ours, reference, channels, label):
            failures += 1
        else:
            print(f"  {label}: matches")
    return failures


def check_corpus(folder: Path, pil: object) -> int:
    """Decode real texture files and compare against the oracle.

    Generated data is uniform in a way real files are not: it never has an
    odd size, a mipmap chain, a stale header field or a trailing byte. This is
    the part of the check that has historically found things.

    Args:
        folder: A folder to walk for textures.
        pil: The Pillow ``Image`` module.

    Returns:
        How many files differed.
    """
    suffixes = {".dds", ".tga", ".bmp"}
    files = sorted(p for p in folder.rglob("*") if p.suffix.lower() in suffixes)
    if not files:
        print(f"no textures under {folder}")
        return 0
    print(f"real files under {folder.name}: {len(files)} texture(s)")
    failures = mismatched = skipped = 0
    for path in files:
        data = path.read_bytes()
        try:
            reference = pil.open(io.BytesIO(data)).convert("RGBA").tobytes()  # type: ignore[attr-defined]
        except Exception:  # noqa: BLE001 -- the oracle not reading it is not our failure
            skipped += 1
            continue
        try:
            ours = read_image(data).pixels
        except ImageError as exc:
            print(f"    {path.name}: we could not decode it: {exc}")
            failures += 1
            continue
        # BC5 is expected to differ in blue; nothing else should differ at all.
        if ours != reference and compare(ours, reference, "RGBA", path.name):
            mismatched += 1
    print(f"  {len(files) - skipped - failures - mismatched} matched, "
          f"{mismatched} differed, {failures} undecodable, {skipped} the oracle skipped")
    return failures + mismatched


def main(argv: list[str] | None = None) -> int:
    """Run every check.

    Args:
        argv: Command-line arguments.

    Returns:
        A process exit code; non-zero when anything did not match.
    """
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--corpus", type=Path, default=None,
                        help="a folder of real textures to check as well")
    args = parser.parse_args(argv)

    try:
        from PIL import Image as PilImage
    except ImportError:
        print("Pillow is not installed; this check needs an independent decoder.",
              file=sys.stderr)
        return 2

    failures = check_block_formats(None, PilImage)
    print()
    failures += check_normal_reconstruction()
    print()
    failures += check_uncompressed(PilImage)
    if args.corpus:
        print()
        failures += check_corpus(args.corpus, PilImage)

    print()
    if failures:
        print(f"{failures} check(s) did not match an independent decoder.")
        return 1
    print("Every decoder agrees with an independent implementation.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
