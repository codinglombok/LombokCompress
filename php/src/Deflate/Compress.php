<?php

declare(strict_types=1);

namespace LombokCompress\Deflate;

use LombokCompress\CompressError;
use LombokCompress\CompressErrorCode;

/**
 * Deflate/gzip/zlib compression — fixed Huffman, LZ77.
 */
final class Compress
{
    /** @var int[] */
    private static array $crc32Table = [];

    private static function initCrc32Table(): void
    {
        if (self::$crc32Table !== []) return;

        for ($n = 0; $n < 256; $n++) {
            $c = $n;
            for ($k = 0; $k < 8; $k++) {
                if ($c & 1) {
                    $c = 0xEDB88320 ^ ($c >> 1);
                } else {
                    $c >>= 1;
                }
            }
            self::$crc32Table[$n] = $c & 0xFFFFFFFF;
        }
    }

    public static function crc32(string $data): int
    {
        self::initCrc32Table();
        $crc = 0xFFFFFFFF;
        $len = strlen($data);
        for ($i = 0; $i < $len; $i++) {
            $crc = self::$crc32Table[($crc ^ ord($data[$i])) & 0xFF] ^ ($crc >> 8);
        }
        return ($crc ^ 0xFFFFFFFF) & 0xFFFFFFFF;
    }

    public static function adler32(string $data): int
    {
        $a = 1;
        $b = 0;
        $len = strlen($data);
        for ($i = 0; $i < $len; $i++) {
            $a = ($a + ord($data[$i])) % 65521;
            $b = ($b + $a) % 65521;
        }
        return (($b << 16) | $a) & 0xFFFFFFFF;
    }

    private static function reverseBits(int $value, int $count): int
    {
        $result = 0;
        for ($i = 0; $i < $count; $i++) {
            $result = ($result << 1) | ($value & 1);
            $value >>= 1;
        }
        return $result;
    }

    /** @return array{int, int} [code, bits] */
    private static function fixedLiteralCode(int $lit): array
    {
        if ($lit <= 143) {
            return [self::reverseBits(0x30 + $lit, 8), 8];
        } elseif ($lit <= 255) {
            return [self::reverseBits(0x190 + $lit - 144, 9), 9];
        } elseif ($lit <= 279) {
            return [self::reverseBits($lit - 256, 7), 7];
        } elseif ($lit <= 287) {
            return [self::reverseBits(0xC0 + $lit - 280, 8), 8];
        }
        throw new CompressError(CompressErrorCode::InternalError, "invalid literal $lit");
    }

    /** @return array{int, int} [code, bits] */
    private static function fixedDistanceCode(int $dist): array
    {
        return [self::reverseBits($dist, 5), 5];
    }

    /** @var array<array{int,int,int}> [minLen, code, extra] */
    private const LENGTH_TABLE = [
        [3,257,0],[4,258,0],[5,259,0],[6,260,0],[7,261,0],[8,262,0],[9,263,0],[10,264,0],
        [11,265,1],[13,266,1],[15,267,1],[17,268,1],
        [19,269,2],[23,270,2],[27,271,2],[31,272,2],
        [35,273,3],[43,274,3],[51,275,3],[59,276,3],
        [67,277,4],[83,278,4],[99,279,4],[115,280,4],
        [131,281,5],[163,282,5],[195,283,5],[227,284,5],
        [258,285,0],
    ];

    /** @var array<array{int,int,int}> [minDist, code, extra] */
    private const DISTANCE_TABLE = [
        [1,0,0],[2,1,0],[3,2,0],[4,3,0],
        [5,4,1],[7,5,1],[9,6,2],[13,7,2],
        [17,8,3],[25,9,3],[33,10,4],[49,11,4],
        [65,12,5],[97,13,5],[129,14,6],[193,15,6],
        [257,16,7],[385,17,7],[513,18,8],[769,19,8],
        [1025,20,9],[1537,21,9],[2049,22,10],[3073,23,10],
        [4097,24,11],[6145,25,11],[8193,26,12],[12289,27,12],
        [16385,28,13],[24577,29,13],
    ];

    /** @return array{int,int,int} [code, extraBits, extraVal] */
    private static function encodeLength(int $length): array
    {
        for ($i = count(self::LENGTH_TABLE) - 1; $i >= 0; $i--) {
            if ($length >= self::LENGTH_TABLE[$i][0]) {
                return [self::LENGTH_TABLE[$i][1], self::LENGTH_TABLE[$i][2], $length - self::LENGTH_TABLE[$i][0]];
            }
        }
        throw new CompressError(CompressErrorCode::InternalError, "invalid length $length");
    }

    /** @return array{int,int,int} [code, extraBits, extraVal] */
    private static function encodeDistance(int $distance): array
    {
        for ($i = count(self::DISTANCE_TABLE) - 1; $i >= 0; $i--) {
            if ($distance >= self::DISTANCE_TABLE[$i][0]) {
                return [self::DISTANCE_TABLE[$i][1], self::DISTANCE_TABLE[$i][2], $distance - self::DISTANCE_TABLE[$i][0]];
            }
        }
        throw new CompressError(CompressErrorCode::InternalError, "invalid distance $distance");
    }

    /**
     * @return list<array{int}|array{int,int}> Tokens: [literal] or [length, distance]
     */
    private static function lz77Compress(string $data): array
    {
        $tokens = [];
        $srcLen = strlen($data);
        if ($srcLen === 0) return $tokens;

        /** @var array<int, int[]> */
        $hashTable = [];
        $pos = 0;

        $hash3 = function(int $p) use ($data, $srcLen): int {
            if ($p + 2 >= $srcLen) return 0;
            return ord($data[$p]) | (ord($data[$p + 1]) << 8) | (ord($data[$p + 2]) << 16);
        };

        while ($pos < $srcLen) {
            if ($pos + 2 >= $srcLen) {
                $tokens[] = [ord($data[$pos])];
                $pos++;
                continue;
            }

            $h = $hash3($pos);
            $chain = $hashTable[$h] ?? [];

            $bestLen = 0;
            $bestDist = 0;
            $checks = 0;

            for ($ci = count($chain) - 1; $ci >= 0; $ci--) {
                $ref = $chain[$ci];
                if ($pos - $ref > 32768) break;
                $checks++;
                if ($checks > 64) break;

                $ml = 0;
                $maxML = min(258, $srcLen - $pos);
                while ($ml < $maxML && $data[$pos + $ml] === $data[$ref + $ml]) {
                    $ml++;
                }

                if ($ml > $bestLen) {
                    $bestLen = $ml;
                    $bestDist = $pos - $ref;
                    if ($ml >= 258) break;
                }
            }

            $hashTable[$h][] = $pos;

            if ($bestLen >= 3) {
                $tokens[] = [$bestLen, $bestDist];
                for ($i = 1; $i < $bestLen; $i++) {
                    if ($pos + $i + 2 < $srcLen) {
                        $ih = $hash3($pos + $i);
                        $hashTable[$ih][] = $pos + $i;
                    }
                }
                $pos += $bestLen;
            } else {
                $tokens[] = [ord($data[$pos])];
                $pos++;
            }
        }

        return $tokens;
    }

    public static function deflateCompress(string $data): string
    {
        $tokens = self::lz77Compress($data);

        $bitBuf = 0;
        $bitCount = 0;
        $output = '';

        $writeBits = function(int $value, int $count) use (&$bitBuf, &$bitCount, &$output): void {
            $bitBuf |= ($value & ((1 << $count) - 1)) << $bitCount;
            $bitCount += $count;
            while ($bitCount >= 8) {
                $output .= chr($bitBuf & 0xFF);
                $bitBuf >>= 8;
                $bitCount -= 8;
            }
        };

        $writeBits(1, 1); // BFINAL
        $writeBits(1, 2); // BTYPE=01

        foreach ($tokens as $token) {
            if (count($token) === 1) {
                [$code, $bits] = self::fixedLiteralCode($token[0]);
                $writeBits($code, $bits);
            } else {
                [$length, $distance] = $token;
                [$lenCode, $lenExtra, $lenExtraVal] = self::encodeLength($length);
                [$code, $bits] = self::fixedLiteralCode($lenCode);
                $writeBits($code, $bits);
                if ($lenExtra > 0) $writeBits($lenExtraVal, $lenExtra);

                [$distCode, $distExtra, $distExtraVal] = self::encodeDistance($distance);
                [$dcode, $dbits] = self::fixedDistanceCode($distCode);
                $writeBits($dcode, $dbits);
                if ($distExtra > 0) $writeBits($distExtraVal, $distExtra);
            }
        }

        // End of block (256)
        [$code, $bits] = self::fixedLiteralCode(256);
        $writeBits($code, $bits);

        // Flush
        if ($bitCount > 0) {
            $output .= chr($bitBuf & 0xFF);
        }

        return $output;
    }

    public static function gzipCompress(string $data): string
    {
        $output = "\x1F\x8B"; // magic
        $output .= "\x08";     // method = deflate
        $output .= "\x00";     // flags
        $output .= "\x00\x00\x00\x00"; // mtime
        $output .= "\x00";     // xfl
        $output .= "\xFF";     // OS = unknown

        $output .= self::deflateCompress($data);

        $output .= pack('V', self::crc32($data));
        $output .= pack('V', strlen($data) & 0xFFFFFFFF);

        return $output;
    }

    public static function zlibCompress(string $data): string
    {
        $cmf = 0x78;
        $flg = 0x01;
        $check = ($cmf * 256 + $flg) % 31;
        if ($check !== 0) {
            $flg += 31 - $check;
        }

        $output = chr($cmf) . chr($flg);
        $output .= self::deflateCompress($data);
        $output .= pack('N', self::adler32($data)); // big-endian

        return $output;
    }
}
