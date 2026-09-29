import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import zlib from 'node:zlib';
import { lz4, deflate, zstd } from '../dist/esm/index.js';

const vectors = JSON.parse(
  readFileSync(new URL('../../test-vectors/compress_vectors.json', import.meta.url), 'utf8'),
);
const hex = (s) => Uint8Array.from(Buffer.from(s, 'hex'));

/** Deterministic xorshift32 so failures are reproducible. */
function rng(seed) {
  let s = seed >>> 0;
  return () => {
    s ^= s << 13; s >>>= 0;
    s ^= s >>> 17;
    s ^= s << 5; s >>>= 0;
    return s;
  };
}

function samples() {
  const next = rng(0x12345678);
  const out = [
    new Uint8Array(0),
    new TextEncoder().encode('a'),
    new TextEncoder().encode('abc'.repeat(1000)),
    new Uint8Array(100_000),
    new TextEncoder().encode('Hello, LombokCompress! '.repeat(5000)),
  ];
  for (const len of [1, 13, 64, 1000, 70_000]) {
    out.push(Uint8Array.from({ length: len }, () => next() & 0xff));
    out.push(Uint8Array.from({ length: len }, () => next() % 3));
  }
  return out;
}

test('xxh32 matches shared vectors', () => {
  for (const v of vectors.xxh32) {
    assert.equal(lz4.xxh32(hex(v.input_hex), v.seed), v.expected, `input ${v.input_hex}`);
  }
});

test('crc32 / adler32 match shared vectors', () => {
  for (const v of vectors.crc32) assert.equal(deflate.crc32(hex(v.input_hex)), v.expected);
  for (const v of vectors.adler32) assert.equal(deflate.adler32(hex(v.input_hex)), v.expected);
});

test('roundtrip all formats', () => {
  for (const data of samples()) {
    assert.deepEqual(lz4.decompressBlock(lz4.compressBlock(data), data.length), data);
    assert.deepEqual(lz4.decompressFrame(lz4.compressFrame(data)), data);
    assert.deepEqual(zstd.zstdDecompress(zstd.zstdCompress(data)), data);
    assert.deepEqual(deflate.deflateDecompress(deflate.deflateCompress(data)), data);
    assert.deepEqual(deflate.gzipDecompress(deflate.gzipCompress(data)), data);
    assert.deepEqual(deflate.zlibDecompress(deflate.zlibCompress(data)), data);
  }
});

test('gzip / zlib / raw deflate interoperate with node:zlib', () => {
  for (const data of samples()) {
    assert.deepEqual(new Uint8Array(zlib.gunzipSync(deflate.gzipCompress(data))), data);
    assert.deepEqual(new Uint8Array(zlib.inflateSync(deflate.zlibCompress(data))), data);
    assert.deepEqual(new Uint8Array(zlib.inflateRawSync(deflate.deflateCompress(data))), data);
    // Stored and fixed-Huffman streams produced by zlib itself.
    const stored = zlib.deflateRawSync(data, { level: 0 });
    assert.deepEqual(deflate.deflateDecompress(new Uint8Array(stored)), data);
    const fixed = zlib.deflateRawSync(data, { strategy: zlib.constants.Z_FIXED });
    assert.deepEqual(deflate.deflateDecompress(new Uint8Array(fixed)), data);
  }
});

test('malformed input throws CompressError, never a different error', async () => {
  const { CompressError } = await import('../dist/esm/index.js');
  const next = rng(0xdeadbeef);
  const seed = new TextEncoder().encode('The quick brown fox jumps over the lazy dog. '.repeat(40));
  const valid = [
    lz4.compressFrame(seed),
    lz4.compressBlock(seed),
    zstd.zstdCompress(seed, 2),
    zstd.zstdCompress(new Uint8Array(5000).fill(7), 1),
    deflate.deflateCompress(seed),
    deflate.gzipCompress(seed),
    deflate.zlibCompress(seed),
  ];
  const cases = [];
  for (let i = 0; i < 300; i++) {
    cases.push(Uint8Array.from({ length: next() % 64 }, () => next() & 0xff));
  }
  for (const v of valid) {
    for (let i = 0; i < 200; i++) {
      let c = v.slice();
      const flips = 1 + (next() % 4);
      for (let f = 0; f < flips; f++) c[next() % c.length] ^= 1 << (next() % 8);
      if (next() % 4 === 0) c = c.slice(0, next() % c.length);
      cases.push(c);
    }
  }
  const decoders = [
    (c) => lz4.decompressBlock(c, 1 << 16),
    (c) => lz4.decompressFrame(c),
    (c) => zstd.zstdDecompress(c),
    (c) => deflate.deflateDecompress(c),
    (c) => deflate.gzipDecompress(c),
    (c) => deflate.zlibDecompress(c),
  ];
  for (const c of cases) {
    for (const d of decoders) {
      try {
        d(c);
      } catch (e) {
        assert.ok(e instanceof CompressError, `unexpected ${e && e.name}: ${e && e.message}`);
      }
    }
  }
});

test('huge declared sizes are rejected without allocating', () => {
  const lz4Frame = new Uint8Array([0x04, 0x22, 0x4d, 0x18, 0x68, 0x40,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x7f, 0x00, 0, 0, 0, 0]);
  assert.throws(() => lz4.decompressFrame(lz4Frame));
  const zs = new Uint8Array([0x28, 0xb5, 0x2f, 0xfd, 0xe0,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x7f, 0x01, 0x00, 0x00]);
  assert.throws(() => zstd.zstdDecompress(zs));
});

test('gzip header without terminator does not hang', () => {
  // FLG.FNAME set but the name never ends before the trailer.
  const g = new Uint8Array([0x1f, 0x8b, 0x08, 0x08, 0, 0, 0, 0, 0, 0xff,
    0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41]);
  assert.throws(() => deflate.gzipDecompress(g));
});

test('deflate distance beyond output is rejected', () => {
  // Fixed-Huffman block whose first symbol is a match (length 3, distance 1).
  const bad = new Uint8Array([0x03, 0x02, 0x00]);
  assert.throws(() => deflate.deflateDecompress(bad));
});

test('output limits stop decompression bombs', () => {
  const big = new Uint8Array(200_000).fill(0x41);
  assert.throws(() => deflate.deflateDecompress(deflate.deflateCompress(big), 1000));
  assert.throws(() => zstd.zstdDecompress(zstd.zstdCompress(big), 1000));
});
