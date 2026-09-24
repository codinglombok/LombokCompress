//! Zstandard compression (levels 1-3, raw blocks).

pub mod compress;
pub mod decompress;

pub use compress::{zstd_compress, is_zstd, ZstdLevel};
pub use decompress::zstd_decompress;
