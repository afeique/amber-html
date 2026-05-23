/*
 * AmberHTML C ABI. See Plans.md (task 6.2).
 *
 * Hand-maintained to match `crates/amber-core/src/capi.rs`; can be regenerated
 * with `cbindgen` (see cbindgen.toml). Link against the `amber_core` cdylib.
 *
 * Ownership: strings returned through `out`/`out_path` are heap-allocated by the
 * library and must be freed with `amber_string_free`; byte buffers from
 * `amber_capture` with `amber_bytes_free`.
 */
#ifndef AMBER_H
#define AMBER_H

#include <stddef.h> /* size_t */
#include <stdint.h> /* uint8_t */

#ifdef __cplusplus
extern "C" {
#endif

/* Status codes. */
#define AMBER_OK 0
#define AMBER_ERR_INVALID_INPUT 1
#define AMBER_ERR_CAPTURE 2

/* Output format selectors (match amber_core::OutputFormat order). */
#define AMBER_FORMAT_HTML 0
#define AMBER_FORMAT_MHTML 1
#define AMBER_FORMAT_MARKDOWN 2
#define AMBER_FORMAT_READABLE 3
#define AMBER_FORMAT_WARC 4
#define AMBER_FORMAT_WACZ 5
#define AMBER_FORMAT_SCREENSHOT 6
#define AMBER_FORMAT_PDF 7

/*
 * Capture `url` and write a newly-allocated, NUL-terminated Markdown string to
 * `*out`. Returns AMBER_OK on success; on error returns a non-zero code and
 * sets `*out` to NULL. The caller owns `*out` and must free it with
 * `amber_string_free`.
 */
int amber_capture_markdown(const char *url, char **out);

/* Like amber_capture_markdown but produces readable plain text. */
int amber_capture_readable(const char *url, char **out);

/*
 * Capture `url` as `format` (an AMBER_FORMAT_* value) into a newly-allocated
 * byte buffer written to `*out`, with its length in `*out_len`. Works for every
 * format, including binary ones (screenshot/PDF/MHTML/WARC/WACZ). On error
 * returns a non-zero code, sets `*out` to NULL and `*out_len` to 0. The caller
 * owns the buffer and must free it with `amber_bytes_free`.
 */
int amber_capture(const char *url, int format, uint8_t **out, size_t *out_len);

/*
 * Capture `url` as `format`, write it into `dir`, and return the written path
 * (NUL-terminated) through `*out_path`. `name` is the file stem, or NULL for a
 * default name; `dir` is created if missing. The caller owns `*out_path` and
 * must free it with `amber_string_free`.
 */
int amber_save(const char *url, int format, const char *dir, const char *name,
               char **out_path);

/* Free a string returned by amber_capture_* or amber_save. NULL is ignored. */
void amber_string_free(char *s);

/* Free a byte buffer returned by amber_capture. NULL is ignored; `len` must be
 * the length reported alongside the buffer. */
void amber_bytes_free(uint8_t *ptr, size_t len);

#ifdef __cplusplus
} /* extern "C" */
#endif

#endif /* AMBER_H */
