//! Decoders must reject malformed input with an error, never panic.

use lombokcompress::deflate::{
    deflate_compress, deflate_decompress, gzip_compress, gzip_decompress, zlib_compress,
    zlib_decompress,
};
use lombokcompress::lz4::{
    compress_block, compress_bound, compress_frame, decompress_block, decompress_frame,
    FrameOptions,
};
use lombokcompress::zstd::{zstd_compress, zstd_decompress, zstd_decompress_limited, ZstdLevel};

/// Small deterministic PRNG (xorshift32) so the test needs no dependencies.
struct Rng(u32);

impl Rng {
    fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0
    }

    fn bytes(&mut self, len: usize) -> Vec<u8> {
        (0..len).map(|_| self.next() as u8).collect()
    }
}

fn sample_inputs(rng: &mut Rng) -> Vec<Vec<u8>> {
    let mut inputs = vec![
        Vec::new(),
        b"a".to_vec(),
        b"abcabcabcabcabcabcabcabcabcabc".to_vec(),
        vec![0u8; 100_000],
        b"Hello, LombokCompress! ".repeat(5_000),
    ];
    for len in [1usize, 13, 64, 1000, 70_000] {
        inputs.push(rng.bytes(len));
        // Low-entropy data exercises long and overlapping matches.
        inputs.push((0..len).map(|_| (rng.next() % 3) as u8).collect());
    }
    inputs
}

fn lz4_block_roundtrip(data: &[u8]) -> Vec<u8> {
    let mut compressed = vec![0u8; compress_bound(data.len())];
    let csize = compress_block(data, &mut compressed).unwrap();
    compressed.truncate(csize);
    let mut out = vec![0u8; data.len()];
    let dsize = decompress_block(&compressed, &mut out).unwrap();
    out.truncate(dsize);
    out
}

#[test]
fn roundtrip_all_formats() {
    let mut rng = Rng(0x1234_5678);
    for data in sample_inputs(&mut rng) {
        assert_eq!(lz4_block_roundtrip(&data), data);

        let frame = compress_frame(&data, &FrameOptions::default()).unwrap();
        assert_eq!(decompress_frame(&frame).unwrap(), data);

        let z = zstd_compress(&data, ZstdLevel::L1).unwrap();
        assert_eq!(zstd_decompress(&z).unwrap(), data);

        assert_eq!(
            deflate_decompress(&deflate_compress(&data), usize::MAX).unwrap(),
            data
        );
        assert_eq!(gzip_decompress(&gzip_compress(&data)).unwrap(), data);
        assert_eq!(zlib_decompress(&zlib_compress(&data)).unwrap(), data);
    }
}

/// Feed every decoder random bytes and bit-flipped valid streams.
#[test]
fn malformed_input_never_panics() {
    let mut rng = Rng(0xDEAD_BEEF);
    let seed = b"The quick brown fox jumps over the lazy dog. ".repeat(40);

    let mut valid = vec![
        compress_frame(&seed, &FrameOptions::default()).unwrap(),
        zstd_compress(&seed, ZstdLevel::L2).unwrap(),
        zstd_compress(&[7u8; 5000], ZstdLevel::L1).unwrap(),
        deflate_compress(&seed),
        gzip_compress(&seed),
        zlib_compress(&seed),
    ];
    let mut block = vec![0u8; compress_bound(seed.len())];
    let n = compress_block(&seed, &mut block).unwrap();
    block.truncate(n);
    valid.push(block);

    let mut cases: Vec<Vec<u8>> = Vec::new();
    for _ in 0..500 {
        let len = (rng.next() % 64) as usize;
        cases.push(rng.bytes(len));
    }
    for v in &valid {
        for _ in 0..300 {
            let mut c = v.clone();
            let flips = 1 + rng.next() % 4;
            for _ in 0..flips {
                let i = rng.next() as usize % c.len();
                c[i] ^= 1 << (rng.next() % 8);
            }
            if rng.next() % 4 == 0 {
                let cut = rng.next() as usize % c.len();
                c.truncate(cut);
            }
            cases.push(c);
        }
    }
    // Headers of every format with random tails.
    for magic in [
        &[0x04u8, 0x22, 0x4D, 0x18][..],
        &[0x28, 0xB5, 0x2F, 0xFD][..],
        &[0x1F, 0x8B, 0x08][..],
        &[0x78, 0x9C][..],
    ] {
        for _ in 0..300 {
            let mut c = magic.to_vec();
            let len = (rng.next() % 40) as usize;
            c.extend(rng.bytes(len));
            cases.push(c);
        }
    }

    let mut out = vec![0u8; 1 << 16];
    for c in &cases {
        let _ = decompress_block(c, &mut out);
        let _ = decompress_frame(c);
        let _ = zstd_decompress_limited(c, 1 << 20);
        let _ = deflate_decompress(c, 1 << 20);
        let _ = gzip_decompress(c);
        let _ = zlib_decompress(c);
    }
}

/// Header size fields are untrusted and must not drive allocation.
#[test]
fn huge_declared_sizes_are_rejected() {
    // LZ4 frame: FLG with content size, BD=64KB, content size = u64::MAX.
    let mut lz4 = vec![0x04, 0x22, 0x4D, 0x18, 0x48, 0x40];
    lz4.extend_from_slice(&u64::MAX.to_le_bytes());
    lz4.extend_from_slice(&[0x00, 0, 0, 0, 0]);
    assert!(decompress_frame(&lz4).is_err());

    // Zstd: single segment, 8-byte FCS = u64::MAX, then a last empty raw block.
    let mut zs = vec![0x28, 0xB5, 0x2F, 0xFD, 0xE0];
    zs.extend_from_slice(&u64::MAX.to_le_bytes());
    zs.extend_from_slice(&[0x01, 0x00, 0x00]);
    assert!(zstd_decompress(&zs).is_err());

    // Zstd RLE bomb stopped by the output limit.
    let mut bomb = vec![0x28, 0xB5, 0x2F, 0xFD, 0x00, 0x48]; // FHD + Window_Descriptor
    for i in 0..64 {
        let last = if i == 63 { 1 } else { 0 };
        let bh: u32 = last | (1 << 1) | ((128 * 1024) << 3);
        bomb.extend_from_slice(&bh.to_le_bytes()[..3]);
        bomb.push(0x41);
    }
    assert!(zstd_decompress_limited(&bomb, 1 << 20).is_err());
    assert_eq!(zstd_decompress(&bomb).unwrap().len(), 64 * 128 * 1024);
}
