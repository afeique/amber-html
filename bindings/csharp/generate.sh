#!/usr/bin/env bash
# Stage the native amber-core library for the C#/.NET (P/Invoke) binding.
#
# Produces (gitignored, regenerated on demand / in CI):
#   src/AmberHtml/runtimes/<rid>/native/libamber_core.{dylib,so}
#
# .NET's RID-based native resolution loads it from there at run time. Run this
# before `dotnet test` / `dotnet pack`.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/../.." && pwd)"

case "$(uname -s)-$(uname -m)" in
  Darwin-arm64)   rid=osx-arm64;   ext=dylib ;;
  Darwin-x86_64)  rid=osx-x64;     ext=dylib ;;
  Linux-x86_64)   rid=linux-x64;   ext=so ;;
  Linux-aarch64)  rid=linux-arm64; ext=so ;;
  *) echo "unsupported platform: $(uname -s)-$(uname -m)" >&2; exit 1 ;;
esac
dst="$here/src/AmberHtml/runtimes/$rid/native"
mkdir -p "$dst"

echo "==> building amber-core cdylib (release)"
( cd "$root" && cargo build -p amber-core --release --lib )

echo "==> staging native library ($rid)"
cp "$root/target/release/libamber_core.$ext" "$dst/libamber_core.$ext"

echo "==> done: $dst/libamber_core.$ext"
