"""The Python image decoders that native/src/img.rs replaced, kept as the parity reference.

Verbatim in substance (docstrings and comments stripped, error types collapsed to
``ValueError``); see ``test_images_native_parity.py``.
"""

# ruff: noqa
# fmt: off
from __future__ import annotations

import math
import struct

_BLOCK_BYTES = {b'DXT1': 8, b'DXT2': 16, b'DXT3': 16, b'DXT4': 16, b'DXT5': 16, b'ATI1': 8, b'BC4U': 8, b'BC4S': 8, b'ATI2': 16, b'BC5U': 16, b'BC5S': 16}

def _expand_565(value: int) -> tuple[int, int, int]:
    r = value >> 11 & 31
    g = value >> 5 & 63
    b = value & 31
    return (r << 3 | r >> 2, g << 2 | g >> 4, b << 3 | b >> 2)

def _color_table(c0: int, c1: int, *, punchthrough: bool) -> list[bytes]:
    (r0, g0, b0) = _expand_565(c0)
    (r1, g1, b1) = _expand_565(c1)
    table = [bytes((r0, g0, b0, 255)), bytes((r1, g1, b1, 255))]
    if punchthrough and c0 <= c1:
        table.append(bytes(((r0 + r1) // 2, (g0 + g1) // 2, (b0 + b1) // 2, 255)))
        table.append(b'\x00\x00\x00\x00')
    else:
        table.append(bytes(((2 * r0 + r1) // 3, (2 * g0 + g1) // 3, (2 * b0 + b1) // 3, 255)))
        table.append(bytes(((r0 + 2 * r1) // 3, (g0 + 2 * g1) // 3, (b0 + 2 * b1) // 3, 255)))
    return table

def _alpha_table(a0: int, a1: int) -> list[int]:
    if a0 > a1:
        return [a0, a1, *[((7 - i) * a0 + i * a1) // 7 for i in range(1, 7)]]
    return [a0, a1, *[((5 - i) * a0 + i * a1) // 5 for i in range(1, 5)], 0, 255]

def _decode_blocks(data: bytes, width: int, height: int, fourcc: bytes) -> bytearray:
    stride = _BLOCK_BYTES[fourcc]
    has_alpha_block = stride == 16
    explicit_alpha = fourcc in (b'DXT2', b'DXT3')
    out = bytearray(width * height * 4)
    offset = 0
    for block_y in range(0, height, 4):
        for block_x in range(0, width, 4):
            if offset + stride > len(data):
                raise ValueError(f'texture data ends early: wanted {stride} byte(s) at {offset}, file holds {len(data) - offset}')
            block = data[offset:offset + stride]
            offset += stride
            alphas: list[int] | None = None
            if has_alpha_block:
                alphas = _explicit_alphas(block[:8]) if explicit_alpha else _interpolated_alphas(block[:8])
                block = block[8:]
            (c0, c1, bits) = struct.unpack_from('<HHI', block, 0)
            table = _color_table(c0, c1, punchthrough=not has_alpha_block)
            wide = block_x + 4 <= width
            for row in range(4):
                y = block_y + row
                if y >= height:
                    break
                shift = 8 * row
                quad = bits >> shift & 255
                indices = (quad & 3, quad >> 2 & 3, quad >> 4 & 3, quad >> 6 & 3)
                start = (y * width + block_x) * 4
                if alphas is None:
                    if wide:
                        out[start:start + 16] = b''.join((table[i] for i in indices))
                        continue
                    for col in range(min(4, width - block_x)):
                        out[start + col * 4:start + col * 4 + 4] = table[indices[col]]
                    continue
                base = 4 * row
                for col in range(4 if wide else max(0, width - block_x)):
                    at = start + col * 4
                    out[at:at + 4] = table[indices[col]]
                    out[at + 3] = alphas[base + col]
    return out

def _decode_one_channel(data: bytes, width: int, height: int) -> bytearray:
    out = bytearray(width * height * 4)
    offset = 0
    for block_y in range(0, height, 4):
        for block_x in range(0, width, 4):
            if offset + 8 > len(data):
                raise ValueError(f'BC4 data ends early at offset {offset}')
            values = _interpolated_alphas(data[offset:offset + 8])
            offset += 8
            for row in range(min(4, height - block_y)):
                start = ((block_y + row) * width + block_x) * 4
                for col in range(min(4, width - block_x)):
                    level = values[row * 4 + col]
                    at = start + col * 4
                    out[at:at + 4] = bytes((level, level, level, 255))
    return out

def _decode_two_channel(data: bytes, width: int, height: int) -> bytearray:
    out = bytearray(width * height * 4)
    offset = 0
    blue_of: dict[int, int] = {}
    for block_y in range(0, height, 4):
        for block_x in range(0, width, 4):
            if offset + 16 > len(data):
                raise ValueError(f'BC5 data ends early at offset {offset}')
            reds = _interpolated_alphas(data[offset:offset + 8])
            greens = _interpolated_alphas(data[offset + 8:offset + 16])
            offset += 16
            for row in range(min(4, height - block_y)):
                start = ((block_y + row) * width + block_x) * 4
                for col in range(min(4, width - block_x)):
                    index = row * 4 + col
                    (red, green) = (reds[index], greens[index])
                    key = red << 8 | green
                    blue = blue_of.get(key)
                    if blue is None:
                        x = red / 127.5 - 1.0
                        y = green / 127.5 - 1.0
                        z = math.sqrt(max(0.0, 1.0 - x * x - y * y))
                        blue = min(255, int((z + 1.0) * 127.5))
                        blue_of[key] = blue
                    at = start + col * 4
                    out[at:at + 4] = bytes((red, green, blue, 255))
    return out

def _explicit_alphas(block: bytes) -> list[int]:
    values: list[int] = []
    for byte in block:
        (low, high) = (byte & 15, byte >> 4)
        values.append(low * 17)
        values.append(high * 17)
    return values

def _interpolated_alphas(block: bytes) -> list[int]:
    table = _alpha_table(block[0], block[1])
    packed = int.from_bytes(block[2:8], 'little')
    return [table[packed >> 3 * i & 7] for i in range(16)]

def _decode_uncompressed(data: bytes, width: int, height: int, bit_count: int, masks: tuple[int, int, int, int]) -> bytearray:
    if bit_count not in (8, 16, 24, 32):
        raise ValueError(f'unsupported uncompressed depth: {bit_count} bits per pixel')
    step = bit_count // 8
    needed = width * height * step
    if len(data) < needed:
        raise ValueError(f'texture data ends early: wanted {needed} byte(s), got {len(data)}')
    shifts = [(_lowest_bit(mask), mask) for mask in masks]
    out = bytearray(width * height * 4)
    for index in range(width * height):
        raw = int.from_bytes(data[index * step:index * step + step], 'little')
        channels = []
        for (position, (shift, mask)) in enumerate(shifts):
            if not mask:
                channels.append(255 if position == 3 else 0)
                continue
            channels.append(_scale_to_byte((raw & mask) >> shift, mask >> shift))
        out[index * 4:index * 4 + 4] = bytes(channels)
    return out

def _lowest_bit(mask: int) -> int:
    return (mask & -mask).bit_length() - 1 if mask else 0

def _scale_to_byte(value: int, maximum: int) -> int:
    return 255 if maximum <= 0 else value * 255 // maximum

BLOCK_BYTES = 16

_SUBSETS = (3, 2, 3, 2, 1, 1, 1, 2)

_PARTITION_BITS = (4, 6, 6, 6, 0, 0, 0, 6)

_ROTATION_BITS = (0, 0, 0, 0, 2, 2, 0, 0)

_SELECTOR_BITS = (0, 0, 0, 0, 1, 0, 0, 0)

_COLOUR_BITS = (4, 6, 5, 7, 5, 7, 7, 5)

_ALPHA_BITS = (0, 0, 0, 0, 6, 8, 7, 5)

_ENDPOINT_P = (1, 0, 0, 1, 0, 0, 1, 1)

_SHARED_P = (0, 1, 0, 0, 0, 0, 0, 0)

_INDEX_BITS = (3, 3, 2, 2, 2, 2, 4, 2)

_INDEX_BITS_2 = (0, 0, 0, 0, 3, 2, 0, 0)

_WEIGHTS = {2: (0, 21, 43, 64), 3: (0, 9, 18, 27, 37, 46, 55, 64), 4: (0, 4, 9, 13, 17, 21, 26, 30, 34, 38, 43, 47, 51, 55, 60, 64)}

_PARTITIONS_2 = ((0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1), (0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1), (0, 1, 1, 1, 0, 1, 1, 1, 0, 1, 1, 1, 0, 1, 1, 1), (0, 0, 0, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0, 1, 1, 1), (0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 1, 1), (0, 0, 1, 1, 0, 1, 1, 1, 0, 1, 1, 1, 1, 1, 1, 1), (0, 0, 0, 1, 0, 0, 1, 1, 0, 1, 1, 1, 1, 1, 1, 1), (0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 1, 1, 0, 1, 1, 1), (0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 1, 1), (0, 0, 1, 1, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1), (0, 0, 0, 0, 0, 0, 0, 1, 0, 1, 1, 1, 1, 1, 1, 1), (0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 1, 1, 1), (0, 0, 0, 1, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1), (0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1), (0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1), (0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1), (0, 0, 0, 0, 1, 0, 0, 0, 1, 1, 1, 0, 1, 1, 1, 1), (0, 1, 1, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0), (0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 1, 1, 1, 0), (0, 1, 1, 1, 0, 0, 1, 1, 0, 0, 0, 1, 0, 0, 0, 0), (0, 0, 1, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0), (0, 0, 0, 0, 1, 0, 0, 0, 1, 1, 0, 0, 1, 1, 1, 0), (0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 1, 1, 0, 0), (0, 1, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 0, 1), (0, 0, 1, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 0), (0, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 1, 0, 0), (0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0), (0, 0, 1, 1, 0, 1, 1, 0, 0, 1, 1, 0, 1, 1, 0, 0), (0, 0, 0, 1, 0, 1, 1, 1, 1, 1, 1, 0, 1, 0, 0, 0), (0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0), (0, 1, 1, 1, 0, 0, 0, 1, 1, 0, 0, 0, 1, 1, 1, 0), (0, 0, 1, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 1, 0, 0), (0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1), (0, 0, 0, 0, 1, 1, 1, 1, 0, 0, 0, 0, 1, 1, 1, 1), (0, 1, 0, 1, 1, 0, 1, 0, 0, 1, 0, 1, 1, 0, 1, 0), (0, 0, 1, 1, 0, 0, 1, 1, 1, 1, 0, 0, 1, 1, 0, 0), (0, 0, 1, 1, 1, 1, 0, 0, 0, 0, 1, 1, 1, 1, 0, 0), (0, 1, 0, 1, 0, 1, 0, 1, 1, 0, 1, 0, 1, 0, 1, 0), (0, 1, 1, 0, 1, 0, 0, 1, 0, 1, 1, 0, 1, 0, 0, 1), (0, 1, 0, 1, 1, 0, 1, 0, 1, 0, 1, 0, 0, 1, 0, 1), (0, 1, 1, 1, 0, 0, 1, 1, 1, 1, 0, 0, 1, 1, 1, 0), (0, 0, 0, 1, 0, 0, 1, 1, 1, 1, 0, 0, 1, 0, 0, 0), (0, 0, 1, 1, 0, 0, 1, 0, 0, 1, 0, 0, 1, 1, 0, 0), (0, 0, 1, 1, 1, 0, 1, 1, 1, 1, 0, 1, 1, 1, 0, 0), (0, 1, 1, 0, 1, 0, 0, 1, 1, 0, 0, 1, 0, 1, 1, 0), (0, 0, 1, 1, 1, 1, 0, 0, 1, 1, 0, 0, 0, 0, 1, 1), (0, 1, 1, 0, 0, 1, 1, 0, 1, 0, 0, 1, 1, 0, 0, 1), (0, 0, 0, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 0, 0, 0), (0, 1, 0, 0, 1, 1, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0), (0, 0, 1, 0, 0, 1, 1, 1, 0, 0, 1, 0, 0, 0, 0, 0), (0, 0, 0, 0, 0, 0, 1, 0, 0, 1, 1, 1, 0, 0, 1, 0), (0, 0, 0, 0, 0, 1, 0, 0, 1, 1, 1, 0, 0, 1, 0, 0), (0, 1, 1, 0, 1, 1, 0, 0, 1, 0, 0, 1, 0, 0, 1, 1), (0, 0, 1, 1, 0, 1, 1, 0, 1, 1, 0, 0, 1, 0, 0, 1), (0, 1, 1, 0, 0, 0, 1, 1, 1, 0, 0, 1, 1, 1, 0, 0), (0, 0, 1, 1, 1, 0, 0, 1, 1, 1, 0, 0, 0, 1, 1, 0), (0, 1, 1, 0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 0, 0, 1), (0, 1, 1, 0, 0, 0, 1, 1, 0, 0, 1, 1, 1, 0, 0, 1), (0, 1, 1, 1, 1, 1, 1, 0, 1, 0, 0, 0, 0, 0, 0, 1), (0, 0, 0, 1, 1, 0, 0, 0, 1, 1, 1, 0, 0, 1, 1, 1), (0, 0, 0, 0, 1, 1, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1), (0, 0, 1, 1, 0, 0, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0), (0, 0, 1, 0, 0, 0, 1, 0, 1, 1, 1, 0, 1, 1, 1, 0), (0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 1, 1, 0, 1, 1, 1))

_PARTITIONS_3 = ((0, 0, 1, 1, 0, 0, 1, 1, 0, 2, 2, 1, 2, 2, 2, 2), (0, 0, 0, 1, 0, 0, 1, 1, 2, 2, 1, 1, 2, 2, 2, 1), (0, 0, 0, 0, 2, 0, 0, 1, 2, 2, 1, 1, 2, 2, 1, 1), (0, 2, 2, 2, 0, 0, 2, 2, 0, 0, 1, 1, 0, 1, 1, 1), (0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 2, 2, 1, 1, 2, 2), (0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 2, 2, 0, 0, 2, 2), (0, 0, 2, 2, 0, 0, 2, 2, 1, 1, 1, 1, 1, 1, 1, 1), (0, 0, 1, 1, 0, 0, 1, 1, 2, 2, 1, 1, 2, 2, 1, 1), (0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2), (0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2), (0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 2, 2, 2, 2), (0, 0, 1, 2, 0, 0, 1, 2, 0, 0, 1, 2, 0, 0, 1, 2), (0, 1, 1, 2, 0, 1, 1, 2, 0, 1, 1, 2, 0, 1, 1, 2), (0, 1, 2, 2, 0, 1, 2, 2, 0, 1, 2, 2, 0, 1, 2, 2), (0, 0, 1, 1, 0, 1, 1, 2, 1, 1, 2, 2, 1, 2, 2, 2), (0, 0, 1, 1, 2, 0, 0, 1, 2, 2, 0, 0, 2, 2, 2, 0), (0, 0, 0, 1, 0, 0, 1, 1, 0, 1, 1, 2, 1, 1, 2, 2), (0, 1, 1, 1, 0, 0, 1, 1, 2, 0, 0, 1, 2, 2, 0, 0), (0, 0, 0, 0, 1, 1, 2, 2, 1, 1, 2, 2, 1, 1, 2, 2), (0, 0, 2, 2, 0, 0, 2, 2, 0, 0, 2, 2, 1, 1, 1, 1), (0, 1, 1, 1, 0, 1, 1, 1, 0, 2, 2, 2, 0, 2, 2, 2), (0, 0, 0, 1, 0, 0, 0, 1, 2, 2, 2, 1, 2, 2, 2, 1), (0, 0, 0, 0, 0, 0, 1, 1, 0, 1, 2, 2, 0, 1, 2, 2), (0, 0, 0, 0, 1, 1, 0, 0, 2, 2, 1, 0, 2, 2, 1, 0), (0, 1, 2, 2, 0, 1, 2, 2, 0, 0, 1, 1, 0, 0, 0, 0), (0, 0, 1, 2, 0, 0, 1, 2, 1, 1, 2, 2, 2, 2, 2, 2), (0, 1, 1, 0, 1, 2, 2, 1, 1, 2, 2, 1, 0, 1, 1, 0), (0, 0, 0, 0, 0, 1, 1, 0, 1, 2, 2, 1, 1, 2, 2, 1), (0, 0, 2, 2, 1, 1, 0, 2, 1, 1, 0, 2, 0, 0, 2, 2), (0, 1, 1, 0, 0, 1, 1, 0, 2, 0, 0, 2, 2, 2, 2, 2), (0, 0, 1, 1, 0, 1, 2, 2, 0, 1, 2, 2, 0, 0, 1, 1), (0, 0, 0, 0, 2, 0, 0, 0, 2, 2, 1, 1, 2, 2, 2, 1), (0, 0, 0, 0, 0, 0, 0, 2, 1, 1, 2, 2, 1, 2, 2, 2), (0, 2, 2, 2, 0, 0, 2, 2, 0, 0, 1, 2, 0, 0, 1, 1), (0, 0, 1, 1, 0, 0, 1, 2, 0, 0, 2, 2, 0, 2, 2, 2), (0, 1, 2, 0, 0, 1, 2, 0, 0, 1, 2, 0, 0, 1, 2, 0), (0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 0, 0, 0, 0), (0, 1, 2, 0, 1, 2, 0, 1, 2, 0, 1, 2, 0, 1, 2, 0), (0, 1, 2, 0, 2, 0, 1, 2, 1, 2, 0, 1, 0, 1, 2, 0), (0, 0, 1, 1, 2, 2, 0, 0, 1, 1, 2, 2, 0, 0, 1, 1), (0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 0, 0, 0, 0, 1, 1), (0, 1, 0, 1, 0, 1, 0, 1, 2, 2, 2, 2, 2, 2, 2, 2), (0, 0, 0, 0, 0, 0, 0, 0, 2, 1, 2, 1, 2, 1, 2, 1), (0, 0, 2, 2, 1, 1, 2, 2, 0, 0, 2, 2, 1, 1, 2, 2), (0, 0, 2, 2, 0, 0, 1, 1, 0, 0, 2, 2, 0, 0, 1, 1), (0, 2, 2, 0, 1, 2, 2, 1, 0, 2, 2, 0, 1, 2, 2, 1), (0, 1, 0, 1, 2, 2, 2, 2, 2, 2, 2, 2, 0, 1, 0, 1), (0, 0, 0, 0, 2, 1, 2, 1, 2, 1, 2, 1, 2, 1, 2, 1), (0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 2, 2, 2, 2), (0, 2, 2, 2, 0, 1, 1, 1, 0, 2, 2, 2, 0, 1, 1, 1), (0, 0, 0, 2, 1, 1, 1, 2, 0, 0, 0, 2, 1, 1, 1, 2), (0, 0, 0, 0, 2, 1, 1, 2, 2, 1, 1, 2, 2, 1, 1, 2), (0, 2, 2, 2, 0, 1, 1, 1, 0, 1, 1, 1, 0, 2, 2, 2), (0, 0, 0, 2, 1, 1, 1, 2, 1, 1, 1, 2, 0, 0, 0, 2), (0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0, 2, 2, 2, 2), (0, 0, 0, 0, 0, 0, 0, 0, 2, 1, 1, 2, 2, 1, 1, 2), (0, 1, 1, 0, 0, 1, 1, 0, 2, 2, 2, 2, 2, 2, 2, 2), (0, 0, 2, 2, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 2, 2), (0, 0, 2, 2, 1, 1, 2, 2, 1, 1, 2, 2, 0, 0, 2, 2), (0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 1, 1, 2), (0, 0, 0, 2, 0, 0, 0, 1, 0, 0, 0, 2, 0, 0, 0, 1), (0, 2, 2, 2, 1, 2, 2, 2, 0, 2, 2, 2, 1, 2, 2, 2), (0, 1, 0, 1, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2), (0, 1, 1, 1, 2, 0, 1, 1, 2, 2, 0, 1, 2, 2, 2, 0))

_ANCHOR_2 = (15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 2, 8, 2, 2, 8, 8, 15, 2, 8, 2, 2, 8, 8, 2, 2, 15, 15, 6, 8, 2, 8, 15, 15, 2, 8, 2, 2, 2, 15, 15, 6, 6, 2, 6, 8, 15, 15, 2, 2, 15, 15, 15, 15, 15, 2, 2, 15)

_ANCHOR_3_1 = (3, 3, 15, 15, 8, 3, 15, 15, 8, 8, 6, 6, 6, 5, 3, 3, 3, 3, 8, 15, 3, 3, 6, 10, 5, 8, 8, 6, 8, 5, 15, 15, 8, 15, 3, 5, 6, 10, 8, 15, 15, 3, 15, 5, 15, 15, 15, 15, 3, 15, 5, 5, 5, 8, 5, 10, 5, 10, 8, 13, 15, 12, 3, 3)

_ANCHOR_3_2 = (15, 8, 8, 3, 15, 15, 3, 8, 15, 15, 15, 15, 15, 15, 15, 8, 15, 8, 15, 3, 15, 8, 15, 8, 3, 15, 6, 10, 15, 15, 10, 8, 15, 3, 15, 10, 10, 8, 9, 10, 6, 15, 8, 15, 3, 6, 6, 8, 15, 3, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 3, 15, 15, 8)

_VOID = bytes(64)

class _Bits:
    __slots__ = ('_pos', '_value')

    def __init__(self, value: int, start: int) -> None:
        self._value = value
        self._pos = start

    def take(self, count: int) -> int:
        if count <= 0:
            return 0
        out = self._value >> self._pos & (1 << count) - 1
        self._pos += count
        return out

def _unquantise(value: int, bits: int) -> int:
    if bits >= 8:
        return value
    return (value << 8 - bits | value >> 2 * bits - 8) & 255

def _interpolate(first: int, second: int, weight: int) -> int:
    return (64 - weight) * first + weight * second + 32 >> 6

def _anchors(subsets: int, partition: int) -> tuple[int, ...]:
    if subsets == 1:
        return (0,)
    if subsets == 2:
        return (0, _ANCHOR_2[partition])
    return (0, _ANCHOR_3_1[partition], _ANCHOR_3_2[partition])

def _read_endpoints(bits: _Bits, mode: int) -> list[list[int]]:
    count = _SUBSETS[mode] * 2
    (color_bits, alpha_bits) = (_COLOUR_BITS[mode], _ALPHA_BITS[mode])
    planes = [[bits.take(color_bits) for _ in range(count)] for _ in range(3)]
    planes.append([bits.take(alpha_bits) for _ in range(count)] if alpha_bits else [0] * count)
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
                raw = raw << 1 | parity[index]
            channels.append(_unquantise(raw, color_width))
        if not alpha_bits:
            channels.append(255)
        else:
            raw = planes[3][index]
            if parity is not None:
                raw = raw << 1 | parity[index]
            channels.append(_unquantise(raw, alpha_width))
        endpoints.append(channels)
    return endpoints

def _read_indices(bits: _Bits, width: int, anchors: tuple[int, ...], subset_of: list[int] | None) -> list[int]:
    del subset_of
    short = set(anchors)
    return [bits.take(width - 1 if pixel in short else width) for pixel in range(16)]

def decode_block(block: bytes) -> bytes:
    if len(block) != BLOCK_BYTES:
        raise ValueError(f'a BC7 block is {BLOCK_BYTES} bytes, got {len(block)}')
    value = int.from_bytes(block, 'little')
    mode = -1
    for candidate in range(8):
        if value >> candidate & 1:
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
    (width_1, width_2) = (_INDEX_BITS[mode], _INDEX_BITS_2[mode])
    indices_1 = _read_indices(bits, width_1, anchors, None)
    indices_2 = _read_indices(bits, width_2, (0,), None) if width_2 else None
    if indices_2 is None:
        (color_idx, color_w) = (indices_1, width_1)
        (alpha_idx, alpha_w) = (indices_1, width_1)
    elif selector:
        (color_idx, color_w) = (indices_2, width_2)
        (alpha_idx, alpha_w) = (indices_1, width_1)
    else:
        (color_idx, color_w) = (indices_1, width_1)
        (alpha_idx, alpha_w) = (indices_2, width_2)
    color_weights = _WEIGHTS[color_w]
    alpha_weights = _WEIGHTS[alpha_w]
    out = bytearray(64)
    for pixel in range(16):
        subset = membership[pixel]
        (low, high) = (endpoints[subset * 2], endpoints[subset * 2 + 1])
        weight = color_weights[color_idx[pixel]]
        red = _interpolate(low[0], high[0], weight)
        green = _interpolate(low[1], high[1], weight)
        blue = _interpolate(low[2], high[2], weight)
        alpha = _interpolate(low[3], high[3], alpha_weights[alpha_idx[pixel]])
        if rotation == 1:
            (red, alpha) = (alpha, red)
        elif rotation == 2:
            (green, alpha) = (alpha, green)
        elif rotation == 3:
            (blue, alpha) = (alpha, blue)
        at = pixel * 4
        out[at:at + 4] = bytes((red, green, blue, alpha))
    return bytes(out)

def decode_surface(data: bytes, width: int, height: int) -> bytearray:
    out = bytearray(width * height * 4)
    offset = 0
    for block_y in range(0, height, 4):
        for block_x in range(0, width, 4):
            if offset + BLOCK_BYTES > len(data):
                raise ValueError(f'BC7 data ends early: wanted {BLOCK_BYTES} byte(s) at {offset}, file holds {len(data) - offset}')
            pixels = decode_block(data[offset:offset + BLOCK_BYTES])
            offset += BLOCK_BYTES
            columns = min(4, width - block_x)
            for row in range(min(4, height - block_y)):
                start = ((block_y + row) * width + block_x) * 4
                source = row * 16
                out[start:start + columns * 4] = pixels[source:source + columns * 4]
    (lambda *a: None)('decoded %dx%d BC7 surface', width, height)
    return out

_HEADER_SIZE = 18

_COLOUR_MAPPED = 1

_TRUE_COLOUR = 2

_GREYSCALE = 3

_RLE_COLOUR_MAPPED = 9

_RLE_TRUE_COLOUR = 10

_RLE_GREYSCALE = 11

_RLE_TYPES = frozenset({_RLE_COLOUR_MAPPED, _RLE_TRUE_COLOUR, _RLE_GREYSCALE})

_MAPPED_TYPES = frozenset({_COLOUR_MAPPED, _RLE_COLOUR_MAPPED})

_TOP_ORIGIN = 32

_RIGHT_ORIGIN = 16

_MAX_PIXELS = 64 << 20

def _unpack_pixel(raw: bytes, depth: int) -> bytes:
    if depth == 32:
        (blue, green, red, alpha) = (raw[0], raw[1], raw[2], raw[3])
        return bytes((red, green, blue, alpha))
    if depth == 24:
        return bytes((raw[2], raw[1], raw[0], 255))
    if depth == 16:
        packed = raw[0] | raw[1] << 8
        red = packed >> 10 & 31
        green = packed >> 5 & 31
        blue = packed & 31
        return bytes((red << 3 | red >> 2, green << 3 | green >> 2, blue << 3 | blue >> 2, 255))
    if depth == 8:
        return bytes((raw[0], raw[0], raw[0], 255))
    raise ValueError(f'unsupported Targa depth: {depth} bits per pixel')

def _read_color_map(data: bytes, offset: int, length: int, depth: int) -> list[bytes]:
    step = (depth + 7) // 8
    if offset + length * step > len(data):
        raise ValueError('Targa color map runs past the end of the file')
    return [_unpack_pixel(data[offset + index * step:offset + index * step + step], depth) for index in range(length)]

def _decode_plain(data: bytes, offset: int, count: int, step: int) -> list[bytes]:
    if offset + count * step > len(data):
        raise ValueError(f'Targa pixel data ends early: wanted {count * step} byte(s), file holds {len(data) - offset}')
    return [data[offset + index * step:offset + index * step + step] for index in range(count)]

def _decode_rle(data: bytes, offset: int, count: int, step: int) -> list[bytes]:
    pixels: list[bytes] = []
    position = offset
    while len(pixels) < count:
        if position >= len(data):
            raise ValueError(f'Targa run-length data ends early: {len(pixels)} of {count} pixel(s) decoded')
        control = data[position]
        position += 1
        run = (control & 127) + 1
        if control & 128:
            if position + step > len(data):
                raise ValueError('Targa run packet is truncated')
            pixel = data[position:position + step]
            position += step
            pixels.extend([pixel] * min(run, count - len(pixels)))
            continue
        if position + run * step > len(data):
            raise ValueError('Targa literal packet is truncated')
        pixels.extend((data[position + index * step:position + index * step + step] for index in range(min(run, count - len(pixels)))))
        position += run * step
    return pixels

def read_tga(data: bytes) -> tuple[int, int, bytes]:
    if len(data) < _HEADER_SIZE:
        raise ValueError('too short to be a Targa image')
    (id_length, has_map, image_type, _map_first, map_length, map_depth, _x_origin, _y_origin, width, height, depth, descriptor) = struct.unpack_from('<BBBHHBHHHHBB', data, 0)
    if width <= 0 or height <= 0:
        raise ValueError(f'implausible dimensions {width}x{height}')
    if width * height > _MAX_PIXELS:
        raise ValueError(f'implausible size: {width}x{height} is {width * height} pixel(s)')
    if image_type not in {_COLOUR_MAPPED, _TRUE_COLOUR, _GREYSCALE, _RLE_COLOUR_MAPPED, _RLE_TRUE_COLOUR, _RLE_GREYSCALE}:
        raise ValueError(f'unsupported Targa image type {image_type}')
    offset = _HEADER_SIZE + id_length
    palette: list[bytes] = []
    if has_map or image_type in _MAPPED_TYPES:
        palette = _read_color_map(data, offset, map_length, map_depth)
        offset += map_length * ((map_depth + 7) // 8)
    if image_type in _MAPPED_TYPES and (not palette):
        raise ValueError('color-mapped Targa carries no color map')
    step = (depth + 7) // 8
    if step < 1:
        raise ValueError(f'unsupported Targa depth: {depth} bits per pixel')
    count = width * height
    stored = _decode_rle(data, offset, count, step) if image_type in _RLE_TYPES else _decode_plain(data, offset, count, step)
    if image_type in _MAPPED_TYPES:
        rgba = [_palette_lookup(palette, entry) for entry in stored]
    else:
        rgba = [_unpack_pixel(entry, depth) for entry in stored]
    return (width, height, bytes(_orient(rgba, width, height, descriptor)))

def _palette_lookup(palette: list[bytes], entry: bytes) -> bytes:
    index = int.from_bytes(entry, 'little')
    if not 0 <= index < len(palette):
        raise ValueError(f'color-map index {index} is outside a map of {len(palette)}')
    return palette[index]

def _orient(rgba: list[bytes], width: int, height: int, descriptor: int) -> bytearray:
    out = bytearray(width * height * 4)
    top_down = bool(descriptor & _TOP_ORIGIN)
    right_to_left = bool(descriptor & _RIGHT_ORIGIN)
    for row in range(height):
        target_row = row if top_down else height - 1 - row
        source = row * width
        start = target_row * width * 4
        if right_to_left:
            for column in range(width):
                at = start + (width - 1 - column) * 4
                out[at:at + 4] = rgba[source + column]
            continue
        out[start:start + width * 4] = b''.join(rgba[source:source + width])
    return out

_SAME = 2

def _measure(left: bytes, right: bytes) -> tuple[int, int, int]:
    changed = 0
    worst = 0
    total = 0
    for start in range(0, len(left), 4):
        biggest = 0
        for offset in range(4):
            gap = left[start + offset] - right[start + offset]
            if gap < 0:
                gap = -gap
            total += gap
            if gap > biggest:
                biggest = gap
        if biggest > _SAME:
            changed += 1
        if biggest > worst:
            worst = biggest
    return (changed, worst, total)
