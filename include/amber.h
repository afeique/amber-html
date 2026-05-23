/*
 * AmberHTML C ABI. See Plans.md (task 6.2).
 *
 * Hand-maintained to match `crates/amber-core/src/capi.rs`; can be regenerated
 * with `cbindgen` (see cbindgen.toml). Link against the `amber_core` cdylib.
 *
 * Strings returned through `out` parameters are heap-allocated by the library
 * and must be released with `amber_string_free`.
 */
#ifndef AMBER_H
#define AMBER_H

#ifdef __cplusplus
extern "C" {
#endif

/* Status codes. */
#define AMBER_OK 0
#define AMBER_ERR_INVALID_INPUT 1
#define AMBER_ERR_CAPTURE 2

/*
 * Capture `url` and write a newly-allocated, NUL-terminated Markdown string to
 * `*out`. Returns AMBER_OK on success; on error returns a non-zero code and
 * sets `*out` to NULL. The caller owns `*out` and must free it with
 * `amber_string_free`.
 */
int amber_capture_markdown(const char *url, char **out);

/* Like amber_capture_markdown but produces readable plain text. */
int amber_capture_readable(const char *url, char **out);

/* Free a string returned by amber_capture_*. NULL is ignored. */
void amber_string_free(char *s);

#ifdef __cplusplus
} /* extern "C" */
#endif

#endif /* AMBER_H */
