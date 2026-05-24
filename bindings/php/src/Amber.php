<?php

declare(strict_types=1);

namespace Amber;

/** Thrown when a capture fails or an argument is rejected. */
final class CaptureException extends \RuntimeException {}

/** Output-format selectors (mirror the C ABI AMBER_FORMAT_*). */
final class Format
{
    public const HTML = 0;
    public const MHTML = 1;
    public const MARKDOWN = 2;
    public const READABLE = 3;
    public const WARC = 4;
    public const WACZ = 5;
    public const SCREENSHOT = 6;
    public const PDF = 7;
}

/**
 * Local-first web-page capture via the amber-core C ABI (PHP FFI). See
 * Plans.md (task 11.1). Run generate.sh first to stage the native library
 * (lib/libamber_core.{dylib,so}); set AMBER_LIB to point elsewhere.
 *
 *   use Amber\Amber;
 *   use Amber\Format;
 *   $md  = Amber::captureMarkdown("https://example.com");
 *   $pdf = Amber::capture("https://example.com", Format::PDF);
 *   $snap = Amber::snapshot("https://example.com", [Format::MARKDOWN, Format::PDF]);
 *   $snap->save(Format::HTML, "out", "page");
 */
final class Amber
{
    private const OK = 0;
    private const ERR_INVALID_INPUT = 1;

    private const CDEF = <<<'C'
        int amber_capture_markdown(const char *url, char **out);
        int amber_capture_readable(const char *url, char **out);
        int amber_capture(const char *url, int format, uint8_t **out, size_t *out_len);
        int amber_save(const char *url, int format, const char *dir, const char *name, char **out_path);
        typedef struct AmberSnapshot AmberSnapshot;
        int amber_snapshot(const char *url, const int *formats, size_t n_formats, AmberSnapshot **out);
        int amber_snapshot_render(const AmberSnapshot *snap, int format, uint8_t **out, size_t *out_len);
        int amber_snapshot_text(const AmberSnapshot *snap, int format, char **out);
        int amber_snapshot_save(const AmberSnapshot *snap, int format, const char *dir, const char *name, char **out_path);
        void amber_snapshot_free(AmberSnapshot *snap);
        void amber_string_free(char *s);
        void amber_bytes_free(uint8_t *ptr, size_t len);
        C;

    private static ?\FFI $ffi = null;

    /** @internal Shared FFI handle (also used by Snapshot). */
    public static function lib(): \FFI
    {
        if (self::$ffi === null) {
            $ext = \PHP_OS_FAMILY === 'Darwin' ? 'dylib' : 'so';
            $lib = getenv('AMBER_LIB') ?: \dirname(__DIR__) . "/lib/libamber_core.$ext";
            self::$ffi = \FFI::cdef(self::CDEF, $lib);
        }
        return self::$ffi;
    }

    /** @internal Map a non-zero status code to an exception. */
    public static function fail(int $rc): CaptureException
    {
        return new CaptureException($rc === self::ERR_INVALID_INPUT ? 'invalid input' : 'capture failed');
    }

    /** Capture $url and return its clean Markdown. */
    public static function captureMarkdown(string $url): string
    {
        $ffi = self::lib();
        $out = $ffi->new('char*');
        $rc = $ffi->amber_capture_markdown($url, \FFI::addr($out));
        if ($rc !== self::OK) {
            throw self::fail($rc);
        }
        try {
            return \FFI::string($out);
        } finally {
            $ffi->amber_string_free($out);
        }
    }

    /** Capture $url and return its readable plain text. */
    public static function captureReadable(string $url): string
    {
        $ffi = self::lib();
        $out = $ffi->new('char*');
        $rc = $ffi->amber_capture_readable($url, \FFI::addr($out));
        if ($rc !== self::OK) {
            throw self::fail($rc);
        }
        try {
            return \FFI::string($out);
        } finally {
            $ffi->amber_string_free($out);
        }
    }

    /**
     * Capture $url as $format and return the encoded bytes (a binary string).
     * Works for every format, including binary ones (Screenshot/PDF/MHTML/…).
     */
    public static function capture(string $url, int $format): string
    {
        $ffi = self::lib();
        $out = $ffi->new('uint8_t*');
        $len = $ffi->new('size_t');
        $rc = $ffi->amber_capture($url, $format, \FFI::addr($out), \FFI::addr($len));
        if ($rc !== self::OK) {
            throw self::fail($rc);
        }
        try {
            return \FFI::string($out, $len->cdata);
        } finally {
            $ffi->amber_bytes_free($out, $len->cdata);
        }
    }

    /**
     * Capture $url as $format, write it into $dir, and return the written path.
     * $name is the file stem (extension follows the format); null uses a default.
     */
    public static function save(string $url, int $format, string $dir, ?string $name = null): string
    {
        $ffi = self::lib();
        $out = $ffi->new('char*');
        $rc = $ffi->amber_save($url, $format, $dir, $name, \FFI::addr($out));
        if ($rc !== self::OK) {
            throw self::fail($rc);
        }
        try {
            return \FFI::string($out);
        } finally {
            $ffi->amber_string_free($out);
        }
    }

    /**
     * Capture $url once for the given $formats, returning a reusable Snapshot —
     * capture once, emit many. $formats must be non-empty.
     *
     * @param int[] $formats
     */
    public static function snapshot(string $url, array $formats): Snapshot
    {
        return Snapshot::capture($url, $formats);
    }
}

/**
 * A captured page, reusable across many output formats (Plans.md 10.1/11.1).
 * One capture serves every format with no re-fetch and no re-render. The native
 * handle is freed on close() or destruction.
 */
final class Snapshot
{
    private \FFI $ffi;
    /** @var \FFI\CData|null AmberSnapshot* */
    private $ptr;

    private function __construct(\FFI $ffi, $ptr)
    {
        $this->ffi = $ffi;
        $this->ptr = $ptr;
    }

    /**
     * Capture $url once for $formats.
     *
     * @param int[] $formats
     */
    public static function capture(string $url, array $formats): self
    {
        $ffi = Amber::lib();
        $n = \count($formats);
        $arr = $n > 0 ? $ffi->new("int[$n]") : null;
        $i = 0;
        foreach ($formats as $f) {
            $arr[$i++] = (int) $f;
        }
        $out = $ffi->new('AmberSnapshot*');
        $rc = $ffi->amber_snapshot($url, $arr, $n, \FFI::addr($out));
        if ($rc !== 0) {
            throw Amber::fail($rc);
        }
        return new self($ffi, $out);
    }

    /** Render $format from the captured page as encoded bytes. */
    public function render(int $format): string
    {
        $out = $this->ffi->new('uint8_t*');
        $len = $this->ffi->new('size_t');
        $rc = $this->ffi->amber_snapshot_render($this->ptr, $format, \FFI::addr($out), \FFI::addr($len));
        if ($rc !== 0) {
            throw Amber::fail($rc);
        }
        try {
            return \FFI::string($out, $len->cdata);
        } finally {
            $this->ffi->amber_bytes_free($out, $len->cdata);
        }
    }

    /** Render $format from the captured page as UTF-8 text. */
    public function text(int $format): string
    {
        $out = $this->ffi->new('char*');
        $rc = $this->ffi->amber_snapshot_text($this->ptr, $format, \FFI::addr($out));
        if ($rc !== 0) {
            throw Amber::fail($rc);
        }
        try {
            return \FFI::string($out);
        } finally {
            $this->ffi->amber_string_free($out);
        }
    }

    /** Write $format into $dir; returns the written path. */
    public function save(int $format, string $dir, ?string $name = null): string
    {
        $out = $this->ffi->new('char*');
        $rc = $this->ffi->amber_snapshot_save($this->ptr, $format, $dir, $name, \FFI::addr($out));
        if ($rc !== 0) {
            throw Amber::fail($rc);
        }
        try {
            return \FFI::string($out);
        } finally {
            $this->ffi->amber_string_free($out);
        }
    }

    /** The captured page's clean Markdown. */
    public function markdown(): string
    {
        return $this->text(Format::MARKDOWN);
    }

    /** The captured page's readable plain text. */
    public function readable(): string
    {
        return $this->text(Format::READABLE);
    }

    /** Release the native handle (idempotent). */
    public function close(): void
    {
        if ($this->ptr !== null) {
            $this->ffi->amber_snapshot_free($this->ptr);
            $this->ptr = null;
        }
    }

    public function __destruct()
    {
        $this->close();
    }
}
