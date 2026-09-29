<?php

declare(strict_types=1);

namespace LombokCompress\Lz4;

use LombokCompress\CompressError;
use LombokCompress\CompressErrorCode;

/**
 * LZ4 block compression/decompression.
 */
final class Block
{
    private const HASH_LOG = 12;
    private const HASH_SIZE = 1 << self::HASH_LOG;
    private const MIN_MATCH = 4;
    private const MF_LIMIT = 12;
    private const LAST_LITERALS = 5;

    public static function compressBound(int $inputSize): int
    {
        return $inputSize + intdiv($inputSize, 255) + 16;
    }

    private static function hash4(int $v): int
    {
        // 32x32-bit multiply mod 2^32, split so the product never exceeds
        // PHP's signed 64-bit int (which would silently become a float).
        $k = 0x9E3779B1;
        $lo = ($v & 0xFFFF) * $k;
        $hi = ((($v >> 16) * $k) & 0xFFFF) << 16;
        return (($lo + $hi) & 0xFFFFFFFF) >> (32 - self::HASH_LOG);
    }

    private static function read32(string $data, int $off): int
    {
        return ord($data[$off])
            | (ord($data[$off + 1]) << 8)
            | (ord($data[$off + 2]) << 16)
            | (ord($data[$off + 3]) << 24);
    }

    /**
     * Compress data using LZ4 block format.
     */
    public static function compress(string $data): string
    {
        $srcLen = strlen($data);
        if ($srcLen === 0) {
            return '';
        }

        $output = '';
        $hashTable = array_fill(0, self::HASH_SIZE, -1);
        $pos = 0;
        $anchor = 0;
        $limit = $srcLen - self::MF_LIMIT;

        while ($pos < $limit) {
            $curVal = self::read32($data, $pos);
            $h = self::hash4($curVal);
            $ref = $hashTable[$h];
            $hashTable[$h] = $pos;

            if ($ref < 0 || $ref < $anchor || $pos - $ref > 65535 || self::read32($data, $ref) !== $curVal) {
                $pos++;
                continue;
            }

            $litLen = $pos - $anchor;

            // Extend match
            $matchPos = $pos + self::MIN_MATCH;
            $refPos = $ref + self::MIN_MATCH;
            // The last LAST_LITERALS bytes must stay literals (LZ4 end-of-block rule).
            $matchLimit = $srcLen - self::LAST_LITERALS;
            while ($matchPos < $matchLimit && $data[$matchPos] === $data[$refPos]) {
                $matchPos++;
                $refPos++;
            }
            $matchLen = $matchPos - $pos - self::MIN_MATCH;

            $tokenLit = min($litLen, 15);
            $tokenMatch = min($matchLen, 15);
            $output .= chr(($tokenLit << 4) | $tokenMatch);

            if ($litLen >= 15) {
                $remaining = $litLen - 15;
                while ($remaining >= 255) {
                    $output .= "\xFF";
                    $remaining -= 255;
                }
                $output .= chr($remaining);
            }

            $output .= substr($data, $anchor, $litLen);

            $offset = $pos - $ref;
            $output .= chr($offset & 0xFF) . chr(($offset >> 8) & 0xFF);

            if ($matchLen >= 15) {
                $remaining = $matchLen - 15;
                while ($remaining >= 255) {
                    $output .= "\xFF";
                    $remaining -= 255;
                }
                $output .= chr($remaining);
            }

            $pos = $matchPos;
            $anchor = $pos;
        }

        // Last literals
        $litLen = $srcLen - $anchor;
        $tokenLit = min($litLen, 15);
        $output .= chr($tokenLit << 4);

        if ($litLen >= 15) {
            $remaining = $litLen - 15;
            while ($remaining >= 255) {
                $output .= "\xFF";
                $remaining -= 255;
            }
            $output .= chr($remaining);
        }

        $output .= substr($data, $anchor);

        return $output;
    }

    /**
     * Decompress LZ4 block format data.
     */
    public static function decompress(string $data, int $uncompressedSize): string
    {
        $srcLen = strlen($data);
        if ($uncompressedSize < 0) {
            throw new CompressError(CompressErrorCode::InvalidInput, 'negative uncompressed size');
        }
        if ($srcLen === 0 && $uncompressedSize === 0) {
            return '';
        }

        $output = '';
        $pos = 0;

        while ($pos < $srcLen) {
            $token = ord($data[$pos]);
            $pos++;
            $litLen = $token >> 4;

            if ($litLen === 15) {
                while (true) {
                    if ($pos >= $srcLen) {
                        throw new CompressError(CompressErrorCode::UnexpectedEof, 'unexpected end reading literal length');
                    }
                    $extra = ord($data[$pos]);
                    $pos++;
                    $litLen += $extra;
                    if ($extra !== 255) break;
                }
            }

            if ($pos + $litLen > $srcLen) {
                throw new CompressError(CompressErrorCode::UnexpectedEof, 'literal data extends past input');
            }
            if (strlen($output) + $litLen > $uncompressedSize) {
                throw new CompressError(CompressErrorCode::OutputTooSmall, 'decompressed data exceeds uncompressed size');
            }
            $output .= substr($data, $pos, $litLen);
            $pos += $litLen;

            if ($pos >= $srcLen) break;

            if ($pos + 2 > $srcLen) {
                throw new CompressError(CompressErrorCode::UnexpectedEof, 'missing match offset');
            }
            $offset = ord($data[$pos]) | (ord($data[$pos + 1]) << 8);
            $pos += 2;

            if ($offset === 0) {
                throw new CompressError(CompressErrorCode::InvalidInput, 'zero match offset');
            }

            $matchLen = ($token & 0x0F) + self::MIN_MATCH;
            if (($token & 0x0F) === 15) {
                while (true) {
                    if ($pos >= $srcLen) {
                        throw new CompressError(CompressErrorCode::UnexpectedEof, 'unexpected end reading match length');
                    }
                    $extra = ord($data[$pos]);
                    $pos++;
                    $matchLen += $extra;
                    if ($extra !== 255) break;
                }
            }

            $outLen = strlen($output);
            $matchStart = $outLen - $offset;
            if ($matchStart < 0) {
                throw new CompressError(CompressErrorCode::InvalidInput, 'match offset beyond output');
            }

            if ($outLen + $matchLen > $uncompressedSize) {
                throw new CompressError(CompressErrorCode::OutputTooSmall, 'decompressed data exceeds uncompressed size');
            }
            for ($i = 0; $i < $matchLen; $i++) {
                $output .= $output[$matchStart + $i];
            }
        }

        if (strlen($output) !== $uncompressedSize) {
            throw new CompressError(CompressErrorCode::InvalidInput, 'decompressed size mismatch');
        }

        return $output;
    }
}
