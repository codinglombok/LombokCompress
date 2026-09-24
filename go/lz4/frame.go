package lz4

import (
	"encoding/binary"

	lombokcompress "github.com/codinglombok/lombokcompress"
)

const lz4Magic uint32 = 0x184D2204

var maxBlockSizes = map[int]int{
	4: 64 * 1024,
	5: 256 * 1024,
	6: 1024 * 1024,
	7: 4 * 1024 * 1024,
}

// FrameOptions controls LZ4 frame compression behavior.
type FrameOptions struct {
	ContentChecksum bool
	ContentSize     bool
	BlockChecksum   bool
	MaxBlockSize    int // 4-7
}

// DefaultFrameOptions returns default frame options.
func DefaultFrameOptions() FrameOptions {
	return FrameOptions{MaxBlockSize: 7}
}

// CompressFrame compresses data into an LZ4 frame.
func CompressFrame(src []byte, opts *FrameOptions) []byte {
	if opts == nil {
		def := DefaultFrameOptions()
		opts = &def
	}

	output := make([]byte, 0, len(src)+64)

	// Magic number (LE32)
	magic := make([]byte, 4)
	binary.LittleEndian.PutUint32(magic, lz4Magic)
	output = append(output, magic...)

	// FLG byte
	flgByte := byte(0x40) // version = 01
	if opts.ContentChecksum {
		flgByte |= 0x04
	}
	if opts.ContentSize {
		flgByte |= 0x08
	}

	// BD byte
	bdByte := byte((opts.MaxBlockSize & 0x07) << 4)

	headerBytes := []byte{flgByte, bdByte}

	if opts.ContentSize {
		sizeBytes := make([]byte, 8)
		binary.LittleEndian.PutUint64(sizeBytes, uint64(len(src)))
		headerBytes = append(headerBytes, sizeBytes...)
	}

	// Header checksum
	hc := byte((XXH32(headerBytes, 0) >> 8) & 0xFF)

	output = append(output, headerBytes...)
	output = append(output, hc)

	// Blocks
	maxBS := maxBlockSizes[opts.MaxBlockSize]
	pos := 0

	for pos < len(src) {
		chunkSize := len(src) - pos
		if chunkSize > maxBS {
			chunkSize = maxBS
		}
		chunk := src[pos : pos+chunkSize]

		compressed := CompressBlock(chunk)

		if len(compressed) < len(chunk) {
			// Compressed block
			blockLen := make([]byte, 4)
			binary.LittleEndian.PutUint32(blockLen, uint32(len(compressed)))
			output = append(output, blockLen...)
			output = append(output, compressed...)

			if opts.BlockChecksum {
				bc := make([]byte, 4)
				binary.LittleEndian.PutUint32(bc, XXH32(compressed, 0))
				output = append(output, bc...)
			}
		} else {
			// Uncompressed block (high bit set)
			blockLen := make([]byte, 4)
			binary.LittleEndian.PutUint32(blockLen, uint32(len(chunk))|0x80000000)
			output = append(output, blockLen...)
			output = append(output, chunk...)

			if opts.BlockChecksum {
				bc := make([]byte, 4)
				binary.LittleEndian.PutUint32(bc, XXH32(chunk, 0))
				output = append(output, bc...)
			}
		}

		pos += chunkSize
	}

	// EndMark
	output = append(output, 0, 0, 0, 0)

	// Content checksum
	if opts.ContentChecksum {
		cc := make([]byte, 4)
		binary.LittleEndian.PutUint32(cc, XXH32(src, 0))
		output = append(output, cc...)
	}

	return output
}

// DecompressFrame decompresses an LZ4 frame.
func DecompressFrame(src []byte) ([]byte, error) {
	if len(src) < 7 {
		return nil, lombokcompress.NewCompressError(
			lombokcompress.ErrUnexpectedEof, "input too short for LZ4 frame")
	}

	pos := 0

	// Magic
	magic := binary.LittleEndian.Uint32(src[pos:])
	if magic != lz4Magic {
		return nil, lombokcompress.NewCompressError(
			lombokcompress.ErrInvalidInput, "invalid LZ4 frame magic number")
	}
	pos = 4

	flg := src[pos]
	pos += 2 // FLG + BD

	contentChecksum := (flg & 0x04) != 0
	hasContentSize := (flg & 0x08) != 0
	blockChecksum := (flg & 0x10) != 0

	headerStart := 4 // after magic

	var contentSize uint64
	if hasContentSize {
		if pos+8 > len(src) {
			return nil, lombokcompress.NewCompressError(
				lombokcompress.ErrUnexpectedEof, "missing content size")
		}
		contentSize = binary.LittleEndian.Uint64(src[pos:])
		pos += 8
	}

	// Verify header checksum
	headerData := src[headerStart:pos]
	expectedHC := byte((XXH32(headerData, 0) >> 8) & 0xFF)
	if pos >= len(src) {
		return nil, lombokcompress.NewCompressError(
			lombokcompress.ErrUnexpectedEof, "missing header checksum")
	}
	actualHC := src[pos]
	pos++

	if actualHC != expectedHC {
		return nil, lombokcompress.NewCompressError(
			lombokcompress.ErrChecksumMismatch, "LZ4 frame header checksum mismatch")
	}

	output := make([]byte, 0)

	for {
		if pos+4 > len(src) {
			return nil, lombokcompress.NewCompressError(
				lombokcompress.ErrUnexpectedEof, "missing block header")
		}

		blockSize := binary.LittleEndian.Uint32(src[pos:])
		pos += 4

		if blockSize == 0 {
			break // EndMark
		}

		isUncompressed := (blockSize & 0x80000000) != 0
		actualSize := int(blockSize & 0x7FFFFFFF)

		if pos+actualSize > len(src) {
			return nil, lombokcompress.NewCompressError(
				lombokcompress.ErrUnexpectedEof, "block data extends past input")
		}

		blockData := src[pos : pos+actualSize]
		pos += actualSize

		if isUncompressed {
			output = append(output, blockData...)
		} else {
			decompressed, err := decompressBlockAdaptive(blockData)
			if err != nil {
				return nil, err
			}
			output = append(output, decompressed...)
		}

		if blockChecksum {
			if pos+4 > len(src) {
				return nil, lombokcompress.NewCompressError(
					lombokcompress.ErrUnexpectedEof, "missing block checksum")
			}
			expectedBC := binary.LittleEndian.Uint32(src[pos:])
			actualBC := XXH32(blockData, 0)
			if actualBC != expectedBC {
				return nil, lombokcompress.NewCompressError(
					lombokcompress.ErrChecksumMismatch, "LZ4 block checksum mismatch")
			}
			pos += 4
		}
	}

	if contentChecksum {
		if pos+4 > len(src) {
			return nil, lombokcompress.NewCompressError(
				lombokcompress.ErrUnexpectedEof, "missing content checksum")
		}
		expectedCC := binary.LittleEndian.Uint32(src[pos:])
		actualCC := XXH32(output, 0)
		if actualCC != expectedCC {
			return nil, lombokcompress.NewCompressError(
				lombokcompress.ErrChecksumMismatch, "LZ4 content checksum mismatch")
		}
	}

	if hasContentSize && uint64(len(output)) != contentSize {
		return nil, lombokcompress.NewCompressError(
			lombokcompress.ErrInvalidInput, "decompressed size != content size")
	}

	return output, nil
}

func decompressBlockAdaptive(src []byte) ([]byte, error) {
	srcLen := len(src)
	if srcLen == 0 {
		return []byte{}, nil
	}

	output := make([]byte, 0)
	pos := 0

	for pos < srcLen {
		token := src[pos]
		pos++
		litLen := int(token >> 4)

		if litLen == 15 {
			for {
				if pos >= srcLen {
					return nil, lombokcompress.NewCompressError(
						lombokcompress.ErrUnexpectedEof, "unexpected end reading literal length")
				}
				extra := src[pos]
				pos++
				litLen += int(extra)
				if extra != 255 {
					break
				}
			}
		}

		if pos+litLen > srcLen {
			return nil, lombokcompress.NewCompressError(
				lombokcompress.ErrUnexpectedEof, "literal data extends past input")
		}
		output = append(output, src[pos:pos+litLen]...)
		pos += litLen

		if pos >= srcLen {
			break
		}

		if pos+2 > srcLen {
			return nil, lombokcompress.NewCompressError(
				lombokcompress.ErrUnexpectedEof, "missing match offset")
		}
		offset := int(src[pos]) | int(src[pos+1])<<8
		pos += 2

		if offset == 0 {
			return nil, lombokcompress.NewCompressError(
				lombokcompress.ErrInvalidInput, "zero match offset")
		}

		matchLen := int(token&0x0F) + 4
		if token&0x0F == 15 {
			for {
				if pos >= srcLen {
					return nil, lombokcompress.NewCompressError(
						lombokcompress.ErrUnexpectedEof, "unexpected end reading match length")
				}
				extra := src[pos]
				pos++
				matchLen += int(extra)
				if extra != 255 {
					break
				}
			}
		}

		matchStart := len(output) - offset
		if matchStart < 0 {
			return nil, lombokcompress.NewCompressError(
				lombokcompress.ErrInvalidInput, "match offset beyond output")
		}

		for i := 0; i < matchLen; i++ {
			output = append(output, output[matchStart+i])
		}
	}

	return output, nil
}
