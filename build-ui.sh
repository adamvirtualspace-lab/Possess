#!/usr/bin/env bash
# Build the Rust/WASM UI into pkg/, which the server serves at /pkg.
#
# trunk is not used: the axum server already serves static files, so its dev
# server would be redundant, and driving wasm-bindgen directly avoids trunk's
# runtime downloads.
set -euo pipefail
cd "$(dirname "$0")"

PROFILE="${1:-release}"
OUT="pkg"
WASM="target/wasm32-unknown-unknown/${PROFILE}/possess_ui.wasm"

echo "[build-ui] compiling (${PROFILE})"
if [ "$PROFILE" = "release" ]; then
    cargo build -p possess-ui --target wasm32-unknown-unknown --release
else
    cargo build -p possess-ui --target wasm32-unknown-unknown
fi

echo "[build-ui] wasm-bindgen"
rm -rf "$OUT"
wasm-bindgen --target web --no-typescript --out-dir "$OUT" "$WASM"

# Optional: shrink the payload. Skipped when wasm-opt isn't installed, since a
# larger .wasm still works and a missing optimiser shouldn't fail the build.
if command -v wasm-opt > /dev/null && [ "$PROFILE" = "release" ]; then
    echo "[build-ui] wasm-opt"
    # rustc emits bulk-memory and non-trapping float casts by default now;
    # wasm-opt refuses to validate them unless the features are named.
    wasm-opt -Oz \
        --enable-bulk-memory \
        --enable-nontrapping-float-to-int \
        --enable-mutable-globals \
        --enable-reference-types \
        --enable-sign-ext \
        -o "$OUT/possess_ui_bg.wasm" "$OUT/possess_ui_bg.wasm"
else
    echo "[build-ui] wasm-opt not found or dev build — skipping"
fi

echo "[build-ui] done: $(du -h "$OUT/possess_ui_bg.wasm" | cut -f1) at $OUT/"
