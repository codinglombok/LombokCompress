/**
 * Zstandard decompressor (raw + RLE blocks).
 */

import { CompressError, CompressErrorCode } from '../error.js';

const ZSTD_MAGIC = 0xfd2fb528;

/** Block_Maximum_Size from RFC 8878 §3.1.1.2.3 (128 KiB). */
const BLOCK_MAX_SIZE = 128 * 1024;

function eof(msg: string): CompressError {
  return new CompressError(CompressErrorCode.UnexpectedEof, msg);
}

/**
 * Decompress a Zstandard frame.
 *
 * `maxOutput` bounds the decompressed size; set it for untrusted input.
 */
export function zstdDecompress(input: Uint8Array, maxOutput: number = Number.MAX_SAFE_INTEGER): Uint8Array {
  if (input.length < 5) {
    throw new CompressError(CompressErrorCode.UnexpectedEof, 'input too short for Zstd');
  }

  let pos = 0;
  const magic = (input[0] | (input[1] << 8) | (input[2] << 16) | ((input[3] << 24) >>> 0)) >>> 0;
  if (magic !== ZSTD_MAGIC) {
    throw new CompressError(CompressErrorCode.InvalidInput, 'invalid Zstd magic number');
  }
  pos += 4;

  const fhd = input[pos++];
  const singleSegment = (fhd & 0x20) !== 0;
  const contentChecksum = (fhd & 0x04) !== 0;
  const dictIdFlag = fhd & 0x03;

  let fcsFieldSize: number;
  const fcsBits = (fhd >> 6) & 0x03;
  switch (fcsBits) {
    case 0: fcsFieldSize = singleSegment ? 1 : 0; break;
    case 1: fcsFieldSize = 2; break;
    case 2: fcsFieldSize = 4; break;
    case 3: fcsFieldSize = 8; break;
    default: fcsFieldSize = 0;
  }

  if (!singleSegment) pos++; // window descriptor

  // Dict ID
  const dictIdBytes = [0, 1, 2, 4][dictIdFlag];
  pos += dictIdBytes;

  // Content size (little-endian, read with arithmetic to avoid int32 overflow)
  let contentSize: number | undefined;
  if (fcsFieldSize > 0) {
    if (pos + fcsFieldSize > input.length) throw eof('missing frame content size');
    let v = 0;
    for (let i = fcsFieldSize - 1; i >= 0; i--) v = v * 256 + input[pos + i];
    contentSize = fcsFieldSize === 2 ? v + 256 : v;
    pos += fcsFieldSize;
  }
  if (contentSize !== undefined && contentSize > maxOutput) {
    throw new CompressError(
      CompressErrorCode.OutputTooSmall,
      `frame content size ${contentSize} exceeds limit ${maxOutput}`,
    );
  }

  const output: number[] = [];

  // Read blocks
  while (true) {
    if (pos + 3 > input.length) {
      throw new CompressError(CompressErrorCode.UnexpectedEof, 'unexpected end of Zstd data');
    }

    const bh = input[pos] | (input[pos + 1] << 8) | (input[pos + 2] << 16);
    pos += 3;

    const lastBlock = (bh & 1) !== 0;
    const blockType = (bh >> 1) & 0x03;
    const blockSize = bh >>> 3;

    if (blockSize > BLOCK_MAX_SIZE) {
      throw new CompressError(CompressErrorCode.InvalidInput, 'Zstd block exceeds Block_Maximum_Size');
    }
    if (blockType < 2 && output.length + blockSize > maxOutput) {
      throw new CompressError(
        CompressErrorCode.OutputTooSmall,
        `decompressed data exceeds limit ${maxOutput}`,
      );
    }

    switch (blockType) {
      case 0: // Raw
        if (pos + blockSize > input.length) {
          throw new CompressError(CompressErrorCode.UnexpectedEof, 'raw block extends past input');
        }
        for (let i = 0; i < blockSize; i++) output.push(input[pos + i]);
        pos += blockSize;
        break;

      case 1: { // RLE
        if (pos >= input.length) {
          throw new CompressError(CompressErrorCode.UnexpectedEof, 'RLE block missing byte');
        }
        const byte = input[pos++];
        for (let i = 0; i < blockSize; i++) output.push(byte);
        break;
      }

      case 2: // Compressed
        throw new CompressError(CompressErrorCode.Unsupported, 'Zstd compressed blocks (FSE) not yet implemented');

      case 3:
        throw new CompressError(CompressErrorCode.InvalidInput, 'reserved Zstd block type');
    }

    if (lastBlock) break;
  }

  // Content checksum (XXH64 not implemented yet): require its presence only.
  if (contentChecksum && pos + 4 > input.length) throw eof('missing content checksum');

  if (contentSize !== undefined && output.length !== contentSize) {
    throw new CompressError(
      CompressErrorCode.InvalidInput,
      `decompressed size ${output.length} != content size ${contentSize}`,
    );
  }

  return new Uint8Array(output);
}
