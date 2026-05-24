#!/usr/bin/env bash
# Generate the AmberHTML Ruby binding from the amber-core cdylib.
#
# Produces (gitignored, regenerated on demand / in CI):
#   lib/amber_core.rb             — the UniFFI-generated FFI module
#   lib/libamber_core.{dylib,so}  — the native library the binding loads
#
# Run this before `gem build amber-html.gemspec` or `ruby test/smoke.rb`.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/../.." && pwd)"
lib="$here/lib"
mkdir -p "$lib"

case "$(uname -s)" in
  Darwin) ext=dylib ;;
  *)      ext=so ;;
esac

echo "==> building amber-core cdylib (release)"
( cd "$root" && cargo build -p amber-core --release --lib )
src="$root/target/release/libamber_core.$ext"

echo "==> generating Ruby binding"
( cd "$root" && cargo run -q -p uniffi-bindgen -- generate \
    --library "$src" --language ruby --out-dir "$lib" )

echo "==> bundling native library + fixing load path"
cp "$src" "$lib/libamber_core.$ext"
# UniFFI hardcodes `ffi_lib 'amber_core'`; load the bundled lib by absolute path,
# falling back to the system search path.
perl -0pi -e "s|ffi_lib 'amber_core'|ffi_lib [File.expand_path('libamber_core.dylib', __dir__), File.expand_path('libamber_core.so', __dir__), 'amber_core']|" "$lib/amber_core.rb"

echo "==> done: $lib/amber_core.rb + libamber_core.$ext"
