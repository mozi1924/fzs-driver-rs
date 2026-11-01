#!/usr/bin/env bash
# ==============================================================================
# FZS Thermal Web Studio - Build & WASM Package Script
# Compiles the Pure-Rust WASM core and bundles JavaScript bindings for the WebUI.
# ==============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"

cd "${ROOT_DIR}"

echo "=========================================================="
echo "🚀 Building FZS Thermal Pure-Rust WASM Core..."
echo "=========================================================="

# 1. Ensure wasm32-unknown-unknown target is installed
if ! rustup target list --installed | grep -q "wasm32-unknown-unknown"; then
    echo "📥 Installing rustup target wasm32-unknown-unknown..."
    rustup target add wasm32-unknown-unknown
fi

# 2. Ensure wasm-bindgen CLI is available
if ! command -v wasm-bindgen &> /dev/null; then
    echo "⚠️ wasm-bindgen CLI not found."
    echo "📥 Installing wasm-bindgen-cli (matching Cargo.lock)..."
    cargo install wasm-bindgen-cli --version 0.2.130
fi

# 3. Compile crates/fzs-wasm in release mode
echo "🦀 Compiling crates/fzs-wasm to wasm32..."
cargo build -p fzs-wasm --target wasm32-unknown-unknown --release

# 4. Generate WebAssembly JS/TS bindings into web/wasm
echo "📦 Generating Web bindings via wasm-bindgen..."
mkdir -p "${ROOT_DIR}/web/wasm"
rm -rf "${ROOT_DIR}/web/wasm"/*

wasm-bindgen "${ROOT_DIR}/target/wasm32-unknown-unknown/release/fzs_wasm.wasm" \
    --out-dir "${ROOT_DIR}/web/wasm" \
    --target web

WASM_SIZE=$(du -h "${ROOT_DIR}/web/wasm/fzs_wasm_bg.wasm" | cut -f1)

echo "=========================================================="
echo "✓ Build complete!"
echo "  Artifact: web/wasm/fzs_wasm_bg.wasm (${WASM_SIZE})"
echo "  Ready to serve: cargo run -p fzs-cli -- web"
echo "=========================================================="
