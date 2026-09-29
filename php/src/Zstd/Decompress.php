<?php

declare(strict_types=1);

namespace LombokCompress\Zstd;

use LombokCompress\CompressError;
use LombokCompress\CompressErrorCode;

/**
 * Zstandard decompressor (raw + RLE blocks).
 */
final class Decompress
{
    private const ZSTD_MAGIC = 0xFD2FB528;

    /** Block_Maximum_Size from RFC 8878 §3.1.1.2.3 (128 KiB). */
    private const BLOCK_MAX_SIZE = 128 * 1024;

    /**
     * Decompress a Zstandard frame.
     *
     * $maxOutput bounds the decompressed size; set it for untrusted input.
     */
    public static function decompress(string $data, int $maxOutput = PHP_INT_MAX): string
    {
        $srcLen = strlen($data);
        if ($srcLen < 5) {
            throw new CompressError(CompressErrorCode::UnexpectedEof, 'input too short for Zstd');
        }

        $pos = 0;

        // Magic number
        $magic = ord($data[0])
            | (ord($data[1]) << 8)
            | (ord($data[2]) << 16)
            | (ord($data[3]) << 24);
        if (($magic & 0xFFFFFFFF) !== self::ZSTD_MAGIC) {
            throw new CompressError(CompressErrorCode::InvalidInput, 'invalid Zstd magic number');
        }
        $pos = 4;

        // Frame header descriptor
        $fhd = ord($data[$pos]);
        $pos++;

        $singleSegment = ($fhd & 0x20) !== 0;
        $contentChecksum = ($fhd & 0x04) !== 0;
        $dictIDFlag = $fhd & 0x03;

        $fcsBits = ($fhd >> 6) & 0x03;
        $fcsFieldSize = match ($fcsBits) {
            0 => $singleSegment ? 1 : 0,
            1 => 2,
            2 => 4,
            3 => 8,
        };

        if (!$singleSegment) $pos++; // window descriptor

        // Dict ID
        $dictIDBytes = [0, 1, 2, 4][$dictIDFlag];
        $pos += $dictIDBytes;

        // Frame content size (untrusted: validated, never used to pre-allocate)
        $contentSize = null;
        if ($fcsFieldSize > 0) {
            if ($pos + $fcsFieldSize > $srcLen) {
                throw new CompressError(CompressErrorCode::UnexpectedEof, 'missing frame content size');
            }
            $contentSize = 0;
            for ($i = $fcsFieldSize - 1; $i >= 0; $i--) {
                $contentSize = $contentSize * 256 + ord($data[$pos + $i]);
            }
            if ($fcsFieldSize === 2) {
                $contentSize += 256;
            }
            $pos += $fcsFieldSize;
            if ($contentSize > $maxOutput) {
                throw new CompressError(CompressErrorCode::OutputTooSmall, 'frame content size exceeds limit');
            }
        }

        $output = '';

        // Read blocks
        while (true) {
            if ($pos + 3 > $srcLen) {
                throw new CompressError(CompressErrorCode::UnexpectedEof, 'unexpected end of Zstd data');
            }

            $bh = ord($data[$pos]) | (ord($data[$pos + 1]) << 8) | (ord($data[$pos + 2]) << 16);
            $pos += 3;

            $lastBlock = ($bh & 1) !== 0;
            $blockType = ($bh >> 1) & 0x03;
            $blockSize = $bh >> 3;

            if ($blockSize > self::BLOCK_MAX_SIZE) {
                throw new CompressError(CompressErrorCode::InvalidInput, 'Zstd block exceeds Block_Maximum_Size');
            }
            if ($blockType < 2 && strlen($output) + $blockSize > $maxOutput) {
                throw new CompressError(CompressErrorCode::OutputTooSmall, 'decompressed data exceeds limit');
            }

            switch ($blockType) {
                case 0: // Raw
                    if ($pos + $blockSize > $srcLen) {
                        throw new CompressError(CompressErrorCode::UnexpectedEof, 'raw block extends past input');
                    }
                    $output .= substr($data, $pos, $blockSize);
                    $pos += $blockSize;
                    break;

                case 1: // RLE
                    if ($pos >= $srcLen) {
                        throw new CompressError(CompressErrorCode::UnexpectedEof, 'RLE block missing byte');
                    }
                    $byte = $data[$pos];
                    $pos++;
                    $output .= str_repeat($byte, $blockSize);
                    break;

                case 2:
                    throw new CompressError(CompressErrorCode::Unsupported, 'Zstd compressed blocks (FSE) not yet implemented');

                case 3:
                    throw new CompressError(CompressErrorCode::InvalidInput, 'reserved Zstd block type');
            }

            if ($lastBlock) break;
        }

        // Content checksum (XXH64 not implemented yet): require its presence only.
        if ($contentChecksum && $pos + 4 > $srcLen) {
            throw new CompressError(CompressErrorCode::UnexpectedEof, 'missing content checksum');
        }
        if ($contentSize !== null && strlen($output) !== $contentSize) {
            throw new CompressError(CompressErrorCode::InvalidInput, 'decompressed size != frame content size');
        }

        return $output;
    }
}
