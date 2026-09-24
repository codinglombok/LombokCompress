//! Error types for LombokCompress.

/// Errors that can occur during compression or decompression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompressError {
    /// Input data is invalid or corrupted.
    InvalidInput(&'static str),
    /// Output buffer is too small to hold the result.
    OutputTooSmall { needed: usize, available: usize },
    /// Checksum verification failed.
    ChecksumMismatch { expected: u32, actual: u32 },
    /// Feature or format not supported.
    Unsupported(&'static str),
    /// Unexpected end of input data.
    UnexpectedEof,
    /// Internal error (should not happen).
    InternalError(&'static str),
}

impl core::fmt::Display for CompressError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidInput(msg) => write!(f, "invalid input: {}", msg),
            Self::OutputTooSmall { needed, available } => {
                write!(f, "output too small: need {} bytes, have {}", needed, available)
            }
            Self::ChecksumMismatch { expected, actual } => {
                write!(f, "checksum mismatch: expected 0x{:08X}, got 0x{:08X}", expected, actual)
            }
            Self::Unsupported(msg) => write!(f, "unsupported: {}", msg),
            Self::UnexpectedEof => write!(f, "unexpected end of input"),
            Self::InternalError(msg) => write!(f, "internal error: {}", msg),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for CompressError {}
