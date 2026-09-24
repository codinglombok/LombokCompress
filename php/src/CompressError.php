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
    public function __construct(
        public readonly CompressErrorCode $code,
        string $message,
    ) {
        parent::__construct($message);
    }
}
