//! LZ4 Frame Format (v1.6.3) implementation.
//!
//! Wraps LZ4 block compression with framing, checksums, and content size.

use crate::error::CompressError;
use super::block;
use super::xxhash::xxh32;

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
        0..=65536 => 4,       // 64KB
        65537..=262144 => 5,  // 256KB
        262145..=1048576 => 6, // 1MB
        _ => 7,                // 4MB
    }
}

/// Compress data into LZ4 frame format.
pub fn compress_frame(input: &[u8], opts: &FrameOptions) -> Result<Vec<u8>, CompressError> {
    let mut output = Vec::with_capacity(input.len() + 32);

    // Magic number (LE)
    output.extend_from_slice(&LZ4_MAGIC.to_le_bytes());

    // Frame descriptor (FLG + BD + optional content size + header checksum)
    let flg: u8 = 0x40 // version = 01
        | if opts.content_size { 0x08 } else { 0 }
        | if opts.content_checksum { 0x04 } else { 0 }
        | if opts.block_checksum { 0x10 } else { 0 };

    let bd: u8 = block_size_id(opts.max_block_size) << 4;

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
    let max_bs = opts.max_block_size;
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
            let start = output.len() - if csize == 0 || csize >= chunk.len() {
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

    // BD
    let _bd = input[pos];
    pos += 1;

    // Content size
    let content_size = if has_content_size {
        if pos + 8 > input.len() {
            return Err(CompressError::UnexpectedEof);
        }
        let sz = u64::from_le_bytes([
            input[pos], input[pos + 1], input[pos + 2], input[pos + 3],
            input[pos + 4], input[pos + 5], input[pos + 6], input[pos + 7],
        ]);
        pos += 8;
        Some(sz as usize)
    } else {
        None
    };

    // Header checksum
    let _hc = input[pos];
    pos += 1;

    let mut output = Vec::with_capacity(content_size.unwrap_or(4096));

    // Read blocks
    loop {
        if pos + 4 > input.len() {
            return Err(CompressError::UnexpectedEof);
        }
        let block_size_raw = u32::from_le_bytes([
            input[pos], input[pos + 1], input[pos + 2], input[pos + 3],
        ]);
        pos += 4;

        if block_size_raw == END_MARK {
            break;
        }

        let is_uncompressed = (block_size_raw & 0x80000000) != 0;
        let block_size = (block_size_raw & 0x7FFFFFFF) as usize;

        if pos + block_size > input.len() {
            return Err(CompressError::UnexpectedEof);
        }

        if is_uncompressed {
            output.extend_from_slice(&input[pos..pos + block_size]);
        } else {
            let block_data = &input[pos..pos + block_size];
            // Estimate decompressed size
            let est_size = content_size.unwrap_or(block_size * 4);
            let old_len = output.len();
            output.resize(old_len + est_size, 0);

            match block::decompress_block(block_data, &mut output[old_len..]) {
                Ok(n) => output.truncate(old_len + n),
                Err(CompressError::OutputTooSmall { .. }) => {
                    // Retry with larger buffer
                    output.resize(old_len + est_size * 4, 0);
                    let n = block::decompress_block(block_data, &mut output[old_len..])?;
                    output.truncate(old_len + n);
                }
                Err(e) => return Err(e),
            }
        }
        pos += block_size;

        if has_block_checksum {
            pos += 4; // skip block checksum
        }
    }

    // Content checksum verification
    if has_content_checksum {
        if pos + 4 > input.len() {
            return Err(CompressError::UnexpectedEof);
        }
        let expected = u32::from_le_bytes([
            input[pos], input[pos + 1], input[pos + 2], input[pos + 3],
        ]);
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
