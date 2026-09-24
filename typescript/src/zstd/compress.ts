/**
 * Zstandard compressor (levels 1-3, raw blocks).
 */

import { CompressError, CompressErrorCode } from '../error.js';

const ZSTD_MAGIC = 0xfd2fb528;

export type ZstdLevel = 1 | 2 | 3;

/** Check if data starts with Zstd magic number. */
export function isZstd(data: Uint8Array): boolean {
  if (data.length < 4) return false;
  const magic = data[0] | (data[1] << 8) | (data[2] << 16) | ((data[3] << 24) >>> 0);
  return (magic >>> 0) === ZSTD_MAGIC;
}

/** Compress data using Zstandard (raw blocks). */
export function zstdCompress(input: Uint8Array, level: ZstdLevel = 1): Uint8Array {
  const output: number[] = [];

  // Magic number (LE)
  output.push(ZSTD_MAGIC & 0xff);
  output.push((ZSTD_MAGIC >> 8) & 0xff);
  output.push((ZSTD_MAGIC >> 16) & 0xff);
  output.push((ZSTD_MAGIC >> 24) & 0xff);

  const contentSize = input.length;

  // Frame header
  if (contentSize <= 255) {
    output.push(0x20); // FHD: Single_Segment=1, FCS=0 (1 byte)
    output.push(contentSize);
  } else if (contentSize <= 65535 + 256) {
    output.push(0x60); // FCS=01 (2 bytes)
    const sz = contentSize - 256;
    output.push(sz & 0xff);
    output.push((sz >> 8) & 0xff);
  } else {
    output.push(0xa0); // FCS=10 (4 bytes)
    output.push(contentSize & 0xff);
    output.push((contentSize >> 8) & 0xff);
    output.push((contentSize >> 16) & 0xff);
    output.push((contentSize >> 24) & 0xff);
  }

  if (input.length === 0) {
    // Empty: one last raw block of size 0
    output.push(0x01, 0x00, 0x00);
    return new Uint8Array(output);
  }

  // Emit blocks
  const maxBlock = 128 * 1024;
  let pos = 0;

  while (pos < input.length) {
    const remaining = input.length - pos;
    const blockSize = Math.min(remaining, maxBlock);
    const isLast = pos + blockSize >= input.length;
    const blockData = input.subarray(pos, pos + blockSize);

    // Check RLE
    const first = blockData[0];
    let isRle = true;
    for (let i = 1; i < blockData.length; i++) {
      if (blockData[i] !== first) { isRle = false; break; }
    }

    if (isRle) {
      const bh = (isLast ? 1 : 0) | (1 << 1) | (blockSize << 3);
      output.push(bh & 0xff, (bh >> 8) & 0xff, (bh >> 16) & 0xff);
      output.push(first);
    } else {
      const bh = (isLast ? 1 : 0) | (0 << 1) | (blockSize << 3);
      output.push(bh & 0xff, (bh >> 8) & 0xff, (bh >> 16) & 0xff);
      for (let i = 0; i < blockData.length; i++) {
        output.push(blockData[i]);
      }
    }

    pos += blockSize;
  }

  return new Uint8Array(output);
}
