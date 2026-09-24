package lz4

import (
	lombokcompress "github.com/codinglombok/lombokcompress"
)

const (
	hashLog  = 12
	hashSize = 1 << hashLog
	minMatch = 4
	mfLimit  = 12
)

// CompressBound returns the maximum compressed size for a given input size.
func CompressBound(inputSize int) int {
	return inputSize + inputSize/255 + 16
}

func hash4(v uint32) uint32 {
	return (v * 0x9E3779B1) >> (32 - hashLog)
}

// CompressBlock compresses data using LZ4 block format.
func CompressBlock(src []byte) []byte {
	srcLen := len(src)
	if srcLen == 0 {
		return []byte{}
	}

	output := make([]byte, 0, CompressBound(srcLen))
	hashTable := make([]int, hashSize)
	// Initialize to -1 (no match)
	for i := range hashTable {
		hashTable[i] = -1
	}

	pos := 0
	anchor := 0
	limit := srcLen - mfLimit

	for pos < limit {
		curVal := readU32LE(src, pos)
		h := hash4(curVal)
		ref := hashTable[h]
		hashTable[h] = pos

		if ref < anchor || pos-ref > 65535 || readU32LE(src, ref) != curVal {
			pos++
			continue
		}

		// Emit literals
		litLen := pos - anchor

		// Extend match forward
		matchPos := pos + minMatch
		refPos := ref + minMatch
		for matchPos < srcLen && src[matchPos] == src[refPos] {
			matchPos++
			refPos++
		}
		matchLen := matchPos - pos - minMatch

		// Token
		tokenLit := litLen
		if tokenLit > 15 {
			tokenLit = 15
		}
		tokenMatch := matchLen
		if tokenMatch > 15 {
			tokenMatch = 15
		}
		output = append(output, byte((tokenLit<<4)|tokenMatch))

		// Extra literal length
		if litLen >= 15 {
			remaining := litLen - 15
			for remaining >= 255 {
				output = append(output, 255)
				remaining -= 255
			}
			output = append(output, byte(remaining))
		}

		// Literals
		output = append(output, src[anchor:anchor+litLen]...)

		// Offset (LE16)
		offset := pos - ref
		output = append(output, byte(offset&0xFF), byte((offset>>8)&0xFF))

		// Extra match length
		if matchLen >= 15 {
			remaining := matchLen - 15
			for remaining >= 255 {
				output = append(output, 255)
				remaining -= 255
			}
			output = append(output, byte(remaining))
		}

		pos = matchPos
		anchor = pos
	}

	// Last literals
	litLen := srcLen - anchor
	tokenLit := litLen
	if tokenLit > 15 {
		tokenLit = 15
	}
	output = append(output, byte(tokenLit<<4))

	if litLen >= 15 {
		remaining := litLen - 15
		for remaining >= 255 {
			output = append(output, 255)
			remaining -= 255
		}
		output = append(output, byte(remaining))
	}

	output = append(output, src[anchor:]...)

	return output
}

// DecompressBlock decompresses LZ4 block format data.
func DecompressBlock(src []byte, uncompressedSize int) ([]byte, error) {
	srcLen := len(src)
	if srcLen == 0 && uncompressedSize == 0 {
		return []byte{}, nil
	}

	output := make([]byte, 0, uncompressedSize)
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

		matchLen := int(token&0x0F) + minMatch
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

	if len(output) != uncompressedSize {
		return nil, lombokcompress.NewCompressError(
			lombokcompress.ErrInvalidInput, "decompressed size mismatch")
	}

	return output, nil
}
