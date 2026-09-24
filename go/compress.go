// Package lombokcompress provides zero-dependency compression: LZ4, Zstd, Deflate.
package lombokcompress

import "fmt"

// CompressErrorCode identifies the category of compression error.
type CompressErrorCode int

const (
	ErrInvalidInput CompressErrorCode = iota
	ErrOutputTooSmall
	ErrChecksumMismatch
	ErrUnsupported
	ErrUnexpectedEof
	ErrInternalError
)

// CompressError is the error type returned by all compression operations.
type CompressError struct {
	Code    CompressErrorCode
	Message string
}

func (e *CompressError) Error() string {
	return fmt.Sprintf("lombokcompress: %s", e.Message)
}

// NewCompressError creates a new CompressError.
func NewCompressError(code CompressErrorCode, message string) *CompressError {
	return &CompressError{Code: code, Message: message}
}
