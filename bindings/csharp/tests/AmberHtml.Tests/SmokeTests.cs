using System;
using System.IO;
using AmberHtml;
using Xunit;

namespace AmberHtml.Tests;

public class SmokeTests
{
    // A data: URL keeps the test self-contained while exercising the real pipeline.
    private const string Url =
        "data:text/html,<html><body><h1>Smoke</h1><p>hello</p></body></html>";

    [Fact]
    public void Markdown()
    {
        var md = Amber.CaptureMarkdown(Url);
        Assert.Contains("Smoke", md);
    }

    [Fact]
    public void BinaryFormats()
    {
        var pdf = Amber.Capture(Url, Format.Pdf);
        Assert.True(pdf.Length > 4);
        Assert.Equal(new byte[] { 0x25, 0x50, 0x44, 0x46 }, pdf[..4]); // %PDF

        var png = Amber.Capture(Url, Format.Screenshot);
        Assert.Equal(new byte[] { 0x89, 0x50, 0x4E, 0x47 }, png[..4]); // \x89PNG
    }

    [Fact]
    public void Save()
    {
        var dir = Path.Combine(Path.GetTempPath(), "amber-csharp-smoke");
        var path = Amber.Save(Url, Format.Html, dir, "page");
        Assert.EndsWith("page.html", path);
        Assert.True(File.Exists(path));
    }

    [Fact]
    public void BadUrlThrows()
    {
        Assert.Throws<AmberException>(() => Amber.CaptureMarkdown("not a url"));
    }
}
