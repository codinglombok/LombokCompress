"""Deflate compression — gzip, zlib, raw deflate."""

from lombokcompress.deflate.compress import (
    deflate_compress,
    gzip_compress,
    zlib_compress,
    crc32,
    adler32,
)
from lombokcompress.deflate.decompress import (
    deflate_decompress,
    gzip_decompress,
    zlib_decompress,
)

__all__ = [
    "deflate_compress",
    "gzip_compress",
    "zlib_compress",
    "deflate_decompress",
    "gzip_decompress",
    "zlib_decompress",
    "crc32",
    "adler32",
]
