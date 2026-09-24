package zstd

import (
	lombokcompress "github.com/codinglombok/lombokcompress"
)

// Decompress decompresses a Zstandard frame.
func Decompress(data []byte) ([]byte, error) {
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
	// contentChecksum := (fhd & 0x04) != 0
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

	// Content size (skip for now)
	pos += fcsFieldSize

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

		switch blockType {
		case 0: // Raw
			if pos+blockSize > len(data) {
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

	return output, nil
}
