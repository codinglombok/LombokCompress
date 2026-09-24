//! Deflate decompression (inflate).

use crate::error::CompressError;
use super::huffman::{self, BitReader};
use super::compress::{crc32, adler32};

/// Decompress raw deflate data.
pub fn deflate_decompress(input: &[u8], max_output: usize) -> Result<Vec<u8>, CompressError> {
    let mut output = Vec::with_capacity(core::cmp::min(max_output, input.len() * 4));
    let mut reader = BitReader::new(input);

    loop {
        let bfinal = reader
            .read_bits(1)
            .map_err(|_| CompressError::UnexpectedEof)?;
        let btype = reader
            .read_bits(2)
            .map_err(|_| CompressError::UnexpectedEof)?;

        match btype {
            0b00 => {
                // Stored (uncompressed)
                reader.align();
                if reader.pos + 4 > reader.data.len() {
                    return Err(CompressError::UnexpectedEof);
                }
                let len = reader.data[reader.pos] as usize
                    | ((reader.data[reader.pos + 1] as usize) << 8);
                let _nlen = reader.data[reader.pos + 2] as usize
                    | ((reader.data[reader.pos + 3] as usize) << 8);
                reader.pos += 4;

                if reader.pos + len > reader.data.len() {
                    return Err(CompressError::UnexpectedEof);
                }
                output.extend_from_slice(&reader.data[reader.pos..reader.pos + len]);
                reader.pos += len;
            }
            0b01 => {
                // Fixed Huffman
                inflate_fixed_huffman(&mut reader, &mut output, max_output)?;
            }
            0b10 => {
                // Dynamic Huffman — simplified: decode the code length tables
                return Err(CompressError::Unsupported("dynamic Huffman not yet implemented"));
            }
            _ => {
                return Err(CompressError::InvalidInput("invalid deflate block type"));
            }
        }

        if bfinal == 1 {
            break;
        }
    }

    Ok(output)
}

/// Inflate a block with fixed Huffman codes.
fn inflate_fixed_huffman(
    reader: &mut BitReader,
    output: &mut Vec<u8>,
    max_output: usize,
) -> Result<(), CompressError> {
    loop {
        // Decode literal/length using fixed Huffman tree
        let sym = decode_fixed_literal(reader)?;

        if sym == 256 {
            // End of block
            break;
        }

        if sym < 256 {
            // Literal
            if output.len() >= max_output {
                return Err(CompressError::OutputTooSmall {
                    needed: output.len() + 1,
                    available: max_output,
                });
            }
            output.push(sym as u8);
        } else {
            // Length code (257-285)
            let extra_bits = length_extra_bits(sym);
            let extra = if extra_bits > 0 {
                reader
                    .read_bits(extra_bits)
                    .map_err(|_| CompressError::UnexpectedEof)? as u16
            } else {
                0
            };
            let length = huffman::decode_length(sym, extra);

            // Decode distance (5-bit fixed code, stored MSB-first)
            let raw_dist = reader
                .read_bits(5)
                .map_err(|_| CompressError::UnexpectedEof)? as u16;
            let dist_code = huffman::reverse_bits(raw_dist, 5);
            let dist_extra_bits = distance_extra_bits(dist_code);
            let dist_extra = if dist_extra_bits > 0 {
                reader
                    .read_bits(dist_extra_bits)
                    .map_err(|_| CompressError::UnexpectedEof)? as u16
            } else {
                0
            };
            let distance = huffman::decode_distance(dist_code, dist_extra);

            if distance > output.len() {
                return Err(CompressError::InvalidInput(
                    "deflate distance exceeds output buffer",
                ));
            }

            // Copy match (may overlap)
            let start = output.len() - distance;
            for i in 0..length {
                if output.len() >= max_output {
                    return Err(CompressError::OutputTooSmall {
                        needed: output.len() + 1,
                        available: max_output,
                    });
                }
                let b = output[start + i];
                output.push(b);
            }
        }
    }

    Ok(())
}

/// Decode a literal/length symbol from fixed Huffman codes.
fn decode_fixed_literal(reader: &mut BitReader) -> Result<u16, CompressError> {
    // Read 7 bits first
    let b7 = reader
        .read_bits(7)
        .map_err(|_| CompressError::UnexpectedEof)? as u16;
    let rev7 = huffman::reverse_bits(b7, 7);

    // Codes 256-279 are 7-bit (0000000 - 0010111)
    if rev7 <= 23 {
        return Ok(rev7 + 256);
    }

    // Read 1 more bit (8 total)
    let b8_extra = reader
        .read_bits(1)
        .map_err(|_| CompressError::UnexpectedEof)? as u16;
    let b8 = (b7 | (b8_extra << 7)) as u16;
    let rev8 = huffman::reverse_bits(b8, 8);

    // Codes 0-143 are 8-bit (00110000 - 10111111)
    if rev8 >= 0x30 && rev8 <= 0xBF {
        return Ok(rev8 - 0x30);
    }

    // Codes 280-287 are 8-bit (11000000 - 11000111)
    if rev8 >= 0xC0 && rev8 <= 0xC7 {
        return Ok(rev8 - 0xC0 + 280);
    }

    // Read 1 more bit (9 total)
    let b9_extra = reader
        .read_bits(1)
        .map_err(|_| CompressError::UnexpectedEof)? as u16;
    let b9 = (b8 | (b9_extra << 8)) as u16;
    let rev9 = huffman::reverse_bits(b9, 9);

    // Codes 144-255 are 9-bit (110010000 - 111111111)
    if rev9 >= 0x190 && rev9 <= 0x1FF {
        return Ok(rev9 - 0x190 + 144);
    }

    Err(CompressError::InvalidInput("invalid fixed Huffman code"))
}

fn length_extra_bits(code: u16) -> u8 {
    match code {
        257..=264 | 285 => 0,
        265..=268 => 1,
        269..=272 => 2,
        273..=276 => 3,
        277..=280 => 4,
        281..=284 => 5,
        _ => 0,
    }
}

fn distance_extra_bits(code: u16) -> u8 {
    if code <= 3 {
        0
    } else if code <= 29 {
        (code / 2 - 1) as u8
    } else {
        0
    }
}

/// Decompress gzip data (RFC 1952).
pub fn gzip_decompress(input: &[u8]) -> Result<Vec<u8>, CompressError> {
    if input.len() < 18 {
        return Err(CompressError::UnexpectedEof);
    }

    // Verify gzip header
    if input[0] != 0x1F || input[1] != 0x8B {
        return Err(CompressError::InvalidInput("invalid gzip magic"));
    }
    if input[2] != 0x08 {
        return Err(CompressError::InvalidInput("unsupported gzip method"));
    }

    let flg = input[3];
    let mut pos = 10; // Skip header

    // Skip optional fields
    if flg & 0x04 != 0 {
        // FEXTRA
        if pos + 2 > input.len() {
            return Err(CompressError::UnexpectedEof);
        }
        let xlen = input[pos] as usize | ((input[pos + 1] as usize) << 8);
        pos += 2 + xlen;
    }
    if flg & 0x08 != 0 {
        // FNAME
        while pos < input.len() && input[pos] != 0 {
            pos += 1;
        }
        pos += 1; // skip null
    }
    if flg & 0x10 != 0 {
        // FCOMMENT
        while pos < input.len() && input[pos] != 0 {
            pos += 1;
        }
        pos += 1;
    }
    if flg & 0x02 != 0 {
        // FHCRC
        pos += 2;
    }

    if pos >= input.len() {
        return Err(CompressError::UnexpectedEof);
    }

    // Deflate data ends 8 bytes before the end (CRC32 + ISIZE)
    let deflate_end = input.len() - 8;
    let deflate_data = &input[pos..deflate_end];

    let decompressed = deflate_decompress(deflate_data, 64 * 1024 * 1024)?;

    // Verify CRC32
    let expected_crc = u32::from_le_bytes([
        input[deflate_end],
        input[deflate_end + 1],
        input[deflate_end + 2],
        input[deflate_end + 3],
    ]);
    let actual_crc = crc32(&decompressed);
    if expected_crc != actual_crc {
        return Err(CompressError::ChecksumMismatch {
            expected: expected_crc,
            actual: actual_crc,
        });
    }

    Ok(decompressed)
}

/// Decompress zlib data (RFC 1950).
pub fn zlib_decompress(input: &[u8]) -> Result<Vec<u8>, CompressError> {
    if input.len() < 6 {
        return Err(CompressError::UnexpectedEof);
    }

    let cmf = input[0];
    let _flg = input[1];

    // Verify CM = 8 (deflate)
    if cmf & 0x0F != 8 {
        return Err(CompressError::InvalidInput("unsupported zlib method"));
    }

    let deflate_data = &input[2..input.len() - 4];
    let decompressed = deflate_decompress(deflate_data, 64 * 1024 * 1024)?;

    // Verify Adler-32
    let expected_adler = u32::from_be_bytes([
        input[input.len() - 4],
        input[input.len() - 3],
        input[input.len() - 2],
        input[input.len() - 1],
    ]);
    let actual_adler = adler32(&decompressed);
    if expected_adler != actual_adler {
        return Err(CompressError::ChecksumMismatch {
            expected: expected_adler,
            actual: actual_adler,
        });
    }

    Ok(decompressed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::compress;

    #[test]
    fn test_deflate_roundtrip() {
        let data = b"Hello World! Hello World! This is a test.";
        let compressed = compress::deflate_compress(data);
        let decompressed = deflate_decompress(&compressed, 1024).unwrap();
        assert_eq!(&decompressed, &data[..]);
    }

    #[test]
    fn test_gzip_roundtrip() {
        let data = b"Test data for gzip compression roundtrip testing.";
        let compressed = compress::gzip_compress(data);
        let decompressed = gzip_decompress(&compressed).unwrap();
        assert_eq!(&decompressed, &data[..]);
    }

    #[test]
    fn test_zlib_roundtrip() {
        let data = b"Zlib compression test data with some repeated content content content.";
        let compressed = compress::zlib_compress(data);
        let decompressed = zlib_decompress(&compressed).unwrap();
        assert_eq!(&decompressed, &data[..]);
    }
}
