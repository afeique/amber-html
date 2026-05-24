# frozen_string_literal: true

# Smoke test for the Ruby binding. Run `bindings/ruby/generate.sh` first, then:
#   ruby bindings/ruby/test/smoke.rb
$LOAD_PATH.unshift(File.expand_path("../lib", __dir__))
require "amber_html"

url = "data:text/html,<html><body><h1>Smoke</h1><p>hello</p></body></html>"

md = AmberHtml.capture_markdown(url)
raise "markdown missing content" unless md.include?("Smoke")

pdf = AmberHtml.capture(url, AmberHtml::OutputFormat::PDF)
raise "not a PDF" unless pdf[0, 4] == "%PDF"

png = AmberHtml.capture(url, AmberHtml::OutputFormat::SCREENSHOT)
raise "not a PNG" unless png[1, 3] == "PNG"

begin
  AmberHtml.capture_markdown("not a url")
  raise "expected an error for a bad URL"
rescue AmberHtml::CaptureError::Failed
  # expected — the facade surfaces capture failures as CaptureError::Failed
end

puts "ruby smoke OK (markdown #{md.bytesize}B, pdf #{pdf.bytesize}B, png #{png.bytesize}B)"
