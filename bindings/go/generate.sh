#!/usr/bin/env bash
# Stage the C ABI for the Go (cgo) binding.
#
# Produces (gitignored, regenerated on demand / in CI):
#   include/amber.h          — the C header (copied from the repo's include/)
#   lib/libamber_core.{dylib,so} — the native library cgo links against
#
# Run this before `go build` / `go test ./...`.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/../.." && pwd)"
mkdir -p "$here/include" "$here/lib"

case "$(uname -s)" in
  Darwin) ext=dylib ;;
  *)      ext=so ;;
esac

echo "==> building amber-core cdylib (release)"
( cd "$root" && cargo build -p amber-core --release --lib )

echo "==> staging header + native library"
cp "$root/include/amber.h" "$here/include/amber.h"
cp "$root/target/release/libamber_core.$ext" "$here/lib/libamber_core.$ext"

echo "==> done: $here/include/amber.h + lib/libamber_core.$ext"
