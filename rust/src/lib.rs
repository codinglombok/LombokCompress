//! # LombokCompress
//!
//! Zero-dependency compression library for the Lombok Ecosystem.
//!
//! Supports LZ4 (block + frame), Zstd (levels 1-3), and Deflate (gzip/zlib).
//!
//! ## Features
//! - `lz4` (default) — LZ4 block and frame format
//! - `zstd` — Zstd levels 1-3 with raw blocks
//! - `deflate` — Deflate/gzip/zlib compression
//! - `std` (default) — Standard library support
//!
//! ## Usage
//! ```rust
//! use lombokcompress::lz4;
//!
//! let data = b"Hello World! Hello World!";
//! let mut compressed = vec![0u8; lz4::compress_bound(data.len())];
//! let csize = lz4::compress_block(data, &mut compressed).unwrap();
//! compressed.truncate(csize);
//!
//! let mut decompressed = vec![0u8; data.len()];
//! let dsize = lz4::decompress_block(&compressed, &mut decompressed).unwrap();
//! assert_eq!(&decompressed[..dsize], &data[..]);
//! ```

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(not(feature = "std"))]
extern crate alloc;

pub mod error;
pub mod traits;

#[cfg(feature = "lz4")]
pub mod lz4;

#[cfg(feature = "deflate")]
pub mod deflate;

#[cfg(feature = "zstd")]
pub mod zstd;

pub use error::CompressError;
pub use traits::{Compress, Decompress};

#[cfg(feature = "std")]
pub use traits::{StreamCompressor, StreamDecompressor};
