/**
 * Deflate compression with fixed Huffman codes.
 */

import { CompressError, CompressErrorCode } from '../error.js';

// CRC32 table
const CRC32_TABLE = new Uint32Array(256);
for (let i = 0; i < 256; i++) {
  let c = i;
  for (let j = 0; j < 8; j++) {
    c = c & 1 ? (c >>> 1) ^ 0xedb88320 : c >>> 1;
  }
  CRC32_TABLE[i] = c >>> 0;
}

/** Compute CRC32 checksum. */
export function crc32(data: Uint8Array): number {
  let crc = 0xffffffff;
  for (let i = 0; i < data.length; i++) {
    crc = CRC32_TABLE[(crc ^ data[i]) & 0xff] ^ (crc >>> 8);
  }
  return (crc ^ 0xffffffff) >>> 0;
}

/** Compute Adler-32 checksum. */
export function adler32(data: Uint8Array): number {
  let a = 1;
  let b = 0;
  const MOD = 65521;
  for (let i = 0; i < data.length; i++) {
    a = (a + data[i]) % MOD;
    b = (b + a) % MOD;
  }
  return ((b << 16) | a) >>> 0;
}

// Bit writer
class BitWriter {
  buf: number[] = [];
  bits = 0;
  nbits = 0;

  writeBits(value: number, count: number): void {
    this.bits |= (value & ((1 << count) - 1)) << this.nbits;
    this.nbits += count;
    while (this.nbits >= 8) {
      this.buf.push(this.bits & 0xff);
      this.bits >>>= 8;
      this.nbits -= 8;
    }
  }

  flush(): void {
    if (this.nbits > 0) {
      this.buf.push(this.bits & 0xff);
      this.bits = 0;
      this.nbits = 0;
    }
  }

  toUint8Array(): Uint8Array {
    return new Uint8Array(this.buf);
  }
}

function reverseBits(value: number, bits: number): number {
  let result = 0;
  let v = value;
  for (let i = 0; i < bits; i++) {
    result = (result << 1) | (v & 1);
    v >>= 1;
  }
  return result;
}

function fixedLiteralCode(value: number): [number, number] {
  if (value <= 143) return [reverseBits(0x30 + value, 8), 8];
  if (value <= 255) return [reverseBits(0x190 + (value - 144), 9), 9];
  if (value <= 279) return [reverseBits(value - 256, 7), 7];
  if (value <= 287) return [reverseBits(0xc0 + (value - 280), 8), 8];
  return [0, 0];
}

function fixedDistanceCode(dist: number): [number, number] {
  return [reverseBits(dist, 5), 5];
}

function encodeLength(length: number): [number, number, number] {
  if (length <= 10) return [257 + length - 3, 0, 0];
  if (length <= 18) {
    const group = Math.floor((length - 11) / 2);
    return [265 + group, 1, (length - 11) % 2];
  }
  if (length <= 34) {
    const group = Math.floor((length - 19) / 4);
    return [269 + group, 2, (length - 19) % 4];
  }
  if (length <= 66) {
    const group = Math.floor((length - 35) / 8);
    return [273 + group, 3, (length - 35) % 8];
  }
  if (length <= 130) {
    const group = Math.floor((length - 67) / 16);
    return [277 + group, 4, (length - 67) % 16];
  }
  if (length <= 257) {
    const group = Math.floor((length - 131) / 32);
    return [281 + group, 5, (length - 131) % 32];
  }
  return [285, 0, 0];
}

function encodeDistance(dist: number): [number, number, number] {
  if (dist <= 4) return [dist - 1, 0, 0];
  const nbits = Math.floor(Math.log2(dist - 1)) - 1;
  const code = nbits * 2 + ((dist - 1) >>> nbits);
  const extra = (dist - 1) & ((1 << nbits) - 1);
  return [code, nbits, extra];
}

interface Lz77Token {
  type: 'literal' | 'match';
  value?: number;
  length?: number;
  distance?: number;
}

function lz77Compress(input: Uint8Array): Lz77Token[] {
  const len = input.length;
  if (len === 0) return [];

  const tokens: Lz77Token[] = [];
  const HASH_BITS = 15;
  const HASH_SIZE = 1 << HASH_BITS;
  const head = new Uint32Array(HASH_SIZE);
  const prev = new Uint32Array(len);
  let pos = 0;

  function hash3(p: number): number {
    const v = input[p] | (input[p + 1] << 8) | (input[p + 2] << 16);
    return (Math.imul(v >>> 0, 0x1e35a7bd) >>> (32 - HASH_BITS)) & (HASH_SIZE - 1);
  }

  while (pos < len) {
    if (pos + 3 > len) {
      tokens.push({ type: 'literal', value: input[pos] });
      pos++;
      continue;
    }

    const h = hash3(pos);
    let bestLen = 2;
    let bestDist = 0;
    let chainCount = 0;
    let matchPos = head[h];

    while (matchPos > 0 && chainCount < 64) {
      const mp = matchPos - 1;
      const dist = pos - mp;
      if (dist > 32768) break;

      const maxLen = Math.min(258, len - pos);
      let ml = 0;
      while (ml < maxLen && input[mp + ml] === input[pos + ml]) ml++;

      if (ml > bestLen) {
        bestLen = ml;
        bestDist = dist;
        if (ml === 258) break;
      }

      matchPos = prev[mp];
      chainCount++;
    }

    prev[pos] = head[h];
    head[h] = pos + 1;

    if (bestLen >= 3) {
      tokens.push({ type: 'match', length: bestLen, distance: bestDist });
      for (let i = 1; i < bestLen; i++) {
        if (pos + i + 3 <= len) {
          const hi = hash3(pos + i);
          prev[pos + i] = head[hi];
          head[hi] = pos + i + 1;
        }
      }
      pos += bestLen;
    } else {
      tokens.push({ type: 'literal', value: input[pos] });
      pos++;
    }
  }

  return tokens;
}

/** Compress data using Deflate with fixed Huffman codes. */
export function deflateCompress(input: Uint8Array): Uint8Array {
  const tokens = lz77Compress(input);
  const bw = new BitWriter();

  bw.writeBits(1, 1); // BFINAL
  bw.writeBits(1, 2); // BTYPE = fixed

  for (const token of tokens) {
    if (token.type === 'literal') {
      const [code, bits] = fixedLiteralCode(token.value!);
      bw.writeBits(code, bits);
    } else {
      const [lenCode, extraBits, extraVal] = encodeLength(token.length!);
      const [code, bits] = fixedLiteralCode(lenCode);
      bw.writeBits(code, bits);
      if (extraBits > 0) bw.writeBits(extraVal, extraBits);

      const [distCode, distExtra, distVal] = encodeDistance(token.distance!);
      const [dcode, dbits] = fixedDistanceCode(distCode);
      bw.writeBits(dcode, dbits);
      if (distExtra > 0) bw.writeBits(distVal, distExtra);
    }
  }

  // End of block
  const [eobCode, eobBits] = fixedLiteralCode(256);
  bw.writeBits(eobCode, eobBits);
  bw.flush();

  return bw.toUint8Array();
}

/** Compress data in gzip format. */
export function gzipCompress(input: Uint8Array): Uint8Array {
  const deflated = deflateCompress(input);
  const checksum = crc32(input);
  const output = new Uint8Array(10 + deflated.length + 8);

  // Header
  output[0] = 0x1f;
  output[1] = 0x8b;
  output[2] = 0x08;
  output[3] = 0x00;
  // MTIME, XFL, OS = 0
  output[9] = 0xff;

  output.set(deflated, 10);

  const pos = 10 + deflated.length;
  output[pos] = checksum & 0xff;
  output[pos + 1] = (checksum >> 8) & 0xff;
  output[pos + 2] = (checksum >> 16) & 0xff;
  output[pos + 3] = (checksum >> 24) & 0xff;
  const isize = input.length & 0xffffffff;
  output[pos + 4] = isize & 0xff;
  output[pos + 5] = (isize >> 8) & 0xff;
  output[pos + 6] = (isize >> 16) & 0xff;
  output[pos + 7] = (isize >> 24) & 0xff;

  return output;
}

/** Compress data in zlib format. */
export function zlibCompress(input: Uint8Array): Uint8Array {
  const deflated = deflateCompress(input);
  const checksum = adler32(input);
  const cmf = 0x78;
  let flg = 0x01;
  const check = (cmf * 256 + flg) % 31;
  if (check !== 0) flg += 31 - check;

  const output = new Uint8Array(2 + deflated.length + 4);
  output[0] = cmf;
  output[1] = flg;
  output.set(deflated, 2);

  const pos = 2 + deflated.length;
  output[pos] = (checksum >> 24) & 0xff;
  output[pos + 1] = (checksum >> 16) & 0xff;
  output[pos + 2] = (checksum >> 8) & 0xff;
  output[pos + 3] = checksum & 0xff;

  return output;
}
