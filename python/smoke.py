"""Smoke test for the Python `amber` wrapper (Plans.md 10.4).

Validates that `import amber` works and exposes the capture-once surface. Run
against a built/generated binding (the maturin wheel, `maturin develop`, or a
`uniffi-bindgen`-generated `amber_core.py` + cdylib on `sys.path`):

    AMBER_CHROMIUM_PATH="$(command -v chromium || true)" python python/smoke.py

A data: URL keeps it self-contained; PDF/screenshot exercise a real browser, so
set AMBER_CHROMIUM_PATH (or let the pinned Chrome for Testing download once).
"""

import sys

import amber

URL = "data:text/html,<html><body><h1>Smoke</h1><p>hello</p></body></html>"


def main() -> int:
    md = amber.capture_markdown(URL)
    assert "Smoke" in md, f"markdown missing content: {md!r}"

    # Capture once, emit many — one render serves every format.
    snap = amber.snapshot(URL, [amber.OutputFormat.MARKDOWN, amber.OutputFormat.PDF])
    assert "Smoke" in snap.markdown(), "snapshot markdown missing content"
    assert snap.render(amber.OutputFormat.PDF)[:4] == b"%PDF", "snapshot not a PDF"

    try:
        amber.snapshot("not a url", [amber.OutputFormat.MARKDOWN])
    except amber.CaptureError:
        pass  # expected
    else:
        raise AssertionError("expected CaptureError for a bad URL")

    print("python amber smoke OK")
    return 0


if __name__ == "__main__":
    sys.exit(main())
