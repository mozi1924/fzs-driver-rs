#!/usr/bin/env bash
# ==============================================================================
# FZS Thermal Web Studio - Build & WASM Package Script
# Compiles the Pure-Rust WASM core and bundles JavaScript bindings for the WebUI.
# ==============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"

cd "${ROOT_DIR}"

REQUIRED_VER="0.2.130"

echo "=========================================================="
echo "🚀 Building FZS Thermal Pure-Rust WASM Core (v${REQUIRED_VER})..."
echo "=========================================================="

# 1. Ensure wasm32-unknown-unknown target is installed
if ! rustup target list --installed | grep -q "wasm32-unknown-unknown"; then
    echo "📥 Installing rustup target wasm32-unknown-unknown..."
    rustup target add wasm32-unknown-unknown
fi

# 2. Ensure matching wasm-bindgen CLI is available
INSTALLED_VER=""
if command -v wasm-bindgen &> /dev/null; then
    INSTALLED_VER="$(wasm-bindgen --version 2>/dev/null | cut -d' ' -f2 || true)"
fi

if [ "${INSTALLED_VER}" != "${REQUIRED_VER}" ]; then
    echo "⚠️ wasm-bindgen CLI (${REQUIRED_VER}) not found (current: '${INSTALLED_VER}')."
    OS="$(uname -s)"
    ARCH="$(uname -m)"
    mkdir -p "${HOME}/.cargo/bin"

    if [ "${OS}" = "Linux" ] && [ "${ARCH}" = "x86_64" ]; then
        echo "📥 Fetching prebuilt wasm-bindgen ${REQUIRED_VER} for Linux x86_64..."
        curl -sSL "https://github.com/wasm-bindgen/wasm-bindgen/releases/download/${REQUIRED_VER}/wasm-bindgen-${REQUIRED_VER}-x86_64-unknown-linux-musl.tar.gz" \
            | tar -xz --strip-components=1 -C "${HOME}/.cargo/bin"
    elif [ "${OS}" = "Darwin" ] && [ "${ARCH}" = "arm64" ]; then
        echo "📥 Fetching prebuilt wasm-bindgen ${REQUIRED_VER} for macOS Apple Silicon..."
        curl -sSL "https://github.com/wasm-bindgen/wasm-bindgen/releases/download/${REQUIRED_VER}/wasm-bindgen-${REQUIRED_VER}-aarch64-apple-darwin.tar.gz" \
            | tar -xz --strip-components=1 -C "${HOME}/.cargo/bin"
    elif [ "${OS}" = "Darwin" ] && [ "${ARCH}" = "x86_64" ]; then
        echo "📥 Fetching prebuilt wasm-bindgen ${REQUIRED_VER} for macOS Intel..."
        curl -sSL "https://github.com/wasm-bindgen/wasm-bindgen/releases/download/${REQUIRED_VER}/wasm-bindgen-${REQUIRED_VER}-x86_64-apple-darwin.tar.gz" \
            | tar -xz --strip-components=1 -C "${HOME}/.cargo/bin"
    else
        echo "📥 Installing wasm-bindgen-cli ${REQUIRED_VER} via cargo..."
        cargo install wasm-bindgen-cli --version "${REQUIRED_VER}"
    fi
    chmod +x "${HOME}/.cargo/bin/wasm-bindgen"
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

# 5. Build Vite self-contained frontend bundle
if command -v pnpm &> /dev/null; then
    echo "⚡ Building Vite self-contained frontend bundle with pnpm..."
    (cd "${ROOT_DIR}/web" && pnpm install --frozen-lockfile && pnpm run build)
fi

echo "=========================================================="
echo "✓ Build complete!"
echo "  Artifact: web/wasm/fzs_wasm_bg.wasm (${WASM_SIZE})"
echo "  Web Dist: web/dist/"
echo "  Ready to serve: cargo run -p fzs-cli -- web"
echo "=========================================================="
