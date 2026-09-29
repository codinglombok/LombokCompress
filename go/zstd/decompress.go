package zstd

import (
	"math"

	lombokcompress "github.com/codinglombok/lombokcompress/go"
)

// blockMaxSize is Block_Maximum_Size from RFC 8878 §3.1.1.2.3 (128 KiB).
const blockMaxSize = 128 * 1024

// Decompress decompresses a Zstandard frame.
func Decompress(data []byte) ([]byte, error) {
	return DecompressLimit(data, math.MaxInt)
}

// DecompressLimit decompresses a Zstandard frame, refusing to produce more
// than maxOutput bytes. Use it for untrusted input to bound memory use.
func DecompressLimit(data []byte, maxOutput int) ([]byte, error) {
	if len(data) < 5 {
		return nil, lombokcompress.NewCompressError(
			lombokcompress.ErrUnexpectedEof, "input too short for Zstd")
	}

	pos := 0

	// Magic number
	magic := uint32(data[0]) | uint32(data[1])<<8 | uint32(data[2])<<16 | uint32(data[3])<<24
	if magic != zstdMagic {
		return nil, lombokcompress.NewCompressError(
			lombokcompress.ErrInvalidInput, "invalid Zstd magic number")
	}
	pos = 4

	// Frame header descriptor
	fhd := data[pos]
	pos++

	singleSegment := (fhd & 0x20) != 0
	contentChecksum := (fhd & 0x04) != 0
	dictIDFlag := fhd & 0x03

	fcsBits := (fhd >> 6) & 0x03
	var fcsFieldSize int
	switch fcsBits {
	case 0:
		if singleSegment {
			fcsFieldSize = 1
		}
	case 1:
		fcsFieldSize = 2
	case 2:
		fcsFieldSize = 4
	case 3:
		fcsFieldSize = 8
	}

	// Window descriptor
	if !singleSegment {
		pos++
	}

	// Dict ID
	dictIDBytes := [4]int{0, 1, 2, 4}
	pos += dictIDBytes[dictIDFlag]

	// Frame content size (untrusted: validated, never used to pre-allocate)
	hasContentSize := fcsFieldSize > 0
	var contentSize uint64
	if hasContentSize {
		if pos+fcsFieldSize > len(data) {
			return nil, lombokcompress.NewCompressError(
				lombokcompress.ErrUnexpectedEof, "missing frame content size")
		}
		for i := fcsFieldSize - 1; i >= 0; i-- {
			contentSize = contentSize<<8 | uint64(data[pos+i])
		}
		if fcsFieldSize == 2 {
			contentSize += 256
		}
		pos += fcsFieldSize
		if contentSize > uint64(maxOutput) {
			return nil, lombokcompress.NewCompressError(
				lombokcompress.ErrOutputTooSmall, "frame content size exceeds limit")
		}
	}

	output := make([]byte, 0)

	// Read blocks
	for {
		if pos+3 > len(data) {
			return nil, lombokcompress.NewCompressError(
				lombokcompress.ErrUnexpectedEof, "unexpected end of Zstd data")
		}

		bh := uint32(data[pos]) | uint32(data[pos+1])<<8 | uint32(data[pos+2])<<16
		pos += 3

		lastBlock := (bh & 1) != 0
		blockType := (bh >> 1) & 0x03
		blockSize := int(bh >> 3)

		if blockSize > blockMaxSize {
			return nil, lombokcompress.NewCompressError(
				lombokcompress.ErrInvalidInput, "Zstd block exceeds Block_Maximum_Size")
		}
		if blockType < 2 && blockSize > maxOutput-len(output) {
			return nil, lombokcompress.NewCompressError(
				lombokcompress.ErrOutputTooSmall, "decompressed data exceeds limit")
		}

		switch blockType {
		case 0: // Raw
			if blockSize > len(data)-pos {
				return nil, lombokcompress.NewCompressError(
					lombokcompress.ErrUnexpectedEof, "raw block extends past input")
			}
			output = append(output, data[pos:pos+blockSize]...)
			pos += blockSize

		case 1: // RLE
			if pos >= len(data) {
				return nil, lombokcompress.NewCompressError(
					lombokcompress.ErrUnexpectedEof, "RLE block missing byte")
			}
			b := data[pos]
			pos++
			for i := 0; i < blockSize; i++ {
				output = append(output, b)
			}

		case 2: // Compressed
			return nil, lombokcompress.NewCompressError(
				lombokcompress.ErrUnsupported, "Zstd compressed blocks (FSE) not yet implemented")

		case 3:
			return nil, lombokcompress.NewCompressError(
				lombokcompress.ErrInvalidInput, "reserved Zstd block type")
		}

		if lastBlock {
			break
		}
	}

	// Content checksum (XXH64 not implemented yet): require its presence only.
	if contentChecksum && len(data)-pos < 4 {
		return nil, lombokcompress.NewCompressError(
			lombokcompress.ErrUnexpectedEof, "missing content checksum")
	}
	if hasContentSize && uint64(len(output)) != contentSize {
		return nil, lombokcompress.NewCompressError(
			lombokcompress.ErrInvalidInput, "decompressed size != frame content size")
	}

	return output, nil
}
