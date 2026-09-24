"""LZ4 compression — block and frame formats."""

from lombokcompress.lz4.xxhash import xxh32
from lombokcompress.lz4.block import compress_block, decompress_block, compress_bound
from lombokcompress.lz4.frame import compress_frame, decompress_frame

__all__ = [
    "xxh32",
    "compress_block",
    "decompress_block",
    "compress_bound",
    "compress_frame",
    "decompress_frame",
]
