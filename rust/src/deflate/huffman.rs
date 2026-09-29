//! Huffman coding for Deflate compression.

use crate::error::CompressError;
use crate::prelude::Vec;

/// Fixed Huffman literal/length codes (RFC 1951 Section 3.2.6).
///
/// Lit Value | Bits | Codes
/// 0-143     | 8    | 00110000 - 10111111
/// 144-255   | 9    | 110010000 - 111111111
/// 256-279   | 7    | 0000000 - 0010111
/// 280-287   | 8    | 11000000 - 11000111
pub fn fixed_literal_code(value: u16) -> (u16, u8) {
    match value {
        0..=143 => {
            let code = 0x30 + value; // 0b00110000 + value
            (reverse_bits(code, 8), 8)
        }
        144..=255 => {
            let code = 0x190 + (value - 144); // 0b110010000 + offset
            (reverse_bits(code, 9), 9)
        }
        256..=279 => {
            let code = value - 256; // 0b0000000 + offset
            (reverse_bits(code, 7), 7)
        }
        280..=287 => {
            let code = 0xC0 + (value - 280); // 0b11000000 + offset
            (reverse_bits(code, 8), 8)
        }
        _ => (0, 0),
    }
}

/// Fixed Huffman distance codes: 5-bit codes 0-29.
pub fn fixed_distance_code(dist: u16) -> (u16, u8) {
    (reverse_bits(dist, 5), 5)
}

/// Reverse the bottom `bits` bits of `value`.
pub fn reverse_bits(value: u16, bits: u8) -> u16 {
    let mut result = 0u16;
    let mut v = value;
    for _ in 0..bits {
        result = (result << 1) | (v & 1);
        v >>= 1;
    }
    result
}

/// Length encoding table for deflate (RFC 1951 Section 3.2.5).
/// Returns (code, extra_bits, extra_value).
pub fn encode_length(length: usize) -> (u16, u8, u16) {
    match length {
        3 => (257, 0, 0),
        4 => (258, 0, 0),
        5 => (259, 0, 0),
        6 => (260, 0, 0),
        7 => (261, 0, 0),
        8 => (262, 0, 0),
        9 => (263, 0, 0),
        10 => (264, 0, 0),
        11..=12 => (265, 1, (length - 11) as u16),
        13..=14 => (266, 1, (length - 13) as u16),
        15..=16 => (267, 1, (length - 15) as u16),
        17..=18 => (268, 1, (length - 17) as u16),
        19..=22 => (269, 2, (length - 19) as u16),
        23..=26 => (270, 2, (length - 23) as u16),
        27..=30 => (271, 2, (length - 27) as u16),
        31..=34 => (272, 2, (length - 31) as u16),
        35..=42 => (273, 3, (length - 35) as u16),
        43..=50 => (274, 3, (length - 43) as u16),
        51..=58 => (275, 3, (length - 51) as u16),
        59..=66 => (276, 3, (length - 59) as u16),
        67..=82 => (277, 4, (length - 67) as u16),
        83..=98 => (278, 4, (length - 83) as u16),
        99..=114 => (279, 4, (length - 99) as u16),
        115..=130 => (280, 4, (length - 115) as u16),
        131..=162 => (281, 5, (length - 131) as u16),
        163..=194 => (282, 5, (length - 163) as u16),
        195..=226 => (283, 5, (length - 195) as u16),
        227..=257 => (284, 5, (length - 227) as u16),
        258 => (285, 0, 0),
        _ => (285, 0, 0),
    }
}

/// Distance encoding table (RFC 1951 Section 3.2.5).
/// Returns (code, extra_bits, extra_value).
pub fn encode_distance(dist: usize) -> (u16, u8, u16) {
    match dist {
        1 => (0, 0, 0),
        2 => (1, 0, 0),
        3 => (2, 0, 0),
        4 => (3, 0, 0),
        5..=6 => (4, 1, (dist - 5) as u16),
        7..=8 => (5, 1, (dist - 7) as u16),
        9..=12 => (6, 2, (dist - 9) as u16),
        13..=16 => (7, 2, (dist - 13) as u16),
        17..=24 => (8, 3, (dist - 17) as u16),
        25..=32 => (9, 3, (dist - 25) as u16),
        33..=48 => (10, 4, (dist - 33) as u16),
        49..=64 => (11, 4, (dist - 49) as u16),
        65..=96 => (12, 5, (dist - 65) as u16),
        97..=128 => (13, 5, (dist - 97) as u16),
        129..=192 => (14, 6, (dist - 129) as u16),
        193..=256 => (15, 6, (dist - 193) as u16),
        257..=384 => (16, 7, (dist - 257) as u16),
        385..=512 => (17, 7, (dist - 385) as u16),
        513..=768 => (18, 8, (dist - 513) as u16),
        769..=1024 => (19, 8, (dist - 769) as u16),
        1025..=1536 => (20, 9, (dist - 1025) as u16),
        1537..=2048 => (21, 9, (dist - 1537) as u16),
        2049..=3072 => (22, 10, (dist - 2049) as u16),
        3073..=4096 => (23, 10, (dist - 3073) as u16),
        4097..=6144 => (24, 11, (dist - 4097) as u16),
        6145..=8192 => (25, 11, (dist - 6145) as u16),
        8193..=12288 => (26, 12, (dist - 8193) as u16),
        12289..=16384 => (27, 12, (dist - 12289) as u16),
        16385..=24576 => (28, 13, (dist - 16385) as u16),
        24577..=32768 => (29, 13, (dist - 24577) as u16),
        _ => (29, 13, 0),
    }
}

/// Decode a length value from a code and extra bits.
pub fn decode_length(code: u16, extra: u16) -> usize {
    match code {
        257..=264 => (code - 257 + 3) as usize,
        265..=268 => (2 * (code - 265) + 11 + extra) as usize,
        269..=272 => (4 * (code - 269) + 19 + extra) as usize,
        273..=276 => (8 * (code - 273) + 35 + extra) as usize,
        277..=280 => (16 * (code - 277) + 67 + extra) as usize,
        281..=284 => (32 * (code - 281) + 131 + extra) as usize,
        285 => 258,
        _ => 0,
    }
}

/// Decode a distance value from a code and extra bits.
pub fn decode_distance(code: u16, extra: u16) -> usize {
    if code <= 3 {
        return (code + 1) as usize;
    }
    let n_extra = (code / 2 - 1) as u32;
    let base = (1u32 << (n_extra + 1)) + 1;
    let offset = (code & 1) << n_extra;
    (base + offset as u32 + extra as u32) as usize
}

/// Bit writer for deflate output.
pub struct BitWriter {
    pub buf: Vec<u8>,
    bits: u32,
    nbits: u8,
}

impl Default for BitWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl BitWriter {
    pub fn new() -> Self {
        Self {
            buf: Vec::new(),
            bits: 0,
            nbits: 0,
        }
    }

    /// Write `count` bits from `value` (LSB first).
    pub fn write_bits(&mut self, value: u32, count: u8) {
        self.bits |= value << self.nbits;
        self.nbits += count;
        while self.nbits >= 8 {
            self.buf.push(self.bits as u8);
            self.bits >>= 8;
            self.nbits -= 8;
        }
    }

    /// Flush remaining bits (pad with zeros).
    pub fn flush(&mut self) {
        if self.nbits > 0 {
            self.buf.push(self.bits as u8);
            self.bits = 0;
            self.nbits = 0;
        }
    }
}

/// Bit reader for deflate input.
pub struct BitReader<'a> {
    pub data: &'a [u8],
    pub pos: usize,
    bits: u32,
    nbits: u8,
}

impl<'a> BitReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
            bits: 0,
            nbits: 0,
        }
    }

    /// Read `count` bits (LSB first).
    pub fn read_bits(&mut self, count: u8) -> Result<u32, CompressError> {
        debug_assert!(count <= 24);
        while self.nbits < count {
            if self.pos >= self.data.len() {
                return Err(CompressError::UnexpectedEof);
            }
            self.bits |= (self.data[self.pos] as u32) << self.nbits;
            self.pos += 1;
            self.nbits += 8;
        }
        let mask = (1u32 << count) - 1;
        let result = self.bits & mask;
        self.bits >>= count;
        self.nbits -= count;
        Ok(result)
    }

    /// Align to byte boundary.
    pub fn align(&mut self) {
        self.bits = 0;
        self.nbits = 0;
    }
}
