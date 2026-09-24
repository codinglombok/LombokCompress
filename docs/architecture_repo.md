# LombokCompress — Architecture Document

**Library:** LombokCompress
**Version:** 0.1.0
**Date:** 2026-09-18
**Author:** codinglombok
**License:** Apache-2.0

---

## 1. Purpose

LombokCompress provides zero-dependency compression/decompression for the Lombok Ecosystem.
Three algorithms cover the full speed-vs-ratio spectrum:

| Algorithm | Use Case | Speed | Ratio |
|-----------|----------|-------|-------|
| LZ4 | Hot-path caching, real-time streaming | Very Fast | Low |
| Zstd (L1-3) | Vector store persistence, serialized indexes | Fast | Medium-High |
| Deflate | Compatibility (gzip/zlib interop) | Medium | Medium |

## 2. ADR-001: Algorithm Selection

**Status:** Accepted

### Context
LombokRAGFrameworks needs compression for:
- Vector store persistence (large binary blobs)
- Cached embeddings (frequent read/write)
- Serialized HNSW indexes (write-once, read-many)
- Network transfer (gzip compatibility)

### Decision
Ship LZ4 + Zstd (levels 1-3) + Deflate. No brotli (web-only), no lzma (too slow).

### Consequences
- (+) LZ4 block format is simple (~300 LOC), ideal for no_std
- (+) Zstd levels 1-3 give 80% of zstd's benefit at 20% complexity
- (+) Deflate ensures gzip/zlib interop with HTTP and legacy systems
- (-) Zstd levels 4+ omitted (diminishing returns, +2000 LOC for FSE)

## 3. ADR-002: Streaming Traits

**Status:** Accepted

### Decision
Define `Compress` and `Decompress` traits for one-shot operations,
plus `StreamCompressor` and `StreamDecompressor` for chunked processing.

```rust
pub trait Compress {
    fn compress(&self, input: &[u8], output: &mut [u8]) -> Result<usize, CompressError>;
    fn compress_bound(&self, input_len: usize) -> usize;
}

pub trait Decompress {
    fn decompress(&self, input: &[u8], output: &mut [u8]) -> Result<usize, CompressError>;
}
```

## 4. ADR-003: Feature Flags

**Status:** Accepted

| Feature | Default | Contents |
|---------|---------|----------|
| `lz4` | Yes | LZ4 block + frame format |
| `zstd` | No | Zstd levels 1-3 (raw blocks) |
| `deflate` | No | Deflate/gzip/zlib |
| `std` | Yes | Standard library support |

Minimal build: `default-features = false, features = ["lz4"]` for no_std LZ4-only.

## 5. Performance Targets

| Operation | Target | Notes |
|-----------|--------|-------|
| LZ4 compress (1MB) | <2ms | Block format, 4KB hash table |
| LZ4 decompress (1MB) | <1ms | Simple copy + match |
| Zstd L1 compress (1MB) | <5ms | Raw blocks only |
| Deflate compress (1MB) | <15ms | Fixed Huffman + LZ77 |
| Deflate decompress (1MB) | <5ms | Inflate |

## 6. Cross-Language Port Strategy

All ports share `test-vectors/compress_vectors.json` for byte-identical output verification.

| Language | Package | Registry |
|----------|---------|----------|
| Rust | `lombokcompress` | crates.io |
| TypeScript | `lombokcompress` | npm |
| Python | `lombokcompress` | PyPI |
| Go | `github.com/codinglombok/lombokcompress` | go.sum |
| PHP | `codinglombok/lombokcompress` | Packagist |

---

*Document version: 0.1.0 — 2026-09-18*
