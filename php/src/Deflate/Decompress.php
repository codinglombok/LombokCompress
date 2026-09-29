<?php

declare(strict_types=1);

namespace LombokCompress\Deflate;

use LombokCompress\CompressError;
use LombokCompress\CompressErrorCode;

/**
 * Deflate/gzip/zlib decompression — fixed Huffman.
 */
final class Decompress
{
    private const LENGTH_BASE = [
        3,4,5,6,7,8,9,10,11,13,15,17,19,23,27,31,35,43,51,59,
        67,83,99,115,131,163,195,227,258,
    ];

    private const LENGTH_EXTRA = [
        0,0,0,0,0,0,0,0,1,1,1,1,2,2,2,2,3,3,3,3,
        4,4,4,4,5,5,5,5,0,
    ];

    private const DIST_BASE = [
        1,2,3,4,5,7,9,13,17,25,33,49,65,97,129,193,257,385,513,769,
        1025,1537,2049,3073,4097,6145,8193,12289,16385,24577,
    ];

    private const DIST_EXTRA = [
        0,0,0,0,1,1,2,2,3,3,4,4,5,5,6,6,7,7,8,8,
        9,9,10,10,11,11,12,12,13,13,
    ];

    private string $data;
    private int $pos;
    private int $bitBuf;
    private int $bitCount;

    private function __construct(string $data)
    {
        $this->data = $data;
        $this->pos = 0;
        $this->bitBuf = 0;
        $this->bitCount = 0;
    }

    private function readBits(int $count): int
    {
        while ($this->bitCount < $count) {
            if ($this->pos >= strlen($this->data)) {
                throw new CompressError(CompressErrorCode::UnexpectedEof, 'unexpected end of deflate data');
            }
            $this->bitBuf |= ord($this->data[$this->pos]) << $this->bitCount;
            $this->pos++;
            $this->bitCount += 8;
        }
        $value = $this->bitBuf & ((1 << $count) - 1);
        $this->bitBuf >>= $count;
        $this->bitCount -= $count;
        return $value;
    }

    private function decodeFixedLiteral(): int
    {
        $code = 0;
        for ($i = 0; $i < 7; $i++) {
            $code = ($code << 1) | $this->readBits(1);
        }

        if ($code <= 23) return 256 + $code;

        $code = ($code << 1) | $this->readBits(1);

        if ($code >= 48 && $code <= 191) return $code - 48;
        if ($code >= 192 && $code <= 199) return 280 + $code - 192;

        $code = ($code << 1) | $this->readBits(1);

        if ($code >= 400 && $code <= 511) return 144 + $code - 400;

        throw new CompressError(CompressErrorCode::InvalidInput, "invalid fixed Huffman code $code");
    }

    private function decodeFixedDistance(): int
    {
        $code = 0;
        for ($i = 0; $i < 5; $i++) {
            $code = ($code << 1) | $this->readBits(1);
        }
        return $code;
    }

    /** Default output cap for gzip/zlib, matching the Rust core (64 MiB). */
    public const DEFAULT_MAX_OUTPUT = 64 * 1024 * 1024;

    private static function tooLarge(): CompressError
    {
        return new CompressError(CompressErrorCode::OutputTooSmall, 'decompressed data exceeds limit');
    }

    /**
     * Decompress raw deflate data.
     *
     * $maxOutput bounds the decompressed size; set it for untrusted input.
     */
    public static function deflateDecompress(string $data, int $maxOutput = PHP_INT_MAX): string
    {
        $br = new self($data);
        $output = '';

        while (true) {
            $bfinal = $br->readBits(1);
            $btype = $br->readBits(2);

            if ($btype === 0) {
                // Stored block
                $br->bitBuf = 0;
                $br->bitCount = 0;

                if ($br->pos + 4 > strlen($br->data)) {
                    throw new CompressError(CompressErrorCode::UnexpectedEof, 'stored block header extends past input');
                }
                $length = ord($br->data[$br->pos]) | (ord($br->data[$br->pos + 1]) << 8);
                $nlength = ord($br->data[$br->pos + 2]) | (ord($br->data[$br->pos + 3]) << 8);
                $br->pos += 4;

                if ($length !== (~$nlength & 0xFFFF)) {
                    throw new CompressError(CompressErrorCode::InvalidInput, 'stored block length check failed');
                }

                if ($br->pos + $length > strlen($br->data)) {
                    throw new CompressError(CompressErrorCode::UnexpectedEof, 'stored block data extends past input');
                }
                if (strlen($output) + $length > $maxOutput) {
                    throw self::tooLarge();
                }
                $output .= substr($br->data, $br->pos, $length);
                $br->pos += $length;

            } elseif ($btype === 1) {
                // Fixed Huffman
                while (true) {
                    $sym = $br->decodeFixedLiteral();

                    if ($sym < 256) {
                        if (strlen($output) >= $maxOutput) {
                            throw self::tooLarge();
                        }
                        $output .= chr($sym);
                    } elseif ($sym === 256) {
                        break;
                    } else {
                        $li = $sym - 257;
                        if ($li >= count(self::LENGTH_BASE)) {
                            throw new CompressError(CompressErrorCode::InvalidInput, "invalid length code $sym");
                        }
                        $matchLen = self::LENGTH_BASE[$li];
                        if (self::LENGTH_EXTRA[$li] > 0) {
                            $matchLen += $br->readBits(self::LENGTH_EXTRA[$li]);
                        }

                        $distCode = $br->decodeFixedDistance();
                        if ($distCode >= count(self::DIST_BASE)) {
                            throw new CompressError(CompressErrorCode::InvalidInput, "invalid distance code $distCode");
                        }
                        $distance = self::DIST_BASE[$distCode];
                        if (self::DIST_EXTRA[$distCode] > 0) {
                            $distance += $br->readBits(self::DIST_EXTRA[$distCode]);
                        }

                        $outLen = strlen($output);
                        $start = $outLen - $distance;
                        if ($start < 0) {
                            throw new CompressError(CompressErrorCode::InvalidInput, 'distance beyond output buffer');
                        }
                        if ($outLen + $matchLen > $maxOutput) {
                            throw self::tooLarge();
                        }
                        for ($i = 0; $i < $matchLen; $i++) {
                            $output .= $output[$start + $i];
                        }
                    }
                }

            } elseif ($btype === 2) {
                throw new CompressError(CompressErrorCode::Unsupported, 'dynamic Huffman not yet implemented');
            } else {
                throw new CompressError(CompressErrorCode::InvalidInput, 'reserved deflate block type');
            }

            if ($bfinal) break;
        }

        return $output;
    }

    public static function gzipDecompress(string $data, int $maxOutput = self::DEFAULT_MAX_OUTPUT): string
    {
        $srcLen = strlen($data);
        if ($srcLen < 18) {
            throw new CompressError(CompressErrorCode::UnexpectedEof, 'input too short for gzip');
        }

        if (ord($data[0]) !== 0x1F || ord($data[1]) !== 0x8B) {
            throw new CompressError(CompressErrorCode::InvalidInput, 'invalid gzip magic number');
        }

        if (ord($data[2]) !== 0x08) {
            throw new CompressError(CompressErrorCode::Unsupported, 'unsupported gzip compression method');
        }

        $flags = ord($data[3]);
        $pos = 10;

        $deflateEnd = $srcLen - 8;
        if ($flags & 0x04) {
            if ($pos + 2 > $deflateEnd) {
                throw new CompressError(CompressErrorCode::UnexpectedEof, 'missing gzip FEXTRA length');
            }
            $xlen = ord($data[$pos]) | (ord($data[$pos + 1]) << 8);
            $pos += 2 + $xlen;
        }
        if ($flags & 0x08) {
            while ($pos < $deflateEnd && $data[$pos] !== "\0") $pos++;
            $pos++;
        }
        if ($flags & 0x10) {
            while ($pos < $deflateEnd && $data[$pos] !== "\0") $pos++;
            $pos++;
        }
        if ($flags & 0x02) {
            $pos += 2;
        }

        if ($pos >= $deflateEnd) {
            throw new CompressError(CompressErrorCode::UnexpectedEof, 'gzip header extends past input');
        }

        $compressedData = substr($data, $pos, $deflateEnd - $pos);
        $decompressed = self::deflateDecompress($compressedData, $maxOutput);

        $trailer = substr($data, -8);
        $expectedCRC = unpack('V', substr($trailer, 0, 4))[1];
        $expectedSize = unpack('V', substr($trailer, 4, 4))[1];

        $actualCRC = Compress::crc32($decompressed);
        if ($actualCRC !== $expectedCRC) {
            throw new CompressError(CompressErrorCode::ChecksumMismatch, 'gzip CRC32 mismatch');
        }

        if ((strlen($decompressed) & 0xFFFFFFFF) !== $expectedSize) {
            throw new CompressError(CompressErrorCode::InvalidInput, 'gzip size mismatch');
        }

        return $decompressed;
    }

    public static function zlibDecompress(string $data, int $maxOutput = self::DEFAULT_MAX_OUTPUT): string
    {
        $srcLen = strlen($data);
        if ($srcLen < 6) {
            throw new CompressError(CompressErrorCode::UnexpectedEof, 'input too short for zlib');
        }

        $cmf = ord($data[0]);
        $flg = ord($data[1]);

        if (($cmf * 256 + $flg) % 31 !== 0) {
            throw new CompressError(CompressErrorCode::InvalidInput, 'invalid zlib header checksum');
        }

        if (($cmf & 0x0F) !== 8) {
            throw new CompressError(CompressErrorCode::Unsupported, 'unsupported zlib compression method');
        }

        if (($flg & 0x20) !== 0) {
            throw new CompressError(CompressErrorCode::Unsupported, 'zlib preset dictionary');
        }

        $compressedData = substr($data, 2, $srcLen - 6);
        $decompressed = self::deflateDecompress($compressedData, $maxOutput);

        $expectedAdler = unpack('N', substr($data, -4))[1];
        $actualAdler = Compress::adler32($decompressed);
        if ($actualAdler !== $expectedAdler) {
            throw new CompressError(CompressErrorCode::ChecksumMismatch, 'zlib Adler-32 mismatch');
        }

        return $decompressed;
    }
}
