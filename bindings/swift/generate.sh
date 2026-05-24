#!/usr/bin/env bash
# Generate the AmberHTML Swift wrapper from the amber-core cdylib.
#
# Produces (gitignored, regenerated on demand / in CI):
#   Sources/AmberHTML/amber_core.swift   — the UniFFI-generated Swift wrapper
#
# Run build-xcframework.sh too (it builds the native code the wrapper links to),
# then `swift build` / `swift test`.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/../.." && pwd)"
out="$here/Sources/AmberHTML"
mkdir -p "$out"

echo "==> building amber-core cdylib (for binding generation)"
( cd "$root" && cargo build -p amber-core --release --lib )

echo "==> generating Swift wrapper"
tmp="$(mktemp -d)"
( cd "$root" && cargo run -q -p uniffi-bindgen -- generate \
    --library "$root/target/release/libamber_core.dylib" \
    --language swift --out-dir "$tmp" )

# The wrapper goes in the Swift target; the header + modulemap go into the
# xcframework (see build-xcframework.sh).
cp "$tmp/amber_core.swift" "$out/amber_core.swift"
echo "==> done: $out/amber_core.swift"
