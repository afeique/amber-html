package = "amber-html"
version = "0.1.0-1"
source = {
  url = "git+https://github.com/afeique/amber-html.git",
}
description = {
  summary = "Local-first web-page capture engine — Lua (LuaJIT FFI) bindings.",
  detailed = [[
    Lua bindings for AmberHTML via the LuaJIT FFI over the amber-core C ABI.
    Requires LuaJIT (for the `ffi` library) and the native amber-core library
    staged alongside (see bindings/lua/generate.sh).
  ]],
  homepage = "https://github.com/afeique/amber-html",
  license = "MIT OR Apache-2.0",
}
dependencies = {
  -- Requires LuaJIT specifically (the `ffi` library); luarocks has no way to
  -- pin the interpreter, so this is documented in the README.
  "lua >= 5.1",
}
build = {
  type = "builtin",
  modules = {
    amber = "amber.lua",
  },
}
