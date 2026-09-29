// Package deflate provides deflate/gzip/zlib compression and decompression.
package deflate

import (
	"encoding/binary"
)

// CRC32 table
var crc32Table [256]uint32

func init() {
	for n := 0; n < 256; n++ {
		c := uint32(n)
		for k := 0; k < 8; k++ {
			if c&1 != 0 {
				c = 0xEDB88320 ^ (c >> 1)
			} else {
				c >>= 1
			}
		}
		crc32Table[n] = c
	}
}

// CRC32 computes CRC32 checksum.
func CRC32(data []byte) uint32 {
	crc := uint32(0xFFFFFFFF)
	for _, b := range data {
		crc = crc32Table[(crc^uint32(b))&0xFF] ^ (crc >> 8)
	}
	return crc ^ 0xFFFFFFFF
}

// Adler32 computes Adler-32 checksum.
func Adler32(data []byte) uint32 {
	a := uint32(1)
	b := uint32(0)
	for _, byte_ := range data {
		a = (a + uint32(byte_)) % 65521
		b = (b + a) % 65521
	}
	return (b << 16) | a
}

type bitWriter struct {
	buffer   []byte
	bitBuf   uint32
	bitCount uint
}

func newBitWriter() *bitWriter {
	return &bitWriter{buffer: make([]byte, 0)}
}

func (bw *bitWriter) writeBits(value uint32, count uint) {
	bw.bitBuf |= (value & ((1 << count) - 1)) << bw.bitCount
	bw.bitCount += count
	for bw.bitCount >= 8 {
		bw.buffer = append(bw.buffer, byte(bw.bitBuf&0xFF))
		bw.bitBuf >>= 8
		bw.bitCount -= 8
	}
}

func (bw *bitWriter) flush() {
	if bw.bitCount > 0 {
		bw.buffer = append(bw.buffer, byte(bw.bitBuf&0xFF))
		bw.bitBuf = 0
		bw.bitCount = 0
	}
}

func reverseBits(value uint32, count uint) uint32 {
	var result uint32
	for i := uint(0); i < count; i++ {
		result = (result << 1) | (value & 1)
		value >>= 1
	}
	return result
}

func fixedLiteralCode(lit int) (uint32, uint) {
	switch {
	case lit <= 143:
		return reverseBits(uint32(0x30+lit), 8), 8
	case lit <= 255:
		return reverseBits(uint32(0x190+lit-144), 9), 9
	case lit <= 279:
		return reverseBits(uint32(lit-256), 7), 7
	case lit <= 287:
		return reverseBits(uint32(0xC0+lit-280), 8), 8
	default:
		return 0, 0
	}
}

func fixedDistanceCode(dist int) (uint32, uint) {
	return reverseBits(uint32(dist), 5), 5
}

type lengthEntry struct {
	minLen, code, extra int
}

type distEntry struct {
	minDist, code, extra int
}

var lengthTable = []lengthEntry{
	{3, 257, 0}, {4, 258, 0}, {5, 259, 0}, {6, 260, 0},
	{7, 261, 0}, {8, 262, 0}, {9, 263, 0}, {10, 264, 0},
	{11, 265, 1}, {13, 266, 1}, {15, 267, 1}, {17, 268, 1},
	{19, 269, 2}, {23, 270, 2}, {27, 271, 2}, {31, 272, 2},
	{35, 273, 3}, {43, 274, 3}, {51, 275, 3}, {59, 276, 3},
	{67, 277, 4}, {83, 278, 4}, {99, 279, 4}, {115, 280, 4},
	{131, 281, 5}, {163, 282, 5}, {195, 283, 5}, {227, 284, 5},
	{258, 285, 0},
}

var distanceTable = []distEntry{
	{1, 0, 0}, {2, 1, 0}, {3, 2, 0}, {4, 3, 0},
	{5, 4, 1}, {7, 5, 1}, {9, 6, 2}, {13, 7, 2},
	{17, 8, 3}, {25, 9, 3}, {33, 10, 4}, {49, 11, 4},
	{65, 12, 5}, {97, 13, 5}, {129, 14, 6}, {193, 15, 6},
	{257, 16, 7}, {385, 17, 7}, {513, 18, 8}, {769, 19, 8},
	{1025, 20, 9}, {1537, 21, 9}, {2049, 22, 10}, {3073, 23, 10},
	{4097, 24, 11}, {6145, 25, 11}, {8193, 26, 12}, {12289, 27, 12},
	{16385, 28, 13}, {24577, 29, 13},
}

func encodeLength(length int) (int, int, int) {
	for i := len(lengthTable) - 1; i >= 0; i-- {
		if length >= lengthTable[i].minLen {
			return lengthTable[i].code, lengthTable[i].extra, length - lengthTable[i].minLen
		}
	}
	return 0, 0, 0
}

func encodeDistance(distance int) (int, int, int) {
	for i := len(distanceTable) - 1; i >= 0; i-- {
		if distance >= distanceTable[i].minDist {
			return distanceTable[i].code, distanceTable[i].extra, distance - distanceTable[i].minDist
		}
	}
	return 0, 0, 0
}

type lz77Token struct {
	isMatch  bool
	literal  byte
	length   int
	distance int
}

func lz77Compress(data []byte) []lz77Token {
	tokens := make([]lz77Token, 0)
	srcLen := len(data)
	if srcLen == 0 {
		return tokens
	}

	hashTable := make(map[uint32][]int)
	pos := 0

	hash3 := func(p int) uint32 {
		if p+2 >= srcLen {
			return 0
		}
		return uint32(data[p]) | uint32(data[p+1])<<8 | uint32(data[p+2])<<16
	}

	for pos < srcLen {
		if pos+2 >= srcLen {
			tokens = append(tokens, lz77Token{literal: data[pos]})
			pos++
			continue
		}

		h := hash3(pos)
		chain := hashTable[h]

		bestLen := 0
		bestDist := 0
		checks := 0

		for i := len(chain) - 1; i >= 0; i-- {
			ref := chain[i]
			if pos-ref > 32768 {
				break
			}
			checks++
			if checks > 64 {
				break
			}

			ml := 0
			maxML := 258
			if srcLen-pos < maxML {
				maxML = srcLen - pos
			}
			for ml < maxML && data[pos+ml] == data[ref+ml] {
				ml++
			}
			if ml > bestLen {
				bestLen = ml
				bestDist = pos - ref
				if ml >= 258 {
					break
				}
			}
		}

		hashTable[h] = append(hashTable[h], pos)

		if bestLen >= 3 {
			tokens = append(tokens, lz77Token{isMatch: true, length: bestLen, distance: bestDist})
			for i := 1; i < bestLen; i++ {
				if pos+i+2 < srcLen {
					ih := hash3(pos + i)
					hashTable[ih] = append(hashTable[ih], pos+i)
				}
			}
			pos += bestLen
		} else {
			tokens = append(tokens, lz77Token{literal: data[pos]})
			pos++
		}
	}

	return tokens
}

// DeflateCompress compresses data using raw deflate (fixed Huffman).
func DeflateCompress(data []byte) []byte {
	tokens := lz77Compress(data)

	bw := newBitWriter()
	bw.writeBits(1, 1) // BFINAL
	bw.writeBits(1, 2) // BTYPE=01

	for _, token := range tokens {
		if !token.isMatch {
			code, bits := fixedLiteralCode(int(token.literal))
			bw.writeBits(code, bits)
		} else {
			lenCode, lenExtra, lenExtraVal := encodeLength(token.length)
			code, bits := fixedLiteralCode(lenCode)
			bw.writeBits(code, bits)
			if lenExtra > 0 {
				bw.writeBits(uint32(lenExtraVal), uint(lenExtra))
			}

			distCode, distExtra, distExtraVal := encodeDistance(token.distance)
			dcode, dbits := fixedDistanceCode(distCode)
			bw.writeBits(dcode, dbits)
			if distExtra > 0 {
				bw.writeBits(uint32(distExtraVal), uint(distExtra))
			}
		}
	}

	// End of block (256)
	code, bits := fixedLiteralCode(256)
	bw.writeBits(code, bits)
	bw.flush()

	return bw.buffer
}

// GzipCompress compresses data using gzip format.
func GzipCompress(data []byte) []byte {
	output := make([]byte, 0, len(data)+64)

	// Gzip header
	output = append(output, 0x1f, 0x8b) // magic
	output = append(output, 0x08)       // method = deflate
	output = append(output, 0x00)       // flags
	output = append(output, 0, 0, 0, 0) // mtime
	output = append(output, 0x00)       // xfl
	output = append(output, 0xFF)       // OS = unknown

	compressed := DeflateCompress(data)
	output = append(output, compressed...)

	// CRC32 + size (LE32)
	crc := make([]byte, 4)
	binary.LittleEndian.PutUint32(crc, CRC32(data))
	output = append(output, crc...)

	size := make([]byte, 4)
	binary.LittleEndian.PutUint32(size, uint32(len(data)))
	output = append(output, size...)

	return output
}

// ZlibCompress compresses data using zlib format.
func ZlibCompress(data []byte) []byte {
	output := make([]byte, 0, len(data)+64)

	cmf := byte(0x78)
	flg := byte(0x01)
	check := (int(cmf)*256 + int(flg)) % 31
	if check != 0 {
		flg += byte(31 - check)
	}

	output = append(output, cmf, flg)

	compressed := DeflateCompress(data)
	output = append(output, compressed...)

	// Adler-32 (big-endian)
	checksum := make([]byte, 4)
	binary.BigEndian.PutUint32(checksum, Adler32(data))
	output = append(output, checksum...)

	return output
}

// Unexported helper needed by decompress
func init() {
	// ensure tables are initialized
	_ = lengthTable
	_ = distanceTable
}
