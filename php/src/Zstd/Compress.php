<?php

declare(strict_types=1);

namespace LombokCompress\Zstd;

use LombokCompress\CompressError;
use LombokCompress\CompressErrorCode;

/**
 * Zstandard compressor (levels 1-3, raw blocks).
 */
final class Compress
{
    private const ZSTD_MAGIC = 0xFD2FB528;

    /**
     * Check if data starts with Zstd magic number.
     */
    public static function isZstd(string $data): bool
    {
        if (strlen($data) < 4) return false;
        $magic = ord($data[0])
            | (ord($data[1]) << 8)
            | (ord($data[2]) << 16)
            | (ord($data[3]) << 24);
        return ($magic & 0xFFFFFFFF) === self::ZSTD_MAGIC;
    }

    /**
     * Compress data using Zstandard (raw blocks).
     *
     * @param int $level 1, 2, or 3
     */
    public static function compress(string $data, int $level = 1): string
    {
        if ($level < 1 || $level > 3) {
            throw new CompressError(CompressErrorCode::InvalidInput, 'zstd level must be 1-3');
        }

        $output = '';

        // Magic (LE)
        $output .= pack('V', self::ZSTD_MAGIC);

        $contentSize = strlen($data);

        // Frame header descriptor
        if ($contentSize <= 255) {
            $output .= chr(0x20); // Single_Segment=1, FCS=0 (1 byte)
            $output .= chr($contentSize);
        } elseif ($contentSize <= 65535 + 256) {
            $output .= chr(0x60); // FCS=01 (2 bytes)
            $sz = $contentSize - 256;
            $output .= chr($sz & 0xFF) . chr(($sz >> 8) & 0xFF);
        } else {
            $output .= chr(0xA0); // FCS=10 (4 bytes)
            $output .= pack('V', $contentSize);
        }

        if ($contentSize === 0) {
            $output .= "\x01\x00\x00";
            return $output;
        }

        // Emit blocks
        $maxBlock = 128 * 1024;
        $pos = 0;

        while ($pos < $contentSize) {
            $remaining = $contentSize - $pos;
            $blockSize = min($remaining, $maxBlock);
            $isLast = ($pos + $blockSize >= $contentSize);
            $blockData = substr($data, $pos, $blockSize);

            // Check RLE
            $first = $blockData[0];
            $isRLE = true;
            for ($i = 1; $i < $blockSize; $i++) {
                if ($blockData[$i] !== $first) {
                    $isRLE = false;
                    break;
                }
            }

            if ($isRLE) {
                $bh = ($isLast ? 1 : 0) | (1 << 1) | ($blockSize << 3);
                $output .= chr($bh & 0xFF) . chr(($bh >> 8) & 0xFF) . chr(($bh >> 16) & 0xFF);
                $output .= $first;
            } else {
                $bh = ($isLast ? 1 : 0) | (0 << 1) | ($blockSize << 3);
                $output .= chr($bh & 0xFF) . chr(($bh >> 8) & 0xFF) . chr(($bh >> 16) & 0xFF);
                $output .= $blockData;
            }

            $pos += $blockSize;
        }

        return $output;
    }
}
