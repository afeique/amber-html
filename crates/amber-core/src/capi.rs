//! C ABI facade for the long tail of languages. See `Plans.md` (task 6.2).
//!
//! AmberHTML's full Rust API doesn't cross a C boundary cleanly (generics,
//! `Vec<u8>`, `Result`), so this module exposes a tiny, idiomatic C surface:
//! capture a URL to Markdown or readable text. The crate builds as a `cdylib`;
//! pair these symbols with the C header in `include/amber.h` (regenerate it with
//! `cbindgen` per `cbindgen.toml`). Strings returned to C are heap-allocated and
//! must be released with [`amber_string_free`].

use std::ffi::{c_char, c_int, CStr, CString};

use crate::{snapshot, CaptureOptions, OutputFormat};

/// Success.
pub const AMBER_OK: c_int = 0;
/// A null pointer or non-UTF-8 input was supplied.
pub const AMBER_ERR_INVALID_INPUT: c_int = 1;
/// The capture itself failed (bad URL, network/browser error, …).
pub const AMBER_ERR_CAPTURE: c_int = 2;

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

/// Free a string previously returned through `out` by an `amber_capture_*`
/// call. A null pointer is ignored.
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

/// Shared body for the `amber_capture_*` entry points.
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

fn capture_text(url: &str, format: OutputFormat) -> crate::Result<String> {
    let snap = snapshot(url, &[format], CaptureOptions::default())?;
    let bytes = snap.render(format)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
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
}
