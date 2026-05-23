/*
 * Minimal C wrapper exercising the AmberHTML C ABI (Plans.md 6.2).
 *
 * Build (after `cargo build -p amber-core`):
 *   clang examples/c/example.c -I include -L target/debug -lamber_core -o /tmp/amber_c
 *   DYLD_LIBRARY_PATH=target/debug /tmp/amber_c   # macOS
 *   LD_LIBRARY_PATH=target/debug  /tmp/amber_c    # Linux
 *
 * It uses an obviously-invalid URL so it returns quickly without any network or
 * browser — it demonstrates the ABI links and the error contract holds.
 */
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include "amber.h"

int main(void) {
    char *out = NULL;

    int rc = amber_capture_markdown("not a url", &out);
    printf("amber_capture_markdown -> %d, out=%s\n", rc, out ? out : "(null)");
    amber_string_free(out); /* safe even when NULL */

    out = NULL;
    rc = amber_capture_readable("not a url", &out);
    printf("amber_capture_readable -> %d, out=%s\n", rc, out ? out : "(null)");
    amber_string_free(out);

    /* The widened surface: raw bytes for any format (here PDF) ... */
    uint8_t *buf = NULL;
    size_t len = 0;
    rc = amber_capture("not a url", AMBER_FORMAT_PDF, &buf, &len);
    printf("amber_capture(PDF) -> %d, len=%zu\n", rc, len);
    amber_bytes_free(buf, len); /* safe even when NULL */

    /* ... and capture-to-file. */
    out = NULL;
    rc = amber_save("not a url", AMBER_FORMAT_HTML, "/tmp", "amber_c_example", &out);
    printf("amber_save(HTML) -> %d, path=%s\n", rc, out ? out : "(null)");
    amber_string_free(out);

    /* Swap "not a url" for a real URL (e.g. "https://example.com") to drive a
     * capture; the bytes path returns the encoded PDF/PNG/etc., and amber_save
     * writes <dir>/<name>.<ext> and returns its path. */
    return 0;
}
