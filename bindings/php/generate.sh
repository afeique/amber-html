#!/usr/bin/env bash
# Stage the native library for the PHP (FFI) binding.
#
# Produces (gitignored, regenerated on demand / in CI):
#   lib/libamber_core.{dylib,so} — the native library PHP FFI loads
#
# Run this before `php test/smoke.php`. The C declarations are inlined in
# src/Amber.php (FFI::cdef), so only the library needs staging.
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
