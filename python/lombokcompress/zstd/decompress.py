"""Zstandard decompressor (raw + RLE blocks) — pure Python."""

from __future__ import annotations

from lombokcompress.error import CompressError, CompressErrorCode

_ZSTD_MAGIC = 0xFD2FB528


def zstd_decompress(data: bytes | bytearray | memoryview) -> bytes:
    """Decompress a Zstandard frame."""
    src = bytes(data)
    if len(src) < 5:
        raise CompressError(
            CompressErrorCode.UNEXPECTED_EOF, "input too short for Zstd"
        )

    pos = 0

    # Magic number
    magic = (
        src[0] | (src[1] << 8) | (src[2] << 16) | ((src[3] << 24) & 0xFFFFFFFF)
    )
    if magic != _ZSTD_MAGIC:
        raise CompressError(
            CompressErrorCode.INVALID_INPUT, "invalid Zstd magic number"
        )
    pos = 4

    # Frame header descriptor
    fhd = src[pos]
    pos += 1

    single_segment = (fhd & 0x20) != 0
    content_checksum = (fhd & 0x04) != 0
    dict_id_flag = fhd & 0x03

    fcs_bits = (fhd >> 6) & 0x03
    if fcs_bits == 0:
        fcs_field_size = 1 if single_segment else 0
    elif fcs_bits == 1:
        fcs_field_size = 2
    elif fcs_bits == 2:
        fcs_field_size = 4
    else:
        fcs_field_size = 8

    # Window descriptor (if not single segment)
    if not single_segment:
        pos += 1

    # Dict ID
    dict_id_bytes = [0, 1, 2, 4][dict_id_flag]
    pos += dict_id_bytes

    # Content size
    content_size = None
    if fcs_field_size > 0:
        if fcs_field_size == 1:
            content_size = src[pos]
        elif fcs_field_size == 2:
            content_size = (src[pos] | (src[pos + 1] << 8)) + 256
        elif fcs_field_size == 4:
            content_size = (
                src[pos]
                | (src[pos + 1] << 8)
                | (src[pos + 2] << 16)
                | ((src[pos + 3] << 24) & 0xFFFFFFFF)
            )
        elif fcs_field_size == 8:
            content_size = (
                src[pos]
                | (src[pos + 1] << 8)
                | (src[pos + 2] << 16)
                | ((src[pos + 3] << 24) & 0xFFFFFFFF)
            )
        pos += fcs_field_size

    output = bytearray()

    # Read blocks
    while True:
        if pos + 3 > len(src):
            raise CompressError(
                CompressErrorCode.UNEXPECTED_EOF, "unexpected end of Zstd data"
            )

        bh = src[pos] | (src[pos + 1] << 8) | (src[pos + 2] << 16)
        pos += 3

        last_block = (bh & 1) != 0
        block_type = (bh >> 1) & 0x03
        block_size = bh >> 3

        if block_type == 0:
            # Raw block
            if pos + block_size > len(src):
                raise CompressError(
                    CompressErrorCode.UNEXPECTED_EOF, "raw block extends past input"
                )
            output.extend(src[pos : pos + block_size])
            pos += block_size

        elif block_type == 1:
            # RLE block
            if pos >= len(src):
                raise CompressError(
                    CompressErrorCode.UNEXPECTED_EOF, "RLE block missing byte"
                )
            byte = src[pos]
            pos += 1
            output.extend(bytes([byte]) * block_size)

        elif block_type == 2:
            raise CompressError(
                CompressErrorCode.UNSUPPORTED,
                "Zstd compressed blocks (FSE) not yet implemented",
            )

        elif block_type == 3:
            raise CompressError(
                CompressErrorCode.INVALID_INPUT, "reserved Zstd block type"
            )

        if last_block:
            break

    return bytes(output)
