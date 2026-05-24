package amber

import (
	"bytes"
	"errors"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// A data: URL keeps the test self-contained while exercising the real pipeline.
const smokeURL = "data:text/html,<html><body><h1>Smoke</h1><p>hello</p></body></html>"

func TestCaptureMarkdown(t *testing.T) {
	md, err := CaptureMarkdown(smokeURL)
	if err != nil {
		t.Fatalf("CaptureMarkdown: %v", err)
	}
	if !strings.Contains(md, "Smoke") {
		t.Fatalf("markdown missing content: %q", md)
	}
}

func TestCaptureBinaryFormats(t *testing.T) {
	pdf, err := Capture(smokeURL, FormatPDF)
	if err != nil {
		t.Fatalf("Capture PDF: %v", err)
	}
	if !bytes.HasPrefix(pdf, []byte("%PDF")) {
		t.Fatalf("not a PDF (len=%d)", len(pdf))
	}

	png, err := Capture(smokeURL, FormatScreenshot)
	if err != nil {
		t.Fatalf("Capture PNG: %v", err)
	}
	if !bytes.HasPrefix(png, []byte{0x89, 'P', 'N', 'G'}) {
		t.Fatalf("not a PNG (len=%d)", len(png))
	}
}

func TestSave(t *testing.T) {
	dir := filepath.Join(os.TempDir(), "amber-go-smoke")
	path, err := Save(smokeURL, FormatHTML, dir, "page")
	if err != nil {
		t.Fatalf("Save: %v", err)
	}
	if !strings.HasSuffix(path, "page.html") {
		t.Fatalf("unexpected path: %q", path)
	}
	if _, err := os.Stat(path); err != nil {
		t.Fatalf("written file missing: %v", err)
	}
}

func TestBadURL(t *testing.T) {
	if _, err := CaptureMarkdown("not a url"); !errors.Is(err, ErrCapture) {
		t.Fatalf("expected ErrCapture, got %v", err)
	}
}

func TestSnapshotRendersManyFromOneCapture(t *testing.T) {
	// One capture, many formats (Plans.md 10.1/10.3).
	snap, err := NewSnapshot(smokeURL, FormatMarkdown, FormatPDF)
	if err != nil {
		t.Fatalf("NewSnapshot: %v", err)
	}
	defer snap.Close()

	md, err := snap.Markdown()
	if err != nil || !strings.Contains(md, "Smoke") {
		t.Fatalf("Markdown: %v / %q", err, md)
	}
	pdf, err := snap.Render(FormatPDF)
	if err != nil || !bytes.HasPrefix(pdf, []byte("%PDF")) {
		t.Fatalf("Render PDF: %v (len=%d)", err, len(pdf))
	}
	dir := filepath.Join(os.TempDir(), "amber-go-smoke")
	path, err := snap.Save(FormatReadable, dir, "snap")
	if err != nil || !strings.HasSuffix(path, "snap.txt") {
		t.Fatalf("Save: %v / %q", err, path)
	}
	if _, err := os.Stat(path); err != nil {
		t.Fatalf("written file missing: %v", err)
	}
}

func TestSnapshotBadArgs(t *testing.T) {
	if _, err := NewSnapshot("not a url", FormatMarkdown); !errors.Is(err, ErrCapture) {
		t.Fatalf("expected ErrCapture for a bad URL, got %v", err)
	}
	// No formats → no default output → invalid input.
	if _, err := NewSnapshot(smokeURL); !errors.Is(err, ErrInvalidInput) {
		t.Fatalf("expected ErrInvalidInput for empty formats, got %v", err)
	}
}
