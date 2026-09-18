#!/usr/bin/env bash
#
# Build the wasm module and assemble the static site into dist/.
#
# bitcoinconsensus compiles Bitcoin Core's script interpreter, so this needs a
# C++ toolchain targeting wasm. The wasi-sdk provides one; we borrow its clang
# and its libc++ but deliberately do not link wasi-libc, because that would add
# a second allocator and a WASI import surface. src/csupport.rs supplies the
# handful of C symbols libc++ actually needs instead.

set -euo pipefail

cd "$(dirname "$0")"

WASI_SDK_VERSION="${WASI_SDK_VERSION:-25.0}"
WASM_BINDGEN_VERSION="${WASM_BINDGEN_VERSION:-0.2.127}"  # keep in step with Cargo.toml
TOOLS_DIR="${TOOLS_DIR:-$PWD/.tools}"
WASI_SDK_PATH="${WASI_SDK_PATH:-$TOOLS_DIR/wasi-sdk-${WASI_SDK_VERSION}-x86_64-linux}"

log() { printf '\033[1;34m==>\033[0m %s\n' "$*"; }

if [ ! -d "$WASI_SDK_PATH" ]; then
  log "Fetching wasi-sdk ${WASI_SDK_VERSION}"
  mkdir -p "$TOOLS_DIR"
  major="${WASI_SDK_VERSION%%.*}"
  curl -fsSL --retry 3 \
    "https://github.com/WebAssembly/wasi-sdk/releases/download/wasi-sdk-${major}/wasi-sdk-${WASI_SDK_VERSION}-x86_64-linux.tar.gz" \
    | tar xz -C "$TOOLS_DIR"
fi

WASM_BINDGEN="$(command -v wasm-bindgen || true)"
if [ -z "$WASM_BINDGEN" ] || ! "$WASM_BINDGEN" --version | grep -q "$WASM_BINDGEN_VERSION"; then
  WASM_BINDGEN="$TOOLS_DIR/bin/wasm-bindgen"
  if [ ! -x "$WASM_BINDGEN" ] || ! "$WASM_BINDGEN" --version | grep -q "$WASM_BINDGEN_VERSION"; then
    log "Fetching wasm-bindgen ${WASM_BINDGEN_VERSION}"
    mkdir -p "$TOOLS_DIR/bin"
    curl -fsSL --retry 3 \
      "https://github.com/rustwasm/wasm-bindgen/releases/download/${WASM_BINDGEN_VERSION}/wasm-bindgen-${WASM_BINDGEN_VERSION}-x86_64-unknown-linux-musl.tar.gz" \
      | tar xz -C "$TOOLS_DIR/bin" --strip-components=1 \
        "wasm-bindgen-${WASM_BINDGEN_VERSION}-x86_64-unknown-linux-musl/wasm-bindgen"
  fi
fi

rustup target add wasm32-unknown-unknown >/dev/null 2>&1 || true

SYSROOT="$WASI_SDK_PATH/share/wasi-sysroot"

# The C++ sources build against the wasi-wasip1 sysroot; the Rust side stays on
# wasm32-unknown-unknown. Only libc++ and libc++abi get linked, never libc.
export CC_wasm32_unknown_unknown="$WASI_SDK_PATH/bin/clang"
export CXX_wasm32_unknown_unknown="$WASI_SDK_PATH/bin/clang++"
export AR_wasm32_unknown_unknown="$WASI_SDK_PATH/bin/llvm-ar"
export CFLAGS_wasm32_unknown_unknown="--target=wasm32-wasip1"
# The release profile sets panic=abort, which makes cc-rs pass -fno-exceptions.
# tinyformat does not compile that way, so switch exceptions back on; the throw
# path itself lands on the trapping stubs in src/csupport.rs.
export CXXFLAGS_wasm32_unknown_unknown="--target=wasm32-wasip1 -fexceptions"
export CXXSTDLIB_wasm32_unknown_unknown="c++"
export RUSTFLAGS="-L $SYSROOT/lib/wasm32-wasip1 -C link-arg=-lc++abi"

log "Building the wasm module"
cargo build --release --target wasm32-unknown-unknown

log "Generating JS bindings"
rm -rf dist
mkdir -p dist
"$WASM_BINDGEN" --target web --no-typescript --out-dir dist \
  target/wasm32-unknown-unknown/release/bdk_reserves_web.wasm

log "Copying the static site"
cp web/index.html web/app.js dist/
cp -r web/example dist/example
cp -r static dist/static

log "Done. dist/ is a static site: $(du -sh dist | cut -f1)"
