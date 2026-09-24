<?php

declare(strict_types=1);

namespace LombokCompress\Lz4;

/**
 * XXH32 hash — pure PHP implementation.
 */
final class Xxhash
{
    private const PRIME1 = 0x9E3779B1;
    private const PRIME2 = 0x85EBCA77;
    private const PRIME3 = 0xC2B2AE3D;
    private const PRIME4 = 0x27D4EB2F;
    private const PRIME5 = 0x165667B1;

    private static function u32(int $x): int
    {
        return $x & 0xFFFFFFFF;
    }

    private static function rotl32(int $x, int $r): int
    {
        return self::u32(($x << $r) | (self::u32($x) >> (32 - $r)));
    }

    private static function readU32LE(string $data, int $offset): int
    {
        return ord($data[$offset])
            | (ord($data[$offset + 1]) << 8)
            | (ord($data[$offset + 2]) << 16)
            | (ord($data[$offset + 3]) << 24);
    }

    private static function mul32(int $a, int $b): int
    {
        // 32-bit multiplication using gmp if available, else manual
        $a = self::u32($a);
        $b = self::u32($b);

        $lo = ($a & 0xFFFF) * ($b & 0xFFFF);
        $mid1 = ($a >> 16) * ($b & 0xFFFF);
        $mid2 = ($a & 0xFFFF) * ($b >> 16);

        return self::u32($lo + ($mid1 << 16) + ($mid2 << 16));
    }

    private static function round(int $acc, int $val): int
    {
        $acc = self::u32($acc + self::mul32($val, self::PRIME2));
        $acc = self::rotl32($acc, 13);
        return self::mul32($acc, self::PRIME1);
    }

    /**
     * Compute XXH32 hash of data with optional seed.
     */
    public static function xxh32(string $data, int $seed = 0): int
    {
        $length = strlen($data);
        $pos = 0;

        if ($length >= 16) {
            $v1 = self::u32($seed + self::PRIME1 + self::PRIME2);
            $v2 = self::u32($seed + self::PRIME2);
            $v3 = self::u32($seed);
            $v4 = self::u32($seed - self::PRIME1);

            $limit = $length - 16;
            while ($pos <= $limit) {
                $v1 = self::round($v1, self::readU32LE($data, $pos));
                $pos += 4;
                $v2 = self::round($v2, self::readU32LE($data, $pos));
                $pos += 4;
                $v3 = self::round($v3, self::readU32LE($data, $pos));
                $pos += 4;
                $v4 = self::round($v4, self::readU32LE($data, $pos));
                $pos += 4;
            }

            $h32 = self::u32(
                self::rotl32($v1, 1)
                + self::rotl32($v2, 7)
                + self::rotl32($v3, 12)
                + self::rotl32($v4, 18)
            );
        } else {
            $h32 = self::u32($seed + self::PRIME5);
        }

        $h32 = self::u32($h32 + $length);

        $limit = $length - 4;
        while ($pos <= $limit) {
            $h32 = self::u32($h32 + self::mul32(self::readU32LE($data, $pos), self::PRIME3));
            $h32 = self::mul32(self::rotl32($h32, 17), self::PRIME4);
            $pos += 4;
        }

        while ($pos < $length) {
            $h32 = self::u32($h32 + self::mul32(ord($data[$pos]), self::PRIME5));
            $h32 = self::mul32(self::rotl32($h32, 11), self::PRIME1);
            $pos++;
        }

        // Avalanche
        $h32 ^= $h32 >> 15;
        $h32 = self::mul32($h32, self::PRIME2);
        $h32 ^= self::u32($h32) >> 13;
        $h32 = self::mul32($h32, self::PRIME3);
        $h32 ^= self::u32($h32) >> 16;

        return self::u32($h32);
    }
}
