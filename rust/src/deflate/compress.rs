//! Deflate compression with fixed Huffman codes.

use super::huffman::{self, BitWriter};
use super::lz77::{self, Lz77Token};
use crate::prelude::{vec, Vec};

/// CRC32 lookup table (IEEE polynomial).
const CRC32_TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut i = 0u32;
    while i < 256 {
        let mut crc = i;
        let mut j = 0;
        while j < 8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xEDB88320;
            } else {
                crc >>= 1;
            }
            j += 1;
        }
        table[i as usize] = crc;
        i += 1;
    }
    table
};

/// Compute CRC32 checksum.
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFFFFFFu32;
    for &b in data {
        crc = CRC32_TABLE[((crc ^ b as u32) & 0xFF) as usize] ^ (crc >> 8);
    }
    crc ^ 0xFFFFFFFF
}

/// Compute Adler-32 checksum.
pub fn adler32(data: &[u8]) -> u32 {
    let mut a: u32 = 1;
    let mut b: u32 = 0;
    const MOD: u32 = 65521;

    for &byte in data {
        a = (a + byte as u32) % MOD;
        b = (b + a) % MOD;
    }

    (b << 16) | a
}

/// Compress data using Deflate with fixed Huffman codes.
pub fn deflate_compress(input: &[u8]) -> Vec<u8> {
    let tokens = lz77::lz77_compress(input);
    let mut bw = BitWriter::new();

    // BFINAL=1 (last block), BTYPE=01 (fixed Huffman)
    bw.write_bits(1, 1); // BFINAL
    bw.write_bits(1, 2); // BTYPE = 01 (fixed)

    for token in &tokens {
        match token {
            Lz77Token::Literal(b) => {
                let (code, bits) = huffman::fixed_literal_code(*b as u16);
                bw.write_bits(code as u32, bits);
            }
            Lz77Token::Match { length, distance } => {
                // Encode length
                let (len_code, extra_bits, extra_val) = huffman::encode_length(*length);
                let (code, bits) = huffman::fixed_literal_code(len_code);
                bw.write_bits(code as u32, bits);
                if extra_bits > 0 {
                    bw.write_bits(extra_val as u32, extra_bits);
                }

                // Encode distance
                let (dist_code, dist_extra_bits, dist_extra_val) =
                    huffman::encode_distance(*distance);
                let (dcode, dbits) = huffman::fixed_distance_code(dist_code);
                bw.write_bits(dcode as u32, dbits);
                if dist_extra_bits > 0 {
                    bw.write_bits(dist_extra_val as u32, dist_extra_bits);
                }
            }
        }
    }

    // End-of-block marker (literal 256)
    let (code, bits) = huffman::fixed_literal_code(256);
    bw.write_bits(code as u32, bits);

    bw.flush();
    bw.buf
}

/// Compress data in gzip format (RFC 1952).
pub fn gzip_compress(input: &[u8]) -> Vec<u8> {
    let mut output = vec![
        0x1F, // ID1
        0x8B, // ID2
        0x08, // CM = deflate
        0x00, // FLG = no flags
        0, 0, 0, 0,    // MTIME
        0x00, // XFL
        0xFF, // OS = unknown
    ];

    // Compressed data
    output.extend_from_slice(&deflate_compress(input));

    // CRC32 + ISIZE
    let checksum = crc32(input);
    output.extend_from_slice(&checksum.to_le_bytes());
    output.extend_from_slice(&(input.len() as u32).to_le_bytes());

    output
}

/// Compress data in zlib format (RFC 1950).
pub fn zlib_compress(input: &[u8]) -> Vec<u8> {
    let mut output = Vec::new();

    // Zlib header
    let cmf: u8 = 0x78; // CM=8 (deflate), CINFO=7 (32K window)
    let flg: u8 = 0x01; // FCHECK (makes CMF*256+FLG divisible by 31)
                        // Adjust FCHECK
    let check = (cmf as u16 * 256 + flg as u16) % 31;
    let flg = if check == 0 {
        flg
    } else {
        flg + (31 - check) as u8
    };
    output.push(cmf);
    output.push(flg);

    // Compressed data
    output.extend_from_slice(&deflate_compress(input));

    // Adler-32
    let checksum = adler32(input);
    output.extend_from_slice(&checksum.to_be_bytes());

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crc32_empty() {
        assert_eq!(crc32(b""), 0);
    }

    #[test]
    fn test_crc32_abc() {
        assert_eq!(crc32(b"abc"), 0x352441C2);
    }

    #[test]
    fn test_adler32_empty() {
        assert_eq!(adler32(b""), 1);
    }

    #[test]
    fn test_adler32_abc() {
        assert_eq!(adler32(b"abc"), 0x024D0127);
    }
}
