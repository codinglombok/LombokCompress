// Package zstd provides Zstandard compression and decompression (levels 1-3, raw blocks).
package zstd

import (
	lombokcompress "github.com/codinglombok/lombokcompress"
)

const zstdMagic uint32 = 0xFD2FB528

// IsZstd checks if data starts with the Zstd magic number.
func IsZstd(data []byte) bool {
	if len(data) < 4 {
		return false
	}
	magic := uint32(data[0]) | uint32(data[1])<<8 | uint32(data[2])<<16 | uint32(data[3])<<24
	return magic == zstdMagic
}

// Compress compresses data using Zstandard (raw blocks).
// Level must be 1, 2, or 3.
func Compress(data []byte, level int) ([]byte, error) {
	if level < 1 || level > 3 {
		return nil, lombokcompress.NewCompressError(
			lombokcompress.ErrInvalidInput, "zstd level must be 1-3")
	}

	output := make([]byte, 0, len(data)+32)

	// Magic number (LE)
	output = append(output,
		byte(zstdMagic&0xFF),
		byte((zstdMagic>>8)&0xFF),
		byte((zstdMagic>>16)&0xFF),
		byte((zstdMagic>>24)&0xFF),
	)

	contentSize := len(data)

	// Frame header descriptor
	if contentSize <= 255 {
		output = append(output, 0x20) // Single_Segment=1, FCS=0 (1 byte)
		output = append(output, byte(contentSize))
	} else if contentSize <= 65535+256 {
		output = append(output, 0x60) // FCS=01 (2 bytes)
		sz := contentSize - 256
		output = append(output, byte(sz&0xFF), byte((sz>>8)&0xFF))
	} else {
		output = append(output, 0xA0) // FCS=10 (4 bytes)
		output = append(output,
			byte(contentSize&0xFF),
			byte((contentSize>>8)&0xFF),
			byte((contentSize>>16)&0xFF),
			byte((contentSize>>24)&0xFF),
		)
	}

	if len(data) == 0 {
		// Empty: one last raw block of size 0
		output = append(output, 0x01, 0x00, 0x00)
		return output, nil
	}

	// Emit blocks
	maxBlock := 128 * 1024
	pos := 0

	for pos < len(data) {
		remaining := len(data) - pos
		blockSize := remaining
		if blockSize > maxBlock {
			blockSize = maxBlock
		}
		isLast := pos+blockSize >= len(data)
		blockData := data[pos : pos+blockSize]

		// Check RLE
		first := blockData[0]
		isRLE := true
		for _, b := range blockData[1:] {
			if b != first {
				isRLE = false
				break
			}
		}

		if isRLE {
			// RLE block: type=1
			var bh uint32
			if isLast {
				bh = 1
			}
			bh |= 1 << 1
			bh |= uint32(blockSize) << 3
			output = append(output, byte(bh&0xFF), byte((bh>>8)&0xFF), byte((bh>>16)&0xFF))
			output = append(output, first)
		} else {
			// Raw block: type=0
			var bh uint32
			if isLast {
				bh = 1
			}
			bh |= uint32(blockSize) << 3
			output = append(output, byte(bh&0xFF), byte((bh>>8)&0xFF), byte((bh>>16)&0xFF))
			output = append(output, blockData...)
		}

		pos += blockSize
	}

	return output, nil
}
