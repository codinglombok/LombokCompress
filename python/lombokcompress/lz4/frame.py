"""LZ4 frame format (v1.6.3) — compress and decompress."""

from __future__ import annotations

from lombokcompress.error import CompressError, CompressErrorCode
from lombokcompress.lz4.xxhash import xxh32
from lombokcompress.lz4.block import compress_block, decompress_block

_LZ4_MAGIC = 0x184D2204

_MAX_BLOCK_SIZES = {
    4: 64 * 1024,
    5: 256 * 1024,
    6: 1024 * 1024,
    7: 4 * 1024 * 1024,
}


class FrameOptions:
    """Options for LZ4 frame compression."""

    def __init__(
        self,
        *,
        content_checksum: bool = False,
        content_size: bool = False,
        block_checksum: bool = False,
        max_block_size: int = 7,
    ) -> None:
        self.content_checksum = content_checksum
        self.content_size = content_size
        self.block_checksum = block_checksum
        if max_block_size not in _MAX_BLOCK_SIZES:
            raise CompressError(
                CompressErrorCode.INVALID_INPUT,
                f"invalid max_block_size {max_block_size}, must be 4-7",
            )
        self.max_block_size = max_block_size


def compress_frame(
    data: bytes | bytearray | memoryview,
    options: FrameOptions | None = None,
) -> bytes:
    """Compress data into an LZ4 frame."""
    if options is None:
        options = FrameOptions()

    src = bytes(data)
    output = bytearray()

    # Magic number (LE32)
    output.append(_LZ4_MAGIC & 0xFF)
    output.append((_LZ4_MAGIC >> 8) & 0xFF)
    output.append((_LZ4_MAGIC >> 16) & 0xFF)
    output.append((_LZ4_MAGIC >> 24) & 0xFF)

    # Frame descriptor
    flg = 0x40  # version = 01 (bits 7-6)
    if options.content_checksum:
        flg |= 0x04
    if options.content_size:
        flg |= 0x08
    flg |= (options.max_block_size & 0x07) << 4  # Wait, BD is separate

    # Actually: FLG byte then BD byte
    flg_byte = 0x40  # version = 01
    if options.content_checksum:
        flg_byte |= 0x04
    if options.content_size:
        flg_byte |= 0x08

    bd_byte = (options.max_block_size & 0x07) << 4

    header_bytes = bytearray([flg_byte, bd_byte])

    if options.content_size:
        size = len(src)
        for _ in range(8):
            header_bytes.append(size & 0xFF)
            size >>= 8

    # Header checksum: XXH32 of header bytes (FLG+BD+optional), second byte of hash
    hc = (xxh32(bytes(header_bytes), 0) >> 8) & 0xFF

    output.extend(header_bytes)
    output.append(hc)

    # Blocks
    max_bs = _MAX_BLOCK_SIZES[options.max_block_size]
    pos = 0

    while pos < len(src):
        chunk_size = min(len(src) - pos, max_bs)
        chunk = src[pos : pos + chunk_size]

        compressed = compress_block(chunk)

        if len(compressed) < len(chunk):
            # Compressed block
            block_len = len(compressed)
            output.append(block_len & 0xFF)
            output.append((block_len >> 8) & 0xFF)
            output.append((block_len >> 16) & 0xFF)
            output.append((block_len >> 24) & 0xFF)
            output.extend(compressed)
        else:
            # Uncompressed block (high bit set)
            block_len = len(chunk) | 0x80000000
            output.append(block_len & 0xFF)
            output.append((block_len >> 8) & 0xFF)
            output.append((block_len >> 16) & 0xFF)
            output.append((block_len >> 24) & 0xFF)
            output.extend(chunk)

        if options.block_checksum:
            block_data = compressed if len(compressed) < len(chunk) else chunk
            bc = xxh32(block_data, 0)
            output.append(bc & 0xFF)
            output.append((bc >> 8) & 0xFF)
            output.append((bc >> 16) & 0xFF)
            output.append((bc >> 24) & 0xFF)

        pos += chunk_size

    # EndMark (4 zero bytes)
    output.extend(b"\x00\x00\x00\x00")

    # Content checksum
    if options.content_checksum:
        cc = xxh32(src, 0)
        output.append(cc & 0xFF)
        output.append((cc >> 8) & 0xFF)
        output.append((cc >> 16) & 0xFF)
        output.append((cc >> 24) & 0xFF)

    return bytes(output)


def decompress_frame(data: bytes | bytearray | memoryview) -> bytes:
    """Decompress an LZ4 frame."""
    src = bytes(data)
    if len(src) < 7:
        raise CompressError(
            CompressErrorCode.UNEXPECTED_EOF, "input too short for LZ4 frame"
        )

    pos = 0

    # Magic number
    magic = (
        src[0] | (src[1] << 8) | (src[2] << 16) | ((src[3] << 24) & 0xFFFFFFFF)
    )
    if magic != _LZ4_MAGIC:
        raise CompressError(
            CompressErrorCode.INVALID_INPUT, "invalid LZ4 frame magic number"
        )
    pos = 4

    # Frame descriptor
    flg = src[pos]
    bd = src[pos + 1]
    header_start = pos
    pos += 2

    content_checksum = (flg & 0x04) != 0
    has_content_size = (flg & 0x08) != 0
    block_checksum = (flg & 0x10) != 0

    content_size = None
    if has_content_size:
        if pos + 8 > len(src):
            raise CompressError(
                CompressErrorCode.UNEXPECTED_EOF, "missing content size"
            )
        content_size = 0
        for i in range(8):
            content_size |= src[pos + i] << (i * 8)
        pos += 8

    # Verify header checksum
    header_data = src[header_start:pos]
    expected_hc = (xxh32(header_data, 0) >> 8) & 0xFF
    if pos >= len(src):
        raise CompressError(
            CompressErrorCode.UNEXPECTED_EOF, "missing header checksum"
        )
    actual_hc = src[pos]
    pos += 1

    if actual_hc != expected_hc:
        raise CompressError(
            CompressErrorCode.CHECKSUM_MISMATCH, "LZ4 frame header checksum mismatch"
        )

    # Read blocks
    output = bytearray()

    while True:
        if pos + 4 > len(src):
            raise CompressError(
                CompressErrorCode.UNEXPECTED_EOF, "missing block header"
            )

        block_size = (
            src[pos]
            | (src[pos + 1] << 8)
            | (src[pos + 2] << 16)
            | ((src[pos + 3] << 24) & 0xFFFFFFFF)
        )
        pos += 4

        if block_size == 0:
            break  # EndMark

        is_uncompressed = (block_size & 0x80000000) != 0
        actual_size = block_size & 0x7FFFFFFF

        if pos + actual_size > len(src):
            raise CompressError(
                CompressErrorCode.UNEXPECTED_EOF, "block data extends past input"
            )

        block_data = src[pos : pos + actual_size]
        pos += actual_size

        if is_uncompressed:
            output.extend(block_data)
        else:
            # Need to know uncompressed size — use content_size or decompress
            # LZ4 block decompression needs uncompressed_size, estimate with content_size
            # or decompress adaptively
            decompressed = _decompress_block_adaptive(block_data)
            output.extend(decompressed)

        if block_checksum:
            if pos + 4 > len(src):
                raise CompressError(
                    CompressErrorCode.UNEXPECTED_EOF, "missing block checksum"
                )
            expected_bc = (
                src[pos]
                | (src[pos + 1] << 8)
                | (src[pos + 2] << 16)
                | ((src[pos + 3] << 24) & 0xFFFFFFFF)
            )
            actual_bc = xxh32(block_data, 0)
            if actual_bc != expected_bc:
                raise CompressError(
                    CompressErrorCode.CHECKSUM_MISMATCH,
                    "LZ4 block checksum mismatch",
                )
            pos += 4

    # Content checksum
    if content_checksum:
        if pos + 4 > len(src):
            raise CompressError(
                CompressErrorCode.UNEXPECTED_EOF, "missing content checksum"
            )
        expected_cc = (
            src[pos]
            | (src[pos + 1] << 8)
            | (src[pos + 2] << 16)
            | ((src[pos + 3] << 24) & 0xFFFFFFFF)
        )
        actual_cc = xxh32(bytes(output), 0)
        if actual_cc != expected_cc:
            raise CompressError(
                CompressErrorCode.CHECKSUM_MISMATCH,
                "LZ4 content checksum mismatch",
            )

    result = bytes(output)

    if content_size is not None and len(result) != content_size:
        raise CompressError(
            CompressErrorCode.INVALID_INPUT,
            f"decompressed size {len(result)} != content size {content_size}",
        )

    return result


def _decompress_block_adaptive(data: bytes) -> bytes:
    """Decompress LZ4 block without knowing uncompressed size upfront."""
    src = data
    src_len = len(src)
    if src_len == 0:
        return b""

    output = bytearray()
    pos = 0

    while pos < src_len:
        token = src[pos]
        pos += 1
        lit_len = token >> 4

        if lit_len == 15:
            while True:
                if pos >= src_len:
                    raise CompressError(
                        CompressErrorCode.UNEXPECTED_EOF,
                        "unexpected end reading literal length",
                    )
                extra = src[pos]
                pos += 1
                lit_len += extra
                if extra != 255:
                    break

        if pos + lit_len > src_len:
            raise CompressError(
                CompressErrorCode.UNEXPECTED_EOF, "literal data extends past input"
            )
        output.extend(src[pos : pos + lit_len])
        pos += lit_len

        if pos >= src_len:
            break

        if pos + 2 > src_len:
            raise CompressError(
                CompressErrorCode.UNEXPECTED_EOF, "missing match offset"
            )
        offset = src[pos] | (src[pos + 1] << 8)
        pos += 2

        if offset == 0:
            raise CompressError(
                CompressErrorCode.INVALID_INPUT, "zero match offset"
            )

        match_len = (token & 0x0F) + 4
        if (token & 0x0F) == 15:
            while True:
                if pos >= src_len:
                    raise CompressError(
                        CompressErrorCode.UNEXPECTED_EOF,
                        "unexpected end reading match length",
                    )
                extra = src[pos]
                pos += 1
                match_len += extra
                if extra != 255:
                    break

        match_start = len(output) - offset
        if match_start < 0:
            raise CompressError(
                CompressErrorCode.INVALID_INPUT, "match offset beyond output"
            )

        for i in range(match_len):
            output.append(output[match_start + i])

    return bytes(output)
