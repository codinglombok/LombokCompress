"""LZ4 block compression/decompression — pure Python."""

from __future__ import annotations

from lombokcompress.error import CompressError, CompressErrorCode

_HASH_LOG = 12
_HASH_SIZE = 1 << _HASH_LOG
_MIN_MATCH = 4
_MF_LIMIT = 12
_LAST_LITERALS = 5
_MASK32 = 0xFFFFFFFF


def compress_bound(input_size: int) -> int:
    """Maximum compressed size for a given input size."""
    return input_size + (input_size // 255) + 16


def compress_block(data: bytes | bytearray | memoryview) -> bytes:
    """Compress data using LZ4 block format."""
    src = bytes(data)
    src_len = len(src)
    if src_len == 0:
        return b""

    output = bytearray()
    hash_table = [-1] * _HASH_SIZE
    pos = 0
    anchor = 0

    def _hash4(v: int) -> int:
        return ((v * 0x9E3779B1) >> (32 - _HASH_LOG)) & (_HASH_SIZE - 1)

    def _read32(buf: bytes, off: int) -> int:
        return (
            buf[off]
            | (buf[off + 1] << 8)
            | (buf[off + 2] << 16)
            | (buf[off + 3] << 24)
        )

    limit = src_len - _MF_LIMIT

    while pos < limit:
        cur_val = _read32(src, pos)
        h = _hash4(cur_val)
        ref = hash_table[h]
        hash_table[h] = pos

        if ref < 0 or ref < anchor or pos - ref > 65535 or _read32(src, ref) != cur_val:
            pos += 1
            continue

        # Emit literals
        lit_len = pos - anchor

        # Extend match forward
        match_pos = pos + _MIN_MATCH
        ref_pos = ref + _MIN_MATCH
        # The last _LAST_LITERALS bytes must stay literals (LZ4 end-of-block rule).
        match_limit = src_len - _LAST_LITERALS
        while match_pos < match_limit and src[match_pos] == src[ref_pos]:
            match_pos += 1
            ref_pos += 1
        match_len = match_pos - pos - _MIN_MATCH

        # Token
        token_lit = min(lit_len, 15)
        token_match = min(match_len, 15)
        output.append((token_lit << 4) | token_match)

        # Extra literal length
        if lit_len >= 15:
            remaining = lit_len - 15
            while remaining >= 255:
                output.append(255)
                remaining -= 255
            output.append(remaining)

        # Literals
        output.extend(src[anchor : anchor + lit_len])

        # Offset (LE16)
        offset = pos - ref
        output.append(offset & 0xFF)
        output.append((offset >> 8) & 0xFF)

        # Extra match length
        if match_len >= 15:
            remaining = match_len - 15
            while remaining >= 255:
                output.append(255)
                remaining -= 255
            output.append(remaining)

        pos = match_pos
        anchor = pos

    # Last literals
    lit_len = src_len - anchor
    token_lit = min(lit_len, 15)
    output.append(token_lit << 4)

    if lit_len >= 15:
        remaining = lit_len - 15
        while remaining >= 255:
            output.append(255)
            remaining -= 255
        output.append(remaining)

    output.extend(src[anchor:])

    return bytes(output)


def decompress_block(
    data: bytes | bytearray | memoryview, uncompressed_size: int
) -> bytes:
    """Decompress LZ4 block format data."""
    src = bytes(data)
    src_len = len(src)
    if src_len == 0 and uncompressed_size == 0:
        return b""

    output = bytearray()
    pos = 0

    while pos < src_len:
        if pos >= src_len:
            raise CompressError(
                CompressErrorCode.UNEXPECTED_EOF, "unexpected end of LZ4 block"
            )

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

        # Copy literals
        if pos + lit_len > src_len:
            raise CompressError(
                CompressErrorCode.UNEXPECTED_EOF, "literal data extends past input"
            )
        output.extend(src[pos : pos + lit_len])
        pos += lit_len

        if pos >= src_len:
            break  # Last block ends after literals

        # Read offset
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

        # Match length
        match_len = (token & 0x0F) + _MIN_MATCH
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

        # Copy match (may overlap)
        match_start = len(output) - offset
        if match_start < 0:
            raise CompressError(
                CompressErrorCode.INVALID_INPUT, "match offset beyond output"
            )

        for i in range(match_len):
            output.append(output[match_start + i])

    if len(output) != uncompressed_size:
        raise CompressError(
            CompressErrorCode.INVALID_INPUT,
            f"decompressed size {len(output)} != expected {uncompressed_size}",
        )

    return bytes(output)
