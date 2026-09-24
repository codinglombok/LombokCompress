"""XXH32 hash — pure Python, matching Rust/TS implementations."""

from __future__ import annotations

_MASK32 = 0xFFFFFFFF

_PRIME1 = 0x9E3779B1
_PRIME2 = 0x85EBCA77
_PRIME3 = 0xC2B2AE3D
_PRIME4 = 0x27D4EB2F
_PRIME5 = 0x165667B1


def _u32(x: int) -> int:
    return x & _MASK32


def _rotl32(x: int, r: int) -> int:
    return _u32((x << r) | (x >> (32 - r)))


def _read_u32_le(data: bytes | bytearray | memoryview, offset: int) -> int:
    return (
        data[offset]
        | (data[offset + 1] << 8)
        | (data[offset + 2] << 16)
        | (data[offset + 3] << 24)
    )


def _round(acc: int, val: int) -> int:
    acc = _u32(acc + _u32(val * _PRIME2))
    acc = _rotl32(acc, 13)
    return _u32(acc * _PRIME1)


def xxh32(data: bytes | bytearray | memoryview, seed: int = 0) -> int:
    """Compute XXH32 hash of data with optional seed."""
    length = len(data)
    pos = 0

    if length >= 16:
        v1 = _u32(seed + _PRIME1 + _PRIME2)
        v2 = _u32(seed + _PRIME2)
        v3 = _u32(seed)
        v4 = _u32(seed - _PRIME1)

        limit = length - 16
        while pos <= limit:
            v1 = _round(v1, _read_u32_le(data, pos))
            pos += 4
            v2 = _round(v2, _read_u32_le(data, pos))
            pos += 4
            v3 = _round(v3, _read_u32_le(data, pos))
            pos += 4
            v4 = _round(v4, _read_u32_le(data, pos))
            pos += 4

        h32 = _u32(
            _rotl32(v1, 1) + _rotl32(v2, 7) + _rotl32(v3, 12) + _rotl32(v4, 18)
        )
    else:
        h32 = _u32(seed + _PRIME5)

    h32 = _u32(h32 + length)

    limit = length - 4
    while pos <= limit:
        h32 = _u32(h32 + _u32(_read_u32_le(data, pos) * _PRIME3))
        h32 = _u32(_rotl32(h32, 17) * _PRIME4)
        pos += 4

    while pos < length:
        h32 = _u32(h32 + _u32(data[pos] * _PRIME5))
        h32 = _u32(_rotl32(h32, 11) * _PRIME1)
        pos += 1

    # Avalanche
    h32 ^= h32 >> 15
    h32 = _u32(h32 * _PRIME2)
    h32 ^= h32 >> 13
    h32 = _u32(h32 * _PRIME3)
    h32 ^= h32 >> 16

    return h32
