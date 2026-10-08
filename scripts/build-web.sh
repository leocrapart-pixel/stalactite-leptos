#!/usr/bin/env bash
# Build the deployable site into ./dist.
#
# `cargo` and `wasm-bindgen` have to live outside this workspace: the toolchain
# is far larger than the 2 GB the workspace is allowed to use, so RUSTUP_HOME and
# CARGO_HOME point at /tmp and the target directory is pinned in .cargo/config.toml.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
export RUSTUP_HOME="${RUSTUP_HOME:-/tmp/rustenv/rustup}"
export CARGO_HOME="${CARGO_HOME:-/tmp/rustenv/cargo}"
export PATH="$CARGO_HOME/bin:$PATH"

cd "$root"
cargo build --release --target wasm32-unknown-unknown

rm -rf dist
mkdir -p dist
wasm-bindgen --target web --no-typescript --out-dir dist --out-name stalactite \
  "${CARGO_TARGET_DIR:-/tmp/lx-target}/wasm32-unknown-unknown/release/stalactite_leptos.wasm"

# `wasm-opt -Oz` is worth about 20% of the transfer size; skip it if binaryen is
# not around rather than failing the build.
if command -v wasm-opt >/dev/null 2>&1; then
  wasm-opt -Oz --enable-bulk-memory --enable-reference-types --enable-multivalue \
    --enable-extended-const --enable-nontrapping-float-to-int \
    -o dist/stalactite_bg.wasm.opt dist/stalactite_bg.wasm
  mv dist/stalactite_bg.wasm.opt dist/stalactite_bg.wasm
fi

cp index.html dist/
echo "built dist/:"
du -sh dist
