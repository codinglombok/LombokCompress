/**
 * XXHash32 implementation for LZ4 frame checksums.
 * All arithmetic uses Math.imul for correct u32 behavior.
 */

const PRIME1 = 0x9E3779B1 | 0;
const PRIME2 = 0x85EBCA77 | 0;
const PRIME3 = 0xC2B2AE3D | 0;
const PRIME4 = 0x27D4EB2F | 0;
const PRIME5 = 0x165667B1 | 0;

function rotl32(v: number, n: number): number {
  return ((v << n) | (v >>> (32 - n))) >>> 0;
}

function readU32LE(buf: Uint8Array, pos: number): number {
  return (buf[pos] | (buf[pos + 1] << 8) | (buf[pos + 2] << 16) | (buf[pos + 3] << 24)) >>> 0;
}

function round(acc: number, input: number): number {
  acc = (acc + Math.imul(input, PRIME2)) >>> 0;
  acc = rotl32(acc, 13);
  acc = Math.imul(acc, PRIME1) >>> 0;
  return acc;
}

/**
 * Compute XXH32 hash.
 */
export function xxh32(input: Uint8Array, seed: number = 0): number {
  const len = input.length;
  let h: number;
  let i = 0;

  seed = seed >>> 0;

  if (len >= 16) {
    let v1 = (seed + PRIME1 + PRIME2) >>> 0;
    let v2 = (seed + PRIME2) >>> 0;
    let v3 = seed >>> 0;
    let v4 = (seed - PRIME1) >>> 0;

    while (i + 16 <= len) {
      v1 = round(v1, readU32LE(input, i));
      v2 = round(v2, readU32LE(input, i + 4));
      v3 = round(v3, readU32LE(input, i + 8));
      v4 = round(v4, readU32LE(input, i + 12));
      i += 16;
    }

    h = (rotl32(v1, 1) + rotl32(v2, 7) + rotl32(v3, 12) + rotl32(v4, 18)) >>> 0;
  } else {
    h = (seed + PRIME5) >>> 0;
  }

  h = (h + len) >>> 0;

  while (i + 4 <= len) {
    h = (h + Math.imul(readU32LE(input, i), PRIME3)) >>> 0;
    h = Math.imul(rotl32(h, 17), PRIME4) >>> 0;
    i += 4;
  }

  while (i < len) {
    h = (h + Math.imul(input[i], PRIME5)) >>> 0;
    h = Math.imul(rotl32(h, 11), PRIME1) >>> 0;
    i += 1;
  }

  h ^= h >>> 15;
  h = Math.imul(h, PRIME2) >>> 0;
  h ^= h >>> 13;
  h = Math.imul(h, PRIME3) >>> 0;
  h ^= h >>> 16;

  return h >>> 0;
}
