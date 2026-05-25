#!/usr/bin/env bash
# Stage the native library for the Dart (dart:ffi) binding.
#
# Produces (gitignored, regenerated on demand / in CI):
#   native/libamber_core.{dylib,so} — the native library dart:ffi loads
#
# Run this before `dart test`. The C signatures are declared in lib/amber.dart,
# so only the library needs staging. Set AMBER_LIB to load it from elsewhere.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/../.." && pwd)"
mkdir -p "$here/native"

case "$(uname -s)" in
  Darwin) ext=dylib ;;
  *)      ext=so ;;
esac

echo "==> building amber-core cdylib (release)"
( cd "$root" && cargo build -p amber-core --release --lib )

echo "==> staging native library"
cp "$root/target/release/libamber_core.$ext" "$here/native/libamber_core.$ext"

echo "==> done: $here/native/libamber_core.$ext"
