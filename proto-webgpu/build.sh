#!/bin/sh
# WebGPU プロトタイプのビルド: cargo → wasm-bindgen → pkg/
set -e
cd "$(dirname "$0")"
export PATH="$HOME/.cargo/bin:$PATH"
cargo build --release --target wasm32-unknown-unknown
wasm-bindgen --target web --no-typescript --out-dir pkg \
  target/wasm32-unknown-unknown/release/proto_webgpu.wasm
echo "built: pkg/proto_webgpu_bg.wasm"
