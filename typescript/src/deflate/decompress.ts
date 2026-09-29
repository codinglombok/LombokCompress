/**
 * Deflate decompression (inflate).
 */

import { CompressError, CompressErrorCode } from '../error.js';
import { crc32, adler32 } from './compress.js';

function reverseBits(value: number, bits: number): number {
  let result = 0;
  let v = value;
  for (let i = 0; i < bits; i++) {
    result = (result << 1) | (v & 1);
    v >>= 1;
  }
  return result;
}

class BitReader {
  data: Uint8Array;
  pos: number = 0;
  bits: number = 0;
  nbits: number = 0;

  constructor(data: Uint8Array) {
    this.data = data;
  }

  readBits(count: number): number {
    while (this.nbits < count) {
      if (this.pos >= this.data.length) {
        throw new CompressError(CompressErrorCode.UnexpectedEof, 'unexpected end of input');
      }
      this.bits |= this.data[this.pos++] << this.nbits;
      this.nbits += 8;
    }
    const mask = (1 << count) - 1;
    const result = this.bits & mask;
    this.bits >>>= count;
    this.nbits -= count;
    return result;
  }

  align(): void {
    this.bits = 0;
    this.nbits = 0;
  }
}

function decodeLength(code: number, extra: number): number {
  if (code <= 264) return code - 257 + 3;
  if (code <= 268) return 2 * (code - 265) + 11 + extra;
  if (code <= 272) return 4 * (code - 269) + 19 + extra;
  if (code <= 276) return 8 * (code - 273) + 35 + extra;
  if (code <= 280) return 16 * (code - 277) + 67 + extra;
  if (code <= 284) return 32 * (code - 281) + 131 + extra;
  if (code === 285) return 258;
  return 0;
}

function decodeDistance(code: number, extra: number): number {
  if (code <= 3) return code + 1;
  const nExtra = Math.floor(code / 2) - 1;
  const base = (1 << (nExtra + 1)) + 1;
  const offset = (code & 1) << nExtra;
  return base + offset + extra;
}

function lengthExtraBits(code: number): number {
  if (code <= 264 || code === 285) return 0;
  if (code <= 268) return 1;
  if (code <= 272) return 2;
  if (code <= 276) return 3;
  if (code <= 280) return 4;
  if (code <= 284) return 5;
  return 0;
}

function distanceExtraBits(code: number): number {
  if (code <= 3) return 0;
  return Math.floor(code / 2) - 1;
}

function decodeFixedLiteral(reader: BitReader): number {
  const b7 = reader.readBits(7);
  const rev7 = reverseBits(b7, 7);

  if (rev7 <= 23) return rev7 + 256;

  const b8Extra = reader.readBits(1);
  const b8 = b7 | (b8Extra << 7);
  const rev8 = reverseBits(b8, 8);

  if (rev8 >= 0x30 && rev8 <= 0xbf) return rev8 - 0x30;
  if (rev8 >= 0xc0 && rev8 <= 0xc7) return rev8 - 0xc0 + 280;

  const b9Extra = reader.readBits(1);
  const b9 = b8 | (b9Extra << 8);
  const rev9 = reverseBits(b9, 9);

  if (rev9 >= 0x190 && rev9 <= 0x1ff) return rev9 - 0x190 + 144;

  throw new CompressError(CompressErrorCode.InvalidInput, 'invalid fixed Huffman code');
}

function invalid(msg: string): CompressError {
  return new CompressError(CompressErrorCode.InvalidInput, msg);
}

function eof(msg: string): CompressError {
  return new CompressError(CompressErrorCode.UnexpectedEof, msg);
}

/** Default output cap for gzip/zlib, matching the Rust core (64 MiB). */
const DEFAULT_MAX_OUTPUT = 64 * 1024 * 1024;

/**
 * Decompress raw deflate data.
 *
 * `maxOutput` bounds the decompressed size; set it for untrusted input.
 */
export function deflateDecompress(input: Uint8Array, maxOutput: number = Number.MAX_SAFE_INTEGER): Uint8Array {
  const output: number[] = [];
  const tooLarge = () =>
    new CompressError(CompressErrorCode.OutputTooSmall, `decompressed data exceeds limit ${maxOutput}`);
  const reader = new BitReader(input);

  let done = false;
  while (!done) {
    const bfinal = reader.readBits(1);
    const btype = reader.readBits(2);

    if (btype === 0) {
      // Stored block
      reader.align();
      if (reader.pos + 4 > reader.data.length) throw eof('missing stored block header');
      const len = reader.data[reader.pos] | (reader.data[reader.pos + 1] << 8);
      const nlen = reader.data[reader.pos + 2] | (reader.data[reader.pos + 3] << 8);
      reader.pos += 4;
      if (len !== (~nlen & 0xffff)) throw invalid('deflate stored block LEN/NLEN mismatch');
      if (reader.pos + len > reader.data.length) throw eof('stored block extends past input');
      if (output.length + len > maxOutput) throw tooLarge();
      for (let i = 0; i < len; i++) {
        output.push(reader.data[reader.pos++]);
      }
    } else if (btype === 1) {
      // Fixed Huffman
      while (true) {
        const sym = decodeFixedLiteral(reader);
        if (sym === 256) break;

        if (sym < 256) {
          if (output.length >= maxOutput) throw tooLarge();
          output.push(sym);
        } else {
          const extraBits = lengthExtraBits(sym);
          const extra = extraBits > 0 ? reader.readBits(extraBits) : 0;
          const length = decodeLength(sym, extra);

          const distCode = reverseBits(reader.readBits(5), 5);
          if (sym > 285 || distCode > 29) throw invalid('invalid deflate length/distance code');
          const distExtraBits = distanceExtraBits(distCode);
          const distExtra = distExtraBits > 0 ? reader.readBits(distExtraBits) : 0;
          const distance = decodeDistance(distCode, distExtra);

          if (distance > output.length) throw invalid('deflate distance exceeds output');
          if (output.length + length > maxOutput) throw tooLarge();
          const start = output.length - distance;
          for (let i = 0; i < length; i++) {
            output.push(output[start + i]);
          }
        }
      }
    } else if (btype === 2) {
      throw new CompressError(CompressErrorCode.Unsupported, 'dynamic Huffman not yet implemented');
    } else {
      throw new CompressError(CompressErrorCode.InvalidInput, 'invalid deflate block type');
    }

    if (bfinal) done = true;
  }

  return new Uint8Array(output);
}

/** Decompress gzip data (RFC 1952). */
export function gzipDecompress(input: Uint8Array, maxOutput: number = DEFAULT_MAX_OUTPUT): Uint8Array {
  if (input.length < 18) {
    throw new CompressError(CompressErrorCode.UnexpectedEof, 'input too short for gzip');
  }
  if (input[0] !== 0x1f || input[1] !== 0x8b) {
    throw new CompressError(CompressErrorCode.InvalidInput, 'invalid gzip magic');
  }

  if (input[2] !== 0x08) throw invalid('unsupported gzip method');

  const flg = input[3];
  let pos = 10;
  const deflateEnd = input.length - 8;
  const skipZeroTerminated = () => {
    while (pos < deflateEnd && input[pos] !== 0) pos++;
    pos++; // terminator
  };

  if (flg & 0x04) { const xlen = input[pos] | (input[pos + 1] << 8); pos += 2 + xlen; }
  if (flg & 0x08) skipZeroTerminated();
  if (flg & 0x10) skipZeroTerminated();
  if (flg & 0x02) { pos += 2; }
  if (pos >= deflateEnd) throw eof('gzip header extends past input');

  const decompressed = deflateDecompress(input.subarray(pos, deflateEnd), maxOutput);

  const expectedCrc =
    input[deflateEnd] |
    (input[deflateEnd + 1] << 8) |
    (input[deflateEnd + 2] << 16) |
    ((input[deflateEnd + 3] << 24) >>> 0);
  const actualCrc = crc32(decompressed);

  if ((expectedCrc >>> 0) !== actualCrc) {
    throw new CompressError(
      CompressErrorCode.ChecksumMismatch,
      `CRC32 mismatch: expected 0x${expectedCrc.toString(16)}, got 0x${actualCrc.toString(16)}`
    );
  }

  const isize =
    (input[deflateEnd + 4] |
      (input[deflateEnd + 5] << 8) |
      (input[deflateEnd + 6] << 16) |
      (input[deflateEnd + 7] << 24)) >>>
    0;
  if (isize !== decompressed.length % 0x100000000) throw invalid('gzip ISIZE mismatch');

  return decompressed;
}

/** Decompress zlib data (RFC 1950). */
export function zlibDecompress(input: Uint8Array, maxOutput: number = DEFAULT_MAX_OUTPUT): Uint8Array {
  if (input.length < 6) {
    throw new CompressError(CompressErrorCode.UnexpectedEof, 'input too short for zlib');
  }
  if ((input[0] & 0x0f) !== 8) {
    throw new CompressError(CompressErrorCode.InvalidInput, 'unsupported zlib method');
  }

  if (((input[0] << 8) | input[1]) % 31 !== 0) throw invalid('zlib header check failed');
  if (input[1] & 0x20) {
    throw new CompressError(CompressErrorCode.Unsupported, 'zlib preset dictionary');
  }

  const decompressed = deflateDecompress(input.subarray(2, input.length - 4), maxOutput);

  const expectedAdler =
    ((input[input.length - 4] << 24) |
      (input[input.length - 3] << 16) |
      (input[input.length - 2] << 8) |
      input[input.length - 1]) >>>
    0;
  const actualAdler = adler32(decompressed);

  if (expectedAdler !== actualAdler) {
    throw new CompressError(
      CompressErrorCode.ChecksumMismatch,
      `Adler-32 mismatch: expected 0x${expectedAdler.toString(16)}, got 0x${actualAdler.toString(16)}`
    );
  }

  return decompressed;
}
