<?php

declare(strict_types=1);

namespace LombokCompress\Lz4;

use LombokCompress\CompressError;
use LombokCompress\CompressErrorCode;

/**
 * LZ4 frame format (v1.6.3) — compress and decompress.
 */
final class Frame
{
    private const LZ4_MAGIC = 0x184D2204;

    private const MAX_BLOCK_SIZES = [
        4 => 64 * 1024,
        5 => 256 * 1024,
        6 => 1024 * 1024,
        7 => 4 * 1024 * 1024,
    ];

    /**
     * Compress data into an LZ4 frame.
     */
    public static function compress(
        string $data,
        bool $contentChecksum = false,
        bool $contentSize = false,
        bool $blockChecksum = false,
        int $maxBlockSize = 7,
    ): string {
        $output = '';

        // Magic (LE32)
        $output .= pack('V', self::LZ4_MAGIC);

        // FLG byte
        // version = 01; blocks are compressed independently (Block_Independence)
        $flg = 0x60;
        if ($contentChecksum) $flg |= 0x04;
        if ($contentSize) $flg |= 0x08;

        $bd = ($maxBlockSize & 0x07) << 4;

        $headerBytes = chr($flg) . chr($bd);

        if ($contentSize) {
            $headerBytes .= pack('P', strlen($data)); // 64-bit LE
        }

        $hc = (Xxhash::xxh32($headerBytes, 0) >> 8) & 0xFF;
        $output .= $headerBytes . chr($hc);

        // Blocks
        $maxBS = self::MAX_BLOCK_SIZES[$maxBlockSize];
        $pos = 0;
        $srcLen = strlen($data);

        while ($pos < $srcLen) {
            $chunkSize = min($srcLen - $pos, $maxBS);
            $chunk = substr($data, $pos, $chunkSize);

            $compressed = Block::compress($chunk);

            if (strlen($compressed) < strlen($chunk)) {
                $output .= pack('V', strlen($compressed));
                $output .= $compressed;
                if ($blockChecksum) {
                    $output .= pack('V', Xxhash::xxh32($compressed, 0));
                }
            } else {
                $output .= pack('V', strlen($chunk) | 0x80000000);
                $output .= $chunk;
                if ($blockChecksum) {
                    $output .= pack('V', Xxhash::xxh32($chunk, 0));
                }
            }

            $pos += $chunkSize;
        }

        // EndMark
        $output .= "\x00\x00\x00\x00";

        if ($contentChecksum) {
            $output .= pack('V', Xxhash::xxh32($data, 0));
        }

        return $output;
    }

    /**
     * Decompress an LZ4 frame.
     */
    public static function decompress(string $data): string
    {
        $srcLen = strlen($data);
        if ($srcLen < 7) {
            throw new CompressError(CompressErrorCode::UnexpectedEof, 'input too short for LZ4 frame');
        }

        $pos = 0;

        // Magic
        $magic = unpack('V', substr($data, 0, 4))[1];
        if ($magic !== self::LZ4_MAGIC) {
            throw new CompressError(CompressErrorCode::InvalidInput, 'invalid LZ4 frame magic number');
        }
        $pos = 4;

        $flg = ord($data[$pos]);
        $bd = ord($data[$pos + 1]);
        $pos += 2; // FLG + BD

        if (($flg >> 6) !== 0x01) {
            throw new CompressError(CompressErrorCode::Unsupported, 'unsupported LZ4 frame version');
        }
        $blockMax = self::MAX_BLOCK_SIZES[($bd >> 4) & 0x07] ?? null;
        if ($blockMax === null) {
            throw new CompressError(CompressErrorCode::InvalidInput, 'invalid LZ4 block maximum size');
        }
        $blockIndependent = ($flg & 0x20) !== 0;

        $hasContentChecksum = ($flg & 0x04) !== 0;
        $hasContentSize = ($flg & 0x08) !== 0;
        $hasBlockChecksum = ($flg & 0x10) !== 0;

        $headerStart = 4;
        $expectedContentSize = null;

        if ($hasContentSize) {
            if ($pos + 8 > $srcLen) {
                throw new CompressError(CompressErrorCode::UnexpectedEof, 'missing content size');
            }
            $expectedContentSize = unpack('P', substr($data, $pos, 8))[1];
            $pos += 8;
        }

        // Header checksum
        if ($pos >= $srcLen) {
            throw new CompressError(CompressErrorCode::UnexpectedEof, 'missing header checksum');
        }
        $headerData = substr($data, $headerStart, $pos - $headerStart);
        $expectedHC = (Xxhash::xxh32($headerData, 0) >> 8) & 0xFF;
        $actualHC = ord($data[$pos]);
        $pos++;

        if ($actualHC !== $expectedHC) {
            throw new CompressError(CompressErrorCode::ChecksumMismatch, 'LZ4 frame header checksum mismatch');
        }

        $output = '';

        while (true) {
            if ($pos + 4 > $srcLen) {
                throw new CompressError(CompressErrorCode::UnexpectedEof, 'missing block header');
            }

            $blockSize = unpack('V', substr($data, $pos, 4))[1];
            $pos += 4;

            if ($blockSize === 0) break;

            $isUncompressed = ($blockSize & 0x80000000) !== 0;
            $actualSize = $blockSize & 0x7FFFFFFF;

            if ($actualSize > $blockMax) {
                throw new CompressError(CompressErrorCode::InvalidInput, 'LZ4 block exceeds declared block maximum size');
            }
            if ($pos + $actualSize > $srcLen) {
                throw new CompressError(CompressErrorCode::UnexpectedEof, 'block data extends past input');
            }

            $blockData = substr($data, $pos, $actualSize);
            $pos += $actualSize;

            if ($isUncompressed) {
                $output .= $blockData;
            } else {
                // Linked blocks may reference up to 64KB of earlier output.
                $history = $blockIndependent ? 0 : min(strlen($output), 64 * 1024);
                self::decodeBlockInto($blockData, $output, strlen($output) - $history, $blockMax);
            }

            if ($hasBlockChecksum) {
                if ($pos + 4 > $srcLen) {
                    throw new CompressError(CompressErrorCode::UnexpectedEof, 'missing block checksum');
                }
                $expectedBC = unpack('V', substr($data, $pos, 4))[1];
                $actualBC = Xxhash::xxh32($blockData, 0);
                if ($actualBC !== $expectedBC) {
                    throw new CompressError(CompressErrorCode::ChecksumMismatch, 'LZ4 block checksum mismatch');
                }
                $pos += 4;
            }
        }

        if ($hasContentChecksum) {
            if ($pos + 4 > $srcLen) {
                throw new CompressError(CompressErrorCode::UnexpectedEof, 'missing content checksum');
            }
            $expectedCC = unpack('V', substr($data, $pos, 4))[1];
            $actualCC = Xxhash::xxh32($output, 0);
            if ($actualCC !== $expectedCC) {
                throw new CompressError(CompressErrorCode::ChecksumMismatch, 'LZ4 content checksum mismatch');
            }
        }

        if ($expectedContentSize !== null && strlen($output) !== $expectedContentSize) {
            throw new CompressError(CompressErrorCode::InvalidInput, 'decompressed size != content size');
        }

        return $output;
    }

    /**
     * Decode one LZ4 block, appending to $output. Matches may reach back to
     * $windowStart; the block may add at most $blockMax bytes.
     */
    private static function decodeBlockInto(string $data, string &$output, int $windowStart, int $blockMax): void
    {
        $srcLen = strlen($data);
        $limit = strlen($output) + $blockMax;
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
            if (strlen($output) + $litLen > $limit) {
                throw new CompressError(CompressErrorCode::OutputTooSmall, 'LZ4 block exceeds declared block maximum size');
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

            $matchLen = ($token & 0x0F) + 4;
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
            if ($matchStart < $windowStart) {
                throw new CompressError(CompressErrorCode::InvalidInput, 'match offset beyond output');
            }

            if ($outLen + $matchLen > $limit) {
                throw new CompressError(CompressErrorCode::OutputTooSmall, 'LZ4 block exceeds declared block maximum size');
            }
            for ($i = 0; $i < $matchLen; $i++) {
                $output .= $output[$matchStart + $i];
            }
        }
    }
}
