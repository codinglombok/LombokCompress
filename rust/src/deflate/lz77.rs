//! LZ77 matching engine for Deflate.
//!
//! Uses a hash chain with a 32KB sliding window.

const WINDOW_SIZE: usize = 32768;
const HASH_BITS: usize = 15;
const HASH_SIZE: usize = 1 << HASH_BITS;
const MIN_MATCH: usize = 3;
const MAX_MATCH: usize = 258;
const MAX_CHAIN: usize = 64; // Max chain length to search

/// An LZ77 token: either a literal byte or a (length, distance) match.
#[derive(Debug, Clone)]
pub enum Lz77Token {
    Literal(u8),
    Match { length: usize, distance: usize },
}

/// Find LZ77 matches in the input data.
pub fn lz77_compress(input: &[u8]) -> Vec<Lz77Token> {
    let len = input.len();
    if len == 0 {
        return Vec::new();
    }

    let mut tokens = Vec::new();
    let mut head = [0u32; HASH_SIZE]; // Hash -> most recent position + 1
    let mut prev = vec![0u32; len]; // Chain: prev position with same hash
    let mut pos = 0;

    while pos < len {
        if pos + MIN_MATCH > len {
            tokens.push(Lz77Token::Literal(input[pos]));
            pos += 1;
            continue;
        }

        let h = hash3(input, pos);
        let mut best_len = MIN_MATCH - 1;
        let mut best_dist = 0;
        let mut chain_count = 0;
        let mut match_pos = head[h] as usize;

        // Search the hash chain
        while match_pos > 0 && chain_count < MAX_CHAIN {
            let mp = match_pos - 1; // head stores pos + 1
            let dist = pos - mp;

            if dist > WINDOW_SIZE {
                break;
            }

            // Compare bytes
            let max_len = core::cmp::min(MAX_MATCH, len - pos);
            let mut ml = 0;
            while ml < max_len && input[mp + ml] == input[pos + ml] {
                ml += 1;
            }

            if ml > best_len {
                best_len = ml;
                best_dist = dist;
                if ml == MAX_MATCH {
                    break;
                }
            }

            match_pos = prev[mp] as usize;
            chain_count += 1;
        }

        // Update hash chain
        prev[pos] = head[h];
        head[h] = (pos + 1) as u32;

        if best_len >= MIN_MATCH {
            tokens.push(Lz77Token::Match {
                length: best_len,
                distance: best_dist,
            });
            // Update hash for skipped positions
            for i in 1..best_len {
                if pos + i + MIN_MATCH <= len {
                    let hi = hash3(input, pos + i);
                    prev[pos + i] = head[hi];
                    head[hi] = (pos + i + 1) as u32;
                }
            }
            pos += best_len;
        } else {
            tokens.push(Lz77Token::Literal(input[pos]));
            pos += 1;
        }
    }

    tokens
}

#[inline(always)]
fn hash3(data: &[u8], pos: usize) -> usize {
    let v = (data[pos] as u32)
        | ((data[pos + 1] as u32) << 8)
        | ((data[pos + 2] as u32) << 16);
    ((v.wrapping_mul(0x1E35A7BD)) >> (32 - HASH_BITS)) as usize
}
