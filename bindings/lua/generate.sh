#!/usr/bin/env bash
# Stage the native library for the Lua (LuaJIT FFI) binding.
#
# Produces (gitignored, regenerated on demand / in CI):
#   lib/libamber_core.{dylib,so} — the native library the FFI loads
#
# Run this before `luajit test/smoke.lua`. The C signatures are declared in
# amber.lua (ffi.cdef), so only the library needs staging. Set AMBER_LIB to load
# it from elsewhere.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/../.." && pwd)"
mkdir -p "$here/lib"

case "$(uname -s)" in
  Darwin) ext=dylib ;;
  *)      ext=so ;;
esac

echo "==> building amber-core cdylib (release)"
( cd "$root" && cargo build -p amber-core --release --lib )

echo "==> staging native library"
cp "$root/target/release/libamber_core.$ext" "$here/lib/libamber_core.$ext"

echo "==> done: $here/lib/libamber_core.$ext"
