# LombokCompress

[![Rust](https://github.com/codinglombok/LombokCompress/actions/workflows/rust.yml/badge.svg)](https://github.com/codinglombok/LombokCompress/actions/workflows/rust.yml)
[![TypeScript](https://github.com/codinglombok/LombokCompress/actions/workflows/typescript.yml/badge.svg)](https://github.com/codinglombok/LombokCompress/actions/workflows/typescript.yml)
[![Python](https://github.com/codinglombok/LombokCompress/actions/workflows/python.yml/badge.svg)](https://github.com/codinglombok/LombokCompress/actions/workflows/python.yml)
[![Go](https://github.com/codinglombok/LombokCompress/actions/workflows/go.yml/badge.svg)](https://github.com/codinglombok/LombokCompress/actions/workflows/go.yml)
[![PHP](https://github.com/codinglombok/LombokCompress/actions/workflows/php.yml/badge.svg)](https://github.com/codinglombok/LombokCompress/actions/workflows/php.yml)
[![Security](https://github.com/codinglombok/LombokCompress/actions/workflows/security.yml/badge.svg)](https://github.com/codinglombok/LombokCompress/actions/workflows/security.yml)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

Universal, zero-dependency compression library — LZ4, Zstandard and Deflate (gzip/zlib) with one API
in Rust, TypeScript, Python, Go and PHP. Part of [Lombok Ecosystem](https://github.com/codinglombok).

## Why this library?

LombokCompress is for anyone who needs to read or write compressed data — application and web
developers, backend and cloud engineers, data/AI pipelines, firmware and hardware makers — from a
weekend project to industrial deployments.

- **One behaviour in five languages.** Every port is checked against the same shared test vectors and
  against the reference implementations (liblz4, libzstd, zlib), so data written in one language
  reads back in any other.
- **No native bindings, no runtime dependencies.** Nothing to compile against C libraries, nothing
  to audit beyond this repository. The Rust core is `#![no_std]` + `alloc` for microcontrollers.
- **Safe on untrusted input.** Decoders validate every header field, never pre-allocate from
  attacker-controlled sizes, reject malformed data with a typed error instead of crashing, and offer
  output limits against decompression bombs. Each port is fuzzed with random and corrupted input in CI.

Typical uses: HTTP `Content-Encoding` (gzip/deflate), log and telemetry archives, message and cache
payload compression, firmware/OTA images (LZ4 on MCUs), reading `.gz`/`.lz4`/`.zst` files produced by
standard tools.

## Features

| Format | Compress | Decompress | Notes |
|--------|----------|------------|-------|
| **LZ4 Block** | ✅ | ✅ | Hash-chain matcher, MIN_MATCH=4, end-of-block rules per spec |
| **LZ4 Frame** | ✅ | ✅ | v1.6.3; XXH32 header/block/content checksums; linked and independent blocks |
| **Deflate** | ✅ | ✅ | RFC 1951; fixed Huffman + stored blocks |
| **Gzip** | ✅ | ✅ | RFC 1952; CRC-32 + ISIZE verified |
| **Zlib** | ✅ | ✅ | RFC 1950; header check + Adler-32 verified |
| **Zstd** | ✅ | ✅ | RFC 8878 frames; raw + RLE blocks |

## Installation

| Language | Min version | Package |
|----------|-------------|---------|
| Rust | 1.70 | `cargo add lombokcompress` ([crates.io](https://crates.io/crates/lombokcompress)) |
| TypeScript / JavaScript | Node 18+, ES2022 | `npm install lombokcompress` (ESM + CommonJS) |
| Python | 3.10+ | `pip install lombokcompress` |
| Go | 1.21+ | `go get github.com/codinglombok/lombokcompress/go` (tags `go/vX.Y.Z`) |
| PHP | 8.1+ | `composer require codinglombok/lombokcompress` |

## Quick Start

### Rust

```rust
use lombokcompress::lz4::{compress_frame, decompress_frame, FrameOptions};

let data = b"Hello, LombokCompress!";
let compressed = compress_frame(data, &FrameOptions::default()).unwrap();
let decompressed = decompress_frame(&compressed).unwrap();
assert_eq!(&decompressed, data);
```

Feature flags: `lz4` (default), `zstd`, `deflate`, `std` (default). Disable `std` for `no_std + alloc`.

### TypeScript

```typescript
import { lz4, deflate, zstd } from 'lombokcompress';

const data = new TextEncoder().encode('Hello, LombokCompress!');
const frame = lz4.compressFrame(data);
const back = lz4.decompressFrame(frame);

const gz = deflate.gzipCompress(data);          // readable by gunzip, browsers, node:zlib
const z = zstd.zstdCompress(data, 1);
```

### Python

```python
from lombokcompress import lz4, deflate, zstd

data = b"Hello, LombokCompress!"
assert lz4.decompress_frame(lz4.compress_frame(data)) == data
assert deflate.gzip_decompress(deflate.gzip_compress(data)) == data
assert zstd.zstd_decompress(zstd.zstd_compress(data)) == data
```

### Go

```go
import "github.com/codinglombok/lombokcompress/go/lz4"

data := []byte("Hello, LombokCompress!")
compressed := lz4.CompressFrame(data, nil)
decompressed, err := lz4.DecompressFrame(compressed)
```

### PHP

```php
use LombokCompress\Lz4\Frame;

$data = "Hello, LombokCompress!";
$compressed = Frame::compress($data);
$decompressed = Frame::decompress($compressed);
```

## Decompressing untrusted input

All decoders return a typed error (`CompressError`) for malformed data. To bound memory, use the
output-limited entry points:

| Language | Deflate | Gzip / Zlib (default cap 64 MiB) | Zstd |
|----------|---------|----------------------------------|------|
| Rust | `deflate_decompress(data, max)` | built in | `zstd_decompress_limited(data, max)` |
| TypeScript | `deflateDecompress(data, max)` | `gzipDecompress(data, max)` | `zstdDecompress(data, max)` |
| Python | `deflate_decompress(data, max_output=)` | `gzip_decompress(data, max_output=)` | `zstd_decompress(data, max_output=)` |
| Go | `DeflateDecompressLimit(data, max)` | `GzipDecompressLimit` / `ZlibDecompressLimit` | `DecompressLimit(data, max)` |
| PHP | `deflateDecompress($data, $max)` | `gzipDecompress($data, $max)` | `Decompress::decompress($data, $max)` |

LZ4 frames are bounded by the block maximum size declared in the frame header.

## Standards implemented

- LZ4 Block Format and LZ4 Frame Format v1.6.3, xxHash32
- RFC 1950 (zlib), RFC 1951 (DEFLATE), RFC 1952 (gzip), CRC-32 (ISO 3309 / ITU-T V.42), Adler-32
- RFC 8878 (Zstandard) frame format

## Limitations (v0.1.x)

- Deflate: decodes fixed-Huffman and stored blocks; dynamic-Huffman blocks (most output of zlib at
  default settings) return `Unsupported` — planned.
- Zstd: raw + RLE blocks; FSE/Huffman-compressed blocks return `Unsupported`; content checksum
  (XXH64) not yet verified.
- LZ4: no dictionary support yet.

## Development

Shared cross-language vectors live in `test-vectors/compress_vectors.json`. Each port has its own
test suite, run in CI:

```sh
(cd rust && cargo test --all-features)
(cd typescript && npm ci && npm run build && npm test)
(cd python && pip install -e . pytest && pytest tests)
(cd go && go test ./...)
(cd php && composer install && php tests/run.php)
```

## Lombok Ecosystem

LombokCompress is a standalone library of [Lombok Ecosystem](https://github.com/codinglombok) and
works with other Lombok libraries without depending on any of them.

## Contributing

Issues and pull requests are welcome. Behaviour must stay identical across ports: add or update a
shared test vector for any change in output, and keep CI green in every language.

## License

Apache-2.0 — see [LICENSE](LICENSE).
