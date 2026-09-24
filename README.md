# LombokCompress

Zero-dependency compression library supporting LZ4, Zstandard, and Deflate (gzip/zlib). Part of the [LombokRAGFrameworks](https://github.com/codinglombok) ecosystem.

## Algorithms

| Algorithm | Compress | Decompress | Notes |
|-----------|----------|------------|-------|
| **LZ4 Block** | ✅ | ✅ | 4KB hash table, MIN_MATCH=4 |
| **LZ4 Frame** | ✅ | ✅ | v1.6.3, XXH32 checksums |
| **Deflate** | ✅ | ✅ | Fixed Huffman, LZ77 (32KB window) |
| **Gzip** | ✅ | ✅ | RFC 1952, CRC32 |
| **Zlib** | ✅ | ✅ | RFC 1950, Adler-32 |
| **Zstd** | ✅ | ✅ | Levels 1-3, raw + RLE blocks |

## Languages

| Language | Min Version | Package |
|----------|-------------|---------|
| Rust | 1.70+ | `lombokcompress` (crates.io) |
| TypeScript | ES2022 / Node 18+ | `lombokcompress` (npm) |
| Python | 3.10+ | `lombokcompress` (PyPI) |
| Go | 1.21+ | `github.com/codinglombok/lombokcompress` |
| PHP | 8.1+ | `codinglombok/lombokcompress` (Packagist) |

## Quick Start

### Rust

```rust
use lombokcompress::lz4::{compress_frame, decompress_frame};

let data = b"Hello, LombokCompress!";
let compressed = compress_frame(data, None).unwrap();
let decompressed = decompress_frame(&compressed).unwrap();
assert_eq!(&decompressed, data);
```

### TypeScript

```typescript
import { lz4 } from 'lombokcompress';

const data = new TextEncoder().encode('Hello, LombokCompress!');
const compressed = lz4.compressBlock(data);
const decompressed = lz4.decompressBlock(compressed, data.length);
```

### Python

```python
from lombokcompress import lz4

data = b"Hello, LombokCompress!"
compressed = lz4.compress_frame(data)
decompressed = lz4.decompress_frame(compressed)
assert decompressed == data
```

### Go

```go
import "github.com/codinglombok/lombokcompress/lz4"

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

## Architecture

- **Rust core** — `#![no_std]` compatible with feature flags (`lz4`, `zstd`, `deflate`, `std`)
- **Cross-language ports** — Identical algorithm implementations, byte-compatible output
- **Zero dependencies** — Every algorithm implemented from scratch
- **Test vectors** — Shared `test-vectors/compress_vectors.json` for cross-language verification

## Limitations (v0.1.0)

- Deflate: Fixed Huffman only (dynamic Huffman TODO)
- Zstd: Raw + RLE blocks only (FSE compressed blocks TODO)
- LZ4: No dictionary support yet

## License

Apache-2.0
