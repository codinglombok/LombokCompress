"""Zstandard compressor (levels 1-3, raw blocks) — pure Python."""

from __future__ import annotations

from typing import Literal

from lombokcompress.error import CompressError, CompressErrorCode

_ZSTD_MAGIC = 0xFD2FB528

ZstdLevel = Literal[1, 2, 3]


def is_zstd(data: bytes | bytearray | memoryview) -> bool:
    """Check if data starts with Zstd magic number."""
    if len(data) < 4:
        return False
    magic = (
        data[0] | (data[1] << 8) | (data[2] << 16) | ((data[3] << 24) & 0xFFFFFFFF)
    )
    return magic == _ZSTD_MAGIC


def zstd_compress(
    data: bytes | bytearray | memoryview, level: ZstdLevel = 1
) -> bytes:
    """Compress data using Zstandard (raw blocks)."""
    src = bytes(data)
    output = bytearray()

    # Magic number (LE)
    output.append(_ZSTD_MAGIC & 0xFF)
    output.append((_ZSTD_MAGIC >> 8) & 0xFF)
    output.append((_ZSTD_MAGIC >> 16) & 0xFF)
    output.append((_ZSTD_MAGIC >> 24) & 0xFF)

    content_size = len(src)

    # Frame header descriptor
    if content_size <= 255:
        output.append(0x20)  # FHD: Single_Segment=1, FCS=0 (1 byte)
        output.append(content_size)
    elif content_size <= 65535 + 256:
        output.append(0x60)  # FCS=01 (2 bytes)
        sz = content_size - 256
        output.append(sz & 0xFF)
        output.append((sz >> 8) & 0xFF)
    else:
        output.append(0xA0)  # FCS=10 (4 bytes)
        output.append(content_size & 0xFF)
        output.append((content_size >> 8) & 0xFF)
        output.append((content_size >> 16) & 0xFF)
        output.append((content_size >> 24) & 0xFF)

    if len(src) == 0:
        # Empty: one last raw block of size 0
        output.extend(b"\x01\x00\x00")
        return bytes(output)

    # Emit blocks
    max_block = 128 * 1024
    pos = 0

    while pos < len(src):
        remaining = len(src) - pos
        block_size = min(remaining, max_block)
        is_last = pos + block_size >= len(src)
        block_data = src[pos : pos + block_size]

        # Check RLE
        first = block_data[0]
        is_rle = all(b == first for b in block_data)

        if is_rle:
            # RLE block: type=1
            bh = (1 if is_last else 0) | (1 << 1) | (block_size << 3)
            output.append(bh & 0xFF)
            output.append((bh >> 8) & 0xFF)
            output.append((bh >> 16) & 0xFF)
            output.append(first)
        else:
            # Raw block: type=0
            bh = (1 if is_last else 0) | (0 << 1) | (block_size << 3)
            output.append(bh & 0xFF)
            output.append((bh >> 8) & 0xFF)
            output.append((bh >> 16) & 0xFF)
            output.extend(block_data)

        pos += block_size

    return bytes(output)
