# LombokCompress — Master Plan

**Version:** 0.1.0
**Date:** 2026-09-18

## Phase 1: Rust Core (v0.1.0)
- [x] Error types, compression traits
- [x] LZ4 block compress/decompress (no_std)
- [x] LZ4 frame format v1.6.3 with XXH32
- [x] Deflate: LZ77 + Huffman (fixed tables)
- [x] Deflate: gzip/zlib wrappers, CRC32, Adler32
- [x] Zstd: levels 1-3 (raw blocks), frame format

## Phase 2: TypeScript Port (v0.1.0)
- [x] LZ4 block + XXH32
- [x] Deflate compress/decompress + gzip/zlib
- [x] Zstd compress/decompress

## Phase 3: Python Port (v0.1.0)
- [x] LZ4 block + XXH32
- [x] Deflate compress/decompress + gzip/zlib
- [x] Zstd compress/decompress

## Phase 4: Go + PHP Ports (v0.1.0)
- [x] Go: LZ4/Deflate/Zstd
- [x] PHP: LZ4/Deflate/Zstd

## Phase 5: CI + Release
- [x] GitHub Actions workflows (5 languages)
- [x] README, LICENSE, .gitignore
- [x] Test vectors JSON
- [x] Registry config files

## Downstream Consumers
- LombokSerde — compressed serialization
- LombokHTTP — gzip content encoding
- LombokRAGFrameworks — vector store persistence

## Registry Targets
| Registry | Package | Status |
|----------|---------|--------|
| crates.io | lombokcompress | Planned |
| npm | lombokcompress | Planned |
| PyPI | lombokcompress | Planned |
| go.sum | codinglombok/lombokcompress | Planned |
| Packagist | codinglombok/lombokcompress | Planned |
