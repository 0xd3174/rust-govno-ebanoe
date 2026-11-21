#!/usr/bin/env bash
# Build the imgui-sys-v1 WebAssembly provider from the pinned dear-imgui-rs checkout.
#
# This is the only step that uses the upstream repository: it compiles the provider
# once and copies three static files into web/provider/. The Rust application keeps
# its crates.io dependencies unchanged.
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
REPO_URL=${PROVIDER_REPO_URL:-https://github.com/Latias94/dear-imgui-rs}
PIN=${PROVIDER_PIN:-7424350518efb54693b8acd4a417b69d9d0f015b}
SRC=${PROVIDER_SRC:-"$ROOT/.provider-src/dear-imgui-rs"}
OUT="$ROOT/web/provider"

if [ ! -d "$SRC/.git" ]; then
  mkdir -p "$(dirname "$SRC")"
  git clone --filter=blob:none --no-checkout "$REPO_URL" "$SRC"
fi
git -C "$SRC" fetch --depth=1 origin "$PIN"
git -C "$SRC" checkout --detach FETCH_HEAD
git -C "$SRC" submodule update --init --recursive

nix develop "$ROOT#provider" -c bash -c "
  set -euo pipefail
  cd '$SRC'
  python3 '$ROOT/scripts/patch_xtask_emscripten.py' xtask/src/main.rs
  cargo run -p xtask -- web-demo
  cargo run -p xtask -- build-cimgui-provider
"

mkdir -p "$OUT"
cp "$SRC/target/web-demo/imgui-sys-v1.js" \
   "$SRC/target/web-demo/imgui-sys-v1.wasm" \
   "$SRC/target/web-demo/imgui-sys-v1-wrapper.js" \
   "$OUT/"
echo "provider copied to $OUT"
