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

# Capture-once Snapshot object (Plans.md 10.1): one capture serves many formats.
require "tmpdir"
snap = AmberHtml.snapshot(url, [AmberHtml::OutputFormat::MARKDOWN, AmberHtml::OutputFormat::PDF])
raise "snapshot markdown missing content" unless snap.markdown.include?("Smoke")
raise "snapshot not a PDF" unless snap.render(AmberHtml::OutputFormat::PDF)[0, 4] == "%PDF"
saved = snap.save(AmberHtml::OutputFormat::READABLE, Dir.tmpdir, "amber_ruby_snap")
raise "snapshot save failed" unless File.exist?(saved)

begin
  AmberHtml.snapshot("not a url", [AmberHtml::OutputFormat::MARKDOWN])
  raise "expected an error for a bad URL (snapshot)"
rescue AmberHtml::CaptureError::Failed
  # expected
end

puts "ruby smoke OK (markdown #{md.bytesize}B, pdf #{pdf.bytesize}B, png #{png.bytesize}B, " \
     "snapshot #{snap.markdown.bytesize}B md)"
