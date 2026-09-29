<?php

declare(strict_types=1);

namespace LombokCompress;

enum CompressErrorCode: string
{
    case InvalidInput = 'invalid_input';
    case OutputTooSmall = 'output_too_small';
    case ChecksumMismatch = 'checksum_mismatch';
    case Unsupported = 'unsupported';
    case UnexpectedEof = 'unexpected_eof';
    case InternalError = 'internal_error';
}

class CompressError extends \RuntimeException
{
    /**
     * `$code` is already declared by \Exception (int, non-readonly), so the
     * typed error code lives in its own property.
     */
    public function __construct(
        public readonly CompressErrorCode $errorCode,
        string $message,
    ) {
        parent::__construct($message);
    }
}
