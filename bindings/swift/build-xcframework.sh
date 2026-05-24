#!/usr/bin/env bash
# Build AmberCoreFFI.xcframework: the native amber-core static library wrapped
# with the UniFFI-generated C header + modulemap, for the Swift package's
# binary target.
#
# This builds for the host macOS arch only (enough for local `swift build`/CI on
# macOS). A release xcframework should add the other Apple slices
# (arm64/x86_64 macOS, arm64 iOS, the simulators) and lipo/`-create-xcframework`
# them together.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/../.." && pwd)"
work="$here/.xcframework-build"
xcf="$here/AmberCoreFFI.xcframework"
rm -rf "$work" "$xcf"
mkdir -p "$work/headers"

echo "==> building amber-core staticlib (release)"
( cd "$root" && cargo build -p amber-core --release --lib )
staticlib="$root/target/release/libamber_core.a"
[ -f "$staticlib" ] || { echo "missing $staticlib (is 'staticlib' in crate-type?)"; exit 1; }

echo "==> generating the C header + modulemap"
( cd "$root" && cargo run -q -p uniffi-bindgen -- generate \
    --library "$root/target/release/libamber_core.dylib" \
    --language swift --out-dir "$work/gen" )
cp "$work/gen/amber_coreFFI.h" "$work/headers/"
# SwiftPM/Clang expect the modulemap to be named `module.modulemap`.
cp "$work/gen/amber_coreFFI.modulemap" "$work/headers/module.modulemap"

echo "==> assembling xcframework"
xcodebuild -create-xcframework \
    -library "$staticlib" -headers "$work/headers" \
    -output "$xcf"

echo "==> done: $xcf"
