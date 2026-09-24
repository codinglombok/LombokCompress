"""LombokCompress — Zero-dependency compression library: LZ4, Zstd, Deflate."""

from lombokcompress.error import CompressError, CompressErrorCode
from lombokcompress import lz4
from lombokcompress import deflate
from lombokcompress import zstd

__all__ = [
    "CompressError",
    "CompressErrorCode",
    "lz4",
    "deflate",
    "zstd",
]
