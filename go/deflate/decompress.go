package deflate

import (
	"encoding/binary"
	"math"

	lombokcompress "github.com/codinglombok/lombokcompress/go"
)

var lengthBase = [29]int{
	3, 4, 5, 6, 7, 8, 9, 10, 11, 13,
	15, 17, 19, 23, 27, 31, 35, 43, 51, 59,
	67, 83, 99, 115, 131, 163, 195, 227, 258,
}

var lengthExtraBits = [29]int{
	0, 0, 0, 0, 0, 0, 0, 0, 1, 1,
	1, 1, 2, 2, 2, 2, 3, 3, 3, 3,
	4, 4, 4, 4, 5, 5, 5, 5, 0,
}

var distBase = [30]int{
	1, 2, 3, 4, 5, 7, 9, 13, 17, 25,
	33, 49, 65, 97, 129, 193, 257, 385, 513, 769,
	1025, 1537, 2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
}

var distExtraBits = [30]int{
	0, 0, 0, 0, 1, 1, 2, 2, 3, 3,
	4, 4, 5, 5, 6, 6, 7, 7, 8, 8,
	9, 9, 10, 10, 11, 11, 12, 12, 13, 13,
}

type bitReader struct {
	data     []byte
	pos      int
	bitBuf   uint32
	bitCount uint
}

func newBitReader(data []byte) *bitReader {
	return &bitReader{data: data}
}

func (br *bitReader) readBits(count uint) (uint32, error) {
	for br.bitCount < count {
		if br.pos >= len(br.data) {
			return 0, lombokcompress.NewCompressError(
				lombokcompress.ErrUnexpectedEof, "unexpected end of deflate data")
		}
		br.bitBuf |= uint32(br.data[br.pos]) << br.bitCount
		br.pos++
		br.bitCount += 8
	}
	value := br.bitBuf & ((1 << count) - 1)
	br.bitBuf >>= count
	br.bitCount -= count
	return value, nil
}

func decodeFixedLiteral(br *bitReader) (int, error) {
	// Read 7 bits
	var code uint32
	for i := 0; i < 7; i++ {
		bit, err := br.readBits(1)
		if err != nil {
			return 0, err
		}
		code = (code << 1) | bit
	}

	// 7-bit codes: 256-279
	if code <= 23 {
		return 256 + int(code), nil
	}

	// Read 8th bit
	bit, err := br.readBits(1)
	if err != nil {
		return 0, err
	}
	code = (code << 1) | bit

	// 8-bit codes: 0-143
	if code >= 48 && code <= 191 {
		return int(code) - 48, nil
	}

	// 8-bit codes: 280-287
	if code >= 192 && code <= 199 {
		return 280 + int(code) - 192, nil
	}

	// Read 9th bit
	bit, err = br.readBits(1)
	if err != nil {
		return 0, err
	}
	code = (code << 1) | bit

	// 9-bit codes: 144-255
	if code >= 400 && code <= 511 {
		return 144 + int(code) - 400, nil
	}

	return 0, lombokcompress.NewCompressError(
		lombokcompress.ErrInvalidInput, "invalid fixed Huffman code")
}

func decodeFixedDistance(br *bitReader) (int, error) {
	var code uint32
	for i := 0; i < 5; i++ {
		bit, err := br.readBits(1)
		if err != nil {
			return 0, err
		}
		code = (code << 1) | bit
	}
	return int(code), nil
}

// defaultMaxOutput caps gzip/zlib output like the Rust core (64 MiB).
const defaultMaxOutput = 64 * 1024 * 1024

func errTooLarge() error {
	return lombokcompress.NewCompressError(
		lombokcompress.ErrOutputTooSmall, "decompressed data exceeds limit")
}

// DeflateDecompress decompresses raw deflate data.
func DeflateDecompress(data []byte) ([]byte, error) {
	return DeflateDecompressLimit(data, math.MaxInt)
}

// DeflateDecompressLimit decompresses raw deflate data, refusing to produce
// more than maxOutput bytes. Use it for untrusted input to bound memory use.
func DeflateDecompressLimit(data []byte, maxOutput int) ([]byte, error) {
	br := newBitReader(data)
	output := make([]byte, 0)

	for {
		bfinal, err := br.readBits(1)
		if err != nil {
			return nil, err
		}
		btype, err := br.readBits(2)
		if err != nil {
			return nil, err
		}

		switch btype {
		case 0:
			// Stored block — align to byte
			br.bitBuf = 0
			br.bitCount = 0

			if br.pos+4 > len(br.data) {
				return nil, lombokcompress.NewCompressError(
					lombokcompress.ErrUnexpectedEof, "stored block header extends past input")
			}
			length := int(br.data[br.pos]) | int(br.data[br.pos+1])<<8
			nlength := int(br.data[br.pos+2]) | int(br.data[br.pos+3])<<8
			br.pos += 4

			if length != (^nlength & 0xFFFF) {
				return nil, lombokcompress.NewCompressError(
					lombokcompress.ErrInvalidInput, "stored block length check failed")
			}

			if br.pos+length > len(br.data) {
				return nil, lombokcompress.NewCompressError(
					lombokcompress.ErrUnexpectedEof, "stored block data extends past input")
			}
			if length > maxOutput-len(output) {
				return nil, errTooLarge()
			}
			output = append(output, br.data[br.pos:br.pos+length]...)
			br.pos += length

		case 1:
			// Fixed Huffman
			for {
				sym, err := decodeFixedLiteral(br)
				if err != nil {
					return nil, err
				}

				if sym < 256 {
					if len(output) >= maxOutput {
						return nil, errTooLarge()
					}
					output = append(output, byte(sym))
				} else if sym == 256 {
					break
				} else {
					li := sym - 257
					if li >= len(lengthBase) {
						return nil, lombokcompress.NewCompressError(
							lombokcompress.ErrInvalidInput, "invalid length code")
					}
					matchLen := lengthBase[li]
					if lengthExtraBits[li] > 0 {
						extra, err := br.readBits(uint(lengthExtraBits[li]))
						if err != nil {
							return nil, err
						}
						matchLen += int(extra)
					}

					distCode, err := decodeFixedDistance(br)
					if err != nil {
						return nil, err
					}
					if distCode >= len(distBase) {
						return nil, lombokcompress.NewCompressError(
							lombokcompress.ErrInvalidInput, "invalid distance code")
					}
					distance := distBase[distCode]
					if distExtraBits[distCode] > 0 {
						extra, err := br.readBits(uint(distExtraBits[distCode]))
						if err != nil {
							return nil, err
						}
						distance += int(extra)
					}

					start := len(output) - distance
					if start < 0 {
						return nil, lombokcompress.NewCompressError(
							lombokcompress.ErrInvalidInput, "distance beyond output buffer")
					}
					if matchLen > maxOutput-len(output) {
						return nil, errTooLarge()
					}
					for i := 0; i < matchLen; i++ {
						output = append(output, output[start+i])
					}
				}
			}

		case 2:
			return nil, lombokcompress.NewCompressError(
				lombokcompress.ErrUnsupported, "dynamic Huffman not yet implemented")

		default:
			return nil, lombokcompress.NewCompressError(
				lombokcompress.ErrInvalidInput, "reserved deflate block type")
		}

		if bfinal == 1 {
			break
		}
	}

	return output, nil
}

// GzipDecompress decompresses gzip format data (output capped at 64 MiB).
func GzipDecompress(data []byte) ([]byte, error) {
	return GzipDecompressLimit(data, defaultMaxOutput)
}

// GzipDecompressLimit decompresses gzip data with an explicit output limit.
func GzipDecompressLimit(data []byte, maxOutput int) ([]byte, error) {
	if len(data) < 18 {
		return nil, lombokcompress.NewCompressError(
			lombokcompress.ErrUnexpectedEof, "input too short for gzip")
	}

	if data[0] != 0x1F || data[1] != 0x8B {
		return nil, lombokcompress.NewCompressError(
			lombokcompress.ErrInvalidInput, "invalid gzip magic number")
	}

	if data[2] != 0x08 {
		return nil, lombokcompress.NewCompressError(
			lombokcompress.ErrUnsupported, "unsupported gzip compression method")
	}

	flags := data[3]
	pos := 10

	// FEXTRA
	if flags&0x04 != 0 {
		if pos+2 > len(data) {
			return nil, lombokcompress.NewCompressError(
				lombokcompress.ErrUnexpectedEof, "missing FEXTRA")
		}
		xlen := int(data[pos]) | int(data[pos+1])<<8
		pos += 2 + xlen
	}

	// FNAME
	if flags&0x08 != 0 {
		for pos < len(data) && data[pos] != 0 {
			pos++
		}
		pos++
	}

	// FCOMMENT
	if flags&0x10 != 0 {
		for pos < len(data) && data[pos] != 0 {
			pos++
		}
		pos++
	}

	// FHCRC
	if flags&0x02 != 0 {
		pos += 2
	}

	if pos >= len(data)-8 {
		return nil, lombokcompress.NewCompressError(
			lombokcompress.ErrUnexpectedEof, "no compressed data in gzip")
	}

	compressedData := data[pos : len(data)-8]
	decompressed, err := DeflateDecompressLimit(compressedData, maxOutput)
	if err != nil {
		return nil, err
	}

	if len(data) < 8 {
		return nil, lombokcompress.NewCompressError(
			lombokcompress.ErrUnexpectedEof, "missing gzip trailer")
	}

	trailer := data[len(data)-8:]
	expectedCRC := binary.LittleEndian.Uint32(trailer[0:4])
	expectedSize := binary.LittleEndian.Uint32(trailer[4:8])

	actualCRC := CRC32(decompressed)
	if actualCRC != expectedCRC {
		return nil, lombokcompress.NewCompressError(
			lombokcompress.ErrChecksumMismatch, "gzip CRC32 mismatch")
	}

	if uint32(len(decompressed)) != expectedSize {
		return nil, lombokcompress.NewCompressError(
			lombokcompress.ErrInvalidInput, "gzip size mismatch")
	}

	return decompressed, nil
}

// ZlibDecompress decompresses zlib format data (output capped at 64 MiB).
func ZlibDecompress(data []byte) ([]byte, error) {
	return ZlibDecompressLimit(data, defaultMaxOutput)
}

// ZlibDecompressLimit decompresses zlib data with an explicit output limit.
func ZlibDecompressLimit(data []byte, maxOutput int) ([]byte, error) {
	if len(data) < 6 {
		return nil, lombokcompress.NewCompressError(
			lombokcompress.ErrUnexpectedEof, "input too short for zlib")
	}

	cmf := data[0]
	flg := data[1]

	if (int(cmf)*256+int(flg))%31 != 0 {
		return nil, lombokcompress.NewCompressError(
			lombokcompress.ErrInvalidInput, "invalid zlib header checksum")
	}

	if cmf&0x0F != 8 {
		return nil, lombokcompress.NewCompressError(
			lombokcompress.ErrUnsupported, "unsupported zlib compression method")
	}

	hasDict := (flg & 0x20) != 0
	if hasDict {
		return nil, lombokcompress.NewCompressError(
			lombokcompress.ErrUnsupported, "zlib preset dictionary")
	}

	compressedData := data[2 : len(data)-4]
	decompressed, err := DeflateDecompressLimit(compressedData, maxOutput)
	if err != nil {
		return nil, err
	}

	expectedAdler := binary.BigEndian.Uint32(data[len(data)-4:])
	actualAdler := Adler32(decompressed)
	if actualAdler != expectedAdler {
		return nil, lombokcompress.NewCompressError(
			lombokcompress.ErrChecksumMismatch, "zlib Adler-32 mismatch")
	}

	return decompressed, nil
}
