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
}
