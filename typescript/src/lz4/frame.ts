/**
 * LZ4 frame format (v1.6.3) — compress and decompress.
 */

import { CompressError, CompressErrorCode } from '../error.js';
import { xxh32 } from './xxhash.js';
import { compressBlock } from './block.js';

const LZ4_MAGIC = 0x184D2204;

/** Block maximum size by BD block size ID (LZ4 frame spec). */
const MAX_BLOCK_SIZES: Record<number, number> = {
  4: 64 * 1024,
  5: 256 * 1024,
  6: 1024 * 1024,
  7: 4 * 1024 * 1024,
};

export interface FrameOptions {
  contentChecksum?: boolean;
  contentSize?: boolean;
  blockChecksum?: boolean;
  maxBlockSize?: 4 | 5 | 6 | 7;
}

function writeU32LE(buf: number[], value: number): void {
  buf.push(value & 0xff, (value >>> 8) & 0xff, (value >>> 16) & 0xff, (value >>> 24) & 0xff);
}

function readU32LE(src: Uint8Array, pos: number): number {
  return (src[pos] | (src[pos + 1] << 8) | (src[pos + 2] << 16) | ((src[pos + 3] << 24) >>> 0)) >>> 0;
}

export function compressFrame(
  data: Uint8Array,
  options?: FrameOptions,
): Uint8Array {
  const opts = {
    contentChecksum: options?.contentChecksum ?? false,
    contentSize: options?.contentSize ?? false,
    blockChecksum: options?.blockChecksum ?? false,
    maxBlockSize: options?.maxBlockSize ?? 7,
  };

  if (!(opts.maxBlockSize in MAX_BLOCK_SIZES)) {
    throw new CompressError(CompressErrorCode.InvalidInput, `invalid maxBlockSize ${opts.maxBlockSize}`);
  }

  const output: number[] = [];

  // Magic number (LE32)
  writeU32LE(output, LZ4_MAGIC);

  // Frame descriptor: FLG + BD
  // version = 01; blocks are compressed independently (Block_Independence)
  let flgByte = 0x60;
  if (opts.contentChecksum) flgByte |= 0x04;
  if (opts.contentSize) flgByte |= 0x08;

  const bdByte = (opts.maxBlockSize & 0x07) << 4;

  const headerBytes: number[] = [flgByte, bdByte];

  if (opts.contentSize) {
    let size = data.length;
    for (let i = 0; i < 8; i++) {
      headerBytes.push(size & 0xff);
      size = Math.floor(size / 256);
    }
  }

  // Header checksum: second byte of XXH32
  const hc = (xxh32(new Uint8Array(headerBytes), 0) >>> 8) & 0xff;
  output.push(...headerBytes, hc);

  // Blocks
  const maxBs = MAX_BLOCK_SIZES[opts.maxBlockSize];
  let pos = 0;

  while (pos < data.length) {
    const chunkSize = Math.min(data.length - pos, maxBs);
    const chunk = data.subarray(pos, pos + chunkSize);

    const compressed = compressBlock(chunk);

    if (compressed.length < chunk.length) {
      writeU32LE(output, compressed.length);
      for (let i = 0; i < compressed.length; i++) output.push(compressed[i]);
    } else {
      writeU32LE(output, (chunk.length | 0x80000000) >>> 0);
      for (let i = 0; i < chunk.length; i++) output.push(chunk[i]);
    }

    if (opts.blockChecksum) {
      const blockData = compressed.length < chunk.length ? compressed : chunk;
      const bc = xxh32(blockData, 0);
      writeU32LE(output, bc);
    }

    pos += chunkSize;
  }

  // EndMark
  writeU32LE(output, 0);

  // Content checksum
  if (opts.contentChecksum) {
    const cc = xxh32(data, 0);
    writeU32LE(output, cc);
  }

  return new Uint8Array(output);
}

export function decompressFrame(data: Uint8Array): Uint8Array {
  if (data.length < 7) {
    throw new CompressError(CompressErrorCode.UnexpectedEof, 'input too short for LZ4 frame');
  }

  let pos = 0;

  // Magic number
  const magic = readU32LE(data, 0);
  if (magic !== LZ4_MAGIC) {
    throw new CompressError(CompressErrorCode.InvalidInput, 'invalid LZ4 frame magic number');
  }
  pos = 4;

  // Frame descriptor
  const flg = data[pos];
  const bd = data[pos + 1];
  const headerStart = pos;
  pos += 2;

  if (flg >> 6 !== 0x01) {
    throw new CompressError(CompressErrorCode.Unsupported, 'unsupported LZ4 frame version');
  }
  const blockMax = MAX_BLOCK_SIZES[(bd >> 4) & 0x07];
  if (blockMax === undefined) {
    throw new CompressError(CompressErrorCode.InvalidInput, 'invalid LZ4 block maximum size');
  }
  const blockIndependent = (flg & 0x20) !== 0;
  const contentChecksum = (flg & 0x04) !== 0;
  const hasContentSize = (flg & 0x08) !== 0;
  const blockChecksum = (flg & 0x10) !== 0;

  let contentSize: number | null = null;
  if (hasContentSize) {
    if (pos + 8 > data.length) {
      throw new CompressError(CompressErrorCode.UnexpectedEof, 'missing content size');
    }
    contentSize = 0;
    for (let i = 0; i < 8; i++) {
      contentSize += data[pos + i] * (256 ** i);
    }
    pos += 8;
  }

  // Header checksum
  const headerData = data.subarray(headerStart, pos);
  const expectedHc = (xxh32(headerData, 0) >>> 8) & 0xff;
  if (pos >= data.length) {
    throw new CompressError(CompressErrorCode.UnexpectedEof, 'missing header checksum');
  }
  if (data[pos] !== expectedHc) {
    throw new CompressError(CompressErrorCode.ChecksumMismatch, 'LZ4 frame header checksum mismatch');
  }
  pos++;

  // Read blocks
  const output: number[] = [];

  while (true) {
    if (pos + 4 > data.length) {
      throw new CompressError(CompressErrorCode.UnexpectedEof, 'missing block header');
    }

    const blockSize = readU32LE(data, pos);
    pos += 4;

    if (blockSize === 0) break; // EndMark

    const isUncompressed = (blockSize & 0x80000000) !== 0;
    const actualSize = blockSize & 0x7fffffff;

    if (actualSize > blockMax) {
      throw new CompressError(CompressErrorCode.InvalidInput, 'LZ4 block exceeds declared block maximum size');
    }

    if (pos + actualSize > data.length) {
      throw new CompressError(CompressErrorCode.UnexpectedEof, 'block data extends past input');
    }

    const blockData = data.subarray(pos, pos + actualSize);
    pos += actualSize;

    if (isUncompressed) {
      for (let i = 0; i < blockData.length; i++) output.push(blockData[i]);
    } else {
      // Linked blocks may reference up to 64KB of earlier output.
      const history = blockIndependent ? 0 : Math.min(output.length, 64 * 1024);
      decompressBlockInto(blockData, output, output.length - history, blockMax);
    }

    if (blockChecksum) {
      if (pos + 4 > data.length) {
        throw new CompressError(CompressErrorCode.UnexpectedEof, 'missing block checksum');
      }
      const expectedBc = readU32LE(data, pos);
      const actualBc = xxh32(blockData, 0);
      if (actualBc !== expectedBc) {
        throw new CompressError(CompressErrorCode.ChecksumMismatch, 'LZ4 block checksum mismatch');
      }
      pos += 4;
    }
  }

  // Content checksum
  if (contentChecksum) {
    if (pos + 4 > data.length) {
      throw new CompressError(CompressErrorCode.UnexpectedEof, 'missing content checksum');
    }
    const expectedCc = readU32LE(data, pos);
    const result = new Uint8Array(output);
    const actualCc = xxh32(result, 0);
    if (actualCc !== expectedCc) {
      throw new CompressError(CompressErrorCode.ChecksumMismatch, 'LZ4 content checksum mismatch');
    }
  }

  const result = new Uint8Array(output);

  if (contentSize !== null && result.length !== contentSize) {
    throw new CompressError(
      CompressErrorCode.InvalidInput,
      `decompressed size ${result.length} != content size ${contentSize}`,
    );
  }

  return result;
}

/**
 * Decode one LZ4 block, appending to `output`. Matches may reach back to
 * `windowStart`; the block may add at most `blockMax` bytes.
 */
function decompressBlockInto(
  data: Uint8Array,
  output: number[],
  windowStart: number,
  blockMax: number,
): void {
  const srcLen = data.length;
  if (srcLen === 0) return;

  const limit = output.length + blockMax;
  const tooLarge = () =>
    new CompressError(CompressErrorCode.OutputTooSmall, 'LZ4 block exceeds declared block maximum size');
  let pos = 0;

  while (pos < srcLen) {
    const token = data[pos++];
    let litLen = token >>> 4;

    if (litLen === 15) {
      while (true) {
        if (pos >= srcLen) {
          throw new CompressError(CompressErrorCode.UnexpectedEof, 'unexpected end reading literal length');
        }
        const extra = data[pos++];
        litLen += extra;
        if (extra !== 255) break;
      }
    }

    if (pos + litLen > srcLen) {
      throw new CompressError(CompressErrorCode.UnexpectedEof, 'literal data extends past input');
    }
    if (output.length + litLen > limit) throw tooLarge();
    for (let i = 0; i < litLen; i++) output.push(data[pos + i]);
    pos += litLen;

    if (pos >= srcLen) break;

    if (pos + 2 > srcLen) {
      throw new CompressError(CompressErrorCode.UnexpectedEof, 'missing match offset');
    }
    const offset = data[pos] | (data[pos + 1] << 8);
    pos += 2;

    if (offset === 0) {
      throw new CompressError(CompressErrorCode.InvalidInput, 'zero match offset');
    }

    let matchLen = (token & 0x0f) + 4;
    if ((token & 0x0f) === 15) {
      while (true) {
        if (pos >= srcLen) {
          throw new CompressError(CompressErrorCode.UnexpectedEof, 'unexpected end reading match length');
        }
        const extra = data[pos++];
        matchLen += extra;
        if (extra !== 255) break;
      }
    }

    const matchStart = output.length - offset;
    if (matchStart < windowStart) {
      throw new CompressError(CompressErrorCode.InvalidInput, 'match offset beyond output');
    }

    if (output.length + matchLen > limit) throw tooLarge();
    for (let i = 0; i < matchLen; i++) {
      output.push(output[matchStart + i]);
    }
  }
}
