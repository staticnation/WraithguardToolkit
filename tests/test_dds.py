"""Tests for the DDS decoder and the PNG encoder.

The decoder was checked against an independent implementation during
development -- all 50 textures in the local corpus decode **byte-for-byte
identically** to Pillow, and every PNG this module writes reads back through
Pillow with the exact pixels it was given. That comparison cannot live here,
because Pillow is not a dependency of this project and the corpus is not in the
repository, so these tests pin the same behaviour down with hand-built blocks
whose expected output can be computed by hand.
"""

from __future__ import annotations

import struct
import zlib

import pytest

from mlox_subset.dds import DdsError, Image, encode_png, read_dds


def dds(
    fourcc: bytes,
    width: int,
    height: int,
    surface: bytes,
    *,
    pf_flags: int = 0x4,
    bit_count: int = 0,
    masks: tuple[int, int, int, int] = (0, 0, 0, 0),
) -> bytes:
    """Assemble a DDS file around a surface.

    Args:
        fourcc: The compression tag, or four NULs for uncompressed.
        width: Surface width.
        height: Surface height.
        surface: The pixel or block data.
        pf_flags: Pixel-format flags.
        bit_count: Bits per pixel, for uncompressed surfaces.
        masks: Channel masks, for uncompressed surfaces.

    Returns:
        The whole file.
    """
    header = bytearray(124)
    struct.pack_into("<IIII", header, 0, 124, 0x1007, height, width)
    # The pixel-format block starts at header offset 72 and its own size field
    # occupies the first four bytes, so the flags are at 76 -- not 72. Getting
    # this wrong shifted every field by four and made eight tests fail against
    # a decoder that was correct.
    struct.pack_into("<II4sI", header, 72, 32, pf_flags, fourcc, bit_count)
    struct.pack_into("<IIII", header, 88, *masks)
    return b"DDS " + bytes(header) + surface


def bc1_block(c0: int, c1: int, indices: int) -> bytes:
    """One DXT1 colour block.

    Args:
        c0: First endpoint, packed 5:6:5.
        c1: Second endpoint, packed 5:6:5.
        indices: Sixteen 2-bit selectors.

    Returns:
        Eight bytes.
    """
    return struct.pack("<HHI", c0, c1, indices)


WHITE_565 = 0xFFFF
BLACK_565 = 0x0000


class TestBc1:
    """DXT1: two endpoints, four colours, and a punchthrough mode."""

    def test_full_scale_white_decodes_to_255(self) -> None:
        """The channel expansion must replicate bits, not zero-fill them.

        Shifting alone decodes white as (248, 252, 248), which is invisible on
        one texture and shows up as a colour cast across a whole comparison.
        """
        image = read_dds(dds(b"DXT1", 4, 4, bc1_block(WHITE_565, WHITE_565, 0)))
        assert image.pixel(0, 0) == (255, 255, 255, 255)

    def test_the_second_endpoint_is_selected_by_index_one(self) -> None:
        """Index 1 picks c1, which pins the bit order of the selector word."""
        image = read_dds(dds(b"DXT1", 4, 4, bc1_block(WHITE_565, BLACK_565, 0b01)))
        assert image.pixel(0, 0) == (0, 0, 0, 255)
        assert image.pixel(1, 0) == (255, 255, 255, 255)

    def test_punchthrough_makes_index_three_transparent(self) -> None:
        """When c0 <= c1 the fourth slot is a hole, not a colour.

        This is the mode that carries cutout foliage, so getting it wrong makes
        leaves opaque black rather than absent.
        """
        image = read_dds(dds(b"DXT1", 4, 4, bc1_block(BLACK_565, WHITE_565, 0b11)))
        assert image.pixel(0, 0) == (0, 0, 0, 0)

    def test_the_opaque_mode_gives_a_third_not_a_hole(self) -> None:
        """A negative control: with c0 > c1 index 3 is a real colour."""
        image = read_dds(dds(b"DXT1", 4, 4, bc1_block(WHITE_565, BLACK_565, 0b11)))
        red, _green, _blue, alpha = image.pixel(0, 0)
        assert alpha == 255
        assert 0 < red < 255


class TestBc2AndBc3Alpha:
    """The two alpha encodings, which is the whole difference between them."""

    def test_bc2_alpha_is_four_bits_a_pixel_scaled_to_full_range(self) -> None:
        """0xF must reach 255, or every texture is faintly transparent."""
        alpha = b"\xf0" + b"\x00" * 7
        surface = alpha + bc1_block(WHITE_565, WHITE_565, 0)
        image = read_dds(dds(b"DXT3", 4, 4, surface))
        assert image.pixel(0, 0)[3] == 0
        assert image.pixel(1, 0)[3] == 255

    def test_bc3_uses_its_endpoints_directly_for_the_first_two_indices(self) -> None:
        """Index 0 is a0 and index 1 is a1, whichever interpolation mode."""
        alpha = bytes((200, 40)) + bytes((0b01_000_000, 0, 0, 0, 0, 0))
        surface = alpha + bc1_block(WHITE_565, WHITE_565, 0)
        image = read_dds(dds(b"DXT5", 4, 4, surface))
        assert image.pixel(0, 0)[3] == 200
        assert image.pixel(2, 0)[3] == 40

    def test_bc3_six_value_mode_has_explicit_transparent_and_opaque_slots(self) -> None:
        """With a0 <= a1, indices 6 and 7 are 0 and 255 rather than interpolants."""
        alpha = bytes((40, 200)) + bytes((0b110, 0, 0, 0, 0, 0))
        surface = alpha + bc1_block(WHITE_565, WHITE_565, 0)
        image = read_dds(dds(b"DXT5", 4, 4, surface))
        assert image.pixel(0, 0)[3] == 0

    def test_bc3_never_uses_the_punchthrough_colour_mode(self) -> None:
        """Its alpha lives in its own block, so index 3 is always a colour.

        Sharing the DXT1 rule here would punch holes in every DXT5 texture
        whose endpoints happen to be ordered the other way.
        """
        surface = b"\xff" * 8 + bc1_block(BLACK_565, WHITE_565, 0b11)
        image = read_dds(dds(b"DXT5", 4, 4, surface))
        assert image.pixel(0, 0)[3] == 255


class TestPartialBlocks:
    """Textures are not always a multiple of four pixels."""

    def test_a_surface_narrower_than_a_block_is_cropped_not_padded(self) -> None:
        """A 2x2 texture is one block with twelve pixels discarded."""
        image = read_dds(dds(b"DXT1", 2, 2, bc1_block(WHITE_565, WHITE_565, 0)))
        assert (image.width, image.height) == (2, 2)
        assert len(image.pixels) == 2 * 2 * 4

    def test_a_non_multiple_size_still_fills_every_pixel(self) -> None:
        """6x6 is four blocks, and no pixel may be left unwritten."""
        surface = bc1_block(WHITE_565, WHITE_565, 0) * 4
        image = read_dds(dds(b"DXT1", 6, 6, surface))
        assert all(image.pixel(x, y) == (255, 255, 255, 255) for x in range(6) for y in range(6))


class TestRefusalsAreFindings:
    """These files come from mod archives and must fail as findings."""

    def test_a_non_dds_is_refused_by_name(self) -> None:
        """Extensions lie."""
        with pytest.raises(DdsError, match="DDS "):
            read_dds(b"\x89PNG\r\n\x1a\n" + b"\x00" * 200)

    def test_an_unsupported_compression_is_named(self) -> None:
        """BC7 is deferred, so it must say so rather than produce garbage."""
        with pytest.raises(DdsError, match="DX10"):
            read_dds(dds(b"DX10", 4, 4, b"\x00" * 16))

    def test_a_truncated_surface_is_refused(self) -> None:
        """Half a block is not half an image."""
        with pytest.raises(DdsError, match="ends early"):
            read_dds(dds(b"DXT1", 64, 64, b"\x00" * 8))

    def test_implausible_dimensions_are_refused(self) -> None:
        """A corrupt header must not become a multi-gigabyte allocation."""
        with pytest.raises(DdsError, match="implausible"):
            read_dds(dds(b"DXT1", 1 << 20, 1 << 20, b"\x00" * 8))

    def test_a_plausible_pair_of_dimensions_can_still_be_an_absurd_area(self) -> None:
        """Bounding each side is not the same as bounding the allocation.

        32768 x 32768 passes a per-dimension check and then asks for 4.3 GB of
        RGBA before any texture data is read. Found by auditing this module
        rather than by a failing file.
        """
        with pytest.raises(DdsError, match="implausible size"):
            read_dds(dds(b"DXT1", 1 << 15, 1 << 15, b"\x00" * 8))


class TestUncompressed:
    """Not every Morrowind texture is compressed."""

    def test_masks_place_the_channels(self) -> None:
        """The masks say where each channel lives, so they must be honoured."""
        # B, G, R, A in memory order, which is what these headers describe.
        surface = bytes((10, 20, 30, 40))
        image = read_dds(
            dds(
                b"\x00\x00\x00\x00",
                1,
                1,
                surface,
                pf_flags=0x41,
                bit_count=32,
                masks=(0x00FF0000, 0x0000FF00, 0x000000FF, 0xFF000000),
            )
        )
        assert image.pixel(0, 0) == (30, 20, 10, 40)

    def test_no_alpha_mask_means_opaque_not_transparent(self) -> None:
        """A 24-bit surface has no alpha, and must not decode as invisible."""
        image = read_dds(
            dds(
                b"\x00\x00\x00\x00",
                1,
                1,
                bytes((10, 20, 30)),
                pf_flags=0x40,
                bit_count=24,
                masks=(0x00FF0000, 0x0000FF00, 0x000000FF, 0),
            )
        )
        assert image.pixel(0, 0)[3] == 255


class TestPng:
    """The encoder writes one thing, and must write it correctly."""

    def test_it_produces_a_parseable_rgba_png(self) -> None:
        """Signature, IHDR, and pixels that survive the round trip.

        Parsed with :mod:`zlib` rather than an image library, so the test has
        the same dependency footprint as the code it covers.
        """
        pixels = bytes(range(16))
        png = encode_png(Image(2, 2, pixels))
        assert png.startswith(b"\x89PNG\r\n\x1a\n")
        assert png[12:16] == b"IHDR"
        width, height, depth, colour = struct.unpack_from(">IIBB", png, 16)
        assert (width, height, depth, colour) == (2, 2, 8, 6)
        start = png.index(b"IDAT") + 4
        length = struct.unpack_from(">I", png, start - 8)[0]
        raw = zlib.decompress(png[start : start + length])
        # One filter byte per scanline, then the pixels unchanged.
        assert raw == b"\x00" + pixels[:8] + b"\x00" + pixels[8:]

    def test_every_chunk_carries_a_correct_crc(self) -> None:
        """A wrong CRC makes a file that some viewers open and others reject."""
        png = encode_png(Image(1, 1, b"\x01\x02\x03\x04"))
        offset = 8
        seen = []
        while offset < len(png):
            length = struct.unpack_from(">I", png, offset)[0]
            tag = png[offset + 4 : offset + 8]
            body = png[offset + 8 : offset + 8 + length]
            stored = struct.unpack_from(">I", png, offset + 8 + length)[0]
            assert stored == zlib.crc32(tag + body) & 0xFFFFFFFF, tag
            seen.append(tag)
            offset += 12 + length
        assert seen == [b"IHDR", b"IDAT", b"IEND"]

    def test_a_mismatched_buffer_is_refused(self) -> None:
        """Otherwise the file opens and is silently wrong, which is worse."""
        with pytest.raises(ValueError, match="RGBA"):
            encode_png(Image(4, 4, b"\x00" * 8))


class TestRemainingRefusals:
    """The error branches an audit found untested.

    These are the paths that run on files from mod archives, so they are the
    ones that most need to fail as findings rather than as tracebacks -- and
    they were the least exercised part of the module.
    """

    def test_reading_outside_the_image_raises(self) -> None:
        """A viewer asking for a pixel that is not there is a caller bug."""
        image = read_dds(dds(b"DXT1", 4, 4, bc1_block(WHITE_565, WHITE_565, 0)))
        with pytest.raises(IndexError, match="outside"):
            image.pixel(4, 0)

    def test_a_wrong_header_size_is_refused(self) -> None:
        """The field is in the file, so it is checked rather than trusted."""
        broken = bytearray(dds(b"DXT1", 4, 4, b"\x00" * 8))
        struct.pack_into("<I", broken, 4, 100)
        with pytest.raises(DdsError, match="header claims"):
            read_dds(bytes(broken))

    def test_an_unsupported_uncompressed_depth_is_refused(self) -> None:
        """16-bit surfaces exist; guessing at one would produce noise."""
        with pytest.raises(DdsError, match="depth"):
            read_dds(
                dds(
                    b"\x00\x00\x00\x00",
                    2,
                    2,
                    b"\x00" * 8,
                    pf_flags=0x40,
                    bit_count=16,
                    masks=(0xF800, 0x07E0, 0x001F, 0),
                )
            )

    def test_a_truncated_uncompressed_surface_is_refused(self) -> None:
        """Same guarantee as the compressed path, which was already tested."""
        with pytest.raises(DdsError, match="ends early"):
            read_dds(
                dds(
                    b"\x00\x00\x00\x00",
                    16,
                    16,
                    b"\x00" * 8,
                    pf_flags=0x41,
                    bit_count=32,
                    masks=(0x00FF0000, 0x0000FF00, 0x000000FF, 0xFF000000),
                )
            )

    def test_a_pixel_format_that_is_neither_is_refused(self) -> None:
        """Luminance and YUV surfaces are neither FourCC nor RGB."""
        with pytest.raises(DdsError, match="pixel format"):
            read_dds(dds(b"\x00\x00\x00\x00", 2, 2, b"\x00" * 16, pf_flags=0x20000))
