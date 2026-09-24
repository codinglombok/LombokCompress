# Changelog

All notable changes to LombokCompress will be documented in this file.

## [0.1.0] - 2026-09-18

### Added
- LZ4 block compress/decompress (no_std compatible)
- LZ4 frame format v1.6.3 with XXH32 checksums
- Zstd compress/decompress levels 1-3 (raw blocks)
- Deflate compress/decompress (fixed Huffman + LZ77)
- Gzip/zlib wrappers with CRC32/Adler32
- Compress/Decompress traits for pluggable algorithms
- StreamCompressor/StreamDecompressor for chunked processing
- Cross-language test vectors (XXH32, LZ4, CRC32, Adler32)
- Ports: TypeScript, Python, Go, PHP
- GitHub Actions CI for all 5 languages
