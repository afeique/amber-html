using System;
using System.Runtime.InteropServices;

namespace AmberHtml;

/// <summary>Output representation selectors (mirror the C ABI AMBER_FORMAT_*).</summary>
public enum Format
{
    Html = 0,
    Mhtml = 1,
    Markdown = 2,
    Readable = 3,
    Warc = 4,
    Wacz = 5,
    Screenshot = 6,
    Pdf = 7,
}

/// <summary>Thrown when a capture fails or an argument is rejected.</summary>
public sealed class AmberException : Exception
{
    public AmberException(string message) : base(message) { }
}

/// <summary>
/// Local-first web-page capture via the amber-core C ABI. The first capture that
/// needs a browser downloads a pinned Chrome for Testing into the cache (set
/// AMBER_CHROMIUM_PATH to reuse an existing Chromium).
/// </summary>
public static class Amber
{
    private const string Lib = "amber_core";
    private const int Ok = 0;
    private const int ErrInvalidInput = 1;

    [DllImport(Lib)]
    private static extern int amber_capture_markdown(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string url, out IntPtr outPtr);

    [DllImport(Lib)]
    private static extern int amber_capture_readable(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string url, out IntPtr outPtr);

    [DllImport(Lib)]
    private static extern int amber_capture(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string url, int format,
        out IntPtr outPtr, out UIntPtr outLen);

    [DllImport(Lib)]
    private static extern int amber_save(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string url, int format,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string dir,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string? name,
        out IntPtr outPath);

    [DllImport(Lib)]
    private static extern void amber_string_free(IntPtr s);

    [DllImport(Lib)]
    private static extern void amber_bytes_free(IntPtr ptr, UIntPtr len);

    private static string Describe(int rc) =>
        rc == ErrInvalidInput ? "invalid input" : "capture failed";

    /// <summary>Capture <paramref name="url"/> and return its clean Markdown.</summary>
    public static string CaptureMarkdown(string url)
    {
        int rc = amber_capture_markdown(url, out IntPtr ptr);
        if (rc != Ok) throw new AmberException(Describe(rc));
        try { return Marshal.PtrToStringUTF8(ptr) ?? string.Empty; }
        finally { amber_string_free(ptr); }
    }

    /// <summary>Capture <paramref name="url"/> and return its readable plain text.</summary>
    public static string CaptureReadable(string url)
    {
        int rc = amber_capture_readable(url, out IntPtr ptr);
        if (rc != Ok) throw new AmberException(Describe(rc));
        try { return Marshal.PtrToStringUTF8(ptr) ?? string.Empty; }
        finally { amber_string_free(ptr); }
    }

    /// <summary>
    /// Capture <paramref name="url"/> as <paramref name="format"/> and return the
    /// encoded bytes. Works for every format, including binary ones
    /// (Screenshot/Pdf/Mhtml/Warc/Wacz).
    /// </summary>
    public static byte[] Capture(string url, Format format)
    {
        int rc = amber_capture(url, (int)format, out IntPtr ptr, out UIntPtr len);
        if (rc != Ok) throw new AmberException(Describe(rc));
        try
        {
            int n = checked((int)len);
            var bytes = new byte[n];
            if (n > 0) Marshal.Copy(ptr, bytes, 0, n);
            return bytes;
        }
        finally { amber_bytes_free(ptr, len); }
    }

    /// <summary>
    /// Capture <paramref name="url"/> as <paramref name="format"/>, write it into
    /// <paramref name="dir"/>, and return the written path. <paramref name="name"/>
    /// is the file stem (the extension follows the format); pass null for a default
    /// name. <paramref name="dir"/> is created if missing.
    /// </summary>
    public static string Save(string url, Format format, string dir, string? name = null)
    {
        int rc = amber_save(url, (int)format, dir, name, out IntPtr ptr);
        if (rc != Ok) throw new AmberException(Describe(rc));
        try { return Marshal.PtrToStringUTF8(ptr) ?? string.Empty; }
        finally { amber_string_free(ptr); }
    }

    /// <summary>
    /// Capture <paramref name="url"/> once for the given <paramref name="formats"/>,
    /// returning a reusable <see cref="Snapshot"/> — capture once, emit many. At
    /// least one format is required (there is no default output).
    /// </summary>
    public static Snapshot Snapshot(string url, params Format[] formats) =>
        AmberHtml.Snapshot.Capture(url, formats);
}

/// <summary>
/// A captured page, reusable across many output formats (Plans.md 10.1/10.3).
/// One capture serves every format with no re-fetch and no re-render. Dispose it
/// (or use a <c>using</c> block) to release the native handle.
/// </summary>
public sealed class Snapshot : IDisposable
{
    private const string Lib = "amber_core";
    private const int Ok = 0;
    private const int ErrInvalidInput = 1;

    private IntPtr _handle;

    private Snapshot(IntPtr handle) => _handle = handle;

    [DllImport(Lib)]
    private static extern int amber_snapshot(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string url,
        int[] formats, UIntPtr nFormats, out IntPtr outSnap);

    [DllImport(Lib)]
    private static extern int amber_snapshot_render(
        IntPtr snap, int format, out IntPtr outPtr, out UIntPtr outLen);

    [DllImport(Lib)]
    private static extern int amber_snapshot_text(IntPtr snap, int format, out IntPtr outPtr);

    [DllImport(Lib)]
    private static extern int amber_snapshot_save(
        IntPtr snap, int format,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string dir,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string? name,
        out IntPtr outPath);

    [DllImport(Lib)]
    private static extern void amber_snapshot_free(IntPtr snap);

    [DllImport(Lib)]
    private static extern void amber_string_free(IntPtr s);

    [DllImport(Lib)]
    private static extern void amber_bytes_free(IntPtr ptr, UIntPtr len);

    private static string Describe(int rc) =>
        rc == ErrInvalidInput ? "invalid input" : "capture failed";

    /// <summary>Capture <paramref name="url"/> once for <paramref name="formats"/>.</summary>
    public static Snapshot Capture(string url, params Format[] formats)
    {
        var selectors = Array.ConvertAll(formats, f => (int)f);
        int rc = amber_snapshot(url, selectors, (UIntPtr)selectors.Length, out IntPtr handle);
        if (rc != Ok) throw new AmberException(Describe(rc));
        return new Snapshot(handle);
    }

    /// <summary>Render <paramref name="format"/> from the capture as encoded bytes.</summary>
    public byte[] Render(Format format)
    {
        int rc = amber_snapshot_render(_handle, (int)format, out IntPtr ptr, out UIntPtr len);
        if (rc != Ok) throw new AmberException(Describe(rc));
        try
        {
            int n = checked((int)len);
            var bytes = new byte[n];
            if (n > 0) Marshal.Copy(ptr, bytes, 0, n);
            return bytes;
        }
        finally { amber_bytes_free(ptr, len); }
    }

    /// <summary>Render <paramref name="format"/> from the capture as UTF-8 text.</summary>
    public string Text(Format format)
    {
        int rc = amber_snapshot_text(_handle, (int)format, out IntPtr ptr);
        if (rc != Ok) throw new AmberException(Describe(rc));
        try { return Marshal.PtrToStringUTF8(ptr) ?? string.Empty; }
        finally { amber_string_free(ptr); }
    }

    /// <summary>Save <paramref name="format"/> into <paramref name="dir"/>; returns the path.</summary>
    public string Save(Format format, string dir, string? name = null)
    {
        int rc = amber_snapshot_save(_handle, (int)format, dir, name, out IntPtr ptr);
        if (rc != Ok) throw new AmberException(Describe(rc));
        try { return Marshal.PtrToStringUTF8(ptr) ?? string.Empty; }
        finally { amber_string_free(ptr); }
    }

    /// <summary>The captured page's clean Markdown.</summary>
    public string Markdown() => Text(Format.Markdown);

    /// <summary>The captured page's readable plain text.</summary>
    public string Readable() => Text(Format.Readable);

    /// <summary>Release the native handle.</summary>
    public void Dispose()
    {
        if (_handle != IntPtr.Zero)
        {
            amber_snapshot_free(_handle);
            _handle = IntPtr.Zero;
        }
    }
}
