# Homebrew formula for the AmberHTML CLI.
#
# Distribute via a tap: create a repo `afeique/homebrew-amber`, drop this file
# in its `Formula/` dir, and bump `url` + `sha256` for each release (see
# RELEASING.md). Users then:  brew install afeique/amber/amber
#
# `sha256` is for the release source tarball:
#   curl -sL https://github.com/afeique/amber-html/archive/refs/tags/v0.1.0.tar.gz | shasum -a 256
class Amber < Formula
  desc "Local-first web-page capture engine (Markdown, HTML, MHTML, WARC/WACZ, PDF, screenshot)"
  homepage "https://github.com/afeique/amber-html"
  url "https://github.com/afeique/amber-html/archive/refs/tags/v0.1.0.tar.gz"
  sha256 "REPLACE_WITH_TARBALL_SHA256"
  license any_of: ["MIT", "Apache-2.0"]
  head "https://github.com/afeique/amber-html.git", branch: "master"

  depends_on "rust" => :build

  def install
    system "cargo", "install", "--locked", "--path", "crates/amber-cli", "--root", prefix
  end

  test do
    # A pinned Chrome for Testing downloads on first browser capture; --help is
    # offline and proves the binary installed and runs.
    assert_match "amber", shell_output("#{bin}/amber --help")
  end
end
