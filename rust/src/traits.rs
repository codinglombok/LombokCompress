//! Compression and decompression traits.

use crate::error::CompressError;
use crate::prelude::Vec;

/// One-shot compression.
pub trait Compress {
    /// Compress `input` into `output`. Returns number of bytes written.
    fn compress(&self, input: &[u8], output: &mut [u8]) -> Result<usize, CompressError>;

    /// Maximum compressed size for a given input length.
    fn compress_bound(&self, input_len: usize) -> usize;
}

/// One-shot decompression.
pub trait Decompress {
    /// Decompress `input` into `output`. Returns number of bytes written.
    fn decompress(&self, input: &[u8], output: &mut [u8]) -> Result<usize, CompressError>;
}

/// Streaming compressor — feed chunks, get compressed output.
pub trait StreamCompressor {
    /// Feed a chunk of input data. Returns compressed bytes (may be empty).
    fn update(&mut self, input: &[u8]) -> Result<Vec<u8>, CompressError>;

    /// Flush remaining data and finalize. Returns final compressed bytes.
    fn finish(self) -> Result<Vec<u8>, CompressError>;
}

/// Streaming decompressor — feed compressed chunks, get decompressed output.
pub trait StreamDecompressor {
    /// Feed a chunk of compressed data. Returns decompressed bytes (may be empty).
    fn update(&mut self, input: &[u8]) -> Result<Vec<u8>, CompressError>;

    /// Finalize decompression. Returns any remaining bytes.
    fn finish(self) -> Result<Vec<u8>, CompressError>;
}
