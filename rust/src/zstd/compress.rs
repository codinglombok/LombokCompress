//! Zstandard compressor (levels 1-3, raw blocks).

use crate::error::CompressError;
use crate::prelude::Vec;

const ZSTD_MAGIC: u32 = 0xFD2FB528;

/// Zstd compression level.
#[derive(Debug, Clone, Copy)]
pub enum ZstdLevel {
    L1,
    L2,
    L3,
}

/// Compress data using Zstandard (raw blocks, no FSE/Huffman entropy coding).
///
/// This is a simplified Zstd implementation that uses raw blocks for
/// compatibility. The output is a valid Zstd frame.
pub fn zstd_compress(input: &[u8], level: ZstdLevel) -> Result<Vec<u8>, CompressError> {
    let mut output = Vec::with_capacity(input.len() + 32);

    // Frame header
    output.extend_from_slice(&ZSTD_MAGIC.to_le_bytes());

    // Frame header descriptor
    // FHD byte: Frame_Content_Size_flag=1 (1 byte size), Single_Segment=1
    // Content size <= 255 uses 1 byte, else 2 or 4 bytes
    let content_size = input.len();

    if content_size <= 255 {
        let fhd: u8 = 0x20; // FCS_Field_Size=0 (1 byte when single_segment), Single_Segment=1
        output.push(fhd);
        output.push(content_size as u8);
    } else if content_size <= 65535 + 256 {
        let fhd: u8 = 0x60; // FCS_Field_Size=01 (2 bytes), Single_Segment=1
        output.push(fhd);
        let sz = (content_size - 256) as u16;
        output.extend_from_slice(&sz.to_le_bytes());
    } else if content_size as u64 <= u32::MAX as u64 {
        let fhd: u8 = 0xA0; // FCS_Field_Size=10 (4 bytes), Single_Segment=1
        output.push(fhd);
        output.extend_from_slice(&(content_size as u32).to_le_bytes());
    } else {
        let fhd: u8 = 0xE0; // FCS_Field_Size=11 (8 bytes), Single_Segment=1
        output.push(fhd);
        output.extend_from_slice(&(content_size as u64).to_le_bytes());
    }

    if input.is_empty() {
        // Empty frame: one last raw block of size 0
        // Block_Header: Last_Block=1, Block_Type=0 (raw), Block_Size=0
        let bh: u32 = 0x01; // last=1, type=raw(0), size=0
        output.push((bh & 0xFF) as u8);
        output.push(((bh >> 8) & 0xFF) as u8);
        output.push(((bh >> 16) & 0xFF) as u8);
        return Ok(output);
    }

    // For this simplified implementation, we try to find matches.
    // If we can't improve, emit raw blocks.
    let compressed_data = zstd_compress_blocks(input, level)?;

    if compressed_data.len() < input.len() {
        // Use compressed output
        output.extend_from_slice(&compressed_data);
    } else {
        // Emit as raw block (uncompressed)
        emit_raw_blocks(&mut output, input);
    }

    Ok(output)
}

/// Emit input as raw blocks (max 128KB each).
fn emit_raw_blocks(output: &mut Vec<u8>, input: &[u8]) {
    let max_block = 128 * 1024; // 128KB max block size
    let mut pos = 0;

    while pos < input.len() {
        let remaining = input.len() - pos;
        let block_size = core::cmp::min(remaining, max_block);
        let is_last = pos + block_size >= input.len();

        // Block header: 3 bytes LE
        // Bit 0: Last_Block
        // Bits 1-2: Block_Type (0=Raw)
        // Bits 3-23: Block_Size
        let bh = (if is_last { 1u32 } else { 0 }) | ((block_size as u32) << 3);
        output.push((bh & 0xFF) as u8);
        output.push(((bh >> 8) & 0xFF) as u8);
        output.push(((bh >> 16) & 0xFF) as u8);

        output.extend_from_slice(&input[pos..pos + block_size]);
        pos += block_size;
    }
}

/// Simple Zstd block compression using RLE blocks for runs.
fn zstd_compress_blocks(input: &[u8], _level: ZstdLevel) -> Result<Vec<u8>, CompressError> {
    let mut output = Vec::new();
    let max_block = 128 * 1024;
    let mut pos = 0;

    while pos < input.len() {
        let remaining = input.len() - pos;
        let block_size = core::cmp::min(remaining, max_block);
        let is_last = pos + block_size >= input.len();
        let block_data = &input[pos..pos + block_size];

        // Check if block is all one byte (RLE)
        let first = block_data[0];
        let is_rle = block_data.iter().all(|&b| b == first);

        if is_rle {
            // RLE block
            let bh = (if is_last { 1u32 } else { 0 })
                | (1 << 1) // RLE block type
                | ((block_size as u32) << 3);
            output.push((bh & 0xFF) as u8);
            output.push(((bh >> 8) & 0xFF) as u8);
            output.push(((bh >> 16) & 0xFF) as u8);
            output.push(first);
        } else {
            // Raw block (no FSE entropy coding in this simplified version)
            let bh = (if is_last { 1u32 } else { 0 }) | ((block_size as u32) << 3);
            output.push((bh & 0xFF) as u8);
            output.push(((bh >> 8) & 0xFF) as u8);
            output.push(((bh >> 16) & 0xFF) as u8);
            output.extend_from_slice(block_data);
        }

        pos += block_size;
    }

    Ok(output)
}

/// Check if data starts with the Zstd magic number.
pub fn is_zstd(data: &[u8]) -> bool {
    data.len() >= 4 && u32::from_le_bytes([data[0], data[1], data[2], data[3]]) == ZSTD_MAGIC
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_zstd() {
        let data = zstd_compress(b"test", ZstdLevel::L1).unwrap();
        assert!(is_zstd(&data));
        assert!(!is_zstd(b"not zstd"));
    }

    #[test]
    fn test_compress_empty() {
        let data = zstd_compress(b"", ZstdLevel::L1).unwrap();
        assert!(is_zstd(&data));
    }
}
