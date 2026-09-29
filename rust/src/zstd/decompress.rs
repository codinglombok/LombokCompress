//! Zstandard decompressor (raw + RLE blocks).

use crate::error::CompressError;
use crate::prelude::Vec;

const ZSTD_MAGIC: u32 = 0xFD2FB528;

/// Block_Maximum_Size from RFC 8878 §3.1.1.2.3 (128 KiB).
const BLOCK_MAX_SIZE: usize = 128 * 1024;

/// Upper bound for buffers pre-allocated from untrusted header fields.
const MAX_INITIAL_CAPACITY: usize = 1 << 20;

/// Decompress a Zstandard frame.
pub fn zstd_decompress(input: &[u8]) -> Result<Vec<u8>, CompressError> {
    zstd_decompress_limited(input, usize::MAX)
}

/// Decompress a Zstandard frame, refusing to produce more than `max_output`
/// bytes. Use this for untrusted input to bound memory use.
pub fn zstd_decompress_limited(input: &[u8], max_output: usize) -> Result<Vec<u8>, CompressError> {
    if input.len() < 5 {
        return Err(CompressError::UnexpectedEof);
    }

    let mut pos = 0;

    // Magic number
    let magic = u32::from_le_bytes([input[0], input[1], input[2], input[3]]);
    if magic != ZSTD_MAGIC {
        return Err(CompressError::InvalidInput("invalid Zstd magic number"));
    }
    pos += 4;

    // Frame header descriptor
    let fhd = input[pos];
    pos += 1;

    let single_segment = (fhd & 0x20) != 0;
    let fcs_field_size = match (fhd >> 6) & 0x03 {
        0 => {
            if single_segment {
                1
            } else {
                0
            }
        }
        1 => 2,
        2 => 4,
        3 => 8,
        _ => unreachable!(),
    };

    let _dict_id_flag = fhd & 0x03;
    let _content_checksum = (fhd & 0x04) != 0;

    // Window descriptor (absent if single segment)
    if !single_segment {
        if pos >= input.len() {
            return Err(CompressError::UnexpectedEof);
        }
        let _window_desc = input[pos];
        pos += 1;
    }

    // Dictionary ID (skip based on flag)
    let dict_id_bytes = match _dict_id_flag {
        0 => 0,
        1 => 1,
        2 => 2,
        3 => 4,
        _ => 0,
    };
    pos += dict_id_bytes;
    if pos > input.len() {
        return Err(CompressError::UnexpectedEof);
    }

    // Frame content size
    let content_size = if fcs_field_size > 0 {
        if pos + fcs_field_size > input.len() {
            return Err(CompressError::UnexpectedEof);
        }
        let sz = match fcs_field_size {
            1 => input[pos] as u64,
            2 => {
                let v = u16::from_le_bytes([input[pos], input[pos + 1]]);
                v as u64 + 256
            }
            4 => u32::from_le_bytes([input[pos], input[pos + 1], input[pos + 2], input[pos + 3]])
                as u64,
            8 => u64::from_le_bytes([
                input[pos],
                input[pos + 1],
                input[pos + 2],
                input[pos + 3],
                input[pos + 4],
                input[pos + 5],
                input[pos + 6],
                input[pos + 7],
            ]),
            _ => 0,
        };
        pos += fcs_field_size;
        Some(
            usize::try_from(sz).map_err(|_| {
                CompressError::Unsupported("Zstd content size exceeds address space")
            })?,
        )
    } else {
        None
    };

    if let Some(sz) = content_size {
        if sz > max_output {
            return Err(CompressError::OutputTooSmall {
                needed: sz,
                available: max_output,
            });
        }
    }

    // The declared content size is untrusted: never pre-allocate from it alone.
    let mut output = Vec::with_capacity(core::cmp::min(
        content_size.unwrap_or(4096),
        MAX_INITIAL_CAPACITY,
    ));

    // Read blocks
    loop {
        if pos + 3 > input.len() {
            return Err(CompressError::UnexpectedEof);
        }

        let bh =
            input[pos] as u32 | ((input[pos + 1] as u32) << 8) | ((input[pos + 2] as u32) << 16);
        pos += 3;

        let last_block = (bh & 1) != 0;
        let block_type = (bh >> 1) & 0x03;
        let block_size = (bh >> 3) as usize;

        if block_size > BLOCK_MAX_SIZE {
            return Err(CompressError::InvalidInput(
                "Zstd block exceeds Block_Maximum_Size",
            ));
        }
        if block_type < 2 && block_size > max_output - output.len() {
            return Err(CompressError::OutputTooSmall {
                needed: output.len() + block_size,
                available: max_output,
            });
        }

        match block_type {
            0 => {
                // Raw block
                if block_size > input.len() - pos {
                    return Err(CompressError::UnexpectedEof);
                }
                output.extend_from_slice(&input[pos..pos + block_size]);
                pos += block_size;
            }
            1 => {
                // RLE block — one byte repeated block_size times
                if pos >= input.len() {
                    return Err(CompressError::UnexpectedEof);
                }
                let byte = input[pos];
                pos += 1;
                output.resize(output.len() + block_size, byte);
            }
            2 => {
                // Compressed block (FSE/Huffman) — not supported in this simplified version
                return Err(CompressError::Unsupported(
                    "Zstd compressed blocks (FSE) not yet implemented",
                ));
            }
            3 => {
                return Err(CompressError::InvalidInput("reserved Zstd block type"));
            }
            _ => unreachable!(),
        }

        if last_block {
            break;
        }
    }

    // Content checksum (4 bytes, if flag set): XXH64 is not implemented yet,
    // so only its presence is checked.
    if _content_checksum && input.len() - pos < 4 {
        return Err(CompressError::UnexpectedEof);
    }

    if let Some(expected) = content_size {
        if output.len() != expected {
            return Err(CompressError::InvalidInput(
                "Zstd frame content size does not match decoded length",
            ));
        }
    }

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::super::compress::{zstd_compress, ZstdLevel};
    use super::*;

    #[test]
    fn test_roundtrip() {
        let data = b"Hello World! This is a Zstd compression test.";
        let compressed = zstd_compress(data, ZstdLevel::L1).unwrap();
        let decompressed = zstd_decompress(&compressed).unwrap();
        assert_eq!(&decompressed, &data[..]);
    }

    #[test]
    fn test_roundtrip_empty() {
        let compressed = zstd_compress(b"", ZstdLevel::L1).unwrap();
        let decompressed = zstd_decompress(&compressed).unwrap();
        assert_eq!(decompressed.len(), 0);
    }

    #[test]
    fn test_roundtrip_rle() {
        let data = vec![0xAA; 1000];
        let compressed = zstd_compress(&data, ZstdLevel::L1).unwrap();
        let decompressed = zstd_decompress(&compressed).unwrap();
        assert_eq!(decompressed, data);
        // RLE should be much smaller
        assert!(compressed.len() < data.len());
    }
}
