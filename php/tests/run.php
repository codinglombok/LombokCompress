<?php

/**
 * Zero-dependency test runner: shared vectors, roundtrips, ext-zlib interop
 * and malformed input. Any PHP warning/notice fails the run.
 */

declare(strict_types=1);

require __DIR__ . '/../vendor/autoload.php';

use LombokCompress\CompressError;
use LombokCompress\Deflate\Compress as DeflateC;
use LombokCompress\Deflate\Decompress as DeflateD;
use LombokCompress\Lz4\Block;
use LombokCompress\Lz4\Frame;
use LombokCompress\Lz4\Xxhash;
use LombokCompress\Zstd\Compress as ZstdC;
use LombokCompress\Zstd\Decompress as ZstdD;

set_error_handler(static function (int $no, string $msg, string $file, int $line): bool {
    throw new ErrorException($msg, 0, $no, $file, $line);
});

$failures = 0;
function check(bool $ok, string $what): void
{
    global $failures;
    if (!$ok) {
        $failures++;
        fwrite(STDERR, "FAIL: $what\n");
    }
}

/** xorshift32 so the corpus is deterministic. */
final class Rng
{
    public function __construct(private int $s) {}

    public function next(): int
    {
        $s = $this->s;
        $s ^= ($s << 13) & 0xFFFFFFFF;
        $s ^= $s >> 17;
        $s ^= ($s << 5) & 0xFFFFFFFF;
        return $this->s = $s;
    }

    public function bytes(int $n): string
    {
        $b = '';
        for ($i = 0; $i < $n; $i++) {
            $b .= chr($this->next() & 0xFF);
        }
        return $b;
    }
}

// --- Shared cross-language vectors
$vectors = json_decode(file_get_contents(__DIR__ . '/../../test-vectors/compress_vectors.json'), true);
foreach ($vectors['xxh32'] as $v) {
    check(Xxhash::xxh32(hex2bin($v['input_hex']), $v['seed']) === $v['expected'], "xxh32 {$v['input_hex']}");
}
foreach ($vectors['crc32'] as $v) {
    check(DeflateC::crc32(hex2bin($v['input_hex'])) === $v['expected'], "crc32 {$v['input_hex']}");
}
foreach ($vectors['adler32'] as $v) {
    check(DeflateC::adler32(hex2bin($v['input_hex'])) === $v['expected'], "adler32 {$v['input_hex']}");
}

// --- Roundtrip + ext-zlib interop
$r = new Rng(0x12345678);
$samples = ['', 'a', str_repeat('abc', 1000), str_repeat("\0", 20000), str_repeat('Hello, LombokCompress! ', 1000)];
foreach ([1, 13, 64, 1000, 20000] as $n) {
    $samples[] = $r->bytes($n);
    $low = '';
    for ($i = 0; $i < $n; $i++) {
        $low .= chr($r->next() % 3);
    }
    $samples[] = $low;
}
foreach ($samples as $i => $d) {
    check(Block::decompress(Block::compress($d), strlen($d)) === $d, "lz4 block roundtrip #$i");
    check(Frame::decompress(Frame::compress($d)) === $d, "lz4 frame roundtrip #$i");
    check(ZstdD::decompress(ZstdC::compress($d)) === $d, "zstd roundtrip #$i");
    check(DeflateD::deflateDecompress(DeflateC::deflateCompress($d)) === $d, "deflate roundtrip #$i");
    check(DeflateD::gzipDecompress(DeflateC::gzipCompress($d)) === $d, "gzip roundtrip #$i");
    check(DeflateD::zlibDecompress(DeflateC::zlibCompress($d)) === $d, "zlib roundtrip #$i");
    if (function_exists('gzdecode')) {
        check(gzdecode(DeflateC::gzipCompress($d)) === $d, "ext-zlib gzdecode #$i");
        check(gzuncompress(DeflateC::zlibCompress($d)) === $d, "ext-zlib gzuncompress #$i");
        check(gzinflate(DeflateC::deflateCompress($d)) === $d, "ext-zlib gzinflate #$i");
        check(DeflateD::deflateDecompress(gzdeflate($d, 0)) === $d, "stored block from ext-zlib #$i");
    }
}

// --- Malformed input must throw CompressError, never warn or crash
$r = new Rng(0xDEADBEEF);
$seed = str_repeat('The quick brown fox jumps over the lazy dog. ', 40);
$valid = [
    Frame::compress($seed), Block::compress($seed), ZstdC::compress($seed, 2),
    ZstdC::compress(str_repeat("\x07", 5000), 1),
    DeflateC::deflateCompress($seed), DeflateC::gzipCompress($seed), DeflateC::zlibCompress($seed),
];
$cases = [];
for ($i = 0; $i < 200; $i++) {
    $cases[] = $r->bytes($r->next() % 64);
}
foreach ($valid as $v) {
    for ($i = 0; $i < 100; $i++) {
        $c = $v;
        $flips = 1 + $r->next() % 4;
        for ($f = 0; $f < $flips; $f++) {
            $k = $r->next() % strlen($c);
            $c[$k] = chr(ord($c[$k]) ^ (1 << ($r->next() % 8)));
        }
        if ($r->next() % 4 === 0) {
            $c = substr($c, 0, $r->next() % strlen($c));
        }
        $cases[] = $c;
    }
}
$cases[] = "\x1f\x8b\x08\x08\0\0\0\0\0\xff" . str_repeat('A', 10);
$cases[] = "\x04\x22\x4d\x18\x68\x40" . str_repeat("\xff", 7) . "\x7f\0\0\0\0\0";
$cases[] = "\x28\xb5\x2f\xfd\xe0" . str_repeat("\xff", 7) . "\x7f\x01\0\0";
$decoders = [
    static fn (string $c) => Block::decompress($c, 1 << 16),
    static fn (string $c) => Frame::decompress($c),
    static fn (string $c) => ZstdD::decompress($c),
    static fn (string $c) => DeflateD::deflateDecompress($c),
    static fn (string $c) => DeflateD::gzipDecompress($c),
    static fn (string $c) => DeflateD::zlibDecompress($c),
];
foreach ($cases as $i => $c) {
    foreach ($decoders as $j => $dec) {
        try {
            $dec($c);
        } catch (CompressError) {
            // expected
        } catch (Throwable $e) {
            check(false, "malformed case $i decoder $j: " . get_class($e) . ': ' . $e->getMessage() . ' ' . bin2hex($c));
        }
    }
}

// --- Output limits
$big = str_repeat('A', 200000);
foreach ([
    'deflate' => static fn () => DeflateD::deflateDecompress(DeflateC::deflateCompress($big), 1000),
    'zstd' => static fn () => ZstdD::decompress(ZstdC::compress($big), 1000),
] as $name => $fn) {
    try {
        $fn();
        check(false, "$name limit not enforced");
    } catch (CompressError) {
    }
}

if ($failures > 0) {
    fwrite(STDERR, "$failures failure(s)\n");
    exit(1);
}
echo "All PHP tests passed\n";
