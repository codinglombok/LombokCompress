/**
 * LZ4 block compression and decompression.
 */

import { CompressError, CompressErrorCode } from '../error.js';

const HASH_LOG = 12;
const HASH_SIZE = 1 << HASH_LOG;
const MIN_MATCH = 4;
const ML_MASK = 15;
const RUN_MASK = 15;
const MF_LIMIT = 12;

/** Maximum compressed size for a given input length. */
export function compressBound(inputLen: number): number {
  if (inputLen === 0) return 0;
  return inputLen + Math.floor(inputLen / 255) + 16;
}

function hash4(data: Uint8Array, pos: number): number {
  const v = data[pos] | (data[pos + 1] << 8) | (data[pos + 2] << 16) | (data[pos + 3] << 24);
  return (Math.imul(v >>> 0, 2654435761) >>> (32 - HASH_LOG)) & (HASH_SIZE - 1);
}

/** Compress a block of data using LZ4. Returns compressed Uint8Array. */
export function compressBlock(input: Uint8Array): Uint8Array {
  const srcLen = input.length;
  if (srcLen === 0) return new Uint8Array(0);

  const output = new Uint8Array(compressBound(srcLen));
  const hashTable = new Uint16Array(HASH_SIZE);
  let srcPos = 0;
  let dstPos = 0;
  let anchor = 0;

  const srcLimit = srcLen > MF_LIMIT ? srcLen - MF_LIMIT : 0;
  if (srcLimit === 0) {
    return writeLastLiterals(input, 0, srcLen, output, 0);
  }

  srcPos = 1;

  outer: while (true) {
    let findPos = srcPos;
    let step = 1;
    let matchPos: number;

    // Find a match
    while (true) {
      srcPos = findPos;
      findPos += step;
      step += 1;

      if (findPos > srcLimit) {
        return writeLastLiterals(input, anchor, srcLen - anchor, output, dstPos);
      }

      const h = hash4(input, srcPos);
      matchPos = hashTable[h];
      hashTable[h] = srcPos;

      if (
        matchPos < srcPos &&
        srcPos - matchPos <= 0xffff &&
        input[matchPos] === input[srcPos] &&
        input[matchPos + 1] === input[srcPos + 1] &&
        input[matchPos + 2] === input[srcPos + 2] &&
        input[matchPos + 3] === input[srcPos + 3]
      ) {
        break;
      }
    }

    // Encode literal length
    const litLen = srcPos - anchor;
    const tokenPos = dstPos;
    dstPos += 1;

    if (litLen >= RUN_MASK) {
      output[tokenPos] = RUN_MASK << 4;
      let remaining = litLen - RUN_MASK;
      while (remaining >= 255) {
        output[dstPos++] = 255;
        remaining -= 255;
      }
      output[dstPos++] = remaining;
    } else {
      output[tokenPos] = litLen << 4;
    }

    // Copy literals
    output.set(input.subarray(anchor, anchor + litLen), dstPos);
    dstPos += litLen;

    // Encode match
    const offset = srcPos - matchPos;
    output[dstPos++] = offset & 0xff;
    output[dstPos++] = (offset >> 8) & 0xff;

    // Count match length
    let matchLen = MIN_MATCH;
    while (
      srcPos + matchLen < srcLen &&
      matchPos + matchLen < srcPos &&
      input[srcPos + matchLen] === input[matchPos + matchLen]
    ) {
      matchLen++;
    }
    const extraMatch = matchLen - MIN_MATCH;

    if (extraMatch >= ML_MASK) {
      output[tokenPos] |= ML_MASK;
      let remaining = extraMatch - ML_MASK;
      while (remaining >= 255) {
        output[dstPos++] = 255;
        remaining -= 255;
      }
      output[dstPos++] = remaining;
    } else {
      output[tokenPos] |= extraMatch;
    }

    srcPos += matchLen;
    anchor = srcPos;

    if (srcPos >= srcLimit) {
      return writeLastLiterals(input, anchor, srcLen - anchor, output, dstPos);
    }

    // Try next match
    const h2 = hash4(input, srcPos);
    hashTable[h2] = srcPos;
    srcPos += 1;
  }
}

function writeLastLiterals(
  input: Uint8Array,
  anchor: number,
  litLen: number,
  output: Uint8Array,
  dstPos: number
): Uint8Array {
  if (litLen === 0) return output.subarray(0, dstPos);

  const tokenPos = dstPos;
  dstPos += 1;

  if (litLen >= RUN_MASK) {
    output[tokenPos] = RUN_MASK << 4;
    let remaining = litLen - RUN_MASK;
    while (remaining >= 255) {
      output[dstPos++] = 255;
      remaining -= 255;
    }
    output[dstPos++] = remaining;
  } else {
    output[tokenPos] = litLen << 4;
  }

  output.set(input.subarray(anchor, anchor + litLen), dstPos);
  dstPos += litLen;

  return output.subarray(0, dstPos);
}

/** Decompress an LZ4 block. */
export function decompressBlock(input: Uint8Array, uncompressedSize: number): Uint8Array {
  const srcLen = input.length;
  if (srcLen === 0) return new Uint8Array(0);

  const output = new Uint8Array(uncompressedSize);
  let srcPos = 0;
  let dstPos = 0;

  while (srcPos < srcLen) {
    const token = input[srcPos++];

    // Literal length
    let litLen = (token >> 4) & 0x0f;
    if (litLen === RUN_MASK) {
      let b: number;
      do {
        if (srcPos >= srcLen) throw new CompressError(CompressErrorCode.UnexpectedEof, 'unexpected end of input');
        b = input[srcPos++];
        litLen += b;
      } while (b === 255);
    }

    // Copy literals
    if (srcPos + litLen > srcLen) {
      throw new CompressError(CompressErrorCode.UnexpectedEof, 'unexpected end of input');
    }
    output.set(input.subarray(srcPos, srcPos + litLen), dstPos);
    srcPos += litLen;
    dstPos += litLen;

    if (srcPos >= srcLen) break;

    // Match offset
    const offset = input[srcPos] | (input[srcPos + 1] << 8);
    srcPos += 2;

    if (offset === 0) {
      throw new CompressError(CompressErrorCode.InvalidInput, 'match offset is zero');
    }

    // Match length
    let matchLen = (token & 0x0f) + MIN_MATCH;
    if ((token & 0x0f) === ML_MASK) {
      let b: number;
      do {
        if (srcPos >= srcLen) throw new CompressError(CompressErrorCode.UnexpectedEof, 'unexpected end of input');
        b = input[srcPos++];
        matchLen += b;
      } while (b === 255);
    }

    // Copy match (may overlap)
    const matchStart = dstPos - offset;
    for (let j = 0; j < matchLen; j++) {
      output[dstPos + j] = output[matchStart + j];
    }
    dstPos += matchLen;
  }

  return output.subarray(0, dstPos);
}
