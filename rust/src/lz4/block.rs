//! LZ4 block compression and decompression.
//!
//! Implements LZ4 block format (no framing). Compatible with the LZ4 specification.

use crate::error::CompressError;

const HASH_LOG: usize = 12;
const HASH_SIZE: usize = 1 << HASH_LOG;
const MIN_MATCH: usize = 4;
const ML_BITS: u32 = 4;
const ML_MASK: usize = (1 << ML_BITS) - 1; // 15
const RUN_BITS: u32 = 4;
const RUN_MASK: usize = (1 << RUN_BITS) - 1; // 15
const LAST_LITERALS: usize = 5;
const MF_LIMIT: usize = 12; // Minimum input size for match finding

/// Maximum compressed size for a given input length.
pub fn compress_bound(input_len: usize) -> usize {
    if input_len == 0 {
        return 0;
    }
    input_len + (input_len / 255) + 16
}

/// Compress a block of data using LZ4.
///
/// Returns the number of bytes written to `output`.
pub fn compress_block(input: &[u8], output: &mut [u8]) -> Result<usize, CompressError> {
    let src_len = input.len();
    if src_len == 0 {
        return Ok(0);
    }

    let mut hash_table = [0u16; HASH_SIZE];
    let mut src_pos: usize = 0;
    let mut dst_pos: usize = 0;
    let mut anchor: usize = 0;

    let src_limit = if src_len > MF_LIMIT {
        src_len - MF_LIMIT
    } else {
        // Input too short for match finding, emit as literals
        return write_last_literals(input, 0, src_len, output, 0);
    };

    src_pos += 1; // Skip first byte

    loop {
        // Find a match
        let mut find_pos = src_pos;
        let mut step = 1u32;
        let mut match_pos;

        loop {
            src_pos = find_pos;
            find_pos += step as usize;
            step += 1;

            if find_pos > src_limit {
                return write_last_literals(input, anchor, src_len - anchor, output, dst_pos);
            }

            let h = hash4(input, src_pos);
            match_pos = hash_table[h] as usize;
            hash_table[h] = src_pos as u16;

            if match_pos < src_pos
                && src_pos - match_pos <= 0xFFFF
                && input[match_pos] == input[src_pos]
                && input[match_pos + 1] == input[src_pos + 1]
                && input[match_pos + 2] == input[src_pos + 2]
                && input[match_pos + 3] == input[src_pos + 3]
            {
                break;
            }
        }

        // Encode literal length
        let lit_len = src_pos - anchor;
        let token_pos = dst_pos;
        dst_pos += 1;

        if dst_pos > output.len() {
            return Err(CompressError::OutputTooSmall {
                needed: dst_pos,
                available: output.len(),
            });
        }

        // Write literal length
        if lit_len >= RUN_MASK {
            output[token_pos] = (RUN_MASK as u8) << 4;
            let mut remaining = lit_len - RUN_MASK;
            while remaining >= 255 {
                if dst_pos >= output.len() {
                    return Err(CompressError::OutputTooSmall {
                        needed: dst_pos + 1,
                        available: output.len(),
                    });
                }
                output[dst_pos] = 255;
                dst_pos += 1;
                remaining -= 255;
            }
            if dst_pos >= output.len() {
                return Err(CompressError::OutputTooSmall {
                    needed: dst_pos + 1,
                    available: output.len(),
                });
            }
            output[dst_pos] = remaining as u8;
            dst_pos += 1;
        } else {
            output[token_pos] = (lit_len as u8) << 4;
        }

        // Copy literals
        if dst_pos + lit_len > output.len() {
            return Err(CompressError::OutputTooSmall {
                needed: dst_pos + lit_len,
                available: output.len(),
            });
        }
        output[dst_pos..dst_pos + lit_len].copy_from_slice(&input[anchor..anchor + lit_len]);
        dst_pos += lit_len;

        // Encode match
        loop {
            // Write offset (little-endian 16-bit)
            let offset = (src_pos - match_pos) as u16;
            if dst_pos + 2 > output.len() {
                return Err(CompressError::OutputTooSmall {
                    needed: dst_pos + 2,
                    available: output.len(),
                });
            }
            output[dst_pos] = offset as u8;
            output[dst_pos + 1] = (offset >> 8) as u8;
            dst_pos += 2;

            // Count match length beyond MIN_MATCH
            let mut match_len = MIN_MATCH;
            while src_pos + match_len < src_len
                && match_pos + match_len < src_pos
                && input[src_pos + match_len] == input[match_pos + match_len]
            {
                match_len += 1;
            }
            let extra_match = match_len - MIN_MATCH;

            // Write match length in token
            if extra_match >= ML_MASK {
                output[token_pos] |= ML_MASK as u8;
                let mut remaining = extra_match - ML_MASK;
                while remaining >= 255 {
                    if dst_pos >= output.len() {
                        return Err(CompressError::OutputTooSmall {
                            needed: dst_pos + 1,
                            available: output.len(),
                        });
                    }
                    output[dst_pos] = 255;
                    dst_pos += 1;
                    remaining -= 255;
                }
                if dst_pos >= output.len() {
                    return Err(CompressError::OutputTooSmall {
                        needed: dst_pos + 1,
                        available: output.len(),
                    });
                }
                output[dst_pos] = remaining as u8;
                dst_pos += 1;
            } else {
                output[token_pos] |= extra_match as u8;
            }

            src_pos += match_len;
            anchor = src_pos;

            if src_pos >= src_limit {
                return write_last_literals(
                    input,
                    anchor,
                    src_len - anchor,
                    output,
                    dst_pos,
                );
            }

            // Update hash table and try to find next match
            let h = hash4(input, src_pos);
            match_pos = hash_table[h] as usize;
            hash_table[h] = src_pos as u16;

            if match_pos >= src_pos
                || src_pos - match_pos > 0xFFFF
                || input[match_pos] != input[src_pos]
                || input[match_pos + 1] != input[src_pos + 1]
                || input[match_pos + 2] != input[src_pos + 2]
                || input[match_pos + 3] != input[src_pos + 3]
            {
                // No match found, go back to outer loop
                break;
            }

            // Found another match, encode it in the same sequence
            // Token for zero literals + new match
            let token_pos_new = dst_pos;
            dst_pos += 1;
            if dst_pos > output.len() {
                return Err(CompressError::OutputTooSmall {
                    needed: dst_pos,
                    available: output.len(),
                });
            }
            output[token_pos_new] = 0; // zero literals
            // Continue loop to encode the match
            // Need to reassign token_pos for the match encoding above
            // Actually we break here and let the outer loop handle it
            break;
        }

        src_pos += 1;
    }
}

fn write_last_literals(
    input: &[u8],
    anchor: usize,
    lit_len: usize,
    output: &mut [u8],
    mut dst_pos: usize,
) -> Result<usize, CompressError> {
    if lit_len == 0 {
        return Ok(dst_pos);
    }

    // Token
    let token_pos = dst_pos;
    dst_pos += 1;

    if lit_len >= RUN_MASK {
        output[token_pos] = (RUN_MASK as u8) << 4;
        let mut remaining = lit_len - RUN_MASK;
        while remaining >= 255 {
            if dst_pos >= output.len() {
                return Err(CompressError::OutputTooSmall {
                    needed: dst_pos + 1,
                    available: output.len(),
                });
            }
            output[dst_pos] = 255;
            dst_pos += 1;
            remaining -= 255;
        }
        if dst_pos >= output.len() {
            return Err(CompressError::OutputTooSmall {
                needed: dst_pos + 1,
                available: output.len(),
            });
        }
        output[dst_pos] = remaining as u8;
        dst_pos += 1;
    } else {
        output[token_pos] = (lit_len as u8) << 4;
    }

    if dst_pos + lit_len > output.len() {
        return Err(CompressError::OutputTooSmall {
            needed: dst_pos + lit_len,
            available: output.len(),
        });
    }
    output[dst_pos..dst_pos + lit_len].copy_from_slice(&input[anchor..anchor + lit_len]);
    dst_pos += lit_len;

    Ok(dst_pos)
}

/// Decompress an LZ4 block.
///
/// `output` must be large enough to hold the decompressed data.
/// Returns the number of bytes written.
pub fn decompress_block(input: &[u8], output: &mut [u8]) -> Result<usize, CompressError> {
    let src_len = input.len();
    if src_len == 0 {
        return Ok(0);
    }

    let mut src_pos: usize = 0;
    let mut dst_pos: usize = 0;

    loop {
        if src_pos >= src_len {
            return Ok(dst_pos);
        }

        // Read token
        let token = input[src_pos];
        src_pos += 1;

        // Decode literal length
        let mut lit_len = ((token >> 4) & 0x0F) as usize;
        if lit_len == RUN_MASK {
            loop {
                if src_pos >= src_len {
                    return Err(CompressError::UnexpectedEof);
                }
                let b = input[src_pos] as usize;
                src_pos += 1;
                lit_len += b;
                if b < 255 {
                    break;
                }
            }
        }

        // Copy literals
        if src_pos + lit_len > src_len {
            return Err(CompressError::UnexpectedEof);
        }
        if dst_pos + lit_len > output.len() {
            return Err(CompressError::OutputTooSmall {
                needed: dst_pos + lit_len,
                available: output.len(),
            });
        }
        output[dst_pos..dst_pos + lit_len].copy_from_slice(&input[src_pos..src_pos + lit_len]);
        src_pos += lit_len;
        dst_pos += lit_len;

        // Check if this is the last sequence (no match after last literals)
        if src_pos >= src_len {
            return Ok(dst_pos);
        }

        // Read match offset (16-bit little-endian)
        if src_pos + 2 > src_len {
            return Err(CompressError::UnexpectedEof);
        }
        let offset = input[src_pos] as usize | ((input[src_pos + 1] as usize) << 8);
        src_pos += 2;

        if offset == 0 {
            return Err(CompressError::InvalidInput("match offset is zero"));
        }
        if offset > dst_pos {
            return Err(CompressError::InvalidInput("match offset exceeds output position"));
        }

        // Decode match length
        let mut match_len = (token & 0x0F) as usize + MIN_MATCH;
        if (token & 0x0F) == ML_MASK as u8 {
            loop {
                if src_pos >= src_len {
                    return Err(CompressError::UnexpectedEof);
                }
                let b = input[src_pos] as usize;
                src_pos += 1;
                match_len += b;
                if b < 255 {
                    break;
                }
            }
        }

        // Copy match (may overlap)
        if dst_pos + match_len > output.len() {
            return Err(CompressError::OutputTooSmall {
                needed: dst_pos + match_len,
                available: output.len(),
            });
        }
        let match_start = dst_pos - offset;
        for j in 0..match_len {
            output[dst_pos + j] = output[match_start + j];
        }
        dst_pos += match_len;
    }
}

#[inline(always)]
fn hash4(data: &[u8], pos: usize) -> usize {
    let v = u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]);
    ((v.wrapping_mul(2654435761)) >> (32 - HASH_LOG)) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_roundtrip_empty() {
        let mut compressed = vec![0u8; 16];
        let csize = compress_block(b"", &mut compressed).unwrap();
        assert_eq!(csize, 0);
    }

    #[test]
    fn test_roundtrip_short() {
        let data = b"Hello";
        let mut compressed = vec![0u8; compress_bound(data.len())];
        let csize = compress_block(data, &mut compressed).unwrap();
        compressed.truncate(csize);

        let mut decompressed = vec![0u8; data.len()];
        let dsize = decompress_block(&compressed, &mut decompressed).unwrap();
        assert_eq!(&decompressed[..dsize], &data[..]);
    }

    #[test]
    fn test_roundtrip_repeated() {
        let data = b"abcdefghijklmnopabcdefghijklmnop";
        let mut compressed = vec![0u8; compress_bound(data.len())];
        let csize = compress_block(data, &mut compressed).unwrap();
        compressed.truncate(csize);

        let mut decompressed = vec![0u8; data.len()];
        let dsize = decompress_block(&compressed, &mut decompressed).unwrap();
        assert_eq!(&decompressed[..dsize], &data[..]);
    }

    #[test]
    fn test_roundtrip_large() {
        let mut data = Vec::new();
        for i in 0..1000u32 {
            data.extend_from_slice(&i.to_le_bytes());
        }
        let mut compressed = vec![0u8; compress_bound(data.len())];
        let csize = compress_block(&data, &mut compressed).unwrap();
        compressed.truncate(csize);

        let mut decompressed = vec![0u8; data.len()];
        let dsize = decompress_block(&compressed, &mut decompressed).unwrap();
        assert_eq!(&decompressed[..dsize], &data[..]);
    }
}
