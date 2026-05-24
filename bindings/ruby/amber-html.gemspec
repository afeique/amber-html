# frozen_string_literal: true

Gem::Specification.new do |s|
  s.name        = "amber-html"
  s.version     = ENV.fetch("AMBER_VERSION", "0.1.0")
  s.summary     = "Local-first web-page capture: Markdown, HTML, MHTML, WARC/WACZ, screenshot, PDF."
  s.description = "Ruby bindings for AmberHTML, a Rust engine that drives a pinned, " \
                  "auto-managed Chromium over the CDP debug pipe to capture web pages " \
                  "locally — only when a page actually needs a browser."
  s.authors     = ["Afeique Sheikh"]
  s.homepage    = "https://github.com/afeique/amber-html"
  s.licenses    = ["MIT", "Apache-2.0"]
  s.required_ruby_version = ">= 2.6"

  # CI builds one platform-specific gem per target; set GEM_PLATFORM, e.g.
  # `GEM_PLATFORM=x86_64-linux gem build amber-html.gemspec`.
  s.platform = ENV["GEM_PLATFORM"] if ENV["GEM_PLATFORM"]

  # The generated module + the native library are produced by generate.sh and
  # bundled here; CI builds one platform-specific gem per target.
  s.files = Dir["lib/**/*.rb"] + Dir["lib/libamber_core.*"] + ["README.md"]
  s.require_paths = ["lib"]

  s.add_runtime_dependency "ffi", "~> 1.15"

  s.metadata = {
    "source_code_uri"   => "https://github.com/afeique/amber-html",
    "bug_tracker_uri"   => "https://github.com/afeique/amber-html/issues",
    "rubygems_mfa_required" => "true",
  }
end
