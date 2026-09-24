"""Compression error types."""

from enum import Enum


class CompressErrorCode(Enum):
    INVALID_INPUT = "invalid_input"
    OUTPUT_TOO_SMALL = "output_too_small"
    CHECKSUM_MISMATCH = "checksum_mismatch"
    UNSUPPORTED = "unsupported"
    UNEXPECTED_EOF = "unexpected_eof"
    INTERNAL_ERROR = "internal_error"


class CompressError(Exception):
    """Compression/decompression error."""

    def __init__(self, code: CompressErrorCode, message: str) -> None:
        super().__init__(message)
        self.code = code
