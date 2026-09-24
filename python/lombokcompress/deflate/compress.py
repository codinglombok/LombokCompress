"""Deflate/gzip/zlib compression — fixed Huffman, LZ77, pure Python."""

from __future__ import annotations

import struct

from lombokcompress.error import CompressError, CompressErrorCode

# Pre-computed CRC32 table
_CRC32_TABLE: list[int] = []
for _n in range(256):
    _c = _n
    for _ in range(8):
        if _c & 1:
            _c = 0xEDB88320 ^ (_c >> 1)
        else:
            _c >>= 1
    _CRC32_TABLE.append(_c & 0xFFFFFFFF)


def crc32(data: bytes | bytearray | memoryview) -> int:
    """Compute CRC32 checksum."""
    crc = 0xFFFFFFFF
    for b in data:
        crc = _CRC32_TABLE[(crc ^ b) & 0xFF] ^ (crc >> 8)
    return crc ^ 0xFFFFFFFF


def adler32(data: bytes | bytearray | memoryview) -> int:
    """Compute Adler-32 checksum."""
    a = 1
    b = 0
    for byte in data:
        a = (a + byte) % 65521
        b = (b + a) % 65521
    return ((b << 16) | a) & 0xFFFFFFFF


class _BitWriter:
    """Writes bits LSB-first into a byte buffer."""

    def __init__(self) -> None:
        self.buffer = bytearray()
        self.bit_buf = 0
        self.bit_count = 0

    def write_bits(self, value: int, count: int) -> None:
        self.bit_buf |= (value & ((1 << count) - 1)) << self.bit_count
        self.bit_count += count
        while self.bit_count >= 8:
            self.buffer.append(self.bit_buf & 0xFF)
            self.bit_buf >>= 8
            self.bit_count -= 8

    def flush(self) -> None:
        if self.bit_count > 0:
            self.buffer.append(self.bit_buf & 0xFF)
            self.bit_buf = 0
            self.bit_count = 0


def _reverse_bits(value: int, count: int) -> int:
    result = 0
    for _ in range(count):
        result = (result << 1) | (value & 1)
        value >>= 1
    return result


def _fixed_literal_code(lit: int) -> tuple[int, int]:
    """Return (code, bit_length) for a literal/length value using fixed Huffman."""
    if lit <= 143:
        return _reverse_bits(0x30 + lit, 8), 8
    elif lit <= 255:
        return _reverse_bits(0x190 + lit - 144, 9), 9
    elif lit <= 279:
        return _reverse_bits(lit - 256, 7), 7
    elif lit <= 287:
        return _reverse_bits(0xC0 + lit - 280, 8), 8
    else:
        raise CompressError(CompressErrorCode.INTERNAL_ERROR, f"invalid literal {lit}")


def _fixed_distance_code(dist: int) -> tuple[int, int]:
    """Return (code, bit_length) for a distance code using fixed Huffman."""
    return _reverse_bits(dist, 5), 5


# Length encoding tables (RFC 1951)
_LENGTH_TABLE: list[tuple[int, int, int]] = [
    # (min_length, code, extra_bits) — RFC 1951 Table
    (3, 257, 0), (4, 258, 0), (5, 259, 0), (6, 260, 0),
    (7, 261, 0), (8, 262, 0), (9, 263, 0), (10, 264, 0),
    (11, 265, 1), (13, 266, 1), (15, 267, 1), (17, 268, 1),
    (19, 269, 2), (23, 270, 2), (27, 271, 2), (31, 272, 2),
    (35, 273, 3), (43, 274, 3), (51, 275, 3), (59, 276, 3),
    (67, 277, 4), (83, 278, 4), (99, 279, 4), (115, 280, 4),
    (131, 281, 5), (163, 282, 5), (195, 283, 5), (227, 284, 5),
    (258, 285, 0),
]

# Distance encoding tables
_DISTANCE_TABLE: list[tuple[int, int, int]] = [
    # (min_distance, code, extra_bits)
    (1, 0, 0), (2, 1, 0), (3, 2, 0), (4, 3, 0),
    (5, 4, 1), (7, 5, 1), (9, 6, 2), (13, 7, 2),
    (17, 8, 3), (25, 9, 3), (33, 10, 4), (49, 11, 4),
    (65, 12, 5), (97, 13, 5), (129, 14, 6), (193, 15, 6),
    (257, 16, 7), (385, 17, 7), (513, 18, 8), (769, 19, 8),
    (1025, 20, 9), (1537, 21, 9), (2049, 22, 10), (3073, 23, 10),
    (4097, 24, 11), (6145, 25, 11), (8193, 26, 12), (12289, 27, 12),
    (16385, 28, 13), (24577, 29, 13),
]


def _encode_length(length: int) -> tuple[int, int, int]:
    """Encode a match length -> (code, extra_bits, extra_value)."""
    for i in range(len(_LENGTH_TABLE) - 1, -1, -1):
        min_len, code, extra = _LENGTH_TABLE[i]
        if length >= min_len:
            return code, extra, length - min_len
    raise CompressError(CompressErrorCode.INTERNAL_ERROR, f"invalid length {length}")


def _encode_distance(distance: int) -> tuple[int, int, int]:
    """Encode a match distance -> (code, extra_bits, extra_value)."""
    for i in range(len(_DISTANCE_TABLE) - 1, -1, -1):
        min_dist, code, extra = _DISTANCE_TABLE[i]
        if distance >= min_dist:
            return code, extra, distance - min_dist
    raise CompressError(
        CompressErrorCode.INTERNAL_ERROR, f"invalid distance {distance}"
    )


def _lz77_compress(
    data: bytes, window_size: int = 32768, max_chain: int = 64
) -> list[tuple[int] | tuple[int, int]]:
    """LZ77 compression with hash chains. Returns list of (literal,) or (length, distance)."""
    tokens: list[tuple[int] | tuple[int, int]] = []
    src_len = len(data)
    if src_len == 0:
        return tokens

    hash_table: dict[int, list[int]] = {}
    pos = 0

    def _hash3(p: int) -> int:
        if p + 2 >= src_len:
            return 0
        return data[p] | (data[p + 1] << 8) | (data[p + 2] << 16)

    while pos < src_len:
        if pos + 2 >= src_len:
            tokens.append((data[pos],))
            pos += 1
            continue

        h = _hash3(pos)
        chain = hash_table.get(h, [])

        best_len = 0
        best_dist = 0
        checks = 0

        for ref in reversed(chain):
            if pos - ref > window_size:
                break
            checks += 1
            if checks > max_chain:
                break

            # Compare
            ml = 0
            max_ml = min(258, src_len - pos)
            while ml < max_ml and data[pos + ml] == data[ref + ml]:
                ml += 1

            if ml > best_len:
                best_len = ml
                best_dist = pos - ref
                if ml >= 258:
                    break

        # Update hash chain
        if h not in hash_table:
            hash_table[h] = []
        hash_table[h].append(pos)

        if best_len >= 3:
            tokens.append((best_len, best_dist))
            # Add intermediate positions to hash
            for i in range(1, best_len):
                if pos + i + 2 < src_len:
                    ih = _hash3(pos + i)
                    if ih not in hash_table:
                        hash_table[ih] = []
                    hash_table[ih].append(pos + i)
            pos += best_len
        else:
            tokens.append((data[pos],))
            pos += 1

    return tokens


def deflate_compress(data: bytes | bytearray | memoryview) -> bytes:
    """Compress data using raw deflate (fixed Huffman)."""
    src = bytes(data)
    tokens = _lz77_compress(src)

    bw = _BitWriter()
    # BFINAL=1, BTYPE=01 (fixed Huffman)
    bw.write_bits(1, 1)  # BFINAL
    bw.write_bits(1, 2)  # BTYPE=01

    for token in tokens:
        if len(token) == 1:
            # Literal
            code, bits = _fixed_literal_code(token[0])
            bw.write_bits(code, bits)
        else:
            # Match (length, distance)
            length, distance = token
            len_code, len_extra_bits, len_extra_val = _encode_length(length)
            code, bits = _fixed_literal_code(len_code)
            bw.write_bits(code, bits)
            if len_extra_bits > 0:
                bw.write_bits(len_extra_val, len_extra_bits)

            dist_code, dist_extra_bits, dist_extra_val = _encode_distance(distance)
            dcode, dbits = _fixed_distance_code(dist_code)
            bw.write_bits(dcode, dbits)
            if dist_extra_bits > 0:
                bw.write_bits(dist_extra_val, dist_extra_bits)

    # End of block (256)
    code, bits = _fixed_literal_code(256)
    bw.write_bits(code, bits)
    bw.flush()

    return bytes(bw.buffer)


def gzip_compress(data: bytes | bytearray | memoryview) -> bytes:
    """Compress data using gzip format."""
    src = bytes(data)
    output = bytearray()

    # Gzip header
    output.extend(b"\x1f\x8b")  # magic
    output.append(0x08)  # method = deflate
    output.append(0x00)  # flags
    output.extend(b"\x00\x00\x00\x00")  # mtime
    output.append(0x00)  # xfl
    output.append(0xFF)  # OS = unknown

    # Compressed data
    compressed = deflate_compress(src)
    output.extend(compressed)

    # CRC32 + original size (LE32)
    checksum = crc32(src)
    output.extend(struct.pack("<I", checksum))
    output.extend(struct.pack("<I", len(src) & 0xFFFFFFFF))

    return bytes(output)


def zlib_compress(data: bytes | bytearray | memoryview) -> bytes:
    """Compress data using zlib format."""
    src = bytes(data)
    output = bytearray()

    # Zlib header
    cmf = 0x78  # CM=8 (deflate), CINFO=7 (32K window)
    flg = 0x01  # FCHECK so (CMF*256+FLG) % 31 == 0
    check = (cmf * 256 + flg) % 31
    if check != 0:
        flg += 31 - check

    output.append(cmf)
    output.append(flg)

    # Compressed data
    compressed = deflate_compress(src)
    output.extend(compressed)

    # Adler-32 (big-endian)
    checksum = adler32(src)
    output.extend(struct.pack(">I", checksum))

    return bytes(output)
