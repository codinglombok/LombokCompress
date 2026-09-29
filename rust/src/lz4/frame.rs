//! LZ4 Frame Format (v1.6.3) implementation.
//!
//! Wraps LZ4 block compression with framing, checksums, and content size.

use super::block;
use super::xxhash::xxh32;
use crate::error::CompressError;
use crate::prelude::{vec, Vec};

const LZ4_MAGIC: u32 = 0x184D2204;
const END_MARK: u32 = 0x00000000;

/// Frame descriptor options.
#[derive(Debug, Clone)]
pub struct FrameOptions {
    /// Include content checksum (XXH32 of original data).
    pub content_checksum: bool,
    /// Include content size in the frame header.
    pub content_size: bool,
    /// Include block checksums.
    pub block_checksum: bool,
    /// Maximum block size (default: 64KB = 0x00040000).
    pub max_block_size: usize,
}

impl Default for FrameOptions {
    fn default() -> Self {
        Self {
            content_checksum: true,
            content_size: true,
            block_checksum: false,
            max_block_size: 64 * 1024, // 64KB
        }
    }
}

/// Block size ID for frame header.
fn block_size_id(max_block_size: usize) -> u8 {
    match max_block_size {
        0..=65536 => 4,        // 64KB
        65537..=262144 => 5,   // 256KB
        262145..=1048576 => 6, // 1MB
        _ => 7,                // 4MB
    }
}

/// Block maximum size in bytes for a frame block size ID (4..=7).
fn block_max_size(id: u8) -> Option<usize> {
    match id {
        4 => Some(64 * 1024),
        5 => Some(256 * 1024),
        6 => Some(1024 * 1024),
        7 => Some(4 * 1024 * 1024),
        _ => None,
    }
}

/// Compress data into LZ4 frame format.
pub fn compress_frame(input: &[u8], opts: &FrameOptions) -> Result<Vec<u8>, CompressError> {
    let mut output = Vec::with_capacity(input.len() + 32);

    // Magic number (LE)
    output.extend_from_slice(&LZ4_MAGIC.to_le_bytes());

    // Frame descriptor (FLG + BD + optional content size + header checksum)
    // Blocks are compressed independently, so advertise Block_Independence.
    let flg: u8 = 0x40 // version = 01
        | 0x20
        | if opts.content_size { 0x08 } else { 0 }
        | if opts.content_checksum { 0x04 } else { 0 }
        | if opts.block_checksum { 0x10 } else { 0 };

    let bs_id = block_size_id(opts.max_block_size);
    let bd: u8 = bs_id << 4;

    output.push(flg);
    output.push(bd);

    if opts.content_size {
        output.extend_from_slice(&(input.len() as u64).to_le_bytes());
    }

    // Header checksum: XXH32 of (FLG..last header byte) >> 8, then & 0xFF
    let header_start = 4; // after magic
    let hc = (xxh32(&output[header_start..], 0) >> 8) as u8;
    output.push(hc);

    // Compress blocks
    let mut pos = 0;
    // Blocks are cut at the size advertised in BD so decoders can bound them.
    let max_bs = block_max_size(bs_id).unwrap_or(64 * 1024);
    let mut block_buf = vec![0u8; block::compress_bound(max_bs)];

    while pos < input.len() {
        let chunk_end = core::cmp::min(pos + max_bs, input.len());
        let chunk = &input[pos..chunk_end];

        let csize = block::compress_block(chunk, &mut block_buf)?;

        if csize == 0 || csize >= chunk.len() {
            // Store uncompressed (set high bit of block size)
            let block_size = chunk.len() as u32 | 0x80000000;
            output.extend_from_slice(&block_size.to_le_bytes());
            output.extend_from_slice(chunk);
        } else {
            // Compressed block
            output.extend_from_slice(&(csize as u32).to_le_bytes());
            output.extend_from_slice(&block_buf[..csize]);
        }

        if opts.block_checksum {
            let start = output.len()
                - if csize == 0 || csize >= chunk.len() {
                    chunk.len()
                } else {
                    csize
                };
            let bc = xxh32(&output[start..], 0);
            output.extend_from_slice(&bc.to_le_bytes());
        }

        pos = chunk_end;
    }

    // End mark
    output.extend_from_slice(&END_MARK.to_le_bytes());

    // Content checksum
    if opts.content_checksum {
        let cc = xxh32(input, 0);
        output.extend_from_slice(&cc.to_le_bytes());
    }

    Ok(output)
}

/// Upper bound for buffers pre-allocated from untrusted header fields.
const MAX_INITIAL_CAPACITY: usize = 1 << 20;

/// Decompress an LZ4 frame.
pub fn decompress_frame(input: &[u8]) -> Result<Vec<u8>, CompressError> {
    if input.len() < 7 {
        return Err(CompressError::UnexpectedEof);
    }

    let mut pos = 0;

    // Magic number
    let magic = u32::from_le_bytes([input[0], input[1], input[2], input[3]]);
    if magic != LZ4_MAGIC {
        return Err(CompressError::InvalidInput("invalid LZ4 magic number"));
    }
    pos += 4;

    // FLG
    let flg = input[pos];
    pos += 1;
    let has_content_size = (flg & 0x08) != 0;
    let has_content_checksum = (flg & 0x04) != 0;
    let has_block_checksum = (flg & 0x10) != 0;
    let block_independent = (flg & 0x20) != 0;
    if flg >> 6 != 0x01 {
        return Err(CompressError::Unsupported("LZ4 frame version"));
    }

    // BD: block maximum size (LZ4 frame spec: IDs 4..=7 → 64KB..4MB)
    let bd = input[pos];
    pos += 1;
    let block_max = block_max_size((bd >> 4) & 0x07).ok_or(CompressError::InvalidInput(
        "invalid LZ4 block maximum size",
    ))?;

    // Content size
    let content_size = if has_content_size {
        if pos + 8 > input.len() {
            return Err(CompressError::UnexpectedEof);
        }
        let sz = u64::from_le_bytes([
            input[pos],
            input[pos + 1],
            input[pos + 2],
            input[pos + 3],
            input[pos + 4],
            input[pos + 5],
            input[pos + 6],
            input[pos + 7],
        ]);
        pos += 8;
        Some(sz as usize)
    } else {
        None
    };

    // Header checksum
    if pos >= input.len() {
        return Err(CompressError::UnexpectedEof);
    }
    let hc = input[pos];
    if hc != (xxh32(&input[4..pos], 0) >> 8) as u8 {
        return Err(CompressError::InvalidInput(
            "LZ4 frame header checksum mismatch",
        ));
    }
    pos += 1;

    // The declared content size is untrusted: never pre-allocate from it alone.
    let mut output = Vec::with_capacity(core::cmp::min(
        content_size.unwrap_or(4096),
        MAX_INITIAL_CAPACITY,
    ));

    // Read blocks
    loop {
        if pos + 4 > input.len() {
            return Err(CompressError::UnexpectedEof);
        }
        let block_size_raw =
            u32::from_le_bytes([input[pos], input[pos + 1], input[pos + 2], input[pos + 3]]);
        pos += 4;

        if block_size_raw == END_MARK {
            break;
        }

        let is_uncompressed = (block_size_raw & 0x80000000) != 0;
        let block_size = (block_size_raw & 0x7FFFFFFF) as usize;

        if block_size > block_max {
            return Err(CompressError::InvalidInput(
                "LZ4 block exceeds declared block maximum size",
            ));
        }
        if block_size > input.len() - pos {
            return Err(CompressError::UnexpectedEof);
        }

        if is_uncompressed {
            output.extend_from_slice(&input[pos..pos + block_size]);
        } else {
            let block_data = &input[pos..pos + block_size];
            // A block never decompresses to more than the block maximum size.
            // Linked blocks may reference up to 64KB of earlier output.
            let old_len = output.len();
            let history = if block_independent {
                0
            } else {
                core::cmp::min(old_len, 64 * 1024)
            };
            let base = old_len - history;
            output.resize(old_len + block_max, 0);
            let n = block::decompress_block_with_prefix(block_data, &mut output[base..], history)?;
            output.truncate(old_len + n);
        }
        pos += block_size;

        if has_block_checksum {
            if input.len() - pos < 4 {
                return Err(CompressError::UnexpectedEof);
            }
            let expected =
                u32::from_le_bytes([input[pos], input[pos + 1], input[pos + 2], input[pos + 3]]);
            let actual = xxh32(&input[pos - block_size..pos], 0);
            if expected != actual {
                return Err(CompressError::ChecksumMismatch { expected, actual });
            }
            pos += 4;
        }
    }

    if let Some(expected) = content_size {
        if output.len() != expected {
            return Err(CompressError::InvalidInput(
                "LZ4 frame content size does not match decoded length",
            ));
        }
    }

    // Content checksum verification
    if has_content_checksum {
        if pos + 4 > input.len() {
            return Err(CompressError::UnexpectedEof);
        }
        let expected =
            u32::from_le_bytes([input[pos], input[pos + 1], input[pos + 2], input[pos + 3]]);
        let actual = xxh32(&output, 0);
        if expected != actual {
            return Err(CompressError::ChecksumMismatch { expected, actual });
        }
    }

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_roundtrip() {
        let data = b"Hello World! Hello World! This is a test of LZ4 frame format.";
        let opts = FrameOptions::default();
        let compressed = compress_frame(data, &opts).unwrap();
        let decompressed = decompress_frame(&compressed).unwrap();
        assert_eq!(&decompressed, &data[..]);
    }

    #[test]
    fn test_frame_empty() {
        let data = b"";
        let opts = FrameOptions::default();
        let compressed = compress_frame(data, &opts).unwrap();
        let decompressed = decompress_frame(&compressed).unwrap();
        assert_eq!(decompressed.len(), 0);
    }

    #[test]
    fn test_frame_no_checksum() {
        let data = b"Test data without content checksum";
        let opts = FrameOptions {
            content_checksum: false,
            content_size: false,
            ..Default::default()
        };
        let compressed = compress_frame(data, &opts).unwrap();
        let decompressed = decompress_frame(&compressed).unwrap();
        assert_eq!(&decompressed, &data[..]);
    }
}
