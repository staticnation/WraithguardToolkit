"""targa.py: color-mapped images, the non-32-bit pixel depths, right-to-left
orientation, and the header/truncation error paths.

test_images.py's TestTarga already pins 32-bit true-color, RLE run/literal
packets (the well-formed and control-byte-truncated cases), and the default
vs. top-origin flip. What it never exercises: color-mapped (paletted)
images entirely, depths other than 32, the RIGHT_ORIGIN descriptor bit, the
non-RLE truncation error, the RLE *literal* packet's own truncation error
(distinct from a run packet's), and every header-level validation check
(dimensions, size ceiling, unknown image type, an empty color map).

Self-contained rather than importing test_images.py's tga() helper: that
builder hardcodes has_map/map_length/map_depth to zero, which is exactly
what color-mapped fixtures need to control.
"""

from __future__ import annotations

import struct

import pytest

from wraithguard.images import TargaError, read_tga


def _tga(
    width: int,
    height: int,
    pixels: bytes,
    *,
    image_type: int = 2,
    depth: int = 32,
    descriptor: int = 0,
    has_map: int = 0,
    map_length: int = 0,
    map_depth: int = 0,
    map_data: bytes = b"",
) -> bytes:
    """Assemble a Targa file, with full control over the color-map fields."""
    header = struct.pack(
        "<BBBHHBHHHHBB",
        0,
        has_map,
        image_type,
        0,
        map_length,
        map_depth,
        0,
        0,
        width,
        height,
        depth,
        descriptor,
    )
    return header + map_data + pixels


class TestHeaderValidation:
    def test_a_file_shorter_than_the_header_is_refused(self) -> None:
        with pytest.raises(TargaError, match="too short"):
            read_tga(b"\x00" * 10)

    def test_zero_width_is_refused(self) -> None:
        with pytest.raises(TargaError, match="implausible dimensions"):
            read_tga(_tga(0, 4, b""))

    def test_zero_height_is_refused(self) -> None:
        with pytest.raises(TargaError, match="implausible dimensions"):
            read_tga(_tga(4, 0, b""))

    def test_a_declared_size_past_the_pixel_ceiling_is_refused(self) -> None:
        # The header is validated before any pixel data is touched, so an
        # oversized declaration is refused with no trailing bytes at all.
        with pytest.raises(TargaError, match="implausible size"):
            read_tga(_tga(60000, 2000, b""))

    def test_an_unknown_image_type_is_refused(self) -> None:
        with pytest.raises(TargaError, match="unsupported Targa image type"):
            read_tga(_tga(1, 1, bytes(4), image_type=7))

    def test_depth_zero_is_refused_before_any_decoding(self) -> None:
        with pytest.raises(TargaError, match="unsupported Targa depth: 0"):
            read_tga(_tga(1, 1, b"", depth=0))

    def test_an_unpacked_depth_the_format_does_not_define_is_refused(self) -> None:
        # depth=4 passes the step-size check (step=1) but has no branch of
        # its own in _unpack_pixel -- a different failure than depth=0.
        with pytest.raises(TargaError, match="unsupported Targa depth: 4"):
            read_tga(_tga(1, 1, bytes((0xFF,)), depth=4))


class TestPlainTruncation:
    def test_non_rle_pixel_data_ending_early_is_refused(self) -> None:
        # 2x1 at 32bpp needs 8 bytes; only 4 are provided.
        with pytest.raises(TargaError, match="ends early"):
            read_tga(_tga(2, 1, bytes((1, 2, 3, 255))))


class TestRleLiteralTruncation:
    def test_a_literal_packet_ending_early_is_refused(self) -> None:
        # Control byte 0x01 claims 2 literal pixels (32bpp = 8 bytes); only
        # one pixel's worth follows. Distinct code path -- and message --
        # from a *run* packet's truncation, which test_images.py covers.
        packet = bytes((0x01,)) + bytes((1, 2, 3, 255))
        with pytest.raises(TargaError, match="literal packet is truncated"):
            read_tga(_tga(2, 1, packet, image_type=10))


class TestPixelDepths:
    def test_24_bit_is_blue_first_with_full_alpha(self) -> None:
        image = read_tga(_tga(1, 1, bytes((30, 20, 10)), depth=24))
        assert image.pixel(0, 0) == (10, 20, 30, 255)

    def test_16_bit_1555_replicates_bits_to_full_scale(self) -> None:
        # All five bits set in every channel -> full 8-bit scale, not a
        # shift-only 0xF8-capped value.
        packed = (0b11111 << 10) | (0b11111 << 5) | 0b11111
        raw = struct.pack("<H", packed)
        image = read_tga(_tga(1, 1, raw, depth=16))
        assert image.pixel(0, 0) == (255, 255, 255, 255)

    def test_16_bit_isolates_each_channel_correctly(self) -> None:
        # Blue only, mid-range -- proves the channels aren't swapped or
        # overlapping in the bit-shift math.
        packed = 0b10101  # blue bits only
        raw = struct.pack("<H", packed)
        image = read_tga(_tga(1, 1, raw, depth=16))
        r, g, b, a = image.pixel(0, 0)
        assert (r, g) == (0, 0)
        assert b > 0
        assert a == 255

    def test_8_bit_greyscale_replicates_into_rgb(self) -> None:
        image = read_tga(_tga(1, 1, bytes((128,)), depth=8, image_type=3))
        assert image.pixel(0, 0) == (128, 128, 128, 255)


class TestColorMapped:
    def test_an_8_bit_index_resolves_through_the_palette(self) -> None:
        # A 2-entry, 24-bit palette; blue-first per entry, same as any other
        # depth-24 pixel.
        palette = bytes((10, 20, 30)) + bytes((40, 50, 60))  # index 0, index 1
        image = read_tga(
            _tga(
                1,
                1,
                bytes((1,)),  # index 1
                image_type=1,
                depth=8,
                has_map=1,
                map_length=2,
                map_depth=24,
                map_data=palette,
            )
        )
        assert image.pixel(0, 0) == (60, 50, 40, 255)

    def test_rle_color_mapped_runs_through_the_palette_too(self) -> None:
        palette = bytes((0, 0, 0)) + bytes((99, 88, 77))
        run_packet = bytes((0x81,)) + bytes((1,))  # run of 2, index 1
        image = read_tga(
            _tga(
                2,
                1,
                run_packet,
                image_type=9,  # RLE color-mapped
                depth=8,
                has_map=1,
                map_length=2,
                map_depth=24,
                map_data=palette,
            )
        )
        assert image.pixel(0, 0) == (77, 88, 99, 255)
        assert image.pixel(1, 0) == (77, 88, 99, 255)

    def test_a_color_mapped_type_with_no_actual_map_is_refused(self) -> None:
        with pytest.raises(TargaError, match="carries no color map"):
            read_tga(_tga(1, 1, bytes((0,)), image_type=1, depth=8))

    def test_a_declared_map_shorter_than_its_own_length_is_refused(self) -> None:
        # map_length says 5 entries at 24 bits each (15 bytes); only 3 are
        # actually provided.
        with pytest.raises(TargaError, match="color map runs past the end"):
            read_tga(
                _tga(
                    1,
                    1,
                    bytes((0,)),
                    image_type=1,
                    depth=8,
                    has_map=1,
                    map_length=5,
                    map_depth=24,
                    map_data=bytes((1, 2, 3)),
                )
            )

    def test_an_out_of_range_index_is_refused(self) -> None:
        palette = bytes((1, 2, 3))  # one entry
        with pytest.raises(TargaError, match="color-map index 5 is outside a map of 1"):
            read_tga(
                _tga(
                    1,
                    1,
                    bytes((5,)),  # index 5, palette only has index 0
                    image_type=1,
                    depth=8,
                    has_map=1,
                    map_length=1,
                    map_depth=24,
                    map_data=palette,
                )
            )

    def test_a_16_bit_palette_index_is_read_little_endian(self) -> None:
        palette_entries = [bytes((0, 0, 0))] * 300  # pad past 8-bit range
        palette_entries[257] = bytes((5, 6, 7))
        palette = b"".join(palette_entries)
        index_bytes = struct.pack("<H", 257)

        image = read_tga(
            _tga(
                1,
                1,
                index_bytes,
                image_type=1,
                depth=16,
                has_map=1,
                map_length=300,
                map_depth=24,
                map_data=palette,
            )
        )
        assert image.pixel(0, 0) == (7, 6, 5, 255)


class TestRightToLeftOrigin:
    def test_the_right_origin_bit_mirrors_columns(self) -> None:
        # Two pixels stored left to right in the file; RIGHT_ORIGIN means
        # the first stored pixel is actually the image's rightmost column.
        # Red channel is byte index 2 (stored blue-first), so put the
        # distinguishing value there.
        pixels = bytes((0, 0, 1, 255)) + bytes((0, 0, 2, 255))
        image = read_tga(_tga(2, 1, pixels, descriptor=0x10))
        assert image.pixel(1, 0)[0] == 1
        assert image.pixel(0, 0)[0] == 2

    def test_right_origin_combines_with_top_origin(self) -> None:
        # Both bits set: no vertical flip (already top-down), but still
        # mirrored horizontally.
        pixels = bytes((0, 0, 1, 255)) + bytes((0, 0, 2, 255))
        image = read_tga(_tga(2, 1, pixels, descriptor=0x20 | 0x10))
        assert image.pixel(1, 0)[0] == 1
        assert image.pixel(0, 0)[0] == 2
