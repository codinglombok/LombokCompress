"""Deflate/gzip/zlib decompression — fixed Huffman, pure Python."""

from __future__ import annotations

import struct

from lombokcompress.error import CompressError, CompressErrorCode
from lombokcompress.deflate.compress import crc32, adler32

# Length base values (codes 257-285)
_LENGTH_BASE = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13,
    15, 17, 19, 23, 27, 31, 35, 43, 51, 59,
    67, 83, 99, 115, 131, 163, 195, 227, 258,
]

_LENGTH_EXTRA = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1,
    1, 1, 2, 2, 2, 2, 3, 3, 3, 3,
    4, 4, 4, 4, 5, 5, 5, 5, 0,
]

# Distance base values (codes 0-29)
_DIST_BASE = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25,
    33, 49, 65, 97, 129, 193, 257, 385, 513, 769,
    1025, 1537, 2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
]

_DIST_EXTRA = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3,
    4, 4, 5, 5, 6, 6, 7, 7, 8, 8,
    9, 9, 10, 10, 11, 11, 12, 12, 13, 13,
]


class _BitReader:
    """Reads bits LSB-first from a byte buffer."""

    def __init__(self, data: bytes) -> None:
        self.data = data
        self.pos = 0
        self.bit_buf = 0
        self.bit_count = 0

    def read_bits(self, count: int) -> int:
        while self.bit_count < count:
            if self.pos >= len(self.data):
                raise CompressError(
                    CompressErrorCode.UNEXPECTED_EOF, "unexpected end of deflate data"
                )
            self.bit_buf |= self.data[self.pos] << self.bit_count
            self.pos += 1
            self.bit_count += 8
        value = self.bit_buf & ((1 << count) - 1)
        self.bit_buf >>= count
        self.bit_count -= count
        return value

    @property
    def bytes_read(self) -> int:
        return self.pos


def _decode_fixed_literal(br: _BitReader) -> int:
    """Decode one literal/length symbol from fixed Huffman codes."""
    # Read 7 bits first
    code = 0
    for i in range(7):
        code = (code << 1) | br.read_bits(1)

    # 7-bit codes: 256-279 (codes 0000000-0010111 = 0-23 -> values 256-279)
    if code <= 23:
        return 256 + code

    # Read 8th bit
    code = (code << 1) | br.read_bits(1)

    # 8-bit codes: 0-143 (codes 00110000-10111111 = 48-191)
    if 48 <= code <= 191:
        return code - 48

    # 8-bit codes: 280-287 (codes 11000000-11000111 = 192-199)
    if 192 <= code <= 199:
        return 280 + code - 192

    # Read 9th bit
    code = (code << 1) | br.read_bits(1)

    # 9-bit codes: 144-255 (codes 110010000-111111111 = 400-511)
    if 400 <= code <= 511:
        return 144 + code - 400

    raise CompressError(
        CompressErrorCode.INVALID_INPUT, f"invalid fixed Huffman code {code}"
    )


def _decode_fixed_distance(br: _BitReader) -> int:
    """Decode one distance symbol from fixed Huffman codes (5-bit)."""
    code = 0
    for _ in range(5):
        code = (code << 1) | br.read_bits(1)
    return code


# Default output cap for gzip/zlib, matching the Rust core (64 MiB).
DEFAULT_MAX_OUTPUT = 64 * 1024 * 1024


def _too_large(max_output: int) -> CompressError:
    return CompressError(
        CompressErrorCode.OUTPUT_TOO_SMALL,
        f"decompressed data exceeds limit {max_output}",
    )


def deflate_decompress(
    data: bytes | bytearray | memoryview, max_output: int | None = None
) -> bytes:
    """Decompress raw deflate data.

    ``max_output`` bounds the decompressed size; set it for untrusted input.
    """
    br = _BitReader(bytes(data))
    output = bytearray()
    limit = max_output if max_output is not None else float("inf")

    while True:
        bfinal = br.read_bits(1)
        btype = br.read_bits(2)

        if btype == 0:
            # Stored block
            # Align to byte boundary
            br.bit_buf = 0
            br.bit_count = 0

            if br.pos + 4 > len(br.data):
                raise CompressError(
                    CompressErrorCode.UNEXPECTED_EOF,
                    "stored block header extends past input",
                )
            length = br.data[br.pos] | (br.data[br.pos + 1] << 8)
            nlength = br.data[br.pos + 2] | (br.data[br.pos + 3] << 8)
            br.pos += 4

            if length != (~nlength & 0xFFFF):
                raise CompressError(
                    CompressErrorCode.INVALID_INPUT,
                    "stored block length check failed",
                )

            if br.pos + length > len(br.data):
                raise CompressError(
                    CompressErrorCode.UNEXPECTED_EOF,
                    "stored block data extends past input",
                )
            if len(output) + length > limit:
                raise _too_large(max_output)
            output.extend(br.data[br.pos : br.pos + length])
            br.pos += length

        elif btype == 1:
            # Fixed Huffman
            while True:
                sym = _decode_fixed_literal(br)

                if sym < 256:
                    if len(output) >= limit:
                        raise _too_large(max_output)
                    output.append(sym)
                elif sym == 256:
                    break
                else:
                    # Length
                    li = sym - 257
                    if li >= len(_LENGTH_BASE):
                        raise CompressError(
                            CompressErrorCode.INVALID_INPUT,
                            f"invalid length code {sym}",
                        )
                    length = _LENGTH_BASE[li]
                    if _LENGTH_EXTRA[li] > 0:
                        length += br.read_bits(_LENGTH_EXTRA[li])

                    # Distance
                    dist_code = _decode_fixed_distance(br)
                    if dist_code >= len(_DIST_BASE):
                        raise CompressError(
                            CompressErrorCode.INVALID_INPUT,
                            f"invalid distance code {dist_code}",
                        )
                    distance = _DIST_BASE[dist_code]
                    if _DIST_EXTRA[dist_code] > 0:
                        distance += br.read_bits(_DIST_EXTRA[dist_code])

                    # Copy match
                    start = len(output) - distance
                    if start < 0:
                        raise CompressError(
                            CompressErrorCode.INVALID_INPUT,
                            "distance beyond output buffer",
                        )
                    if len(output) + length > limit:
                        raise _too_large(max_output)
                    for i in range(length):
                        output.append(output[start + i])

        elif btype == 2:
            raise CompressError(
                CompressErrorCode.UNSUPPORTED,
                "dynamic Huffman not yet implemented",
            )
        else:
            raise CompressError(
                CompressErrorCode.INVALID_INPUT, "reserved deflate block type"
            )

        if bfinal:
            break

    return bytes(output)


def gzip_decompress(
    data: bytes | bytearray | memoryview, max_output: int = DEFAULT_MAX_OUTPUT
) -> bytes:
    """Decompress gzip format data (RFC 1952)."""
    src = bytes(data)
    if len(src) < 18:
        raise CompressError(
            CompressErrorCode.UNEXPECTED_EOF, "input too short for gzip"
        )

    if src[0] != 0x1F or src[1] != 0x8B:
        raise CompressError(
            CompressErrorCode.INVALID_INPUT, "invalid gzip magic number"
        )

    if src[2] != 0x08:
        raise CompressError(
            CompressErrorCode.UNSUPPORTED, "unsupported gzip compression method"
        )

    flags = src[3]
    pos = 10

    # FEXTRA
    if flags & 0x04:
        if pos + 2 > len(src):
            raise CompressError(CompressErrorCode.UNEXPECTED_EOF, "missing FEXTRA")
        xlen = src[pos] | (src[pos + 1] << 8)
        pos += 2 + xlen

    # FNAME
    if flags & 0x08:
        while pos < len(src) and src[pos] != 0:
            pos += 1
        pos += 1  # skip null terminator

    # FCOMMENT
    if flags & 0x10:
        while pos < len(src) and src[pos] != 0:
            pos += 1
        pos += 1

    # FHCRC
    if flags & 0x02:
        pos += 2

    if pos >= len(src) - 8:
        raise CompressError(
            CompressErrorCode.UNEXPECTED_EOF, "no compressed data in gzip"
        )

    # Decompress
    compressed_data = src[pos:-8]
    decompressed = deflate_decompress(compressed_data, max_output)

    # Verify CRC32 and size
    if len(src) < 8:
        raise CompressError(
            CompressErrorCode.UNEXPECTED_EOF, "missing gzip trailer"
        )

    trailer = src[-8:]
    expected_crc = struct.unpack("<I", trailer[0:4])[0]
    expected_size = struct.unpack("<I", trailer[4:8])[0]

    actual_crc = crc32(decompressed)
    if actual_crc != expected_crc:
        raise CompressError(
            CompressErrorCode.CHECKSUM_MISMATCH, "gzip CRC32 mismatch"
        )

    if (len(decompressed) & 0xFFFFFFFF) != expected_size:
        raise CompressError(
            CompressErrorCode.INVALID_INPUT, "gzip size mismatch"
        )

    return decompressed


def zlib_decompress(
    data: bytes | bytearray | memoryview, max_output: int = DEFAULT_MAX_OUTPUT
) -> bytes:
    """Decompress zlib format data (RFC 1950)."""
    src = bytes(data)
    if len(src) < 6:
        raise CompressError(
            CompressErrorCode.UNEXPECTED_EOF, "input too short for zlib"
        )

    cmf = src[0]
    flg = src[1]

    if (cmf * 256 + flg) % 31 != 0:
        raise CompressError(
            CompressErrorCode.INVALID_INPUT, "invalid zlib header checksum"
        )

    cm = cmf & 0x0F
    if cm != 8:
        raise CompressError(
            CompressErrorCode.UNSUPPORTED, "unsupported zlib compression method"
        )

    if flg & 0x20:
        raise CompressError(CompressErrorCode.UNSUPPORTED, "zlib preset dictionary")

    # Decompress (exclude 4-byte Adler-32 trailer)
    compressed_data = src[2:-4]
    decompressed = deflate_decompress(compressed_data, max_output)

    # Verify Adler-32 (big-endian)
    expected_adler = struct.unpack(">I", src[-4:])[0]
    actual_adler = adler32(decompressed)
    if actual_adler != expected_adler:
        raise CompressError(
            CompressErrorCode.CHECKSUM_MISMATCH, "zlib Adler-32 mismatch"
        )

    return decompressed
