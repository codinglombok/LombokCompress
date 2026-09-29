//! Deflate compression — gzip/zlib compatible.

pub mod compress;
pub mod decompress;
pub mod huffman;
pub mod lz77;

pub use compress::{adler32, crc32, deflate_compress, gzip_compress, zlib_compress};
pub use decompress::{deflate_decompress, gzip_decompress, zlib_decompress};
