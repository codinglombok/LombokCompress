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

    /**
     * Decompress a Zstandard frame.
     */
    public static function decompress(string $data): string
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

        // Content size (skip)
        $pos += $fcsFieldSize;

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

        return $output;
    }
}
