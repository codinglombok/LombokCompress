"""Zstandard compression — levels 1-3, raw + RLE blocks."""

from lombokcompress.zstd.compress import zstd_compress, is_zstd
from lombokcompress.zstd.decompress import zstd_decompress

__all__ = [
    "zstd_compress",
    "is_zstd",
    "zstd_decompress",
]
