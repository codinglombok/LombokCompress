//! LZ4 compression — block and frame formats.

pub mod block;
pub mod frame;
pub mod xxhash;

pub use block::{compress_block, compress_bound, decompress_block};
pub use frame::{compress_frame, decompress_frame, FrameOptions};
pub use xxhash::xxh32;
