"""AmberHTML — local-first web-page capture (Python).

A friendly alias over the UniFFI-generated ``amber_core`` module so you can
``import amber`` instead of ``import amber_core`` (Plans.md 10.4). Everything in
``amber_core`` is re-exported here unchanged; ``import amber_core`` keeps working
too.

    import amber

    # one capture, many formats:
    snap = amber.snapshot("https://example.com",
                          [amber.OutputFormat.MARKDOWN, amber.OutputFormat.PDF])
    md  = snap.markdown()
    pdf = snap.render(amber.OutputFormat.PDF)
    snap.save(amber.OutputFormat.HTML, "out", "page")

    # or one-shot convenience helpers:
    amber.capture_markdown("https://example.com")
    amber.capture("https://example.com", amber.OutputFormat.PDF)  # -> bytes
"""

from amber_core import *  # noqa: F401,F403  (re-export the whole surface)
import amber_core as _core

# Pin the friendly names explicitly so the public surface is stable even if the
# generated module ever omits something from a wildcard import.
snapshot = _core.snapshot
capture = _core.capture
capture_text = _core.capture_text
capture_markdown = _core.capture_markdown
capture_readable = _core.capture_readable
save = _core.save
Snapshot = _core.Snapshot
OutputFormat = _core.OutputFormat
CaptureError = _core.CaptureError

__all__ = [
    "snapshot",
    "capture",
    "capture_text",
    "capture_markdown",
    "capture_readable",
    "save",
    "Snapshot",
    "OutputFormat",
    "CaptureError",
]

__version__ = getattr(_core, "__version__", None)
