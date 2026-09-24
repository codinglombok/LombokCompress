//! Zstandard decompressor (raw + RLE blocks).

use crate::error::CompressError;

const ZSTD_MAGIC: u32 = 0xFD2FB528;

/// Decompress a Zstandard frame.
pub fn zstd_decompress(input: &[u8]) -> Result<Vec<u8>, CompressError> {
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
            4 => u32::from_le_bytes([
                input[pos],
                input[pos + 1],
                input[pos + 2],
                input[pos + 3],
            ]) as u64,
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
        Some(sz as usize)
    } else {
        None
    };

    let mut output = Vec::with_capacity(content_size.unwrap_or(4096));

    // Read blocks
    loop {
        if pos + 3 > input.len() {
            return Err(CompressError::UnexpectedEof);
        }

        let bh = input[pos] as u32
            | ((input[pos + 1] as u32) << 8)
            | ((input[pos + 2] as u32) << 16);
        pos += 3;

        let last_block = (bh & 1) != 0;
        let block_type = (bh >> 1) & 0x03;
        let block_size = (bh >> 3) as usize;

        match block_type {
            0 => {
                // Raw block
                if pos + block_size > input.len() {
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
                for _ in 0..block_size {
                    output.push(byte);
                }
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

    // Content checksum (4 bytes, if flag set) — skip for now
    if _content_checksum && pos + 4 <= input.len() {
        // Could verify XXH64 lower 32 bits here
        pos += 4;
    }

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::compress::{zstd_compress, ZstdLevel};

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
