"""bitmap.py: 16-bit BITFIELDS decoding, header-level validation errors,
palette/pixel truncation, and _read_masks' three layouts.

test_images.py's TestBitmap already pins row padding/order, 8-bit paletted,
32-bit's no-implicit-alpha rule, and RLE refusal by name. What it never
exercises: 16-bit at all (default or explicit masks), a mask of zero inside
_channel, any of read_bmp's own header-validation branches (bad magic, an
unrecognised header size, a truncated header, an unsupported compression,
implausible dimensions, the pixel-ceiling), a truncated palette table, an
out-of-range palette index, truncated pixel data, or _read_masks' three
branches (the 108+-byte inline layout, the 40-byte BITFIELDS layout
immediately after the header, and both of those when the file is too short
to actually hold the masks).

Self-contained rather than importing test_images.py's bmp() helper: that
builder is fixed at a 40-byte BI_RGB header with no room for masks, a
12-byte core header, or a deliberately short/malformed file.
"""

from __future__ import annotations

import struct

import pytest

from wraithguard.images import BitmapError, read_bmp

_FILE_HEADER = 14


def _bmp(
    *,
    magic: bytes = b"BM",
    header_size: int = 40,
    width: int = 1,
    height: int = 1,
    depth: int = 24,
    compression: int = 0,
    palette_count: int = 0,
    mask_bytes: bytes | None = None,
    palette: bytes = b"",
    pixel_data: bytes = b"",
    pixel_offset: int | None = None,
    truncate_after_info: bool = False,
) -> bytes:
    """Assemble a bitmap, with full control over header version and masks.

    mask_bytes is placed verbatim right after the (40-byte) info header --
    the caller decides how many bytes and what they contain, so both the
    108-byte inline layout (pad header_size out and put IIII there) and the
    40-byte BITFIELDS layout (put it immediately after, no padding) can be
    built with the same parameter.
    """
    if header_size == 12:
        info = struct.pack("<IHHHH", 12, width, height, 1, depth)
    else:
        info = struct.pack(
            "<IiiHHIIiiII",
            header_size,
            width,
            height,
            1,
            depth,
            compression,
            len(pixel_data),
            0,
            0,
            palette_count,
            0,
        )
        if header_size > 40:
            pad = bytearray(header_size - 40)
            if mask_bytes:
                pad[: len(mask_bytes)] = mask_bytes
            info += bytes(pad)

    body = info
    if truncate_after_info:
        file_header = struct.pack("<2sIHHI", magic, 0, 0, 0, _FILE_HEADER + len(body))
        return file_header + body

    if header_size == 40 and mask_bytes:
        body += mask_bytes
    body += palette

    if pixel_offset is None:
        pixel_offset = _FILE_HEADER + len(body)
    file_header = struct.pack("<2sIHHI", magic, 0, 0, 0, pixel_offset)
    prefix = file_header + body
    if len(prefix) < pixel_offset:
        prefix += bytes(pixel_offset - len(prefix))
    return prefix + pixel_data


class TestHeaderValidation:
    def test_missing_bm_magic_is_refused(self) -> None:
        with pytest.raises(BitmapError, match="missing the 'BM' magic"):
            read_bmp(_bmp(magic=b"XX"))

    def test_a_file_too_short_to_hold_even_the_magic_is_refused(self) -> None:
        with pytest.raises(BitmapError, match="missing the 'BM' magic"):
            read_bmp(b"BM\x00\x00")

    def test_an_unrecognised_header_size_is_refused(self) -> None:
        with pytest.raises(BitmapError, match="unrecognised bitmap header of 16 byte"):
            read_bmp(_bmp(header_size=16, truncate_after_info=True))

    def test_a_truncated_40_byte_header_is_refused(self) -> None:
        with pytest.raises(BitmapError, match="header is truncated"):
            # Declares the standard 40-byte header but the file ends 4 bytes
            # into it -- struct.error inside the try, not a length pre-check.
            read_bmp(struct.pack("<2sIHHI", b"BM", 0, 0, 0, 54) + struct.pack("<Ii", 40, 1))

    def test_an_unsupported_compression_code_is_refused_by_number(self) -> None:
        with pytest.raises(BitmapError, match="unsupported bitmap compression 9"):
            read_bmp(_bmp(compression=9, pixel_data=bytes(4)))

    def test_a_refused_compression_is_named_not_numbered(self) -> None:
        with pytest.raises(BitmapError, match="run-length encoded 4-bit"):
            read_bmp(_bmp(compression=2, pixel_data=bytes(4)))

    def test_zero_width_is_refused(self) -> None:
        with pytest.raises(BitmapError, match="implausible dimensions"):
            read_bmp(_bmp(width=0, height=1))

    def test_zero_height_is_refused(self) -> None:
        with pytest.raises(BitmapError, match="implausible dimensions"):
            read_bmp(_bmp(width=1, height=0))

    def test_a_declared_size_past_the_pixel_ceiling_is_refused(self) -> None:
        with pytest.raises(BitmapError, match="implausible size"):
            read_bmp(_bmp(width=60000, height=2000))


class TestPaletteAndPixelTruncation:
    def test_a_palette_table_shorter_than_declared_is_refused(self) -> None:
        with pytest.raises(BitmapError, match="color table runs past the end"):
            read_bmp(
                _bmp(
                    depth=8,
                    palette_count=4,
                    palette=bytes((1, 2, 3, 0)),  # only 1 of 4 entries
                    pixel_data=bytes((0,)),
                )
            )

    def test_an_out_of_range_palette_index_is_refused(self) -> None:
        palette = bytes((1, 2, 3, 0)) + bytes((4, 5, 6, 0))  # 2 entries
        with pytest.raises(BitmapError, match="palette index 5 is outside a table of 2"):
            read_bmp(
                _bmp(depth=8, palette_count=2, palette=palette, pixel_data=bytes((5, 0, 0, 0)))
            )

    def test_pixel_data_ending_early_is_refused(self) -> None:
        # 2x1 at 24bpp needs an 8-byte padded row; only 3 bytes given.
        with pytest.raises(BitmapError, match="pixel data ends early"):
            read_bmp(_bmp(width=2, height=1, depth=24, pixel_data=bytes((1, 2, 3))))


class Test16Bit:
    def test_default_masks_apply_when_the_header_declares_none(self) -> None:
        # BI_RGB, 40-byte header -- _read_masks returns None, and _expand_row
        # falls back to 5:5:5.
        packed = (0b11111 << 10) | (0b11111 << 5) | 0b11111
        raw = struct.pack("<H", packed) + b"\x00\x00"  # padded to the 4-byte row stride
        image = read_bmp(_bmp(depth=16, pixel_data=raw))
        assert image.pixel(0, 0) == (255, 255, 255, 255)

    def test_explicit_bitfields_masks_are_honoured(self) -> None:
        # 5:6:5, the other common 16-bit layout -- proves the header's own
        # masks are used, not just the default.
        red_mask, green_mask, blue_mask = 0xF800, 0x07E0, 0x001F
        masks = struct.pack("<III", red_mask, green_mask, blue_mask)
        packed = red_mask  # red channel fully on, everything else off
        raw = struct.pack("<H", packed) + b"\x00\x00"  # padded to the 4-byte row stride
        image = read_bmp(_bmp(depth=16, compression=3, mask_bytes=masks, pixel_data=raw))
        r, g, b, a = image.pixel(0, 0)
        assert r == 255
        assert g == 0
        assert b == 0
        assert a == 255  # no alpha mask given -> opaque

    def test_a_zero_mask_channel_decodes_to_zero_not_a_crash(self) -> None:
        # An explicit green mask of 0 -- a real bitmap could declare this,
        # and _channel's own "no mask" guard is what has to catch it.
        masks = struct.pack("<III", 0xF800, 0, 0x001F)
        raw = struct.pack("<H", 0xFFFF) + b"\x00\x00"  # padded to the 4-byte row stride
        image = read_bmp(_bmp(depth=16, compression=3, mask_bytes=masks, pixel_data=raw))
        assert image.pixel(0, 0)[1] == 0


class TestReadMasksLayouts:
    def test_a_108_byte_header_reads_masks_inline(self) -> None:
        masks = struct.pack("<IIII", 0xF800, 0x07E0, 0x001F, 0)
        raw = struct.pack("<H", 0xF800) + b"\x00\x00"  # padded to stride; red channel fully on
        image = read_bmp(
            _bmp(header_size=108, depth=16, compression=3, mask_bytes=masks, pixel_data=raw)
        )
        assert image.pixel(0, 0)[0] == 255

    def test_a_108_byte_header_too_short_to_hold_masks_falls_back_gracefully(
        self,
    ) -> None:
        # header_size claims 108 bytes but the file is truncated right after
        # the 40-byte prefix -- _read_masks must swallow the struct.error and
        # return None rather than raising, and decoding continues with the
        # depth's default masks.
        raw = struct.pack("<H", (0b11111 << 10) | (0b11111 << 5) | 0b11111) + b"\x00\x00"
        bmp_bytes = _bmp(header_size=108, depth=16, pixel_data=raw)
        # Trim the file down to right past the 40-byte info header, then fix
        # the pixel offset up to match -- simulating a header that promises
        # 108 bytes but a file that was cut short.
        bmp_bytes[:_FILE_HEADER]
        info40 = bmp_bytes[_FILE_HEADER : _FILE_HEADER + 40]
        short_pixel_offset = _FILE_HEADER + 40
        fixed_header = struct.pack("<2sIHHI", b"BM", 0, 0, 0, short_pixel_offset)
        truncated = fixed_header + info40 + raw

        image = read_bmp(truncated)

        assert image.pixel(0, 0) == (255, 255, 255, 255)

    def test_bitfields_at_40_bytes_falls_back_when_truncated(self) -> None:
        # Same idea at the other layout: BI_BITFIELDS with a 40-byte header
        # promises 3 masks right after it, but the file ends before they
        # arrive.
        file_header = struct.pack("<2sIHHI", b"BM", 0, 0, 0, _FILE_HEADER + 40)
        info = struct.pack("<IiiHHIIiiII", 40, 1, 1, 1, 16, 3, 2, 0, 0, 0, 0)
        # No mask bytes and no pixel data follow at all.
        truncated = file_header + info

        with pytest.raises(BitmapError, match="pixel data ends early"):
            read_bmp(truncated)


class TestCoreHeader:
    def test_the_12_byte_os2_header_still_decodes(self) -> None:
        image = read_bmp(
            _bmp(header_size=12, width=1, height=1, depth=24, pixel_data=bytes((1, 2, 3, 0)))
        )
        assert image.pixel(0, 0) == (3, 2, 1, 255)
