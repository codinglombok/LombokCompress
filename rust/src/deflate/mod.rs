//! Deflate compression — gzip/zlib compatible.

pub mod huffman;
pub mod lz77;
pub mod compress;
pub mod decompress;

pub use compress::{deflate_compress, gzip_compress, zlib_compress, crc32, adler32};
pub use decompress::{deflate_decompress, gzip_decompress, zlib_decompress};
