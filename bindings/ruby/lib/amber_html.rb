# frozen_string_literal: true

# AmberHTML — local-first web-page capture, Ruby binding.
#
# Loads the UniFFI-generated module produced by `bindings/ruby/generate.sh`
# (run that before building/using the gem) and exposes it as `AmberHtml`:
#
#   require "amber_html"
#   md  = AmberHtml.capture_markdown("https://example.com")
#   pdf = AmberHtml.capture("https://example.com", AmberHtml::OutputFormat::PDF)
#   AmberHtml.save("https://example.com", AmberHtml::OutputFormat::HTML, "out", "page")
require_relative "amber_core"

# `AmberCore` is the UniFFI namespace (the Rust crate name); expose it under the
# friendlier `AmberHtml` constant so callers say `AmberHtml.capture_markdown`.
AmberHtml = AmberCore
