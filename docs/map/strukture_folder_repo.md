# LombokCompress — Folder Structure

```
LombokCompress/
├── docs/
│   ├── architecture_repo.md
│   ├── masterplan_repo.md
│   ├── changelog.md
│   └── map/
│       └── strukture_folder_repo.md
├── test-vectors/
│   └── compress_vectors.json
├── rust/
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── error.rs
│       ├── traits.rs
│       ├── lz4/
│       │   ├── mod.rs
│       │   ├── block.rs
│       │   ├── frame.rs
│       │   └── xxhash.rs
│       ├── deflate/
│       │   ├── mod.rs
│       │   ├── huffman.rs
│       │   ├── lz77.rs
│       │   ├── compress.rs
│       │   └── decompress.rs
│       └── zstd/
│           ├── mod.rs
│           ├── compress.rs
│           └── decompress.rs
├── typescript/
│   ├── package.json
│   ├── tsconfig.json
│   └── src/
│       ├── index.ts
│       ├── error.ts
│       ├── lz4/
│       │   ├── index.ts
│       │   ├── block.ts
│       │   └── xxhash.ts
│       ├── deflate/
│       │   ├── index.ts
│       │   ├── compress.ts
│       │   └── decompress.ts
│       └── zstd/
│           ├── index.ts
│           ├── compress.ts
│           └── decompress.ts
├── python/
│   ├── pyproject.toml
│   └── lombokcompress/
│       ├── __init__.py
│       ├── error.py
│       ├── lz4/
│       │   ├── __init__.py
│       │   ├── block.py
│       │   ├── frame.py
│       │   └── xxhash.py
│       ├── deflate/
│       │   ├── __init__.py
│       │   ├── compress.py
│       │   └── decompress.py
│       └── zstd/
│           ├── __init__.py
│           ├── compress.py
│           └── decompress.py
├── go/
│   ├── go.mod
│   ├── compress.go
│   ├── lz4/
│   │   ├── block.go
│   │   ├── frame.go
│   │   └── xxhash.go
│   ├── deflate/
│   │   ├── compress.go
│   │   └── decompress.go
│   └── zstd/
│       ├── compress.go
│       └── decompress.go
├── php/
│   ├── composer.json
│   └── src/
│       ├── CompressError.php
│       ├── Lz4/
│       │   ├── Block.php
│       │   ├── Frame.php
│       │   └── Xxhash.php
│       ├── Deflate/
│       │   ├── Compress.php
│       │   └── Decompress.php
│       └── Zstd/
│           ├── Compress.php
│           └── Decompress.php
├── .github/
│   └── workflows/
│       ├── rust.yml
│       ├── typescript.yml
│       ├── python.yml
│       ├── go.yml
│       └── php.yml
├── README.md
├── LICENSE
└── .gitignore
```
