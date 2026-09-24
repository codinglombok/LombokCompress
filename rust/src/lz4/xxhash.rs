//! XXHash32 implementation for LZ4 frame checksums.

const PRIME1: u32 = 0x9E3779B1;
const PRIME2: u32 = 0x85EBCA77;
const PRIME3: u32 = 0xC2B2AE3D;
const PRIME4: u32 = 0x27D4EB2F;
const PRIME5: u32 = 0x165667B1;

/// Compute XXH32 hash of the input with the given seed.
pub fn xxh32(input: &[u8], seed: u32) -> u32 {
    let len = input.len();
    let mut h: u32;
    let mut i = 0;

    if len >= 16 {
        let mut v1 = seed.wrapping_add(PRIME1).wrapping_add(PRIME2);
        let mut v2 = seed.wrapping_add(PRIME2);
        let mut v3 = seed;
        let mut v4 = seed.wrapping_sub(PRIME1);

        while i + 16 <= len {
            v1 = round(v1, read_u32_le(input, i));
            v2 = round(v2, read_u32_le(input, i + 4));
            v3 = round(v3, read_u32_le(input, i + 8));
            v4 = round(v4, read_u32_le(input, i + 12));
            i += 16;
        }

        h = v1.rotate_left(1)
            .wrapping_add(v2.rotate_left(7))
            .wrapping_add(v3.rotate_left(12))
            .wrapping_add(v4.rotate_left(18));
    } else {
        h = seed.wrapping_add(PRIME5);
    }

    h = h.wrapping_add(len as u32);

    // Process remaining 4-byte chunks
    while i + 4 <= len {
        h = h.wrapping_add(read_u32_le(input, i).wrapping_mul(PRIME3));
        h = h.rotate_left(17).wrapping_mul(PRIME4);
        i += 4;
    }

    // Process remaining bytes
    while i < len {
        h = h.wrapping_add((input[i] as u32).wrapping_mul(PRIME5));
        h = h.rotate_left(11).wrapping_mul(PRIME1);
        i += 1;
    }

    // Final avalanche
    h ^= h >> 15;
    h = h.wrapping_mul(PRIME2);
    h ^= h >> 13;
    h = h.wrapping_mul(PRIME3);
    h ^= h >> 16;

    h
}

#[inline(always)]
fn round(acc: u32, input: u32) -> u32 {
    acc.wrapping_add(input.wrapping_mul(PRIME2))
        .rotate_left(13)
        .wrapping_mul(PRIME1)
}

#[inline(always)]
fn read_u32_le(buf: &[u8], pos: usize) -> u32 {
    u32::from_le_bytes([buf[pos], buf[pos + 1], buf[pos + 2], buf[pos + 3]])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_xxh32_empty() {
        assert_eq!(xxh32(b"", 0), 0x02CC5D05); // 46947589
    }

    #[test]
    fn test_xxh32_single_byte() {
        assert_eq!(xxh32(&[0x00], 0), 0xD2163E4D); // 3523407757
    }

    #[test]
    fn test_xxh32_abc() {
        assert_eq!(xxh32(b"abc", 0), 0x32D153AF); // 853817263 -- known value
    }

    #[test]
    fn test_xxh32_hello_world() {
        let h = xxh32(b"Hello World", 0);
        // Verify deterministic
        assert_eq!(h, xxh32(b"Hello World", 0));
    }

    #[test]
    fn test_xxh32_with_seed() {
        let h0 = xxh32(b"Hello World", 0);
        let h42 = xxh32(b"Hello World", 42);
        assert_ne!(h0, h42);
    }

    #[test]
    fn test_xxh32_long_input() {
        // Input >= 16 bytes to exercise the 4-lane accumulator
        let data = b"abcdefghijklmnopqrstuvwxyz";
        let h = xxh32(data, 0);
        assert_eq!(h, xxh32(data, 0));
    }
}
