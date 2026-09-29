//! Zstandard compression (levels 1-3, raw blocks).

pub mod compress;
pub mod decompress;

pub use compress::{is_zstd, zstd_compress, ZstdLevel};
pub use decompress::{zstd_decompress, zstd_decompress_limited};
