"""Cross-language vectors, roundtrips, stdlib interop and malformed input."""

from __future__ import annotations

import gzip
import json
import zlib
from pathlib import Path

import pytest

from lombokcompress import CompressError, deflate, lz4, zstd

VECTORS = json.loads(
    (Path(__file__).resolve().parents[2] / "test-vectors" / "compress_vectors.json").read_text()
)


class Rng:
    """xorshift32 so the corpus is deterministic."""

    def __init__(self, seed: int) -> None:
        self.s = seed

    def next(self) -> int:
        s = self.s
        s ^= (s << 13) & 0xFFFFFFFF
        s ^= s >> 17
        s ^= (s << 5) & 0xFFFFFFFF
        self.s = s
        return s

    def bytes(self, n: int) -> bytes:
        return bytes(self.next() & 0xFF for _ in range(n))


def samples() -> list[bytes]:
    r = Rng(0x12345678)
    out = [
        b"",
        b"a",
        b"abc" * 1000,
        bytes(20_000),
        b"Hello, LombokCompress! " * 1000,
    ]
    for n in (1, 13, 64, 1000, 20_000):
        out.append(r.bytes(n))
        out.append(bytes(r.next() % 3 for _ in range(n)))
    return out


def test_vectors() -> None:
    for v in VECTORS["xxh32"]:
        assert lz4.xxh32(bytes.fromhex(v["input_hex"]), v["seed"]) == v["expected"]
    for v in VECTORS["crc32"]:
        assert deflate.crc32(bytes.fromhex(v["input_hex"])) == v["expected"]
    for v in VECTORS["adler32"]:
        assert deflate.adler32(bytes.fromhex(v["input_hex"])) == v["expected"]


@pytest.mark.parametrize("data", samples())
def test_roundtrip(data: bytes) -> None:
    assert lz4.decompress_block(lz4.compress_block(data), len(data)) == data
    assert lz4.decompress_frame(lz4.compress_frame(data)) == data
    assert zstd.zstd_decompress(zstd.zstd_compress(data)) == data
    assert deflate.deflate_decompress(deflate.deflate_compress(data)) == data
    assert deflate.gzip_decompress(deflate.gzip_compress(data)) == data
    assert deflate.zlib_decompress(deflate.zlib_compress(data)) == data


@pytest.mark.parametrize("data", samples())
def test_stdlib_interop(data: bytes) -> None:
    assert gzip.decompress(deflate.gzip_compress(data)) == data
    assert zlib.decompress(deflate.zlib_compress(data)) == data
    assert zlib.decompress(deflate.deflate_compress(data), -15) == data
    stored = zlib.compressobj(0, zlib.DEFLATED, -15)
    raw = stored.compress(data) + stored.flush()
    assert deflate.deflate_decompress(raw) == data


def _cases() -> list[bytes]:
    r = Rng(0xDEADBEEF)
    seed = b"The quick brown fox jumps over the lazy dog. " * 40
    valid = [
        lz4.compress_frame(seed),
        lz4.compress_block(seed),
        zstd.zstd_compress(seed, 2),
        zstd.zstd_compress(bytes([7]) * 5000, 1),
        deflate.deflate_compress(seed),
        deflate.gzip_compress(seed),
        deflate.zlib_compress(seed),
    ]
    cases = [r.bytes(r.next() % 64) for _ in range(200)]
    for v in valid:
        for _ in range(100):
            c = bytearray(v)
            for _ in range(1 + r.next() % 4):
                c[r.next() % len(c)] ^= 1 << (r.next() % 8)
            if r.next() % 4 == 0:
                c = c[: r.next() % len(c)]
            cases.append(bytes(c))
    cases += [
        bytes([0x1F, 0x8B, 0x08, 0x08, 0, 0, 0, 0, 0, 0xFF]) + b"A" * 10,
        bytes([0x04, 0x22, 0x4D, 0x18, 0x68, 0x40]) + b"\xff" * 7 + b"\x7f\x00\x00\x00\x00\x00",
        bytes([0x28, 0xB5, 0x2F, 0xFD, 0xE0]) + b"\xff" * 7 + b"\x7f\x01\x00\x00",
        bytes([0x03, 0x02, 0x00]),  # deflate match before any output
    ]
    return cases


def test_malformed_input_raises_compress_error_only() -> None:
    decoders = [
        lambda c: lz4.decompress_block(c, 1 << 16),
        lz4.decompress_frame,
        zstd.zstd_decompress,
        deflate.deflate_decompress,
        deflate.gzip_decompress,
        deflate.zlib_decompress,
    ]
    for c in _cases():
        for d in decoders:
            try:
                d(c)
            except CompressError:
                pass


def test_output_limits() -> None:
    big = b"A" * 200_000
    with pytest.raises(CompressError):
        deflate.deflate_decompress(deflate.deflate_compress(big), max_output=1000)
    with pytest.raises(CompressError):
        zstd.zstd_decompress(zstd.zstd_compress(big), max_output=1000)
