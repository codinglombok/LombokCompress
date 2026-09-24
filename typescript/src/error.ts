/**
 * Error codes for LombokCompress operations.
 */
export enum CompressErrorCode {
  InvalidInput = 'INVALID_INPUT',
  OutputTooSmall = 'OUTPUT_TOO_SMALL',
  ChecksumMismatch = 'CHECKSUM_MISMATCH',
  Unsupported = 'UNSUPPORTED',
  UnexpectedEof = 'UNEXPECTED_EOF',
  InternalError = 'INTERNAL_ERROR',
}

/**
 * Error class for compression/decompression failures.
 */
export class CompressError extends Error {
  readonly code: CompressErrorCode;

  constructor(code: CompressErrorCode, message: string) {
    super(message);
    this.name = 'CompressError';
    this.code = code;
  }
}
