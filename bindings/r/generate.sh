#!/usr/bin/env bash
# Stage the C header + native library into src/ for the R package build.
#
# Produces (gitignored, regenerated on demand / in CI):
#   src/amber.h                  — the C header (copied from the repo's include/)
#   src/libamber_core.{dylib,so} — the native library the package links against
#
# Then build/install:  R CMD INSTALL bindings/r   (set (DY)LD_LIBRARY_PATH to
# the lib's location at run time if the rpath can't resolve it).
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/../.." && pwd)"
mkdir -p "$here/src"

case "$(uname -s)" in
  Darwin) ext=dylib ;;
  *)      ext=so ;;
esac

echo "==> building amber-core cdylib (release)"
( cd "$root" && cargo build -p amber-core --release --lib )

echo "==> staging header + native library into src/"
cp "$root/include/amber.h" "$here/src/amber.h"
cp "$root/target/release/libamber_core.$ext" "$here/src/libamber_core.$ext"

echo "==> done: $here/src/amber.h + libamber_core.$ext"
