// Package lz4 provides LZ4 block and frame compression/decompression.
package lz4

const (
	prime1 uint32 = 0x9E3779B1
	prime2 uint32 = 0x85EBCA77
	prime3 uint32 = 0xC2B2AE3D
	prime4 uint32 = 0x27D4EB2F
	prime5 uint32 = 0x165667B1
)

func rotl32(x uint32, r uint) uint32 {
	return (x << r) | (x >> (32 - r))
}

func readU32LE(data []byte, offset int) uint32 {
	return uint32(data[offset]) |
		uint32(data[offset+1])<<8 |
		uint32(data[offset+2])<<16 |
		uint32(data[offset+3])<<24
}

func xxh32Round(acc, val uint32) uint32 {
	acc += val * prime2
	acc = rotl32(acc, 13)
	return acc * prime1
}

// XXH32 computes the XXH32 hash of data with the given seed.
func XXH32(data []byte, seed uint32) uint32 {
	length := len(data)
	pos := 0
	var h32 uint32

	if length >= 16 {
		v1 := seed + prime1 + prime2
		v2 := seed + prime2
		v3 := seed
		v4 := seed - prime1

		limit := length - 16
		for pos <= limit {
			v1 = xxh32Round(v1, readU32LE(data, pos))
			pos += 4
			v2 = xxh32Round(v2, readU32LE(data, pos))
			pos += 4
			v3 = xxh32Round(v3, readU32LE(data, pos))
			pos += 4
			v4 = xxh32Round(v4, readU32LE(data, pos))
			pos += 4
		}

		h32 = rotl32(v1, 1) + rotl32(v2, 7) + rotl32(v3, 12) + rotl32(v4, 18)
	} else {
		h32 = seed + prime5
	}

	h32 += uint32(length)

	limit := length - 4
	for pos <= limit {
		h32 += readU32LE(data, pos) * prime3
		h32 = rotl32(h32, 17) * prime4
		pos += 4
	}

	for pos < length {
		h32 += uint32(data[pos]) * prime5
		h32 = rotl32(h32, 11) * prime1
		pos++
	}

	// Avalanche
	h32 ^= h32 >> 15
	h32 *= prime2
	h32 ^= h32 >> 13
	h32 *= prime3
	h32 ^= h32 >> 16

	return h32
}
