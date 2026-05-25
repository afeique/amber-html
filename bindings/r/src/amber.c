/*
 * R C shim for AmberHTML over the amber-core C ABI (Plans.md 11.4).
 *
 * R has no direct FFI, so this adapts the C ABI to R's .Call/SEXP interface.
 * generate.sh stages amber.h + libamber_core into src/ for the build.
 */
#include <R.h>
#include <Rinternals.h>
#include <R_ext/Rdynload.h>
#include <string.h>

#include "amber.h"

static void amber_check(int rc) {
    if (rc == AMBER_OK) return;
    if (rc == AMBER_ERR_INVALID_INPUT) Rf_error("amber: invalid input");
    Rf_error("amber: capture failed");
}

/* markdown == TRUE -> markdown, else readable. */
SEXP r_amber_capture_text(SEXP url, SEXP markdown) {
    const char *u = CHAR(STRING_ELT(url, 0));
    char *out = NULL;
    int rc = Rf_asLogical(markdown) ? amber_capture_markdown(u, &out)
                                    : amber_capture_readable(u, &out);
    amber_check(rc);
    SEXP res = PROTECT(Rf_mkString(out));
    amber_string_free(out);
    UNPROTECT(1);
    return res;
}

SEXP r_amber_capture(SEXP url, SEXP format) {
    const char *u = CHAR(STRING_ELT(url, 0));
    uint8_t *out = NULL;
    size_t len = 0;
    amber_check(amber_capture(u, Rf_asInteger(format), &out, &len));
    SEXP res = PROTECT(Rf_allocVector(RAWSXP, (R_xlen_t) len));
    if (len) memcpy(RAW(res), out, len);
    amber_bytes_free(out, len);
    UNPROTECT(1);
    return res;
}

SEXP r_amber_save(SEXP url, SEXP format, SEXP dir, SEXP name) {
    const char *u = CHAR(STRING_ELT(url, 0));
    const char *d = CHAR(STRING_ELT(dir, 0));
    const char *n = (name == R_NilValue) ? NULL : CHAR(STRING_ELT(name, 0));
    char *out = NULL;
    amber_check(amber_save(u, Rf_asInteger(format), d, n, &out));
    SEXP res = PROTECT(Rf_mkString(out));
    amber_string_free(out);
    UNPROTECT(1);
    return res;
}

static void snapshot_finalizer(SEXP ext) {
    AmberSnapshot *p = (AmberSnapshot *) R_ExternalPtrAddr(ext);
    if (p) {
        amber_snapshot_free(p);
        R_ClearExternalPtr(ext);
    }
}

SEXP r_amber_snapshot(SEXP url, SEXP formats) {
    const char *u = CHAR(STRING_ELT(url, 0));
    int n = LENGTH(formats);
    int *fmts = INTEGER(formats);
    AmberSnapshot *snap = NULL;
    amber_check(amber_snapshot(u, fmts, (size_t) n, &snap));
    SEXP ext = PROTECT(R_MakeExternalPtr(snap, R_NilValue, R_NilValue));
    R_RegisterCFinalizerEx(ext, snapshot_finalizer, TRUE);
    UNPROTECT(1);
    return ext;
}

static AmberSnapshot *snapshot_ptr(SEXP ext) {
    AmberSnapshot *p = (AmberSnapshot *) R_ExternalPtrAddr(ext);
    if (!p) Rf_error("amber: snapshot is closed");
    return p;
}

SEXP r_amber_snapshot_render(SEXP ext, SEXP format) {
    AmberSnapshot *p = snapshot_ptr(ext);
    uint8_t *out = NULL;
    size_t len = 0;
    amber_check(amber_snapshot_render(p, Rf_asInteger(format), &out, &len));
    SEXP res = PROTECT(Rf_allocVector(RAWSXP, (R_xlen_t) len));
    if (len) memcpy(RAW(res), out, len);
    amber_bytes_free(out, len);
    UNPROTECT(1);
    return res;
}

SEXP r_amber_snapshot_text(SEXP ext, SEXP format) {
    AmberSnapshot *p = snapshot_ptr(ext);
    char *out = NULL;
    amber_check(amber_snapshot_text(p, Rf_asInteger(format), &out));
    SEXP res = PROTECT(Rf_mkString(out));
    amber_string_free(out);
    UNPROTECT(1);
    return res;
}

SEXP r_amber_snapshot_save(SEXP ext, SEXP format, SEXP dir, SEXP name) {
    AmberSnapshot *p = snapshot_ptr(ext);
    const char *d = CHAR(STRING_ELT(dir, 0));
    const char *n = (name == R_NilValue) ? NULL : CHAR(STRING_ELT(name, 0));
    char *out = NULL;
    amber_check(amber_snapshot_save(p, Rf_asInteger(format), d, n, &out));
    SEXP res = PROTECT(Rf_mkString(out));
    amber_string_free(out);
    UNPROTECT(1);
    return res;
}

SEXP r_amber_snapshot_close(SEXP ext) {
    AmberSnapshot *p = (AmberSnapshot *) R_ExternalPtrAddr(ext);
    if (p) {
        amber_snapshot_free(p);
        R_ClearExternalPtr(ext);
    }
    return R_NilValue;
}

static const R_CallMethodDef CallEntries[] = {
    {"r_amber_capture_text", (DL_FUNC) &r_amber_capture_text, 2},
    {"r_amber_capture", (DL_FUNC) &r_amber_capture, 2},
    {"r_amber_save", (DL_FUNC) &r_amber_save, 4},
    {"r_amber_snapshot", (DL_FUNC) &r_amber_snapshot, 2},
    {"r_amber_snapshot_render", (DL_FUNC) &r_amber_snapshot_render, 2},
    {"r_amber_snapshot_text", (DL_FUNC) &r_amber_snapshot_text, 2},
    {"r_amber_snapshot_save", (DL_FUNC) &r_amber_snapshot_save, 4},
    {"r_amber_snapshot_close", (DL_FUNC) &r_amber_snapshot_close, 1},
    {NULL, NULL, 0}
};

void R_init_amber(DllInfo *dll) {
    R_registerRoutines(dll, NULL, CallEntries, NULL, NULL);
    R_useDynamicSymbols(dll, FALSE);
}
