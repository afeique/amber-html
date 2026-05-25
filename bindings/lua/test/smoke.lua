-- Smoke test for the Lua (LuaJIT FFI) binding. Run generate.sh first, then:
--   cd bindings/lua && luajit test/smoke.lua
-- A data: URL keeps it self-contained; PDF/screenshot drive a real browser, so
-- set AMBER_CHROMIUM_PATH (or let the pinned Chrome for Testing download once).
package.path = "./?.lua;" .. package.path

local amber = require("amber")
local F = amber.Format

local url = "data:text/html,<html><body><h1>Smoke</h1><p>hello</p></body></html>"

local md = amber.capture_markdown(url)
assert(md:find("Smoke"), "markdown missing content")

local pdf = amber.capture(url, F.PDF)
assert(pdf:sub(1, 4) == "%PDF", "not a PDF")

local png = amber.capture(url, F.SCREENSHOT)
assert(png:sub(2, 4) == "PNG", "not a PNG")

-- Capture once, emit many (Plans.md 10.1/11.3).
local snap = amber.snapshot(url, { F.MARKDOWN, F.PDF })
assert(snap:markdown():find("Smoke"), "snapshot markdown missing content")
assert(snap:render(F.PDF):sub(1, 4) == "%PDF", "snapshot not a PDF")
snap:close()

local ok = pcall(function() return amber.capture_markdown("not a url") end)
assert(not ok, "expected an error for a bad URL")

print(("lua smoke OK (markdown %dB, pdf %dB, png %dB)"):format(#md, #pdf, #png))
