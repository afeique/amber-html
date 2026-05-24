//! C ABI facade for the long tail of languages. See `Plans.md` (task 6.2).
//!
//! AmberHTML's full Rust API doesn't cross a C boundary cleanly (generics,
//! `Result`), so this module exposes an idiomatic C surface: capture any
//! [`OutputFormat`] to text, raw bytes, or a file. The crate builds as a
//! `cdylib`; pair these symbols with the C header in `include/amber.h`.
//!
//! Ownership: strings handed back through `out`/`out_path` are heap-allocated
//! and must be released with [`amber_string_free`]; byte buffers from
//! [`amber_capture`] with [`amber_bytes_free`].

use std::ffi::{c_char, c_int, CStr, CString};
use std::path::Path;

use crate::{snapshot, CaptureOptions, OutputFormat};

/// Success.
pub const AMBER_OK: c_int = 0;
/// A null pointer, non-UTF-8 input, or unknown format was supplied.
pub const AMBER_ERR_INVALID_INPUT: c_int = 1;
/// The capture itself failed (bad URL, network/browser error, …).
pub const AMBER_ERR_CAPTURE: c_int = 2;

// Format selectors — these match `OutputFormat::ALL`'s order and the
// `AMBER_FORMAT_*` macros in `include/amber.h`.
/// Single-file inlined HTML.
pub const AMBER_FORMAT_HTML: c_int = 0;
/// MHTML bundle.
pub const AMBER_FORMAT_MHTML: c_int = 1;
/// Clean Markdown.
pub const AMBER_FORMAT_MARKDOWN: c_int = 2;
/// Readable plain text.
pub const AMBER_FORMAT_READABLE: c_int = 3;
/// WARC archive.
pub const AMBER_FORMAT_WARC: c_int = 4;
/// WACZ archive.
pub const AMBER_FORMAT_WACZ: c_int = 5;
/// Full-page PNG screenshot.
pub const AMBER_FORMAT_SCREENSHOT: c_int = 6;
/// PDF.
pub const AMBER_FORMAT_PDF: c_int = 7;

/// Map a C format selector to an [`OutputFormat`], or `None` if unknown.
fn format_from_int(f: c_int) -> Option<OutputFormat> {
    Some(match f {
        AMBER_FORMAT_HTML => OutputFormat::Html,
        AMBER_FORMAT_MHTML => OutputFormat::Mhtml,
        AMBER_FORMAT_MARKDOWN => OutputFormat::Markdown,
        AMBER_FORMAT_READABLE => OutputFormat::Readable,
        AMBER_FORMAT_WARC => OutputFormat::Warc,
        AMBER_FORMAT_WACZ => OutputFormat::Wacz,
        AMBER_FORMAT_SCREENSHOT => OutputFormat::Screenshot,
        AMBER_FORMAT_PDF => OutputFormat::Pdf,
        _ => return None,
    })
}

/// Capture `url` and write a newly-allocated, NUL-terminated Markdown C string
/// to `*out`. Returns [`AMBER_OK`] on success; on error returns a non-zero code
/// and sets `*out` to null. The caller owns `*out` and must release it with
/// [`amber_string_free`].
///
/// # Safety
/// `url` must be a valid NUL-terminated C string and `out` a valid, writable
/// pointer to a `char *`.
#[no_mangle]
pub unsafe extern "C" fn amber_capture_markdown(
    url: *const c_char,
    out: *mut *mut c_char,
) -> c_int {
    capture_to_c(url, out, OutputFormat::Markdown)
}

/// Like [`amber_capture_markdown`] but produces readable plain text.
///
/// # Safety
/// Same contract as [`amber_capture_markdown`].
#[no_mangle]
pub unsafe extern "C" fn amber_capture_readable(
    url: *const c_char,
    out: *mut *mut c_char,
) -> c_int {
    capture_to_c(url, out, OutputFormat::Readable)
}

/// Capture `url` as `format` and write a newly-allocated byte buffer to `*out`
/// (length in `*out_len`). Works for every format, including the binary ones
/// (Screenshot/Pdf/Mhtml/Warc/Wacz). Returns [`AMBER_OK`] on success; on error
/// returns a non-zero code, sets `*out` to null and `*out_len` to 0. The caller
/// owns the buffer and must release it with [`amber_bytes_free`].
///
/// # Safety
/// `url` must be a valid NUL-terminated C string; `out`/`out_len` must be valid,
/// writable pointers.
#[no_mangle]
pub unsafe extern "C" fn amber_capture(
    url: *const c_char,
    format: c_int,
    out: *mut *mut u8,
    out_len: *mut usize,
) -> c_int {
    if url.is_null() || out.is_null() || out_len.is_null() {
        return AMBER_ERR_INVALID_INPUT;
    }
    *out = std::ptr::null_mut();
    *out_len = 0;

    let Some(format) = format_from_int(format) else {
        return AMBER_ERR_INVALID_INPUT;
    };
    let Ok(url) = CStr::from_ptr(url).to_str() else {
        return AMBER_ERR_INVALID_INPUT;
    };
    let bytes = match capture_bytes(url, format) {
        Ok(bytes) => bytes,
        Err(_) => return AMBER_ERR_CAPTURE,
    };
    // A boxed slice has capacity == length, so the (ptr, len) round-trips
    // exactly through `amber_bytes_free`.
    let boxed = bytes.into_boxed_slice();
    *out_len = boxed.len();
    *out = Box::into_raw(boxed) as *mut u8;
    AMBER_OK
}

/// Capture `url` as `format`, write it into `dir`, and return the written path
/// (a NUL-terminated C string) through `*out_path`. `name` is the file stem (the
/// extension is chosen by the format) or null for a default `<safe-url> <date>
/// <time>` name. `dir` is created if missing. The caller owns `*out_path` and
/// must release it with [`amber_string_free`].
///
/// # Safety
/// `url`/`dir` must be valid NUL-terminated C strings; `name` such a string or
/// null; `out_path` a valid, writable pointer to a `char *`.
#[no_mangle]
pub unsafe extern "C" fn amber_save(
    url: *const c_char,
    format: c_int,
    dir: *const c_char,
    name: *const c_char,
    out_path: *mut *mut c_char,
) -> c_int {
    if url.is_null() || dir.is_null() || out_path.is_null() {
        return AMBER_ERR_INVALID_INPUT;
    }
    *out_path = std::ptr::null_mut();

    let Some(format) = format_from_int(format) else {
        return AMBER_ERR_INVALID_INPUT;
    };
    let Ok(url) = CStr::from_ptr(url).to_str() else {
        return AMBER_ERR_INVALID_INPUT;
    };
    let Ok(dir) = CStr::from_ptr(dir).to_str() else {
        return AMBER_ERR_INVALID_INPUT;
    };
    let name = if name.is_null() {
        None
    } else {
        match CStr::from_ptr(name).to_str() {
            Ok(name) => Some(name),
            Err(_) => return AMBER_ERR_INVALID_INPUT,
        }
    };

    let path = match save_file(url, format, dir, name) {
        Ok(path) => path,
        Err(_) => return AMBER_ERR_CAPTURE,
    };
    match CString::new(path) {
        Ok(cstr) => {
            *out_path = cstr.into_raw();
            AMBER_OK
        }
        Err(_) => AMBER_ERR_CAPTURE,
    }
}

/// Free a string previously returned through `out`/`out_path` by an
/// `amber_capture_*`/`amber_save` call. A null pointer is ignored.
///
/// # Safety
/// `s` must be a pointer obtained from this library (or null), freed at most
/// once.
#[no_mangle]
pub unsafe extern "C" fn amber_string_free(s: *mut c_char) {
    if !s.is_null() {
        drop(CString::from_raw(s));
    }
}

/// Free a byte buffer previously returned by [`amber_capture`]. A null pointer
/// is ignored. `len` must be the length reported alongside the buffer.
///
/// # Safety
/// `ptr`/`len` must be a pair obtained from [`amber_capture`], freed at most
/// once.
#[no_mangle]
pub unsafe extern "C" fn amber_bytes_free(ptr: *mut u8, len: usize) {
    if !ptr.is_null() {
        drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(ptr, len)));
    }
}

/// An opaque captured page (Plans.md 10.1). Produced by [`amber_snapshot`],
/// reusable across formats, freed with [`amber_snapshot_free`].
pub struct AmberSnapshot {
    inner: crate::Snapshot,
}

/// Capture `url` **once** for the `n_formats` selectors in `formats`, returning
/// an opaque handle through `*out`. The handle then serves any of those formats
/// via [`amber_snapshot_render`] / [`amber_snapshot_text`] / [`amber_snapshot_save`]
/// with no re-fetch and no re-render — capturing many formats costs one pass.
///
/// `formats` is an array of `AMBER_FORMAT_*` selectors; it must be non-empty
/// (there is no default output). Returns [`AMBER_OK`] on success; on error a
/// non-zero code with `*out` set to null. The caller owns the handle.
///
/// # Safety
/// `url` must be a valid NUL-terminated C string; `formats` must point to
/// `n_formats` `int`s (or be null only when `n_formats == 0`); `out` must be a
/// valid, writable pointer to an `AmberSnapshot *`.
#[no_mangle]
pub unsafe extern "C" fn amber_snapshot(
    url: *const c_char,
    formats: *const c_int,
    n_formats: usize,
    out: *mut *mut AmberSnapshot,
) -> c_int {
    if url.is_null() || out.is_null() || (formats.is_null() && n_formats != 0) {
        return AMBER_ERR_INVALID_INPUT;
    }
    *out = std::ptr::null_mut();

    let Ok(url) = CStr::from_ptr(url).to_str() else {
        return AMBER_ERR_INVALID_INPUT;
    };
    let selectors: &[c_int] = if n_formats == 0 {
        &[]
    } else {
        std::slice::from_raw_parts(formats, n_formats)
    };
    let mut fmts = Vec::with_capacity(selectors.len());
    for &f in selectors {
        match format_from_int(f) {
            Some(fmt) => fmts.push(fmt),
            None => return AMBER_ERR_INVALID_INPUT,
        }
    }
    if fmts.is_empty() {
        // No default output (Plans.md): an empty set is invalid input.
        return AMBER_ERR_INVALID_INPUT;
    }

    let snap = match snapshot(url, &fmts, CaptureOptions::default()) {
        Ok(snap) => snap,
        Err(_) => return AMBER_ERR_CAPTURE,
    };
    *out = Box::into_raw(Box::new(AmberSnapshot { inner: snap }));
    AMBER_OK
}

/// Render `format` from a handle into a newly-allocated byte buffer (`*out`,
/// length `*out_len`). Works for every format. On error returns non-zero with
/// `*out` null and `*out_len` 0. Free the buffer with [`amber_bytes_free`].
///
/// # Safety
/// `snap` must come from [`amber_snapshot`] (not yet freed); `out`/`out_len`
/// must be valid, writable pointers.
#[no_mangle]
pub unsafe extern "C" fn amber_snapshot_render(
    snap: *const AmberSnapshot,
    format: c_int,
    out: *mut *mut u8,
    out_len: *mut usize,
) -> c_int {
    if snap.is_null() || out.is_null() || out_len.is_null() {
        return AMBER_ERR_INVALID_INPUT;
    }
    *out = std::ptr::null_mut();
    *out_len = 0;

    let Some(format) = format_from_int(format) else {
        return AMBER_ERR_INVALID_INPUT;
    };
    let bytes = match (*snap).inner.render(format) {
        Ok(bytes) => bytes,
        Err(_) => return AMBER_ERR_CAPTURE,
    };
    let boxed = bytes.into_boxed_slice();
    *out_len = boxed.len();
    *out = Box::into_raw(boxed) as *mut u8;
    AMBER_OK
}

/// Like [`amber_snapshot_render`] but writes a NUL-terminated C string to `*out`
/// (for the text formats). Free with [`amber_string_free`].
///
/// # Safety
/// `snap` must come from [`amber_snapshot`]; `out` a valid, writable `char *`.
#[no_mangle]
pub unsafe extern "C" fn amber_snapshot_text(
    snap: *const AmberSnapshot,
    format: c_int,
    out: *mut *mut c_char,
) -> c_int {
    if snap.is_null() || out.is_null() {
        return AMBER_ERR_INVALID_INPUT;
    }
    *out = std::ptr::null_mut();

    let Some(format) = format_from_int(format) else {
        return AMBER_ERR_INVALID_INPUT;
    };
    let bytes = match (*snap).inner.render(format) {
        Ok(bytes) => bytes,
        Err(_) => return AMBER_ERR_CAPTURE,
    };
    let text = String::from_utf8_lossy(&bytes).into_owned();
    match CString::new(text) {
        Ok(cstr) => {
            *out = cstr.into_raw();
            AMBER_OK
        }
        Err(_) => AMBER_ERR_CAPTURE,
    }
}

/// Save `format` from a handle into `dir`, returning the written path through
/// `*out_path`. `name` is the file stem or null for a default name; `dir` is
/// created if missing. Free `*out_path` with [`amber_string_free`].
///
/// # Safety
/// `snap` must come from [`amber_snapshot`]; `dir` a valid NUL-terminated C
/// string; `name` such a string or null; `out_path` a valid, writable `char *`.
#[no_mangle]
pub unsafe extern "C" fn amber_snapshot_save(
    snap: *const AmberSnapshot,
    format: c_int,
    dir: *const c_char,
    name: *const c_char,
    out_path: *mut *mut c_char,
) -> c_int {
    if snap.is_null() || dir.is_null() || out_path.is_null() {
        return AMBER_ERR_INVALID_INPUT;
    }
    *out_path = std::ptr::null_mut();

    let Some(format) = format_from_int(format) else {
        return AMBER_ERR_INVALID_INPUT;
    };
    let Ok(dir) = CStr::from_ptr(dir).to_str() else {
        return AMBER_ERR_INVALID_INPUT;
    };
    let name = if name.is_null() {
        None
    } else {
        match CStr::from_ptr(name).to_str() {
            Ok(name) => Some(name),
            Err(_) => return AMBER_ERR_INVALID_INPUT,
        }
    };

    let path = match (*snap).inner.save(format, Path::new(dir), name) {
        Ok(path) => path,
        Err(_) => return AMBER_ERR_CAPTURE,
    };
    match CString::new(path.display().to_string()) {
        Ok(cstr) => {
            *out_path = cstr.into_raw();
            AMBER_OK
        }
        Err(_) => AMBER_ERR_CAPTURE,
    }
}

/// Free a handle returned by [`amber_snapshot`]. A null pointer is ignored.
///
/// # Safety
/// `snap` must be a pointer from [`amber_snapshot`] (or null), freed at most once.
#[no_mangle]
pub unsafe extern "C" fn amber_snapshot_free(snap: *mut AmberSnapshot) {
    if !snap.is_null() {
        drop(Box::from_raw(snap));
    }
}

/// Shared body for the `amber_capture_*` text entry points.
///
/// # Safety
/// `url`/`out` must satisfy the contract documented on the public entry points.
unsafe fn capture_to_c(url: *const c_char, out: *mut *mut c_char, format: OutputFormat) -> c_int {
    if url.is_null() || out.is_null() {
        return AMBER_ERR_INVALID_INPUT;
    }
    // Default to "no output" so an early error never leaves a dangling pointer.
    *out = std::ptr::null_mut();

    let Ok(url) = CStr::from_ptr(url).to_str() else {
        return AMBER_ERR_INVALID_INPUT;
    };
    let text = match capture_text(url, format) {
        Ok(text) => text,
        Err(_) => return AMBER_ERR_CAPTURE,
    };
    // CString::new fails only if the text contains an interior NUL.
    match CString::new(text) {
        Ok(cstr) => {
            *out = cstr.into_raw();
            AMBER_OK
        }
        Err(_) => AMBER_ERR_CAPTURE,
    }
}

fn capture_bytes(url: &str, format: OutputFormat) -> crate::Result<Vec<u8>> {
    let snap = snapshot(url, &[format], CaptureOptions::default())?;
    snap.render(format)
}

fn capture_text(url: &str, format: OutputFormat) -> crate::Result<String> {
    let bytes = capture_bytes(url, format)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn save_file(
    url: &str,
    format: OutputFormat,
    dir: &str,
    name: Option<&str>,
) -> crate::Result<String> {
    let snap = snapshot(url, &[format], CaptureOptions::default())?;
    let path = snap.save(format, Path::new(dir), name)?;
    Ok(path.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_arguments_are_rejected() {
        let mut out: *mut c_char = std::ptr::null_mut();
        unsafe {
            assert_eq!(
                amber_capture_markdown(std::ptr::null(), &mut out),
                AMBER_ERR_INVALID_INPUT
            );
            let url = CString::new("https://example.com/").unwrap();
            assert_eq!(
                amber_capture_markdown(url.as_ptr(), std::ptr::null_mut()),
                AMBER_ERR_INVALID_INPUT
            );
        }
    }

    #[test]
    fn bad_url_reports_capture_error_and_nulls_out() {
        let url = CString::new("not a url").unwrap();
        let mut out: *mut c_char = std::ptr::null_mut();
        let rc = unsafe { amber_capture_markdown(url.as_ptr(), &mut out) };
        assert_eq!(rc, AMBER_ERR_CAPTURE);
        assert!(
            out.is_null(),
            "out must be null on error (no dangling pointer)"
        );
    }

    #[test]
    fn string_free_handles_null_and_owned() {
        unsafe {
            // Null is a no-op.
            amber_string_free(std::ptr::null_mut());
            // A pointer produced the same way the capture functions produce one
            // round-trips through free without leaking or double-freeing.
            let owned = CString::new("hello from C ABI").unwrap().into_raw();
            amber_string_free(owned);
        }
    }

    #[test]
    fn format_selectors_round_trip_and_reject_unknown() {
        assert_eq!(format_from_int(AMBER_FORMAT_HTML), Some(OutputFormat::Html));
        assert_eq!(format_from_int(AMBER_FORMAT_PDF), Some(OutputFormat::Pdf));
        assert_eq!(format_from_int(7), Some(OutputFormat::Pdf));
        assert_eq!(format_from_int(99), None);
        assert_eq!(format_from_int(-1), None);
    }

    #[test]
    fn capture_bytes_rejects_unknown_format_and_bad_url() {
        let url = CString::new("https://example.com/").unwrap();
        let mut out: *mut u8 = std::ptr::null_mut();
        let mut len: usize = 7;
        unsafe {
            // Unknown format is rejected before any capture, out/len cleared.
            assert_eq!(
                amber_capture(url.as_ptr(), 42, &mut out, &mut len),
                AMBER_ERR_INVALID_INPUT
            );
            assert!(out.is_null());
            assert_eq!(len, 0);

            // Bad URL surfaces a capture error, out stays null.
            let bad = CString::new("not a url").unwrap();
            assert_eq!(
                amber_capture(bad.as_ptr(), AMBER_FORMAT_PDF, &mut out, &mut len),
                AMBER_ERR_CAPTURE
            );
            assert!(out.is_null());
            assert_eq!(len, 0);
        }
    }

    #[test]
    fn bytes_free_handles_null() {
        // Null is a no-op; a real round-trip is exercised by the C example.
        unsafe { amber_bytes_free(std::ptr::null_mut(), 0) };
    }

    #[test]
    fn save_rejects_unknown_format_and_bad_url() {
        let url = CString::new("not a url").unwrap();
        let dir = CString::new("/tmp").unwrap();
        let mut out: *mut c_char = std::ptr::null_mut();
        unsafe {
            assert_eq!(
                amber_save(url.as_ptr(), 99, dir.as_ptr(), std::ptr::null(), &mut out),
                AMBER_ERR_INVALID_INPUT
            );
            assert!(out.is_null());
            assert_eq!(
                amber_save(
                    url.as_ptr(),
                    AMBER_FORMAT_PDF,
                    dir.as_ptr(),
                    std::ptr::null(),
                    &mut out
                ),
                AMBER_ERR_CAPTURE
            );
            assert!(out.is_null());
        }
    }

    #[test]
    fn snapshot_handle_rejects_bad_args() {
        let mut snap: *mut AmberSnapshot = std::ptr::null_mut();
        let url = CString::new("https://example.com/").unwrap();
        let fmts = [AMBER_FORMAT_MARKDOWN];
        unsafe {
            // Null url / null out.
            assert_eq!(
                amber_snapshot(std::ptr::null(), fmts.as_ptr(), 1, &mut snap),
                AMBER_ERR_INVALID_INPUT
            );
            assert_eq!(
                amber_snapshot(url.as_ptr(), fmts.as_ptr(), 1, std::ptr::null_mut()),
                AMBER_ERR_INVALID_INPUT
            );
            // Empty format set (no default output) and unknown selector.
            assert_eq!(
                amber_snapshot(url.as_ptr(), std::ptr::null(), 0, &mut snap),
                AMBER_ERR_INVALID_INPUT
            );
            assert!(snap.is_null());
            let bad = [42];
            assert_eq!(
                amber_snapshot(url.as_ptr(), bad.as_ptr(), 1, &mut snap),
                AMBER_ERR_INVALID_INPUT
            );
            assert!(snap.is_null());
            // Bad URL → capture error, handle stays null.
            let bad_url = CString::new("not a url").unwrap();
            assert_eq!(
                amber_snapshot(bad_url.as_ptr(), fmts.as_ptr(), 1, &mut snap),
                AMBER_ERR_CAPTURE
            );
            assert!(snap.is_null());
        }
    }

    #[test]
    fn snapshot_handle_free_handles_null() {
        unsafe { amber_snapshot_free(std::ptr::null_mut()) };
    }

    #[test]
    fn snapshot_handle_renders_many_from_one_capture() {
        // Build a handle from a known capture (no browser/network) and prove it
        // serves multiple formats and frees cleanly — the C-ABI half of the
        // "capture once, emit many" contract (Plans.md 10.1).
        let url = url::Url::parse("https://example.com/").unwrap();
        let raw = crate::RawCapture {
            rendered_html: Some(
                "<html><head><title>T</title></head><body><h1>Hi</h1><p>Body text here.</p></body></html>"
                    .to_string(),
            ),
            ..Default::default()
        };
        let boxed = Box::into_raw(Box::new(AmberSnapshot {
            inner: crate::Snapshot::from_parts(url, raw),
        }));

        unsafe {
            // Markdown as text.
            let mut text: *mut c_char = std::ptr::null_mut();
            assert_eq!(
                amber_snapshot_text(boxed, AMBER_FORMAT_MARKDOWN, &mut text),
                AMBER_OK
            );
            assert!(!text.is_null());
            let md = CStr::from_ptr(text).to_str().unwrap().to_owned();
            assert!(md.contains("Hi"), "markdown: {md:?}");
            amber_string_free(text);

            // Readable as raw bytes — same capture, second format.
            let mut buf: *mut u8 = std::ptr::null_mut();
            let mut len: usize = 0;
            assert_eq!(
                amber_snapshot_render(boxed, AMBER_FORMAT_READABLE, &mut buf, &mut len),
                AMBER_OK
            );
            assert!(!buf.is_null() && len > 0);
            amber_bytes_free(buf, len);

            amber_snapshot_free(boxed);
        }
    }
}
