--- Lua bindings for AmberHTML, a local-first web-page capture engine.
--- Uses the LuaJIT FFI over the `amber-core` C ABI (requires LuaJIT for `ffi`).
--- See Plans.md (task 11.3). Run generate.sh first to stage the native library.
---
---   local amber = require("amber")
---   local F = amber.Format
---   local md  = amber.capture_markdown("https://example.com")
---   local pdf = amber.capture("https://example.com", F.PDF)   -- binary string
---   local snap = amber.snapshot("https://example.com", { F.MARKDOWN, F.PDF })
---   print(snap:markdown())          -- one capture, many formats
---   snap:save(F.HTML, "out", "page")
---   snap:close()

local ffi = require("ffi")

ffi.cdef([[
int amber_capture_markdown(const char *url, char **out);
int amber_capture_readable(const char *url, char **out);
int amber_capture(const char *url, int format, uint8_t **out, size_t *out_len);
int amber_save(const char *url, int format, const char *dir, const char *name, char **out_path);
typedef struct AmberSnapshot AmberSnapshot;
int amber_snapshot(const char *url, const int *formats, size_t n_formats, AmberSnapshot **out);
int amber_snapshot_render(const AmberSnapshot *snap, int format, uint8_t **out, size_t *out_len);
int amber_snapshot_text(const AmberSnapshot *snap, int format, char **out);
int amber_snapshot_save(const AmberSnapshot *snap, int format, const char *dir, const char *name, char **out_path);
void amber_snapshot_free(AmberSnapshot *snap);
void amber_string_free(char *s);
void amber_bytes_free(uint8_t *ptr, size_t len);
]])

local function load_lib()
  local override = os.getenv("AMBER_LIB")
  if override and #override > 0 then
    return ffi.load(override)
  end
  local ext = (jit.os == "OSX" and "dylib") or (jit.os == "Windows" and "dll") or "so"
  -- Try the staged library (generate.sh) before the system search path.
  for _, path in ipairs({ "lib/libamber_core." .. ext, "native/libamber_core." .. ext }) do
    local ok, lib = pcall(ffi.load, path)
    if ok then return lib end
  end
  return ffi.load("amber_core") -- fall back to the system loader
end

local C = load_lib()

local OK = 0
local ERR_INVALID_INPUT = 1

local function fail(rc)
  error(rc == ERR_INVALID_INPUT and "amber: invalid input" or "amber: capture failed", 3)
end

local M = {}

--- Output-format selectors (mirror the C ABI AMBER_FORMAT_*).
M.Format = {
  HTML = 0,
  MHTML = 1,
  MARKDOWN = 2,
  READABLE = 3,
  WARC = 4,
  WACZ = 5,
  SCREENSHOT = 6,
  PDF = 7,
}

local function capture_text_c(fn, url)
  local out = ffi.new("char*[1]")
  local rc = fn(url, out)
  if rc ~= OK then fail(rc) end
  local s = ffi.string(out[0])
  C.amber_string_free(out[0])
  return s
end

--- Capture `url` and return its clean Markdown.
function M.capture_markdown(url)
  return capture_text_c(C.amber_capture_markdown, url)
end

--- Capture `url` and return its readable plain text.
function M.capture_readable(url)
  return capture_text_c(C.amber_capture_readable, url)
end

--- Capture `url` as `format` and return the encoded bytes (a binary string).
function M.capture(url, format)
  local out = ffi.new("uint8_t*[1]")
  local len = ffi.new("size_t[1]")
  local rc = C.amber_capture(url, format, out, len)
  if rc ~= OK then fail(rc) end
  local s = ffi.string(out[0], len[0])
  C.amber_bytes_free(out[0], len[0])
  return s
end

--- Capture `url` as `format`, write it into `dir`, return the written path.
--- `name` is the file stem (extension follows the format); nil uses a default.
function M.save(url, format, dir, name)
  local out = ffi.new("char*[1]")
  local rc = C.amber_save(url, format, dir, name, out)
  if rc ~= OK then fail(rc) end
  local s = ffi.string(out[0])
  C.amber_string_free(out[0])
  return s
end

--- A captured page, reusable across many output formats (Plans.md 10.1/11.3).
local Snapshot = {}
Snapshot.__index = Snapshot

--- Capture `url` once for `formats` (an array of Format values); must be non-empty.
function M.snapshot(url, formats)
  local n = #formats
  local arr = n > 0 and ffi.new("int[?]", n) or nil
  for i = 1, n do
    arr[i - 1] = formats[i]
  end
  local out = ffi.new("AmberSnapshot*[1]")
  local rc = C.amber_snapshot(url, arr, n, out)
  if rc ~= OK then fail(rc) end
  -- Auto-free on GC; close() detaches this finalizer to avoid a double free.
  local ptr = ffi.gc(out[0], C.amber_snapshot_free)
  return setmetatable({ _ptr = ptr }, Snapshot)
end

--- Render `format` from the captured page as encoded bytes.
function Snapshot:render(format)
  local out = ffi.new("uint8_t*[1]")
  local len = ffi.new("size_t[1]")
  local rc = C.amber_snapshot_render(self._ptr, format, out, len)
  if rc ~= OK then fail(rc) end
  local s = ffi.string(out[0], len[0])
  C.amber_bytes_free(out[0], len[0])
  return s
end

--- Render `format` from the captured page as UTF-8 text.
function Snapshot:text(format)
  local out = ffi.new("char*[1]")
  local rc = C.amber_snapshot_text(self._ptr, format, out)
  if rc ~= OK then fail(rc) end
  local s = ffi.string(out[0])
  C.amber_string_free(out[0])
  return s
end

--- Write `format` into `dir`; returns the written path.
function Snapshot:save(format, dir, name)
  local out = ffi.new("char*[1]")
  local rc = C.amber_snapshot_save(self._ptr, format, dir, name, out)
  if rc ~= OK then fail(rc) end
  local s = ffi.string(out[0])
  C.amber_string_free(out[0])
  return s
end

--- The captured page's clean Markdown.
function Snapshot:markdown()
  return self:text(M.Format.MARKDOWN)
end

--- The captured page's readable plain text.
function Snapshot:readable()
  return self:text(M.Format.READABLE)
end

--- Release the native handle (idempotent).
function Snapshot:close()
  if self._ptr ~= nil then
    ffi.gc(self._ptr, nil) -- detach the GC finalizer before freeing
    C.amber_snapshot_free(self._ptr)
    self._ptr = nil
  end
end

return M
