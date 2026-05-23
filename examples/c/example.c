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

    return 0;
}
