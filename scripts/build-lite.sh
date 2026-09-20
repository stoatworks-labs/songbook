#!/usr/bin/env bash
# Build Songbook Lite: the Rust core to WebAssembly, then the site.
#   rustup target add wasm32-unknown-unknown   (once)
#   npm ci                                     (once)
# Output: dist-lite/ — what wrangler deploys.
set -euo pipefail
cd "$(dirname "$0")/.."

( cd src-tauri && cargo build --profile wasm -p songbook-wasm --target wasm32-unknown-unknown )
mkdir -p lite/public
cp src-tauri/target/wasm32-unknown-unknown/wasm/songbook_wasm.wasm lite/public/songbook.wasm
printf 'lite/public/songbook.wasm  %s bytes\n' "$(wc -c < lite/public/songbook.wasm | tr -d ' ')"

npx vite build --config lite/vite.config.ts
ls -la dist-lite
