package lombokcompress_test

import (
	"bytes"
	"compress/flate"
	"compress/gzip"
	"compress/zlib"
	"encoding/hex"
	"encoding/json"
	"io"
	"os"
	"testing"

	"github.com/codinglombok/lombokcompress/go/deflate"
	"github.com/codinglombok/lombokcompress/go/lz4"
	"github.com/codinglombok/lombokcompress/go/zstd"
)

type vectors struct {
	XXH32 []struct {
		InputHex string `json:"input_hex"`
		Seed     uint32 `json:"seed"`
		Expected uint32 `json:"expected"`
	} `json:"xxh32"`
	CRC32 []struct {
		InputHex string `json:"input_hex"`
		Expected uint32 `json:"expected"`
	} `json:"crc32"`
	Adler32 []struct {
		InputHex string `json:"input_hex"`
		Expected uint32 `json:"expected"`
	} `json:"adler32"`
}

func loadVectors(t *testing.T) vectors {
	t.Helper()
	raw, err := os.ReadFile("../test-vectors/compress_vectors.json")
	if err != nil {
		t.Fatal(err)
	}
	var v vectors
	if err := json.Unmarshal(raw, &v); err != nil {
		t.Fatal(err)
	}
	return v
}

func mustHex(t *testing.T, s string) []byte {
	t.Helper()
	b, err := hex.DecodeString(s)
	if err != nil {
		t.Fatal(err)
	}
	return b
}

// xorshift32 keeps the corpus deterministic.
type rng uint32

func (r *rng) next() uint32 {
	x := uint32(*r)
	x ^= x << 13
	x ^= x >> 17
	x ^= x << 5
	*r = rng(x)
	return x
}

func (r *rng) bytes(n int) []byte {
	b := make([]byte, n)
	for i := range b {
		b[i] = byte(r.next())
	}
	return b
}

func samples() [][]byte {
	r := rng(0x12345678)
	out := [][]byte{
		{},
		[]byte("a"),
		bytes.Repeat([]byte("abc"), 1000),
		make([]byte, 100000),
		bytes.Repeat([]byte("Hello, LombokCompress! "), 5000),
	}
	for _, n := range []int{1, 13, 64, 1000, 70000} {
		out = append(out, r.bytes(n))
		low := make([]byte, n)
		for i := range low {
			low[i] = byte(r.next() % 3)
		}
		out = append(out, low)
	}
	return out
}

func TestVectors(t *testing.T) {
	v := loadVectors(t)
	for _, c := range v.XXH32 {
		if got := lz4.XXH32(mustHex(t, c.InputHex), c.Seed); got != c.Expected {
			t.Errorf("XXH32(%s, %d) = %d, want %d", c.InputHex, c.Seed, got, c.Expected)
		}
	}
	for _, c := range v.CRC32 {
		if got := deflate.CRC32(mustHex(t, c.InputHex)); got != c.Expected {
			t.Errorf("CRC32(%s) = %d, want %d", c.InputHex, got, c.Expected)
		}
	}
	for _, c := range v.Adler32 {
		if got := deflate.Adler32(mustHex(t, c.InputHex)); got != c.Expected {
			t.Errorf("Adler32(%s) = %d, want %d", c.InputHex, got, c.Expected)
		}
	}
}

func TestRoundtrip(t *testing.T) {
	for i, data := range samples() {
		blk, err := lz4.DecompressBlock(lz4.CompressBlock(data), len(data))
		if err != nil || !bytes.Equal(blk, data) {
			t.Errorf("sample %d: lz4 block roundtrip failed: %v", i, err)
		}
		opts := lz4.DefaultFrameOptions()
		fr, err := lz4.DecompressFrame(lz4.CompressFrame(data, &opts))
		if err != nil || !bytes.Equal(fr, data) {
			t.Errorf("sample %d: lz4 frame roundtrip failed: %v", i, err)
		}
		zc, err := zstd.Compress(data, 1)
		if err != nil {
			t.Fatal(err)
		}
		zd, err := zstd.Decompress(zc)
		if err != nil || !bytes.Equal(zd, data) {
			t.Errorf("sample %d: zstd roundtrip failed: %v", i, err)
		}
		for name, pair := range map[string][2]func([]byte) ([]byte, error){
			"deflate": {func(b []byte) ([]byte, error) { return deflate.DeflateCompress(b), nil }, deflate.DeflateDecompress},
			"gzip":    {func(b []byte) ([]byte, error) { return deflate.GzipCompress(b), nil }, deflate.GzipDecompress},
			"zlib":    {func(b []byte) ([]byte, error) { return deflate.ZlibCompress(b), nil }, deflate.ZlibDecompress},
		} {
			c, _ := pair[0](data)
			d, err := pair[1](c)
			if err != nil || !bytes.Equal(d, data) {
				t.Errorf("sample %d: %s roundtrip failed: %v", i, name, err)
			}
		}
	}
}

func TestStdlibInterop(t *testing.T) {
	for i, data := range samples() {
		gr, err := gzip.NewReader(bytes.NewReader(deflate.GzipCompress(data)))
		if err != nil {
			t.Fatalf("sample %d: gzip header: %v", i, err)
		}
		if got, err := io.ReadAll(gr); err != nil || !bytes.Equal(got, data) {
			t.Errorf("sample %d: stdlib gunzip failed: %v", i, err)
		}
		zr, err := zlib.NewReader(bytes.NewReader(deflate.ZlibCompress(data)))
		if err != nil {
			t.Fatalf("sample %d: zlib header: %v", i, err)
		}
		if got, err := io.ReadAll(zr); err != nil || !bytes.Equal(got, data) {
			t.Errorf("sample %d: stdlib zlib failed: %v", i, err)
		}
		// Stored blocks written by the standard library.
		var buf bytes.Buffer
		w, _ := flate.NewWriter(&buf, flate.NoCompression)
		w.Write(data)
		w.Close()
		if got, err := deflate.DeflateDecompress(buf.Bytes()); err != nil || !bytes.Equal(got, data) {
			t.Errorf("sample %d: stored block decode failed: %v", i, err)
		}
	}
}

// Decoders must return an error for malformed input, never panic or hang.
func TestMalformedInput(t *testing.T) {
	r := rng(0xdeadbeef)
	seed := bytes.Repeat([]byte("The quick brown fox jumps over the lazy dog. "), 40)
	opts := lz4.DefaultFrameOptions()
	z1, _ := zstd.Compress(seed, 2)
	z2, _ := zstd.Compress(bytes.Repeat([]byte{7}, 5000), 1)
	valid := [][]byte{
		lz4.CompressFrame(seed, &opts),
		lz4.CompressBlock(seed),
		z1, z2,
		deflate.DeflateCompress(seed),
		deflate.GzipCompress(seed),
		deflate.ZlibCompress(seed),
	}
	var cases [][]byte
	for i := 0; i < 300; i++ {
		cases = append(cases, r.bytes(int(r.next()%64)))
	}
	for _, v := range valid {
		for i := 0; i < 200; i++ {
			c := append([]byte(nil), v...)
			flips := 1 + r.next()%4
			for f := uint32(0); f < flips; f++ {
				c[r.next()%uint32(len(c))] ^= 1 << (r.next() % 8)
			}
			if r.next()%4 == 0 {
				c = c[:r.next()%uint32(len(c))]
			}
			cases = append(cases, c)
		}
	}
	// Unterminated gzip FNAME and absurd declared sizes.
	cases = append(cases,
		[]byte{0x1f, 0x8b, 0x08, 0x08, 0, 0, 0, 0, 0, 0xff, 'A', 'A', 'A', 'A', 'A', 'A', 'A', 'A', 'A', 'A'},
		[]byte{0x04, 0x22, 0x4d, 0x18, 0x68, 0x40, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x7f, 0, 0, 0, 0, 0},
		[]byte{0x28, 0xb5, 0x2f, 0xfd, 0xe0, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x7f, 0x01, 0x00, 0x00},
	)

	for i, c := range cases {
		func() {
			defer func() {
				if p := recover(); p != nil {
					t.Fatalf("case %d (%x): panic: %v", i, c, p)
				}
			}()
			lz4.DecompressBlock(c, 1<<16)
			lz4.DecompressFrame(c)
			zstd.Decompress(c)
			deflate.DeflateDecompress(c)
			deflate.GzipDecompress(c)
			deflate.ZlibDecompress(c)
		}()
	}
}

func TestOutputLimits(t *testing.T) {
	big := bytes.Repeat([]byte{'A'}, 200000)
	if _, err := deflate.DeflateDecompressLimit(deflate.DeflateCompress(big), 1000); err == nil {
		t.Error("deflate limit not enforced")
	}
	zc, _ := zstd.Compress(big, 1)
	if _, err := zstd.DecompressLimit(zc, 1000); err == nil {
		t.Error("zstd limit not enforced")
	}
	if got, err := zstd.DecompressLimit(zc, len(big)); err != nil || !bytes.Equal(got, big) {
		t.Errorf("zstd limit rejected valid data: %v", err)
	}
}
