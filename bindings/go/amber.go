// Package amber provides Go bindings for AmberHTML, a local-first web-page
// capture engine. It wraps the amber-core C ABI via cgo.
//
// Run generate.sh first: it builds the native library and copies the C header
// and library into this package (include/ and lib/), which cgo links against.
//
//	md, err := amber.CaptureMarkdown("https://example.com")
//	pdf, err := amber.Capture("https://example.com", amber.FormatPDF)
//	path, err := amber.Save("https://example.com", amber.FormatHTML, "out", "page")
package amber

/*
#cgo CFLAGS: -I${SRCDIR}/include
#cgo LDFLAGS: -L${SRCDIR}/lib -lamber_core -Wl,-rpath,${SRCDIR}/lib
#include <stdlib.h>
#include "amber.h"
*/
import "C"

import (
	"errors"
	"unsafe"
)

// Format selects the output representation. The values mirror the C ABI's
// AMBER_FORMAT_* selectors.
type Format int

const (
	FormatHTML       Format = C.AMBER_FORMAT_HTML
	FormatMHTML      Format = C.AMBER_FORMAT_MHTML
	FormatMarkdown   Format = C.AMBER_FORMAT_MARKDOWN
	FormatReadable   Format = C.AMBER_FORMAT_READABLE
	FormatWARC       Format = C.AMBER_FORMAT_WARC
	FormatWACZ       Format = C.AMBER_FORMAT_WACZ
	FormatScreenshot Format = C.AMBER_FORMAT_SCREENSHOT
	FormatPDF        Format = C.AMBER_FORMAT_PDF
)

// ErrInvalidInput is returned when an argument is rejected before any capture
// (a bad string or an unknown format). ErrCapture is returned when the capture
// itself fails (bad URL, network/browser error, …).
var (
	ErrInvalidInput = errors.New("amber: invalid input")
	ErrCapture      = errors.New("amber: capture failed")
)

func errFromCode(rc C.int) error {
	switch rc {
	case C.AMBER_OK:
		return nil
	case C.AMBER_ERR_INVALID_INPUT:
		return ErrInvalidInput
	default:
		return ErrCapture
	}
}

// CaptureMarkdown captures url and returns its clean Markdown.
func CaptureMarkdown(url string) (string, error) {
	curl := C.CString(url)
	defer C.free(unsafe.Pointer(curl))

	var out *C.char
	if rc := C.amber_capture_markdown(curl, &out); rc != C.AMBER_OK {
		return "", errFromCode(rc)
	}
	defer C.amber_string_free(out)
	return C.GoString(out), nil
}

// CaptureReadable captures url and returns its readable plain text.
func CaptureReadable(url string) (string, error) {
	curl := C.CString(url)
	defer C.free(unsafe.Pointer(curl))

	var out *C.char
	if rc := C.amber_capture_readable(curl, &out); rc != C.AMBER_OK {
		return "", errFromCode(rc)
	}
	defer C.amber_string_free(out)
	return C.GoString(out), nil
}

// Capture captures url as format and returns the encoded bytes. Works for every
// format, including the binary ones (Screenshot/PDF/MHTML/WARC/WACZ).
func Capture(url string, format Format) ([]byte, error) {
	curl := C.CString(url)
	defer C.free(unsafe.Pointer(curl))

	var out *C.uint8_t
	var outLen C.size_t
	if rc := C.amber_capture(curl, C.int(format), &out, &outLen); rc != C.AMBER_OK {
		return nil, errFromCode(rc)
	}
	defer C.amber_bytes_free(out, outLen)
	return C.GoBytes(unsafe.Pointer(out), C.int(outLen)), nil
}

// Save captures url as format, writes it into dir, and returns the written
// path. name is the file stem (the extension follows the format); an empty name
// uses a default <safe-url> <date> <time> name. dir is created if missing.
func Save(url string, format Format, dir, name string) (string, error) {
	curl := C.CString(url)
	defer C.free(unsafe.Pointer(curl))
	cdir := C.CString(dir)
	defer C.free(unsafe.Pointer(cdir))

	var cname *C.char
	if name != "" {
		cname = C.CString(name)
		defer C.free(unsafe.Pointer(cname))
	}

	var out *C.char
	if rc := C.amber_save(curl, C.int(format), cdir, cname, &out); rc != C.AMBER_OK {
		return "", errFromCode(rc)
	}
	defer C.amber_string_free(out)
	return C.GoString(out), nil
}

// Snapshot is a captured page, reusable across formats — capture once, emit
// many. Create it with NewSnapshot, render or save any format without
// re-capturing, and release it with Close.
type Snapshot struct {
	ptr *C.AmberSnapshot
}

// NewSnapshot captures url once for the given formats, returning a handle that
// renders or saves any of them with no re-fetch and no re-render. formats must
// be non-empty (there is no default output). Call Close when done.
func NewSnapshot(url string, formats ...Format) (*Snapshot, error) {
	curl := C.CString(url)
	defer C.free(unsafe.Pointer(curl))

	var cformats *C.int
	if len(formats) > 0 {
		sel := make([]C.int, len(formats))
		for i, f := range formats {
			sel[i] = C.int(f)
		}
		// amber_snapshot reads the array synchronously and does not retain it,
		// so passing a pointer into Go memory for the call is allowed by cgo.
		cformats = &sel[0]
	}

	var ptr *C.AmberSnapshot
	rc := C.amber_snapshot(curl, cformats, C.size_t(len(formats)), &ptr)
	if rc != C.AMBER_OK {
		return nil, errFromCode(rc)
	}
	return &Snapshot{ptr: ptr}, nil
}

// Render returns format from the captured page as encoded bytes.
func (s *Snapshot) Render(format Format) ([]byte, error) {
	var out *C.uint8_t
	var outLen C.size_t
	if rc := C.amber_snapshot_render(s.ptr, C.int(format), &out, &outLen); rc != C.AMBER_OK {
		return nil, errFromCode(rc)
	}
	defer C.amber_bytes_free(out, outLen)
	return C.GoBytes(unsafe.Pointer(out), C.int(outLen)), nil
}

// Text returns format from the captured page as UTF-8 text (text formats).
func (s *Snapshot) Text(format Format) (string, error) {
	var out *C.char
	if rc := C.amber_snapshot_text(s.ptr, C.int(format), &out); rc != C.AMBER_OK {
		return "", errFromCode(rc)
	}
	defer C.amber_string_free(out)
	return C.GoString(out), nil
}

// Save writes format from the captured page into dir, returning the written
// path. name is the file stem (an empty name uses a default); dir is created if
// missing.
func (s *Snapshot) Save(format Format, dir, name string) (string, error) {
	cdir := C.CString(dir)
	defer C.free(unsafe.Pointer(cdir))

	var cname *C.char
	if name != "" {
		cname = C.CString(name)
		defer C.free(unsafe.Pointer(cname))
	}

	var out *C.char
	if rc := C.amber_snapshot_save(s.ptr, C.int(format), cdir, cname, &out); rc != C.AMBER_OK {
		return "", errFromCode(rc)
	}
	defer C.amber_string_free(out)
	return C.GoString(out), nil
}

// Markdown returns the captured page's clean Markdown.
func (s *Snapshot) Markdown() (string, error) { return s.Text(FormatMarkdown) }

// Readable returns the captured page's readable plain text.
func (s *Snapshot) Readable() (string, error) { return s.Text(FormatReadable) }

// Close releases the snapshot's native resources. Safe to call more than once.
func (s *Snapshot) Close() {
	if s.ptr != nil {
		C.amber_snapshot_free(s.ptr)
		s.ptr = nil
	}
}
