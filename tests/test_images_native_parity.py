"""The Rust texture decoders against the Python they replaced (native/src/img.rs).

DXT1-5, BC4, BC5, BC7, uncompressed DDS, Targa and the two comparison loops moved
to Rust. The Python is kept in ``_images_reference.py`` as the reference, and
every path here must agree with it byte for byte -- on random blocks (which
exercise every BC7 mode, partition and rotation), odd surface sizes, truncated
data (same error message), and every Targa variant.
"""

from __future__ import annotations

import os
import random
import struct
from pathlib import Path

import pytest

wraithguard_native = pytest.importorskip("wraithguard_native")

from tests import _images_reference as ref  # noqa: E402
from wraithguard.images import bc7  # noqa: E402
from wraithguard.images.compare import _measure, difference_image  # noqa: E402
from wraithguard.images.dds import DdsError, read_dds  # noqa: E402
from wraithguard.images.image import Image, ImageError  # noqa: E402
from wraithguard.images.targa import TargaError, read_tga  # noqa: E402

SIZES = [(4, 4), (8, 4), (1, 1), (3, 5), (6, 7), (13, 9), (32, 16)]
FOURCCS = [b"DXT1", b"DXT2", b"DXT3", b"DXT4", b"DXT5", b"ATI1", b"BC4U", b"ATI2", b"BC5S"]


def _blocks(w: int, h: int) -> int:
    return ((w + 3) // 4) * ((h + 3) // 4)


def _reference_compressed(surface: bytes, w: int, h: int, fourcc: bytes) -> bytes:
    if fourcc == b"DX10":
        return bytes(ref.decode_surface(surface, w, h))
    if fourcc in (b"ATI2", b"BC5U", b"BC5S"):
        return bytes(ref._decode_two_channel(surface, w, h))
    if fourcc in (b"ATI1", b"BC4U", b"BC4S"):
        return bytes(ref._decode_one_channel(surface, w, h))
    return bytes(ref._decode_blocks(surface, w, h, fourcc))


def _stride(fourcc: bytes) -> int:
    return 16 if fourcc == b"DX10" else ref._BLOCK_BYTES[fourcc]


@pytest.mark.parametrize("fourcc", [*FOURCCS, b"DX10"])
def test_block_formats_match(fourcc: bytes) -> None:
    rng = random.Random(fourcc)  # noqa: S311 - test data, not security
    for w, h in SIZES:
        surface = rng.randbytes(_blocks(w, h) * _stride(fourcc))
        assert wraithguard_native.dds_decode(surface, w, h, fourcc) == _reference_compressed(
            surface, w, h, fourcc
        ), (fourcc, w, h)


def test_bc7_every_mode_and_reserved() -> None:
    # Random 128-bit blocks, each forced into one mode (0-7) or the reserved one.
    rng = random.Random(7)  # noqa: S311 - test data, not security
    for mode in range(9):
        blocks = bytearray()
        for _ in range(512):
            value = rng.getrandbits(128)
            if mode < 8:
                value = (value & ~((1 << (mode + 1)) - 1)) | (1 << mode)
            else:
                value &= ~0xFF
            blocks += value.to_bytes(16, "little")
        surface = bytes(blocks)
        assert wraithguard_native.dds_decode(surface, 64, 32, b"DX10") == bytes(
            ref.decode_surface(surface, 64, 32)
        ), mode
        assert bc7.decode_block(surface[:16]) == ref.decode_block(surface[:16])


@pytest.mark.parametrize("fourcc", [*FOURCCS, b"DX10"])
def test_truncated_surface_messages_match(fourcc: bytes) -> None:
    surface = bytes(_blocks(8, 8) * _stride(fourcc) - 3)
    with pytest.raises(ValueError) as expected:
        _reference_compressed(surface, 8, 8, fourcc)
    with pytest.raises(ValueError) as got:
        wraithguard_native.dds_decode(surface, 8, 8, fourcc)
    assert str(got.value) == str(expected.value)


@pytest.mark.parametrize(
    ("bits", "masks"),
    [
        (32, (0xFF0000, 0xFF00, 0xFF, 0xFF000000)),
        (32, (0xFF, 0xFF00, 0xFF0000, 0)),
        (32, (0xFFFFFFFF, 0xFFFFFFFF, 0xFFFFFFFF, 0)),
        (24, (0xFF0000, 0xFF00, 0xFF, 0)),
        (16, (0xF800, 0x07E0, 0x001F, 0)),
        (16, (0x7C00, 0x03E0, 0x001F, 0x8000)),
        (16, (0x0F00, 0x00F0, 0x000F, 0xF000)),
        (8, (0xFF, 0xFF, 0xFF, 0)),
        (8, (0xE0, 0x1C, 0x03, 0)),
    ],
)
def test_uncompressed_matches(bits: int, masks: tuple[int, int, int, int]) -> None:
    rng = random.Random(bits)  # noqa: S311 - test data, not security
    for w, h in SIZES:
        surface = rng.randbytes(w * h * bits // 8 + 2)
        ours = wraithguard_native.dds_uncompressed(surface, w, h, bits, masks)
        assert ours == bytes(ref._decode_uncompressed(surface, w, h, bits, masks))


@pytest.mark.parametrize(("bits", "size"), [(12, 64), (32, 10)])
def test_uncompressed_errors_match(bits: int, size: int) -> None:
    masks = (0xFF0000, 0xFF00, 0xFF, 0)
    with pytest.raises(ValueError) as expected:
        ref._decode_uncompressed(bytes(size), 4, 4, bits, masks)
    with pytest.raises(ValueError) as got:
        wraithguard_native.dds_uncompressed(bytes(size), 4, 4, bits, masks)
    assert str(got.value) == str(expected.value)


def test_wrappers_keep_their_error_types() -> None:
    with pytest.raises(ImageError, match="BC7 data ends early"):
        bc7.decode_surface(bytes(8), 4, 4)
    header = bytearray(128)
    header[:4] = b"DDS "
    struct.pack_into("<IIII", header, 4, 124, 0, 4, 4)
    struct.pack_into("<I4s", header, 4 + 76, 0x4, b"DXT5")
    with pytest.raises(DdsError, match="texture data ends early"):
        read_dds(bytes(header) + bytes(8))
    with pytest.raises(TargaError, match="too short"):
        read_tga(b"\x00")


# ---------------------------------------------------------------------------
# Targa


def _tga(
    image_type: int,
    depth: int,
    w: int,
    h: int,
    payload: bytes,
    *,
    descriptor: int = 0,
    cmap: tuple[int, int, bytes] | None = None,
    ident: bytes = b"",
) -> bytes:
    has_map, map_len, map_depth, map_data = 0, 0, 0, b""
    if cmap is not None:
        has_map = 1
        map_len, map_depth, map_data = cmap
    header = struct.pack(
        "<BBBHHBHHHHBB",
        len(ident), has_map, image_type, 0, map_len, map_depth, 0, 0, w, h, depth, descriptor,
    )  # fmt: skip
    return header + ident + map_data + payload


def _rle(rng: random.Random, count: int, step: int) -> bytes:
    out = bytearray()
    left = count
    while left > 0:
        run = min(left, rng.randint(1, 128))
        if rng.random() < 0.5:
            out.append(0x80 | (run - 1))
            out += rng.randbytes(step)
        else:
            out.append(run - 1)
            out += rng.randbytes(run * step)
        left -= run
    return bytes(out)


def _tga_cases() -> list[bytes]:
    rng = random.Random(1998)  # noqa: S311 - test data, not security
    cases: list[bytes] = []
    for w, h in [(1, 1), (5, 3), (16, 9)]:
        for descriptor in (0, 0x20, 0x10, 0x30):
            for depth in (8, 16, 24, 32):
                step = depth // 8
                cases.append(
                    _tga(2, depth, w, h, rng.randbytes(w * h * step), descriptor=descriptor)
                )
                cases.append(_tga(3, 8, w, h, rng.randbytes(w * h), descriptor=descriptor))
                cases.append(_tga(10, depth, w, h, _rle(rng, w * h, step), descriptor=descriptor))
                cases.append(_tga(11, 8, w, h, _rle(rng, w * h, 1), descriptor=descriptor))
            for map_depth in (16, 24, 32):
                entries = 200
                cmap = (entries, map_depth, rng.randbytes(entries * map_depth // 8))
                indices = bytes(rng.randrange(entries) for _ in range(w * h))
                cases.append(_tga(1, 8, w, h, indices, descriptor=descriptor, cmap=cmap))
                full = (256, map_depth, rng.randbytes(256 * map_depth // 8))
                cases.append(_tga(9, 8, w, h, _rle(rng, w * h, 1), cmap=full))
    good = _tga(2, 24, 4, 4, bytes(48), ident=b"hello")
    cases.append(good)
    # The failure paths, each of which must say the same thing.
    cases += [
        b"\x00" * 5,
        _tga(2, 24, 0, 4, b""),
        _tga(4, 24, 4, 4, bytes(48)),
        _tga(2, 24, 4, 4, bytes(40)),
        _tga(2, 15, 2, 2, bytes(8)),
        _tga(2, 0, 2, 2, bytes(8)),
        _tga(1, 8, 2, 2, bytes(4)),
        _tga(1, 8, 2, 2, bytes([0, 1, 2, 9]), cmap=(3, 24, bytes(9))),
        _tga(1, 8, 2, 2, bytes(4), cmap=(3, 24, bytes(5))),
        _tga(1, 8, 2, 2, bytes(4), cmap=(3, 12, bytes(6))),
        _tga(10, 24, 4, 4, bytes([0x83, 1, 2])),
        _tga(10, 24, 4, 4, bytes([0x03, 1, 2, 3])),
        _tga(10, 24, 4, 4, bytes([0x80, 1, 2, 3])),
        _tga(2, 24, 4, 4, b"", ident=bytes(255)),
    ]
    return cases


def _outcome(fn, data: bytes):  # type: ignore[no-untyped-def]
    try:
        return fn(data)
    except ValueError as exc:
        return ("error", str(exc))


def test_targa_matches() -> None:
    for data in _tga_cases():
        expected = _outcome(ref.read_tga, data)
        got = _outcome(wraithguard_native.tga_decode, data)
        assert got == expected, data[:18]


# ---------------------------------------------------------------------------
# Comparison loops


def _reference_difference(first: bytes, second: bytes, amplify: int) -> bytes:
    out = bytearray(len(first))
    for index in range(0, len(first), 4):
        for offset in range(3):
            out[index + offset] = min(
                255, abs(first[index + offset] - second[index + offset]) * amplify
            )
        alpha_gap = abs(first[index + 3] - second[index + 3])
        if alpha_gap:
            capped = min(255, alpha_gap * amplify)
            for offset in range(3):
                out[index + offset] = max(out[index + offset], capped)
        out[index + 3] = 255
    return bytes(out)


def test_measure_and_difference_match() -> None:
    rng = random.Random(3)  # noqa: S311 - test data, not security
    for _ in range(20):
        n = rng.randint(1, 300)
        left = rng.randbytes(n * 4)
        right = bytes(min(255, max(0, b + rng.randint(-6, 6))) for b in left)
        assert _measure(left, right) == ref._measure(left, right)
        other = rng.randbytes(n * 4)
        assert _measure(left, other) == ref._measure(left, other)
        for amplify in (1, 4, 8, 300):
            a, b = Image(n, 1, left), Image(n, 1, right)
            assert difference_image(a, b, amplify=amplify).pixels == _reference_difference(
                left, right, amplify
            )


# ---------------------------------------------------------------------------
# Real textures, when a corpus is at hand (WG_TEXTURE_CORPUS=dir)


def _corpus() -> list[Path]:
    root = os.environ.get("WG_TEXTURE_CORPUS")
    if not root:
        return []
    return sorted(
        p for p in Path(root).rglob("*") if p.suffix.lower() in (".dds", ".tga") and p.is_file()
    )[:400]


@pytest.mark.skipif(not _corpus(), reason="set WG_TEXTURE_CORPUS to a folder of .dds/.tga files")
def test_corpus_matches() -> None:
    for path in _corpus():
        data = path.read_bytes()
        if path.suffix.lower() == ".tga":
            assert _outcome(wraithguard_native.tga_decode, data) == _outcome(ref.read_tga, data)
            continue
        try:
            image = read_dds(data)
        except ImageError:
            continue
        # Re-decode the same surface through the reference.
        pf_flags, fourcc, bits, *masks = struct.unpack_from("<I4sIIIII", data, 80)
        start = 128
        if pf_flags & 0x4 and fourcc == b"DX10":
            start = 148
        surface = data[start:]
        if pf_flags & 0x4:
            if fourcc == b"DX10":
                (dxgi,) = struct.unpack_from("<I", data, 128)
                fourcc = {98: b"DX10", 99: b"DX10", 97: b"DX10"}.get(dxgi) or {
                    70: b"DXT1", 71: b"DXT1", 72: b"DXT1", 73: b"DXT3", 74: b"DXT3", 75: b"DXT3",
                    76: b"DXT5", 77: b"DXT5", 78: b"DXT5", 79: b"ATI1", 80: b"ATI1", 81: b"ATI1",
                    82: b"ATI2", 83: b"ATI2", 84: b"ATI2",
                }[dxgi]  # fmt: skip
            expected = _reference_compressed(surface, image.width, image.height, fourcc)
        else:
            if pf_flags & 0x20000 and not masks[0]:
                masks[0] = masks[1] = masks[2] = (1 << bits) - 1
            expected = bytes(
                ref._decode_uncompressed(surface, image.width, image.height, bits, tuple(masks))
            )
        assert image.pixels == expected, path
