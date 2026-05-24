#!/usr/bin/env bash
# Generate the AmberHTML Kotlin binding from the amber-core cdylib.
#
# Produces (gitignored, regenerated on demand / in CI):
#   src/main/kotlin/uniffi/amber_core/amber_core.kt   — the UniFFI Kotlin wrapper
#   src/main/resources/<jna-platform>/libamber_core.* — the native library JNA loads
#
# Run this before `./gradlew build` / `./gradlew test`.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/../.." && pwd)"
ktout="$here/src/main/kotlin"
mkdir -p "$ktout"

# JNA bundles native libs on the classpath under <os>-<arch> directories.
case "$(uname -s)-$(uname -m)" in
  Darwin-arm64)   plat=darwin-aarch64; ext=dylib ;;
  Darwin-x86_64)  plat=darwin-x86-64;  ext=dylib ;;
  Linux-x86_64)   plat=linux-x86-64;   ext=so ;;
  Linux-aarch64)  plat=linux-aarch64;  ext=so ;;
  *) echo "unsupported platform: $(uname -s)-$(uname -m)" >&2; exit 1 ;;
esac
res="$here/src/main/resources/$plat"
mkdir -p "$res"

echo "==> building amber-core cdylib (release)"
( cd "$root" && cargo build -p amber-core --release --lib )
src="$root/target/release/libamber_core.$ext"

echo "==> generating Kotlin binding"
( cd "$root" && cargo run -q -p uniffi-bindgen -- generate \
    --library "$src" --language kotlin --out-dir "$ktout" )

echo "==> bundling native library for JNA ($plat)"
cp "$src" "$res/libamber_core.$ext"

echo "==> done: $ktout/uniffi/amber_core/amber_core.kt + $res/libamber_core.$ext"
